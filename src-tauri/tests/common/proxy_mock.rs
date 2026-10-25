//! Mock HTTP 正向代理 / Mock SOCKS5 服务器（Phase 3 代理行为测试，仅绑定 127.0.0.1）。
//!
//! HTTP 代理 mock 三种模式：
//! - `Forward200`：绝对 URI 的普通请求（`GET http://target/... HTTP/1.1`）→ 直接回 200
//!   （记录 `saw_absolute_get`，验证"经过代理"）；
//! - `TunnelTo`：CONNECT → 回 200 建立隧道，把字节双向转发到本机指定端口
//!   （用于制造"隧道建立但目标是纯 HTTP → TLS 失败"，证明 CONNECT 成功）；
//! - `Always407`：对 CONNECT 回 407（记录 `replied_407`）。
//!
//! SOCKS5 mock：完成方法协商与请求解析，**记录 ATYP**（1=IPv4 / 3=域名 / 4=IPv6）
//! 与目标地址——这正是 socks5（本地解析→ATYP=1）与 socks5h（远端解析→ATYP=3）
//! 的实证区分点；握手成功后把连接转发到本机 mock HTTP 服务器形成完整隧道。

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

// ---------------------------------------------------------------------------
// Mock HTTP 正向代理
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub enum HttpProxyMode {
    Forward200,
    TunnelTo { target_port: u16 },
    Always407,
}

#[allow(dead_code)]
pub struct MockHttpProxy {
    pub port: u16,
    saw_absolute_get: Arc<AtomicBool>,
    connect_targets: Arc<StdMutex<Vec<String>>>,
    replied_407: Arc<AtomicBool>,
    total_connections: Arc<AtomicUsize>,
    shutdown: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

#[allow(dead_code)]
impl MockHttpProxy {
    pub async fn spawn(mode: HttpProxyMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("绑定代理端口");
        let port = listener.local_addr().unwrap().port();
        let saw_absolute_get = Arc::new(AtomicBool::new(false));
        let connect_targets: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(Vec::new()));
        let replied_407 = Arc::new(AtomicBool::new(false));
        let total_connections = Arc::new(AtomicUsize::new(0));
        let shutdown = CancellationToken::new();

        let shutdown_loop = shutdown.clone();
        let saw = saw_absolute_get.clone();
        let connects = connect_targets.clone();
        let replied = replied_407.clone();
        let connections = total_connections.clone();
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_loop.cancelled() => break,
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _)) => {
                            let saw = saw.clone();
                            let connects = connects.clone();
                            let replied = replied.clone();
                            let connections = connections.clone();
                            let (mode_marker, target_port) = match &mode {
                                HttpProxyMode::Forward200 => (0u8, 0u16),
                                HttpProxyMode::TunnelTo { target_port } => (1, *target_port),
                                HttpProxyMode::Always407 => (2, 0),
                            };
                            connections.fetch_add(1, Ordering::SeqCst);
                            tokio::spawn(async move {
                                run_http_proxy_conn(
                                    stream,
                                    mode_marker,
                                    target_port,
                                    saw,
                                    connects,
                                    replied,
                                )
                                .await;
                            });
                        }
                        Err(_) => break,
                    }
                }
            }
        });

        Self {
            port,
            saw_absolute_get,
            connect_targets,
            replied_407,
            total_connections,
            shutdown,
            handle,
        }
    }

    pub fn saw_absolute_get(&self) -> bool {
        self.saw_absolute_get.load(Ordering::SeqCst)
    }

    pub fn connect_seen(&self) -> bool {
        !self.connect_targets.lock().unwrap().is_empty()
    }

    pub fn replied_407(&self) -> bool {
        self.replied_407.load(Ordering::SeqCst)
    }

    pub fn total_connections(&self) -> usize {
        self.total_connections.load(Ordering::SeqCst)
    }

    pub async fn shutdown(self) {
        self.shutdown.cancel();
        let _ = self.handle.await;
    }
}

async fn run_http_proxy_conn(
    mut stream: TcpStream,
    mode: u8,
    tunnel_target_port: u16,
    saw_absolute_get: Arc<AtomicBool>,
    connect_targets: Arc<StdMutex<Vec<String>>>,
    replied_407: Arc<AtomicBool>,
) {
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
    let request_line = request_text.lines().next().unwrap_or_default().to_string();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();

    if method == "CONNECT" {
        connect_targets.lock().unwrap().push(target.clone());
        if mode == 2 {
            let response = "HTTP/1.1 407 Proxy Authentication Required\r\n\
                Proxy-Authenticate: Basic realm=\"mock\"\r\n\
                Content-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
            replied_407.store(true, Ordering::SeqCst);
            return;
        }
        // 200：建立隧道后按模式转发。
        let response = "HTTP/1.1 200 Connection established\r\n\r\n";
        if stream.write_all(response.as_bytes()).await.is_err() {
            return;
        }
        if mode == 1
            && let Ok(target_stream) = TcpStream::connect(("127.0.0.1", tunnel_target_port)).await
        {
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut { target_stream }).await;
        }
        let _ = stream.shutdown().await;
        return;
    }

    if target.starts_with("http://") {
        saw_absolute_get.store(true, Ordering::SeqCst);
        let body = b"proxied-ok";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.write_all(body).await;
        let _ = stream.shutdown().await;
    }
}

