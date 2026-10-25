//! 测试公共设施：本地 HTTP mock 服务器（仅绑定 127.0.0.1 随机端口，测试结束即关闭）。
//!
//! 设计约束（TEST_PLAN §3）：自动化测试不依赖公网；不写防火墙规则；不需要提权。
//! 支持的路由：
//! - `/status/{code}`          → 返回对应状态码（301/302 附带 Location: /status/200）
//! - `/method-check`           → HEAD → 405；GET → 200（验证 HEAD→GET 回退）
//! - `/redirect/loop`          → 301 自我循环（验证重定向上限）
//! - `/redirect/exit`          → 301 指向白名单外域名（验证越界停止）
//! - `/redirect/to-401`        → 302 → /status/401（验证跟随后分类）
//! - `/slow?ms={n}`            → 延迟 n 毫秒后 200（超时/取消/截止）
//! - `/huge-405`               → HEAD → 405；GET → 200 + 2MB body（读体上限）
//! - `/close`                  → 直接断开（TLS 失败模拟：对 https:// 请求同样适用）
//!
//! 并发统计：active / max_active（验证并发上限）。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

#[allow(dead_code)]
pub struct MockServer {
    pub port: u16,
    max_active: Arc<AtomicUsize>,
    total_written: Arc<AtomicUsize>,
    write_failed: Arc<AtomicBool>,
    total_connections: Arc<AtomicUsize>,
    shutdown: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

#[allow(dead_code)]
impl MockServer {
    pub async fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("绑定 127.0.0.1 随机端口");
        let port = listener.local_addr().expect("本地地址").port();
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let total_written = Arc::new(AtomicUsize::new(0));
        let write_failed = Arc::new(AtomicBool::new(false));
        let total_connections = Arc::new(AtomicUsize::new(0));
        let shutdown = CancellationToken::new();

