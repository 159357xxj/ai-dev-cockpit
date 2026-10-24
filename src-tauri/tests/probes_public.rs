//! 公网预设目标人工验证（默认 `#[ignore]`，绝不随 `cargo test` 自动运行）。
//!
//! 运行方式（仅在用户明确授权联网后执行）：
//!   cargo test --test probes_public -- --ignored --nocapture
//!
//! 验证目标：≥2 个白名单公网目标的真实 DNS/TCP/HTTPS 采样各一次，
//! 打印每次采样的真实结果（含状态码与耗时），供人工留档。
//! 注意：HTTP 200/3xx 只说明"收到响应"，不代表业务可登录可用（D-007）。

use std::time::Duration;

use ai_dev_cockpit_lib::diagnostic::dns::probe_dns;
use ai_dev_cockpit_lib::diagnostic::https::{probe_https, HttpsProbeParams, RedirectRules};
use ai_dev_cockpit_lib::diagnostic::tcp::probe_tcp;
use ai_dev_cockpit_lib::diagnostic::targets::{find_target, TARGETS};
use ai_dev_cockpit_lib::diagnostic::types::RouteMode;
use tokio_util::sync::CancellationToken;

fn print_sample(label: &str, sample: &ai_dev_cockpit_lib::diagnostic::types::ProbeSample) {
    println!(
        "  [{label}] success={} duration={:?} http_status={:?} outcome={:?} error={:?}",
        sample.success,
        sample.duration_ms,
        sample.http_status,
        sample.http_outcome,
        sample.error.as_ref().map(|error| (error.kind, error.summary.clone())),
    );
}

fn production_rules() -> RedirectRules {
    let hosts: std::collections::HashSet<String> =
        TARGETS.iter().map(|target| target.host.to_string()).collect();
    RedirectRules::production(hosts)
}

#[tokio::test]
#[ignore = "需要用户明确授权联网；cargo test 默认跳过"]
async fn public_target_github() {
    let target = find_target("github").expect("白名单含 github");
    let cancel = CancellationToken::new();
    println!("== github.com（https://github.com/）==");
    let dns = probe_dns("github", target.host, RouteMode::NoAppProxy, Duration::from_secs(10), cancel.clone()).await;
    print_sample("DNS", &dns);
    let tcp = probe_tcp("github", target.host, 443, RouteMode::NoAppProxy, Duration::from_secs(10), cancel.clone()).await;
    print_sample("TCP", &tcp);
    let https = probe_https(
        &HttpsProbeParams {
            target_id: "github".to_string(),
            url: target.url.to_string(),
            route_mode: RouteMode::NoAppProxy,
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
    let dns = probe_dns("zcode", target.host, RouteMode::NoAppProxy, Duration::from_secs(10), cancel.clone()).await;
    print_sample("DNS", &dns);
    let tcp = probe_tcp("zcode", target.host, 443, RouteMode::NoAppProxy, Duration::from_secs(10), cancel.clone()).await;
    print_sample("TCP", &tcp);
    let https = probe_https(
        &HttpsProbeParams {
            target_id: "zcode".to_string(),
            url: target.url.to_string(),
            route_mode: RouteMode::NoAppProxy,
            timeout: Duration::from_secs(10),
            rules: production_rules(),
        },
        cancel,
    )
    .await;
    print_sample("HTTPS", &https);
}