// ---------------------------------------------------------------------------
// Mock SOCKS5
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct MockSocks5 {
    pub port: u16,
    last_atyp: Arc<AtomicU8>,
    last_addr: Arc<StdMutex<Vec<String>>>,
    shutdown: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

#[allow(dead_code)]
impl MockSocks5 {
    pub async fn spawn(target_port: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("绑定 socks 端口");
        let port = listener.local_addr().unwrap().port();
        let last_atyp = Arc::new(AtomicU8::new(0));
        let last_addr: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(Vec::new()));
        let shutdown = CancellationToken::new();

        let shutdown_loop = shutdown.clone();
        let atyp_flag = last_atyp.clone();
        let addr_flag = last_addr.clone();
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_loop.cancelled() => break,
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _)) => {
                            let atyp_flag = atyp_flag.clone();
                            let addr_flag = addr_flag.clone();
                            tokio::spawn(async move {
                                run_socks5_conn(stream, target_port, atyp_flag, addr_flag).await;
                            });
                        }
                        Err(_) => break,
                    }
                }
            }
        });
        Self {
            port,
            last_atyp,
            last_addr,
            shutdown,
            handle,
        }
    }

    /// 最后一次请求的 ATYP：1=IPv4（客户端本地解析，socks5 语义）；
    /// 3=域名（由代理解析，socks5h 语义）。
    pub fn last_atyp(&self) -> u8 {
        self.last_atyp.load(Ordering::SeqCst)
    }

    pub fn last_addr(&self) -> String {
        self.last_addr
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_default()
    }

    pub async fn shutdown(self) {
        self.shutdown.cancel();
        let _ = self.handle.await;
    }
}

async fn run_socks5_conn(
    mut stream: TcpStream,
    target_port: u16,
    last_atyp: Arc<AtomicU8>,
    last_addr: Arc<StdMutex<Vec<String>>>,
) {
    // 1) 方法协商：VER=5, NMETHODS → 回 0x05 0x00（无认证）。
    let mut header = [0u8; 2];
    if stream.read_exact(&mut header).await.is_err() || header[0] != 0x05 {
        return;
    }
    let nmethods = header[1] as usize;
    let mut methods = vec![0u8; nmethods];
    if stream.read_exact(&mut methods).await.is_err() {
        return;
    }
    if stream.write_all(&[0x05, 0x00]).await.is_err() {
        return;
    }

    // 2) 请求：VER CMD RSV ATYP ADDR PORT。
    let mut request_head = [0u8; 4];
    if stream.read_exact(&mut request_head).await.is_err() {
        return;
    }
    let command = request_head[1];
    let atyp = request_head[3];
    last_atyp.store(atyp, Ordering::SeqCst);
    let address = match atyp {
        1 => {
            let mut octets = [0u8; 4];
            if stream.read_exact(&mut octets).await.is_err() {
                return;
            }
            std::net::Ipv4Addr::from(octets).to_string()
        }
        3 => {
            let mut length = [0u8; 1];
            if stream.read_exact(&mut length).await.is_err() {
                return;
            }
            let mut name = vec![0u8; length[0] as usize];
            if stream.read_exact(&mut name).await.is_err() {
                return;
            }
            String::from_utf8_lossy(&name).to_string()
        }
        4 => {
            let mut octets = [0u8; 16];
            if stream.read_exact(&mut octets).await.is_err() {
                return;
            }
            std::net::Ipv6Addr::from(octets).to_string()
        }
        _ => return,
    };
    let mut port_bytes = [0u8; 2];
    if stream.read_exact(&mut port_bytes).await.is_err() {
        return;
    }
    last_addr.lock().unwrap().push(address);

    // 3) 应答：成功（BND.ADDR 全零）。
    if stream
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
        .is_err()
    {
        return;
    }

    // 4) CONNECT：把隧道转发到本机 mock HTTP 服务器。
    if command == 0x01
        && let Ok(target_stream) = TcpStream::connect(("127.0.0.1", target_port)).await
    {
        let _ = tokio::io::copy_bidirectional(&mut stream, &mut { target_stream }).await;
    }
    let _ = stream.shutdown().await;
}