        let shutdown_loop = shutdown.clone();
        let active_loop = active.clone();
        let max_loop = max_active.clone();
        let written_loop = total_written.clone();
        let failed_loop = write_failed.clone();
        let conn_total_loop = total_connections.clone();
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_loop.cancelled() => break,
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _addr)) => {
                            let active = active_loop.clone();
                            let max = max_loop.clone();
                            let written = written_loop.clone();
                            let failed = failed_loop.clone();
                            let conn_total = conn_total_loop.clone();
                            tokio::spawn(async move {
                                conn_total.fetch_add(1, Ordering::SeqCst);
                                active.fetch_add(1, Ordering::SeqCst);
                                let now = active.load(Ordering::SeqCst);
                                max.fetch_max(now, Ordering::SeqCst);
                                handle_connection(stream, written, failed).await;
                                active.fetch_sub(1, Ordering::SeqCst);
                            });
                        }
                        Err(_) => break,
                    }
                }
            }
        });

        Self {
            port,
            max_active,
            total_written,
            write_failed,
            total_connections,
            shutdown,
            handle,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    pub fn max_active(&self) -> usize {
        self.max_active.load(Ordering::SeqCst)
    }

    pub fn total_written(&self) -> usize {
        self.total_written.load(Ordering::SeqCst)
    }

    /// 是否出现过对端提前断开导致的写失败（用于验证客户端读体上限）。
    pub fn write_failed(&self) -> bool {
        self.write_failed.load(Ordering::SeqCst)
    }

    /// 累计接受的连接数（验证"一次采样 = 恰好一次请求"，无隐式重试）。
    pub fn total_connections(&self) -> usize {
        self.total_connections.load(Ordering::SeqCst)
    }

    pub async fn shutdown(self) {
        self.shutdown.cancel();
        let _ = self.handle.await;
    }
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        301 => "Moved Permanently",
        302 => "Found",
        401 => "Unauthorized",
        403 => "Forbidden",
        405 => "Method Not Allowed",
        407 => "Proxy Authentication Required",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

async fn handle_connection(
    mut stream: TcpStream,
    total_written: Arc<AtomicUsize>,
    write_failed: Arc<AtomicBool>,
) {
    // 读取请求头；TLS ClientHello 等二进制垃圾不会形成合法请求行 → 直接断开。
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let read = tokio::time::timeout(Duration::from_millis(1000), stream.read(&mut chunk)).await;
        match read {
            Err(_) | Ok(Err(_)) | Ok(Ok(0)) => return,
            Ok(Ok(n)) => {
                buffer.extend_from_slice(&chunk[..n]);
                if let Some(end) = find_header_end(&buffer) {
                    break end;
                }
                if buffer.len() > 16 * 1024 {
                    return;
                }
            }
        }
    };

    let request_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let request_line = request_text.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let full_path = parts.next().unwrap_or("/");
    let is_method_token = !method.is_empty() && method.bytes().all(|b| b.is_ascii_alphabetic());
    if !is_method_token {
        return; // 非 HTTP 流量（含 TLS 握手字节）：立即关闭，制造握手失败。
    }

    let (path, query) = match full_path.split_once('?') {
        Some((path, query)) => (path, query),
        None => (full_path, ""),
    };
    let delay_ms: u64 = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("ms="))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);

    match path {
        "/close" => {} // 不响应直接断开
        "/slow" => {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            respond(&mut stream, method, 200, "", b"ok", &total_written).await;
        }
        "/method-check" => {
            if method == "HEAD" {
                respond(&mut stream, method, 405, "", b"", &total_written).await;
            } else {
                respond(&mut stream, method, 200, "", b"ok", &total_written).await;
            }
        }
        "/huge-405" => {
            if method == "HEAD" {
                respond(&mut stream, method, 405, "", b"", &total_written).await;
            } else {
                // GET：200 + 2MB body，按 8KB 慢滴流写出。客户端在 64KiB 处
                // 中止读取并断开 → 服务端后续 write 必然失败（write_failed）。
                let head = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2097152\r\nConnection: close\r\n\r\n";
                if write_ok(&mut stream, head.as_bytes(), &total_written).await {
                    let chunk = [b'x'; 8 * 1024];
                    let mut sent = 0usize;
                    while sent < 2 * 1024 * 1024 {
                        tokio::time::sleep(Duration::from_millis(2)).await;
                        if !write_ok(&mut stream, &chunk, &total_written).await {
                            write_failed.store(true, Ordering::SeqCst);
                            break;
                        }
                        sent += chunk.len();
                    }
                }
                let _ = stream.shutdown().await;
            }
        }
        "/redirect/loop" => {
            respond(
                &mut stream,
                method,
                301,
                "Location: /redirect/loop\r\n",
                b"",
                &total_written,
            )
            .await;
        }
        "/redirect/exit" => {
            respond(
                &mut stream,
                method,
                301,
                "Location: https://nonexistent.external.invalid/\r\n",
                b"",
                &total_written,
            )
            .await;
        }
        "/redirect/to-401" => {
            respond(
                &mut stream,
                method,
                302,
                "Location: /status/401\r\n",
                b"",
                &total_written,
            )
            .await;
        }
        path if path.starts_with("/status/") => {
            let code: u16 = path.trim_start_matches("/status/").parse().unwrap_or(200);
            let extra = if code == 301 || code == 302 {
                "Location: /status/200\r\n"
            } else {
                ""
            };
            respond(&mut stream, method, code, extra, b"ok", &total_written).await;
        }
        _ => {
            respond(&mut stream, method, 404, "", b"not found", &total_written).await;
        }
    }
}

async fn respond(
    stream: &mut TcpStream,
    method: &str,
    code: u16,
    extra_headers: &str,
    body: &[u8],
    total_written: &Arc<AtomicUsize>,
) {
    // HEAD 请求不发送 body（Content-Length: 0）。
    let body_bytes: &[u8] = if method == "HEAD" { b"" } else { body };
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        code,
        reason(code),
        body_bytes.len(),
    );
    let _ = write_ok(stream, head.as_bytes(), total_written).await;
    if !body_bytes.is_empty() {
        let _ = write_ok(stream, body_bytes, total_written).await;
    }
    let _ = stream.shutdown().await;
}

/// 写出并累计字节数；对端断开（写失败）返回 false。
async fn write_ok(stream: &mut TcpStream, bytes: &[u8], total_written: &Arc<AtomicUsize>) -> bool {
    match stream.write_all(bytes).await {
        Ok(()) => {
            total_written.fetch_add(bytes.len(), Ordering::SeqCst);
            true
        }
        Err(_) => false,
    }
}

pub mod proxy_mock;
