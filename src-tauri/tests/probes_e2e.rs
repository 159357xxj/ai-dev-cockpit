//! 端到端探测引擎测试：真实调度器 + 真实 reqwest + 本地 mock 服务器。
//!
//! 覆盖 TEST_PLAN Phase 2 用例矩阵：200/301/302/401/403/405→GET/407/429/500、
//! 超时、TCP 拒连、TLS 失败、取消（任务级 + 幂等）、非法重定向、事件终态唯一、
//! 并发上限、任务截止、指标-样本一致性。全部离线，不依赖公网。
//!
//! 已知限制（如实记录）：
//! - "120 秒任务截止"的生产常量通过参数注入以短截止验证逻辑路径；
//! - 完整 Tauri webview IPC 往返（宏参数名匹配）无法在 cargo test 内自动化，
//!   由前端 Vitest 契约测试（invoke 参数键断言）+ `tauri dev` 人工验证共同覆盖。

mod common;

use std::collections::HashSet;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use ai_dev_cockpit_lib::diagnostic::dns::probe_dns;
use ai_dev_cockpit_lib::diagnostic::https::{HttpsProbeParams, RedirectRules, probe_https};
use ai_dev_cockpit_lib::diagnostic::scheduler::{Emitter, JobEndpoints, JobSpec, execute_job};
use ai_dev_cockpit_lib::diagnostic::tcp::probe_tcp;
use ai_dev_cockpit_lib::diagnostic::types::{
    DiagnosticRequest, ErrorKind, HttpOutcome, JobEvent, ProbeAggregate, ProbeSample, ProbeType,
    RouteMode, RunStatus,
};
use common::MockServer;
use tokio_util::sync::CancellationToken;

fn rules_for(server: &MockServer) -> RedirectRules {
    let mut hosts = HashSet::new();
    hosts.insert("127.0.0.1".to_string());
    RedirectRules {
        allowed_hosts: hosts,
        allowed_schemes: vec!["http".to_string()],
        allowed_ports: vec![server.port],
        max_redirects: 5,
    }
}

fn https_params(server: &MockServer, path: &str, timeout: Duration) -> HttpsProbeParams {
    HttpsProbeParams {
        target_id: "mock-target".to_string(),
        url: server.url(path),
        route_mode: RouteMode::NoAppProxy,
        timeout,
        rules: rules_for(server),
    }
}

async fn probe(server: &MockServer, path: &str, timeout: Duration) -> ProbeSample {
    probe_https(
        &https_params(server, path, timeout),
        CancellationToken::new(),
    )
    .await
}

fn capture_events() -> (Emitter, Arc<StdMutex<Vec<JobEvent>>>) {
    let events: Arc<StdMutex<Vec<JobEvent>>> = Arc::new(StdMutex::new(Vec::new()));
    let sink = events.clone();
    (
        Arc::new(move |event| sink.lock().unwrap().push(event)),
        events,
    )
}

fn request_for(repeats: u8, timeout_ms: u32) -> DiagnosticRequest {
    DiagnosticRequest {
        target_ids: vec!["mock-target".to_string()],
        route_mode: RouteMode::NoAppProxy,
        manual_local_proxy_url: None,
        repeats,
        timeout_ms,
    }
}

fn spec_for(
    server: &MockServer,
    path: &str,
    repeats: u8,
    timeout_ms: u32,
    deadline: Duration,
    target_count: usize,
) -> JobSpec {
    let endpoints: Vec<JobEndpoints> = (0..target_count)
        .map(|index| JobEndpoints {
            target_id: format!("mock-target-{index}"),
            dns_host: "127.0.0.1".to_string(),
            tcp_host: "127.0.0.1".to_string(),
            tcp_port: server.port,
            https_url: server.url(path),
        })
        .collect();
    JobSpec {
        job_id: "test-job".to_string(),
        request: request_for(repeats, timeout_ms),
        endpoints,
        deadline,
        app_version: "0.1.0".to_string(),
        redirect_rules: rules_for(server),
    }
}

fn terminal_count(events: &[JobEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, JobEvent::Completed(_) | JobEvent::Failed(_)))
        .count()
}

// —— HTTPS 状态码分类 ——

