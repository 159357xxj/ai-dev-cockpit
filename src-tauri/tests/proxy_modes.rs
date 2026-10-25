//! 代理行为端到端测试（Phase 3）：三种路由模式 × 本地 mock 代理。
//!
//! 全部离线（仅 127.0.0.1），覆盖：
//! - manual http 代理的绝对 URI 转发（证明"经过代理"）；
//! - manual https 代理的 CONNECT 隧道建立（隧道成功 + 终端 TLS 失败如实分类）；
//! - 代理 407 → `proxy_unavailable`；
//! - 指定代理不可达 → `proxy_unavailable` 且**绝不回落直连**；
//! - socks5（本地解析，ATYP=1）与 socks5h（远端解析，ATYP=3）的实际 DNS 语义；
//! - library_default 遵循环境变量（含优先级），no_app_proxy 不受环境变量影响。
//!
//! 环境变量操作用互斥锁串行化；本文件是独立测试二进制，env 改动不外泄。

mod common;

use std::time::Duration;

use ai_dev_cockpit_lib::diagnostic::https::{HttpsProbeParams, RedirectRules, probe_https};
use ai_dev_cockpit_lib::diagnostic::types::{ErrorKind, RouteMode};
use common::MockServer;
use common::proxy_mock::{HttpProxyMode, MockHttpProxy, MockSocks5};
use tokio_util::sync::CancellationToken;

/// 环境变量是进程级状态：env 测试必须串行（tokio Mutex：守卫可跨 await）。
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// env 测试统一入口：取锁并写入占位 NO_PROXY，确保 no 槽位不被本机注册表
/// ProxyOverride（含 127.*）占用。本机实测：系统代理开启（Clash 系，
/// 127.0.0.1:7890）且 ProxyOverride 含 127.*，该占位同时实证了
/// "env NO_PROXY 优先于注册表 ProxyOverride" 的优先级。
async fn lock_env() -> tokio::sync::MutexGuard<'static, ()> {
    let guard = ENV_LOCK.lock().await;
    unsafe { std::env::set_var("NO_PROXY", "example.com") };
    guard
}

fn rules_any_http(port: u16) -> RedirectRules {
    let mut hosts = std::collections::HashSet::new();
    hosts.insert("127.0.0.1".to_string());
    hosts.insert("localhost".to_string());
    RedirectRules {
        allowed_hosts: hosts,
        allowed_schemes: vec!["http".to_string(), "https".to_string()],
        allowed_ports: vec![443, port],
        max_redirects: 5,
    }
}

fn params(
    target_url: String,
    route_mode: RouteMode,
    manual_proxy: Option<String>,
    timeout: Duration,
    rules: RedirectRules,
) -> HttpsProbeParams {
    HttpsProbeParams {
        target_id: "mock-target".to_string(),
        url: target_url,
        route_mode,
        manual_proxy_url: manual_proxy,
        timeout,
        rules,
    }
}

async fn run(params: &HttpsProbeParams) -> ai_dev_cockpit_lib::diagnostic::types::ProbeSample {
    probe_https(params, CancellationToken::new()).await
}

// ---------------------------------------------------------------------------
// manual_local_proxy（HTTP 代理）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn manual_http_proxy_forwards_absolute_uri_and_succeeds() {
    let target = MockServer::spawn().await;
    let proxy = MockHttpProxy::spawn(HttpProxyMode::Forward200).await;

    let sample = run(&params(
        target.url("/status/200"),
        RouteMode::ManualLocalProxy,
        Some(format!("http://127.0.0.1:{}", proxy.port)),
        Duration::from_secs(5),
        rules_any_http(target.port),
    ))
    .await;

    assert!(sample.success, "经可用 HTTP 代理应成功：{:?}", sample.error);
    assert_eq!(sample.http_status, Some(200));
    assert!(
        proxy.saw_absolute_get(),
        "HTTP 代理应收到绝对 URI 请求（证明流量经过代理而非直连）"
    );
    proxy.shutdown().await;
    target.shutdown().await;
}

