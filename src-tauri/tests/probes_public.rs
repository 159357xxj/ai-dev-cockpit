//! 公网预设目标人工验证（默认 `#[ignore]`，绝不随 `cargo test` 自动运行）。
//!
//! 运行方式（仅在用户明确授权联网后执行）：
//!   cargo test --test probes_public -- --ignored --nocapture
//!
//! 验证目标：≥2 个白名单公网目标的真实 DNS/TCP/HTTPS 采样各一次，
//! 打印每次采样的真实结果（含状态码与耗时），供人工留档。
//! 注意：HTTP 200/3xx 只说明"收到响应"，不代表业务可登录可用（D-007）。

use std::time::Duration;

use ai_dev_cockpit_lib::diagnostic::dns::{probe_dns, system_resolver};
use ai_dev_cockpit_lib::diagnostic::https::{HttpsProbeParams, RedirectRules, probe_https};
use ai_dev_cockpit_lib::diagnostic::targets::{TARGETS, find_target};
use ai_dev_cockpit_lib::diagnostic::tcp::probe_tcp;
use ai_dev_cockpit_lib::diagnostic::types::RouteMode;
use tokio_util::sync::CancellationToken;

fn print_sample(label: &str, sample: &ai_dev_cockpit_lib::diagnostic::types::ProbeSample) {
    println!(
        "  [{label}] success={} duration={:?} http_status={:?} outcome={:?} error={:?}",
        sample.success,
        sample.duration_ms,
        sample.http_status,
        sample.http_outcome,
        sample
            .error
            .as_ref()
            .map(|error| (error.kind, error.summary.clone())),
    );
}

fn production_rules() -> RedirectRules {
    let hosts: std::collections::HashSet<String> = TARGETS
        .iter()
        .map(|target| target.host.to_string())
        .collect();
    RedirectRules::production(hosts)
}

#[tokio::test]
#[ignore = "需要用户明确授权联网；cargo test 默认跳过"]
async fn public_target_github() {
    let target = find_target("github").expect("白名单含 github");
    let cancel = CancellationToken::new();
    println!("== github.com（https://github.com/）==");
    let dns = probe_dns(
        "github",
        target.host,
        RouteMode::NoAppProxy,
        Duration::from_secs(10),
        cancel.clone(),
        &system_resolver(),
    )
    .await;
    print_sample("DNS", &dns);
    let tcp = probe_tcp(
        "github",
        target.host,
        443,
        RouteMode::NoAppProxy,
        Duration::from_secs(10),
        cancel.clone(),
        &system_resolver(),
    )
    .await;
    print_sample("TCP", &tcp);
    let https = probe_https(
        &HttpsProbeParams {
            target_id: "github".to_string(),
            url: target.url.to_string(),
            route_mode: RouteMode::NoAppProxy,
            manual_proxy_url: None,
            timeout: Duration::from_secs(10),
            rules: production_rules(),
        },
        cancel,
    )
    .await;
    print_sample("HTTPS", &https);
}

#[tokio::test]
#[ignore = "需要用户明确授权联网；cargo test 默认跳过"]
async fn public_target_zcode() {
    let target = find_target("zcode").expect("白名单含 zcode");
    let cancel = CancellationToken::new();
    println!("== zcode.z.ai（https://zcode.z.ai/）==");
    let dns = probe_dns(
        "zcode",
        target.host,
        RouteMode::NoAppProxy,
        Duration::from_secs(10),
        cancel.clone(),
        &system_resolver(),
    )
    .await;
    print_sample("DNS", &dns);
    let tcp = probe_tcp(
        "zcode",
        target.host,
        443,
        RouteMode::NoAppProxy,
        Duration::from_secs(10),
        cancel.clone(),
        &system_resolver(),
    )
    .await;
    print_sample("TCP", &tcp);
    let https = probe_https(
        &HttpsProbeParams {
            target_id: "zcode".to_string(),
            url: target.url.to_string(),
            route_mode: RouteMode::NoAppProxy,
            manual_proxy_url: None,
            timeout: Duration::from_secs(10),
            rules: production_rules(),
        },
        cancel,
    )
    .await;
    print_sample("HTTPS", &https);
}

/// Phase 3 真实环境验证（用户已授权，2026-10-25）：
/// github.com × 三种路由模式 × 各 3 次采样。manual 模式使用本机系统代理端口
/// （注册表 ProxyServer=127.0.0.1:7890，选项文本中明示；不修改任何代理客户端配置）。
/// 注意：HTTP 200 只代表"收到响应"，不代表登录/业务可用（D-007）。
#[tokio::test]
#[ignore = "需要用户明确授权联网；cargo test 默认跳过"]
async fn public_compare_three_modes_on_github() {
    use std::time::Instant;

    use ai_dev_cockpit_lib::diagnostic::types::{ProbeAggregate, RouteMode};
    use ai_dev_cockpit_lib::security::validation::validate_manual_proxy_url;

    let target = find_target("github").expect("白名单含 github");
    let rules = production_rules();
    let manual_proxy = "http://127.0.0.1:7890".to_string();
    validate_manual_proxy_url(&manual_proxy).expect("本机代理地址合法");

    for (mode_name, mode, manual_url) in [
        ("no_app_proxy", RouteMode::NoAppProxy, None),
        ("library_default", RouteMode::LibraryDefault, None),
        (
            "manual_local_proxy(http://127.0.0.1:7890)",
            RouteMode::ManualLocalProxy,
            Some(manual_proxy),
        ),
    ] {
        println!(
            "
== {mode_name} → https://github.com/（3 次采样）=="
        );
        let started = Instant::now();
        let mut samples = Vec::new();
        for attempt in 1..=3u32 {
            let sample = probe_https(
                &HttpsProbeParams {
                    target_id: "github".to_string(),
                    url: target.url.to_string(),
                    route_mode: mode,
                    manual_proxy_url: manual_url.clone(),
                    timeout: Duration::from_secs(10),
                    rules: rules.clone(),
                },
                CancellationToken::new(),
            )
            .await;
            print_sample(&format!("  采样{attempt}"), &sample);
            samples.push(sample);
        }
        let aggregate = ProbeAggregate::from_samples(
            ai_dev_cockpit_lib::diagnostic::types::ProbeType::Https,
            &samples,
        );
        println!(
            "  [汇总] 样本={} 成功={} 传输失败={} 收到响应={} 非2xx比例={:?} 中位={:?} 耗时={:?}",
            aggregate.attempts_total,
            aggregate.success_count,
            aggregate.transport_failure_count,
            aggregate.http_response_count,
            aggregate.non_2xx_response_ratio(),
            aggregate.duration_median_ms,
            started.elapsed()
        );
    }
}
