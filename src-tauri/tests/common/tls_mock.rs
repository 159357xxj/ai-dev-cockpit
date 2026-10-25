//! 本地 TLS mock 服务器（Phase 4 补强 2）。
//!
//! 测试运行时生成专用 CA 与叶证书（SAN 含 127.0.0.1/localhost），服务器用叶
//! 证书做 TLS；客户端通过 `HttpsProbeParams::additional_trusted_root_der` 把
//! 该 CA 加入信任库——**这只是扩展信任库，生产证书校验路径完全不变**：
//! 未注入根证书时对本服务器访问必然 TLS 失败（有负向测试守护）。

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_util::sync::CancellationToken;

#[allow(dead_code)]
pub struct MockTlsServer {
    pub port: u16,
    /// 测试 CA 证书（DER）——注入到被测客户端的信任库。
    pub root_cert_der: Vec<u8>,
    shutdown: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

/// 生成测试 CA + 叶证书（RSA-2048，SAN：IP 127.0.0.1 / DNS localhost）。
///
/// 为什么用 openssl CLI 而非 rcgen：SChannel（生产 native-tls 后端）无法验证
/// ring 生成的 ECDSA 证书签名（TRUST_E_CERT_SIGNATURE，已知兼容性问题），
/// 而 rcgen 的 ring 后端不支持 RSA 生成。openssl 随 Git for Windows 分发，
/// 仅生成测试专用密钥材料（写入系统临时目录，测试后清理），不触碰任何系统信任库。
#[allow(dead_code)]
fn generate_certs() -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>, Vec<u8>), String> {
    let work_dir = std::env::temp_dir().join(format!(
        "adc-tls-mock-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&work_dir).map_err(|error| format!("{error}"))?;

    // OpenSSL 可移植性（V2 QA 发现：干净 Windows 环境下 openssl 会找
    // `C:\Program Files\Common Files\ssl\openssl.cnf` 而失败）。
    // 生成最小配置文件，并以**进程级** OPENSSL_CONF 传给 openssl 子进程——
    // 不修改系统环境变量，不依赖 Conda/Git Bash 的配置路径。
    let openssl_conf = work_dir.join("openssl.cnf");
    std::fs::write(
        &openssl_conf,
        "[openssl_init]\n# 最小可用配置（仅让 openssl 启动）\n",
    )
    .map_err(|error| format!("{error}"))?;

    let run = |args: &[&str]| -> Result<String, String> {
        let output = std::process::Command::new("openssl")
            .args(args)
            .current_dir(&work_dir)
            .env("OPENSSL_CONF", &openssl_conf)
            .output()
            .map_err(|error| {
                format!("openssl 不可用（{error}）；完整 TLS 成功测试需要 openssl 生成 RSA 证书")
            })?;
        if !output.status.success() {
            return Err(format!(
                "openssl 失败：{}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    };

    // 1) 自签名 CA（RSA-2048）。
    run(&[
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-keyout",
        "ca.key",
        "-out",
        "ca.pem",
        "-days",
        "2",
        "-subj",
        "/CN=adc-test-ca",
    ])?;

    // 2) 叶证书 CSR（SAN：IP + DNS）。
    run(&[
        "req",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-keyout",
        "leaf.key",
        "-out",
        "leaf.csr",
        "-days",
        "2",
        "-subj",
        "/CN=127.0.0.1",
        "-addext",
        "subjectAltName=IP:127.0.0.1,DNS:localhost",
        "-addext",
        "extendedKeyUsage=serverAuth",
        "-addext",
        "keyUsage=digitalSignature,keyEncipherment",
    ])?;

    // 3) CA 签发叶证书（带上 SAN 扩展）。
    run(&[
        "x509",
        "-req",
        "-in",
        "leaf.csr",
        "-CA",
        "ca.pem",
        "-CAkey",
        "ca.key",
        "-days",
        "2",
        "-copy_extensions",
        "copy",
        "-CAcreateserial",
        "-out",
        "leaf.pem",
    ])?;

    // 4) PEM → DER；私钥转 PKCS#8（rustls 的 PrivatePkcs8KeyDer 要求，
    //    openssl 3 的 `pkey -outform der` 默认输出 PKCS#1 会导致解析失败）。
    run(&["x509", "-in", "ca.pem", "-outform", "der", "-out", "ca.der"])?;
    run(&[
        "x509", "-in", "leaf.pem", "-outform", "der", "-out", "leaf.der",
    ])?;
    run(&[
        "pkcs8",
        "-topk8",
        "-nocrypt",
        "-in",
        "leaf.key",
        "-outform",
        "der",
        "-out",
        "leaf.key.der",
    ])?;

    let ca_der = std::fs::read(work_dir.join("ca.der")).map_err(|error| format!("{error}"))?;
    let leaf_der = std::fs::read(work_dir.join("leaf.der")).map_err(|error| format!("{error}"))?;
    let key_der =
        std::fs::read(work_dir.join("leaf.key.der")).map_err(|error| format!("{error}"))?;
    let _ = std::fs::remove_dir_all(&work_dir);

    Ok((
        CertificateDer::from(leaf_der),
        PrivateKeyDer::Pkcs8(tokio_rustls::rustls::pki_types::PrivatePkcs8KeyDer::from(
            key_der,
        )),
        ca_der,
    ))
}

#[allow(dead_code)]
impl MockTlsServer {
    pub async fn spawn() -> Result<Self, String> {
        let (leaf_der, key_der, root_cert_der) = generate_certs()?;
        // 服务端下发完整链（叶 + CA）：SChannel 部分场景要求显式链。
        let ca_chain_cert = CertificateDer::from(root_cert_der.clone());

        let mut server_config = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![leaf_der, ca_chain_cert], key_der)
            .map_err(|error| format!("TLS 服务端配置失败：{error}"))?;
        server_config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|error| format!("绑定 TLS 端口失败：{error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("{error}"))?
            .port();
        let shutdown = CancellationToken::new();

        let shutdown_loop = shutdown.clone();
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_loop.cancelled() => break,
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _addr)) => {
                            let acceptor = acceptor.clone();
                            tokio::spawn(async move {
                                if let Ok(mut tls_stream) = acceptor.accept(stream).await {
                                    // 读取 HTTP 请求头后直接回 200（测试只需成功事务）。
                                    let mut buffer = Vec::new();
                                    let mut chunk = [0u8; 1024];
                                    loop {
                                        match tokio::time::timeout(
                                            std::time::Duration::from_millis(1000),
                                            tls_stream.read(&mut chunk),
                                        )
                                        .await
                                        {
                                            Ok(Ok(0)) | Err(_) | Ok(Err(_)) => break,
                                            Ok(Ok(n)) => {
                                                buffer.extend_from_slice(&chunk[..n]);
                                                if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    let body = b"tls-ok";
                                    let response = format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                        body.len()
                                    );
                                    let _ = tls_stream.write_all(response.as_bytes()).await;
                                    let _ = tls_stream.write_all(body).await;
                                    let _ = tls_stream.shutdown().await;
                                }
                            });
                        }
                        Err(_) => break,
                    }
                }
            }
        });

        Ok(Self {
            port,
            root_cert_der,
            shutdown,
            handle,
        })
    }

    pub fn url(&self, path: &str) -> String {
        format!("https://127.0.0.1:{}{path}", self.port)
    }

    pub async fn shutdown(self) {
        self.shutdown.cancel();
        let _ = self.handle.await;
    }
}