#[tokio::test]
async fn manual_https_proxy_establishes_connect_tunnel() {
    // 隧道目标 = 纯 HTTP mock：CONNECT 成功 + 客户端 TLS 握手失败（如实分类为 tls_failure）。
    let target = MockServer::spawn().await;
    let proxy = MockHttpProxy::spawn(HttpProxyMode::TunnelTo {
        target_port: target.port,
    })
    .await;

    let sample = run(&params(
        format!("https://127.0.0.1:{}/status/200", target.port),
        RouteMode::ManualLocalProxy,
        Some(format!("http://127.0.0.1:{}", proxy.port)),
        Duration::from_secs(5),
        rules_any_http(target.port),
    ))
    .await;

    assert!(proxy.connect_seen(), "代理必须收到 CONNECT（隧道协商成功）");
    assert!(
        !sample.success,
        "对纯 HTTP 端口做 TLS 必然失败，不得伪造成功"
    );
    assert_eq!(
        sample.error.as_ref().map(|error| error.kind),
        Some(ErrorKind::TlsFailure),
        "隧道可用、TLS 失败：实际分类 {:?}",
        sample.error
    );
    proxy.shutdown().await;
    target.shutdown().await;
}

#[tokio::test]
async fn proxy_407_maps_to_proxy_unavailable() {
    let target = MockServer::spawn().await;
    let proxy = MockHttpProxy::spawn(HttpProxyMode::Always407).await;

    let sample = run(&params(
        format!("https://127.0.0.1:{}/status/200", target.port),
        RouteMode::ManualLocalProxy,
        Some(format!("http://127.0.0.1:{}", proxy.port)),
        Duration::from_secs(5),
        rules_any_http(target.port),
    ))
    .await;

    assert!(proxy.replied_407(), "代理应已回复 407");
    assert!(!sample.success);
    assert_eq!(
        sample.error.as_ref().map(|error| error.kind),
        Some(ErrorKind::ProxyUnavailable),
        "407 属于『指定代理不可用』语义：实际分类 {:?}",
        sample.error
    );
    proxy.shutdown().await;
    target.shutdown().await;
}

// ---------------------------------------------------------------------------
// manual_local_proxy：不可达绝不回落
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dead_manual_proxy_never_falls_back_to_direct() {
    // 目标 mock 服务器活着；代理端口无监听。
    let target = MockServer::spawn().await;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("占位端口");
    let dead_port = listener.local_addr().unwrap().port();
    drop(listener);

    for scheme in ["http", "socks5", "socks5h"] {
        let sample = run(&params(
            target.url("/status/200"),
            RouteMode::ManualLocalProxy,
            Some(format!("{scheme}://127.0.0.1:{dead_port}")),
            Duration::from_secs(3),
            rules_any_http(target.port),
        ))
        .await;
        assert!(!sample.success, "{scheme} 死代理不得回落为直连成功");
        assert_eq!(
            sample.error.as_ref().map(|error| error.kind),
            Some(ErrorKind::ProxyUnavailable),
            "{scheme} 死代理应归类 proxy_unavailable：实际 {:?}",
            sample.error
        );
    }
    target.shutdown().await;
}

// ---------------------------------------------------------------------------
// manual_local_proxy：SOCKS5 与 SOCKS5h 的 DNS 语义实证
// ---------------------------------------------------------------------------

