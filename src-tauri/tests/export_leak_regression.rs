//! F-02 回归：导出脱敏（假凭证探针，覆盖验收报告确认的四类泄漏场景）。
//!
//! 探针数据全部为人工编造的假凭证，不接触任何真实 API Key。
//! 场景映射（RELEASE_ACCEPTANCE_REPORT §8）：
//! - Cookie 头泄漏（Json）
//! - X-API-Key / Api-Key 头泄漏（Json）
//! - 私有 IP 泄漏（Json）
//! - 敏感主机名泄漏（Json）
//!
//! 附加：多次出现、大小写变化、JSON 转义、诊断结论字段、Markdown 同策略、
//! 预览与导出一致性、无死循环/异常耗时。

use ai_dev_cockpit_lib::commands::report::{REPORT_NOTE, ReportFormat, build_report};
use ai_dev_cockpit_lib::diagnostic::types::{
    DiagnosticError, DiagnosticRun, ErrorKind, ProbeAggregate, ProbeSample, ProbeType, RouteMode,
    RunStatus, TargetResult,
};
use ai_dev_cockpit_lib::security::redaction::sanitize_text;
use std::time::Instant;

/// 假凭证探针（全部人工编造，非真实凭证）。
const FAKE_COOKIE: &str = "Cookie: sessionid=FAKESESSION1234567890; token=FAKETOKEN987654321";
const FAKE_API_KEY_HEADER: &str = "X-API-Key: sk-FAKEKEY0123456789abcdef";
const FAKE_API_KEY_HEADER2: &str = "Api-Key: FAKEAPIKEYAAAAAAAABBBB";
const FAKE_PRIVATE_IP: &str = "内网网关 192.168.1.1 与 10.0.0.5 及回环 127.0.0.1";
const FAKE_INTERNAL_HOST: &str = "db.internal.corp.example 拒绝了连接";

fn contaminated_run() -> DiagnosticRun {
    let sample = |attempt: u32, summary: &str, detail: Option<&str>| ProbeSample {
        target_id: "github".to_string(),
        probe_type: ProbeType::Https,
        route_mode: RouteMode::NoAppProxy,
        attempt_no: attempt,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: Some(DiagnosticError {
            kind: ErrorKind::Unknown,
            summary: summary.to_string(),
            detail: detail.map(str::to_string),
        }),
        tested_at: "2026-10-25T00:00:00Z".to_string(),
    };
    let samples = vec![
        sample(1, FAKE_COOKIE, None),
        sample(2, FAKE_API_KEY_HEADER, Some(FAKE_API_KEY_HEADER2)),
        sample(3, FAKE_PRIVATE_IP, None),
        sample(4, FAKE_INTERNAL_HOST, None),
    ];
    let aggregates = vec![ProbeAggregate::from_samples(ProbeType::Https, &samples)];
    DiagnosticRun {
        run_id: "run-f02".to_string(),
        started_at: "2026-10-25T00:00:00Z".to_string(),
        finished_at: Some("2026-10-25T00:00:05Z".to_string()),
        status: RunStatus::Completed,
        route_mode: RouteMode::NoAppProxy,
        repeats: 4,
        app_version: "0.1.0".to_string(),
        targets: vec![TargetResult {
            target_id: "github".to_string(),
            samples,
            aggregates,
        }],
    }
}

/// 四类泄漏场景：JSON 与 Markdown 均不得出现任何假凭证明文。
#[test]
fn regression_f02_no_leak_in_json_and_markdown() {
    let run = contaminated_run();
    let (json, _) = build_report(&run, ReportFormat::Json);
    let (markdown, _) = build_report(&run, ReportFormat::Markdown);

    let secrets = [
        "FAKESESSION1234567890",
        "FAKETOKEN987654321",
        "sk-FAKEKEY0123456789abcdef",
        "FAKEAPIKEYAAAAAAAABBBB",
        "192.168.1.1",
        "10.0.0.5",
        "db.internal.corp.example",
    ];
    for secret in secrets {
        assert!(!json.contains(secret), "JSON 泄漏 {secret}");
        assert!(!markdown.contains(secret), "Markdown 泄漏 {secret}");
    }
    // 头名保留但值遮蔽。
    assert!(json.contains("[已隐藏]"), "敏感头值应被遮蔽");
    // 私有 IP / 内部主机遮蔽标记存在。
    assert!(json.contains("[内网地址]"));
    assert!(json.contains("[内部主机]"));
}