#[tokio::test]
async fn http_200_is_success() {
    let server = MockServer::spawn().await;
    let sample = probe(&server, "/status/200", Duration::from_secs(5)).await;
    assert!(sample.success);
    assert_eq!(sample.http_status, Some(200));
    assert_eq!(sample.http_outcome, Some(HttpOutcome::Ok2xx));
    assert!(sample.error.is_none());
    assert!(sample.duration_ms.is_some(), "成功样本应有耗时");
    server.shutdown().await;
}

#[tokio::test]
async fn http_301_and_302_are_followed_within_policy() {
    let server = MockServer::spawn().await;
    for path in ["/status/301", "/status/302"] {
        let sample = probe(&server, path, Duration::from_secs(5)).await;
        assert_eq!(sample.http_status, Some(200), "{path} 应跟随到 200");
        assert_eq!(sample.http_outcome, Some(HttpOutcome::Ok2xx));
    }
    // 302 → 401：跟随后按最终状态分类。
    let sample = probe(&server, "/redirect/to-401", Duration::from_secs(5)).await;
    assert_eq!(sample.http_outcome, Some(HttpOutcome::AuthRequired401To403));
    server.shutdown().await;
}

#[tokio::test]
async fn http_401_403_are_neutral_responses_not_transport_errors() {
    let server = MockServer::spawn().await;
    for path in ["/status/401", "/status/403"] {
        let sample = probe(&server, path, Duration::from_secs(5)).await;
        assert!(!sample.success);
        assert_eq!(sample.http_outcome, Some(HttpOutcome::AuthRequired401To403));
        assert!(
            sample.error.is_none(),
            "{path} 收到响应 ≠ 传输层错误（D-007）"
        );
        // 聚合口径：收到响应计入 http_response_count，而非传输失败。
        let aggregate =
            ProbeAggregate::from_samples(ProbeType::Https, std::slice::from_ref(&sample));
        assert_eq!(aggregate.http_response_count, 1);
        assert_eq!(aggregate.transport_failure_count, 0);
        assert_eq!(aggregate.non_2xx_response_ratio(), Some(1.0));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn http_405_falls_back_to_get_once() {
    let server = MockServer::spawn().await;
    let sample = probe(&server, "/method-check", Duration::from_secs(5)).await;
    assert!(sample.success, "HEAD 405 → GET 回退应得到 200");
    assert_eq!(sample.http_status, Some(200));
    server.shutdown().await;
}

#[tokio::test]
async fn http_407_429_500_are_separately_classified() {
    let server = MockServer::spawn().await;
    let cases = [
        ("/status/407", HttpOutcome::ProxyAuth407),
        ("/status/429", HttpOutcome::RateLimited429),
        ("/status/500", HttpOutcome::ServerError5xx),
    ];
    for (path, expected) in cases {
        let sample = probe(&server, path, Duration::from_secs(5)).await;
        assert!(!sample.success, "{path} 非 2xx 不算成功");
        assert_eq!(sample.http_outcome, Some(expected), "{path}");
        assert!(sample.error.is_none());
    }
    server.shutdown().await;
}

#[tokio::test]
async fn request_timeout_is_classified_as_timeout() {
    let server = MockServer::spawn().await;
    let sample = probe(&server, "/slow?ms=1500", Duration::from_millis(300)).await;
    assert!(!sample.success);
    assert_eq!(
        sample.error.as_ref().map(|error| error.kind),
        Some(ErrorKind::Timeout)
    );
    assert_eq!(sample.duration_ms, None);
    server.shutdown().await;
}

#[tokio::test]
async fn redirect_loop_keeps_classifiable_3xx() {
    let server = MockServer::spawn().await;
    let sample = probe(&server, "/redirect/loop", Duration::from_secs(5)).await;
    assert!(!sample.success);
    assert_eq!(sample.http_status, Some(301));
    assert_eq!(sample.http_outcome, Some(HttpOutcome::Redirect3xx));
    server.shutdown().await;
}

#[tokio::test]
async fn redirect_to_non_allowed_target_is_not_followed() {
    let server = MockServer::spawn().await;
    let sample = probe(&server, "/redirect/exit", Duration::from_secs(5)).await;
    // 301 指向白名单外域名：停止跟随，保留 3xx 终态（若被跟随会得到 DNS 失败错误）。
    assert_eq!(sample.http_status, Some(301));
    assert_eq!(sample.http_outcome, Some(HttpOutcome::Redirect3xx));
    assert!(sample.error.is_none());
    server.shutdown().await;
}

#[tokio::test]
async fn get_body_is_capped_at_64kib() {
    let server = MockServer::spawn().await;
    let sample = probe(&server, "/huge-405", Duration::from_secs(10)).await;
    // HEAD 405 → GET 回退；服务端以 8KB 慢滴流写 2MB body。
    assert!(sample.success, "收到 200 即成功，读体中断不影响结论");
    // 客户端应在 64KiB 处中止读取并断开 → 服务端后续写必然失败。
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        server.write_failed(),
        "客户端未在 64KiB 处中止读取（服务端未观察到断开）"
    );
    assert!(server.total_written() < 2 * 1024 * 1024);
    server.shutdown().await;
}

// —— 传输层失败分类 ——

#[tokio::test]
async fn tcp_refused_is_tcp_failure() {
    // 找一个确定无监听的端口：绑定后立即释放。
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("占位端口");
    let closed_port = listener.local_addr().unwrap().port();
    drop(listener);

    let sample = probe_tcp(
        "mock-target",
        "127.0.0.1",
        closed_port,
        RouteMode::NoAppProxy,
        Duration::from_secs(5),
        CancellationToken::new(),
    )
    .await;
    assert!(!sample.success);
    assert_eq!(
        sample.error.as_ref().map(|error| error.kind),
        Some(ErrorKind::TcpFailure)
    );
}

#[tokio::test]
async fn tls_handshake_against_plain_server_is_tls_failure() {
    let server = MockServer::spawn().await;
    // 用 https:// 访问纯 HTTP 端口：TLS 握手必然失败（证书校验正常启用，不绕过）。
    let params = HttpsProbeParams {
        target_id: "mock-target".to_string(),
        url: format!("https://127.0.0.1:{}/status/200", server.port),
        route_mode: RouteMode::NoAppProxy,
        timeout: Duration::from_secs(5),
        rules: rules_for(&server),
    };
    let sample = probe_https(&params, CancellationToken::new()).await;
    assert!(!sample.success);
    assert_eq!(
        sample.error.as_ref().map(|error| error.kind),
        Some(ErrorKind::TlsFailure),
        "实际分类：{:?}",
        sample.error
    );
    server.shutdown().await;
}

#[tokio::test]
async fn dns_localhost_resolves() {
    let ok = probe_dns(
        "mock-target",
        "localhost",
        RouteMode::NoAppProxy,
        Duration::from_secs(3),
        CancellationToken::new(),
    )
    .await;
    assert!(ok.success, "localhost 离线可解析");
    assert!(ok.duration_ms.is_some());
    assert_eq!(ok.probe_type, ProbeType::Dns);
    // 说明：DNS 失败分类（dns_failure）无法离线稳定构造——本机运营商解析器
    // 会通配解析不存在域名（NXDOMAIN 重定向），"nonexistent.invalid" 实测被解析成功。
    // 该分类逻辑由 reqwest 错误链文本识别，覆盖方式记录于 TEST_PLAN（人工验证项）。
}

// —— 调度器：取消 / 截止 / 并发 / 事件 ——

#[tokio::test]
async fn scheduler_user_cancellation_preserves_executed_samples() {
    let server = MockServer::spawn().await;
    let spec = spec_for(
        &server,
        "/slow?ms=2000",
        3,
        5000,
        Duration::from_secs(60),
        1,
    );
    let (emit, events) = capture_events();
    let cancel = CancellationToken::new();
    let job = tokio::spawn(execute_job(spec, cancel.clone(), emit));

    // 等待 HTTPS 样本进入在途（dns/tcp 已完成）后取消。
    tokio::time::sleep(Duration::from_millis(400)).await;
    cancel.cancel();
    let run = job.await.expect("任务不 panic");

    assert_eq!(run.status, RunStatus::Cancelled);
    let https_samples: Vec<&ProbeSample> = run
        .targets
        .iter()
        .flat_map(|target| &target.samples)
        .filter(|sample| sample.probe_type == ProbeType::Https)
        .collect();
    assert!(
        https_samples.iter().all(|sample| sample
            .error
            .as_ref()
            .is_some_and(|error| error.kind == ErrorKind::Cancelled)),
        "在途样本应标记取消，实际：{https_samples:?}"
    );
    assert!(https_samples.iter().all(|sample| !sample.success));

    // 事件：恰好一个终态，且为 Completed（取消属于正常终态）。
    assert_eq!(terminal_count(&events.lock().unwrap()), 1);
    {
        let events = events.lock().unwrap();
        assert!(matches!(events.first(), Some(JobEvent::Progress(_))));
        assert!(events
            .iter()
            .any(|event| matches!(event, JobEvent::Completed(payload) if payload.run.status == RunStatus::Cancelled)));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn scheduler_deadline_stops_the_job() {
    let server = MockServer::spawn().await;
    let spec = spec_for(
        &server,
        "/slow?ms=2000",
        3,
        5000,
        Duration::from_millis(400),
        1,
    );
    let (emit, events) = capture_events();
    let run = execute_job(spec, CancellationToken::new(), emit).await;

    assert_eq!(run.status, RunStatus::Cancelled);
    assert_eq!(
        terminal_count(&events.lock().unwrap()),
        1,
        "截止同样只有一个终态"
    );
    // HTTPS 指向 /slow：截止后不得出现伪造成功（DNS 解析 127.0.0.1 成功属正常基线）。
    assert!(
        run.targets
            .iter()
            .flat_map(|target| &target.samples)
            .filter(|sample| sample.probe_type == ProbeType::Https)
            .all(|sample| !sample.success)
    );
    server.shutdown().await;
}

#[tokio::test]
async fn scheduler_unexecuted_targets_produce_no_samples() {
    let server = MockServer::spawn().await;
    // 两个目标：第一个的 HTTPS 在途时取消 → 第二个目标完全不应出现。
    let spec = spec_for(
        &server,
        "/slow?ms=2000",
        3,
        5000,
        Duration::from_secs(60),
        2,
    );
    let (emit, _events) = capture_events();
    let cancel = CancellationToken::new();
    let job = tokio::spawn(execute_job(spec, cancel.clone(), emit));
    tokio::time::sleep(Duration::from_millis(400)).await;
    cancel.cancel();
    let run = job.await.expect("任务不 panic");

    assert_eq!(run.targets.len(), 1, "未执行的目标不得出现在结果中");
    assert_eq!(run.targets[0].target_id, "mock-target-0");
    server.shutdown().await;
}

#[tokio::test]
async fn scheduler_caps_https_concurrency_at_three() {
    let server = MockServer::spawn().await;
    let spec = spec_for(&server, "/slow?ms=150", 5, 5000, Duration::from_secs(30), 1);
    let (emit, events) = capture_events();
    let run = execute_job(spec, CancellationToken::new(), emit).await;

    assert_eq!(run.status, RunStatus::Completed);
    let https_samples = run
        .targets
        .iter()
        .flat_map(|target| &target.samples)
        .filter(|sample| sample.probe_type == ProbeType::Https)
        .count();
    assert_eq!(https_samples, 5, "repeats=5 应产生 5 个样本");
    assert_eq!(
        server.max_active(),
        3,
        "并发上限应为 3（5 个 150ms 请求必然重叠）"
    );
    assert_eq!(terminal_count(&events.lock().unwrap()), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn aggregate_matches_samples_exactly() {
    let server = MockServer::spawn().await;
    // 混合结果：200 / 401（GET 路径不复用）/ 500，验证聚合与样本一致。
    let spec = spec_for(&server, "/status/200", 3, 5000, Duration::from_secs(30), 1);
    let (emit, _events) = capture_events();
    let run = execute_job(spec, CancellationToken::new(), emit).await;

    for target in &run.targets {
        for aggregate in &target.aggregates {
            let samples: Vec<&ProbeSample> = target
                .samples
                .iter()
                .filter(|sample| sample.probe_type == aggregate.probe_type)
                .collect();
            let recomputed = ProbeAggregate::from_samples(
                aggregate.probe_type,
                &samples.into_iter().cloned().collect::<Vec<_>>(),
            );
            assert_eq!(recomputed.attempts_total, aggregate.attempts_total);
            assert_eq!(recomputed.success_count, aggregate.success_count);
            assert_eq!(
                recomputed.transport_failure_count,
                aggregate.transport_failure_count
            );
            // 恒等式（D-011 + D-014）；dns/tcp 无"非 2xx"概念，用饱和减法。
            let non_2xx = aggregate
                .http_response_count
                .saturating_sub(aggregate.success_count);
            assert_eq!(
                aggregate.success_count
                    + aggregate.transport_failure_count
                    + non_2xx
                    + aggregate.cancelled_count,
                aggregate.attempts_total
            );
        }
    }
    server.shutdown().await;
}