#[tokio::test]
async fn socks5_resolves_locally_atyp_is_ipv4() {
    let target = MockServer::spawn().await;
    let socks = MockSocks5::spawn(target.port).await;

    let sample = run(&params(
        format!("http://localhost:{}/status/200", target.port),
        RouteMode::ManualLocalProxy,
        Some(format!("socks5://127.0.0.1:{}", socks.port)),
        Duration::from_secs(5),
        rules_any_http(target.port),
    ))
    .await;

    assert!(
        sample.success,
        "socks5 经 mock 隧道应成功：{:?}",
        sample.error
    );
    // socks5：客户端本地解析——代理收到的是地址（IPv4/IPv6 均可，localhost 常解析为 ::1），
    // 绝不是域名（那是 socks5h 语义）。
    let atyp = socks.last_atyp();
    assert!(
        atyp == 1 || atyp == 4,
        "socks5 应由客户端本地解析（ATYP∈{{1,4}}），实际 {atyp}"
    );
    assert!(
        socks.last_addr() == "127.0.0.1" || socks.last_addr() == "::1",
        "本地解析结果应为回环地址，实际 {}",
        socks.last_addr()
    );
    socks.shutdown().await;
    target.shutdown().await;
}

#[tokio::test]
async fn socks5h_delegates_dns_to_proxy_atyp_is_domain() {
    let target = MockServer::spawn().await;
    let socks = MockSocks5::spawn(target.port).await;

    let sample = run(&params(
        format!("http://localhost:{}/status/200", target.port),
        RouteMode::ManualLocalProxy,
        Some(format!("socks5h://127.0.0.1:{}", socks.port)),
        Duration::from_secs(5),
        rules_any_http(target.port),
    ))
    .await;

    assert!(
        sample.success,
        "socks5h 经 mock 隧道应成功：{:?}",
        sample.error
    );
    assert_eq!(
        socks.last_atyp(),
        3,
        "socks5h 应把域名交给代理解析（ATYP=domain）"
    );
    assert_eq!(socks.last_addr(), "localhost");
    socks.shutdown().await;
    target.shutdown().await;
}

// ---------------------------------------------------------------------------
// library_default 与 no_app_proxy：环境变量优先级
// ---------------------------------------------------------------------------

#[tokio::test]
async fn library_default_honours_https_proxy_env_var() {
    let _guard = lock_env().await;
    let target = MockServer::spawn().await;
    let proxy = MockHttpProxy::spawn(HttpProxyMode::TunnelTo {
        target_port: target.port,
    })
    .await;

    // SAFETY（测试进程）：互斥锁内修改 env；仅影响本测试二进制。
    unsafe { std::env::set_var("HTTPS_PROXY", format!("http://127.0.0.1:{}", proxy.port)) };
    eprintln!("diag: env={:?}", std::env::var("HTTPS_PROXY"));
    let sample = run(&params(
        format!("https://127.0.0.1:{}/status/200", target.port),
        RouteMode::LibraryDefault,
        None,
        Duration::from_secs(5),
        rules_any_http(target.port),
    ))
    .await;
    unsafe { std::env::remove_var("HTTPS_PROXY") };
    unsafe { std::env::remove_var("NO_PROXY") };
    eprintln!("diag: sample={:?}", sample.error);

    assert!(
        proxy.connect_seen(),
        "library_default 必须遵循 HTTPS_PROXY 环境变量（CONNECT 被代理看到）"
    );
    // 隧道目标是纯 HTTP → TLS 失败如实分类（重点是"经过了代理"）。
    assert_eq!(
        sample.error.as_ref().map(|error| error.kind),
        Some(ErrorKind::TlsFailure)
    );
    proxy.shutdown().await;
    target.shutdown().await;
}