/// 大小写变化与多次出现。
#[test]
fn regression_f02_case_variants_and_multiple_occurrences() {
    let text = "Cookie: a=1\nCOOKIE: b=2\ncOoKiE: c=3";
    let masked = sanitize_text(text);
    assert_eq!(masked.matches("a=1").count(), 0);
    assert_eq!(masked.matches("b=2").count(), 0);
    assert_eq!(masked.matches("c=3").count(), 0);
    assert_eq!(masked.matches("[已隐藏]").count(), 3);

    let ips = "路径 10.1.2.3 然后 10.1.2.3 再 172.20.1.1";
    let masked = sanitize_text(ips);
    assert_eq!(masked.matches("10.1.2.3").count(), 0);
    assert_eq!(masked.matches("172.20.1.1").count(), 0);
}

/// JSON 转义形态（错误文本嵌在 JSON 字符串值内）。
#[test]
fn regression_f02_json_escaped_payload() {
    let run = contaminated_run();
    let (json, _) = build_report(&run, ReportFormat::Json);
    // 反查：假凭证的任何子串（含 JSON 转义后）不得出现。
    assert!(!json.contains("FAKESESSION"));
    assert!(!json.contains("sk-FAKEKEY"));
}

/// 诊断结论字段不重新泄漏（诊断文案当前为固定模板，但规则引擎未来可能
/// 携带样本摘要——安全化必须覆盖该通道）。
#[test]
fn regression_f02_diagnoses_fields_sanitized() {
    let mut run = contaminated_run();
    // 构造会触发 R04（超时）且摘要含假凭证的样本。
    run.targets[0].samples.push(ProbeSample {
        target_id: "github".to_string(),
        probe_type: ProbeType::Https,
        route_mode: RouteMode::NoAppProxy,
        attempt_no: 5,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: Some(DiagnosticError {
            kind: ErrorKind::Timeout,
            summary: format!("超时；重试头 {FAKE_API_KEY_HEADER}"),
            detail: None,
        }),
        tested_at: "2026-10-25T00:00:00Z".to_string(),
    });
    let (json, _) = build_report(&run, ReportFormat::Json);
    let (markdown2, _) = build_report(&run, ReportFormat::Markdown);
    for output in [&json, &markdown2] {
        assert!(!output.contains("sk-FAKEKEY"), "诊断字段泄漏");
    }
}

/// 公网 IP 与诊断白名单域名保留（不误伤合法内容）。
#[test]
fn regression_f02_keeps_public_ips_and_whitelist_domains() {
    let text = "连接 8.8.8.8 与 github.com/api.openai.com 正常";
    let masked = sanitize_text(text);
    assert!(masked.contains("8.8.8.8"), "公网 IP 不应误伤：{masked}");
    assert!(masked.contains("github.com"));
    assert!(masked.contains("api.openai.com"));
    // 版本号等数字不受 IP 规则影响。
    assert_eq!(
        sanitize_text("版本 1.2.3.4 build 0.1.0"),
        "版本 1.2.3.4 build 0.1.0"
    );
}

/// 无死循环 / 异常耗时。
#[test]
fn regression_f02_no_hang_on_adversarial_input() {
    let started = Instant::now();
    let adversarial = [
        String::new(),
        "?token=".repeat(200),
        "?token=[已隐藏]&token=".repeat(100),
        "Cookie:".repeat(500),
        "192.168.".repeat(500),
        "@".repeat(1000),
        "://@://@".repeat(200),
        "\u{0}\u{1}\u{2}控制字符".to_string(),
    ];
    for payload in &adversarial {
        let _ = sanitize_text(payload);
    }
    assert!(
        started.elapsed().as_millis() < 2000,
        "脱敏异常耗时：{:?}",
        started.elapsed()
    );
}

/// 预览与导出使用同一 build_report → 内容一致（预览即所见即所得）。
#[test]
fn regression_f02_preview_matches_export_bytes() {
    let run = contaminated_run();
    let (json_content, json_name) = build_report(&run, ReportFormat::Json);
    let (md_content, md_name) = build_report(&run, ReportFormat::Markdown);
    assert!(json_name.ends_with(".json") && md_name.ends_with(".md"));
    assert!(json_content.contains(REPORT_NOTE));
    assert!(md_content.contains(REPORT_NOTE));
    // 两次调用确定性一致（除时间戳字段外的关键结构稳定）。
    let (json2, _) = build_report(&run, ReportFormat::Json);
    assert_eq!(json_content.len(), json2.len(), "报告生成应确定性");
}