#[tokio::test]
async fn library_default_prefers_https_proxy_over_all_proxy() {
    let _guard = lock_env().await;
    let target = MockServer::spawn().await;
    let https_proxy = MockHttpProxy::spawn(HttpProxyMode::Forward200).await;
    let all_proxy = MockHttpProxy::spawn(HttpProxyMode::Forward200).await;

    unsafe {
        std::env::set_var(
            "HTTPS_PROXY",
            format!("http://127.0.0.1:{}", https_proxy.port),
        );
        std::env::set_var("ALL_PROXY", format!("http://127.0.0.1:{}", all_proxy.port));
    }
    let sample = run(&params(
        format!("https://127.0.0.1:{}/status/200", target.port),
        RouteMode::LibraryDefault,
        None,
        Duration::from_secs(5),
        rules_any_http(target.port),
    ))
    .await;
    unsafe {
        std::env::remove_var("HTTPS_PROXY");
        std::env::remove_var("ALL_PROXY");
        std::env::remove_var("NO_PROXY");
    }

    // 优先级（源码实证 + 实测）：HTTPS_PROXY > ALL_PROXY。
    assert!(https_proxy.connect_seen(), "HTTPS_PROXY 应优先被使用");
    assert!(
        !all_proxy.connect_seen(),
        "ALL_PROXY 不应在 HTTPS_PROXY 存在时被使用"
    );
    assert!(
        sample.error.is_some(),
        "Forward200 代理不转发 TLS → 失败属预期"
    );
    https_proxy.shutdown().await;
    all_proxy.shutdown().await;
    target.shutdown().await;
}

#[tokio::test]
async fn no_app_proxy_ignores_proxy_env_vars() {
    let _guard = lock_env().await;
    let target = MockServer::spawn().await;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("占位端口");
    let dead_port = listener.local_addr().unwrap().port();
    drop(listener);

    unsafe { std::env::set_var("HTTPS_PROXY", format!("http://127.0.0.1:{dead_port}")) };
    let sample = run(&params(
        target.url("/status/200"),
        RouteMode::NoAppProxy,
        None,
        Duration::from_secs(5),
        rules_any_http(target.port),
    ))
    .await;
    unsafe { std::env::remove_var("HTTPS_PROXY") };
    unsafe { std::env::remove_var("NO_PROXY") };

    assert!(
        sample.success,
        "no_app_proxy 不受 HTTPS_PROXY 影响（直连 mock 成功）：{:?}",
        sample.error
    );
    target.shutdown().await;
}

#[tokio::test]
async fn library_default_honours_system_proxy_override_registry() {
    // 真实现场观测（不修改注册表）：本机系统代理开启（ProxyEnable=1，Clash 系）
    // 且 ProxyOverride 含 127.* 时，library_default 对 127.0.0.1 目标即使设置了
    // env HTTPS_PROXY 也会直连——因为 no 槽位被注册表例外占据（源码实证 + 实测）。
    // 仅在本机确实启用用户级系统代理时断言；否则跳过（记录，不伪造）。
    let _guard = lock_env().await;
    // 观测注册表例外要求 no 槽位为空：移除占位 NO_PROXY，
    // 让注册表 ProxyOverride（含 127.*）进入匹配器。
    unsafe { std::env::remove_var("NO_PROXY") };
    let snapshot = ai_dev_cockpit_lib::platform::windows::proxy::snapshot();
    if snapshot.manual_proxy_enabled != Some(true) {
        eprintln!("skip: 本机未启用用户级系统代理，跳过注册表例外观测");
        return;
    }
    let target = MockServer::spawn().await;
    let proxy = MockHttpProxy::spawn(HttpProxyMode::TunnelTo {
        target_port: target.port,
    })
    .await;

    unsafe { std::env::set_var("HTTPS_PROXY", format!("http://127.0.0.1:{}", proxy.port)) };
    let sample = run(&params(
        format!("https://127.0.0.1:{}/status/200", target.port),
        RouteMode::LibraryDefault,
        None,
        Duration::from_secs(5),
        rules_any_http(target.port),
    ))
    .await;
    unsafe { std::env::remove_var("HTTPS_PROXY") };
    unsafe { std::env::remove_var("NO_PROXY") };

    assert!(
        !proxy.connect_seen(),
        "注册表 ProxyOverride 含 127.* 时，library_default 对 127.0.0.1 应直连"
    );
    assert_eq!(
        sample.error.as_ref().map(|error| error.kind),
        Some(ErrorKind::TlsFailure),
        "直连纯 HTTP 端口 → TLS 失败"
    );
    proxy.shutdown().await;
    target.shutdown().await;
}
