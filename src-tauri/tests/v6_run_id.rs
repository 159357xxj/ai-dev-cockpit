//! V6-R3（契约 §3 U-1、U-2）：run_id 必须是 **UUID v4** 的规范 36 字符形式。
//!
//! 合法输入统一输出小写带连字符形式；其余任何输入（v1/v7、错误 variant、
//! 花括号、`urn:` 前缀、长度错误、非十六进制字符）统一输出固定占位符
//! `invalid-run-id`，不使导出失败，也不回显原值。

use ai_dev_cockpit_lib::commands::report::{ReportFormat, build_report};
use ai_dev_cockpit_lib::commands::report_export::SafeRunId;
use ai_dev_cockpit_lib::diagnostic::types::{
    DiagnosticError, DiagnosticRun, ProbeAggregate, ProbeSample, ProbeType, RouteMode, RunStatus,
    TargetResult,
};

const INVALID: &str = "invalid-run-id";

/// 合法 UUID v4：第 15 位 `4`（version），第 20 位 `8`（RFC4122 variant）。
const V4_LOWER: &str = "0f0e0d0c-0b0a-4009-8007-060504030201";
const V4_UPPER: &str = "0F0E0D0C-0B0A-4009-8007-060504030201";

fn sample_run() -> DiagnosticRun {
    let sample = ProbeSample {
        target_id: "github".to_string(),
        probe_type: ProbeType::Https,
        route_mode: RouteMode::NoAppProxy,
        attempt_no: 1,
        success: true,
        duration_ms: Some(120),
        http_status: Some(200),
        http_outcome: None,
        error: None::<DiagnosticError>,
        tested_at: "2026-10-25T00:00:00Z".to_string(),
    };
    let samples = vec![sample];
    let aggregates = vec![ProbeAggregate::from_samples(ProbeType::Https, &samples)];
    DiagnosticRun {
        run_id: V4_LOWER.to_string(),
        started_at: "2026-10-25T00:00:00Z".to_string(),
        finished_at: Some("2026-10-25T00:00:05Z".to_string()),
        status: RunStatus::Completed,
        route_mode: RouteMode::NoAppProxy,
        repeats: 1,
        app_version: "0.1.0".to_string(),
        targets: vec![TargetResult {
            target_id: "github".to_string(),
            samples,
            aggregates,
        }],
    }
}

/// U-1 / U-2：严格 UUID v4 校验与固定占位符。
#[test]
fn run_id_uuid_v4_strict() {
    // 合法 v4：小写原样保留，大写归一为小写（U-1）。
    assert_eq!(
        SafeRunId::from_raw(V4_LOWER).as_str(),
        V4_LOWER,
        "合法小写 v4 必须原样输出"
    );
    assert_eq!(
        SafeRunId::from_raw(V4_UPPER).as_str(),
        V4_LOWER,
        "合法大写 v4 必须归一为小写"
    );

    let invalid_inputs = [
        // v1（时间戳版本）：variant 合法，仅版本不符。
        "f81d4fae-7dec-11d0-a765-00a0c91e6bf6",
        // v7（sort-random）：variant 合法，仅版本不符。
        "018f3b3c-1234-7abc-8def-0123456789ab",
        // 错误 variant：第 20 个字符（0-based 19）为 'c'（0b110 = Microsoft）。
        "0f0e0d0c-0b0a-4009-c007-060504030201",
        // 花括号形式。
        "{0f0e0d0c-0b0a-4009-8007-060504030201}",
        // urn 前缀。
        "urn:uuid:0f0e0d0c-0b0a-4009-8007-060504030201",
        // 35 位。
        "0f0e0d0c-0b0a-4009-8007-06050403020",
        // 37 位。
        "0f0e0d0c-0b0a-4009-8007-0605040302011",
        // 含非十六进制字符。
        "0f0e0d0c-0b0a-4009-8007-06050403020g",
        // 无连字符（32 位）。
        "0f0e0d0c0b0a40098007060504030201",
        // 空串。
        "",
    ];

    for raw in invalid_inputs {
        let normalized = SafeRunId::from_raw(raw);
        assert_eq!(
            normalized.as_str(),
            INVALID,
            "`{raw}` 必须输出固定占位符 invalid-run-id"
        );
        if !raw.is_empty() {
            assert!(
                !normalized.as_str().contains(raw),
                "输出不得回显原始输入：`{raw}`"
            );
        }
    }
}

/// U-1 / U-2 在导出链路上的体现：JSON 的 `run_id` 与文件名都只使用规范化结果。
#[test]
fn exported_report_uses_normalized_run_id() {
    // 非法：urn 前缀形式 → 占位符，且不使导出失败、不回显原值。
    let mut run = sample_run();
    run.run_id = "urn:uuid:0f0e0d0c-0b0a-4009-8007-060504030201".to_string();
    let (json, filename) = build_report(&run, ReportFormat::Json);
    assert!(
        json.contains("\"run_id\": \"invalid-run-id\""),
        "非法 run_id 应输出占位符：{json}"
    );
    assert!(!json.contains("urn:uuid:"), "不得回显原始 run_id");
    assert_eq!(filename, "adc-report-invalid-run-id.json");

    // 合法大写 v4 → 小写，文件名同样使用规范化结果。
    let mut run = sample_run();
    run.run_id = V4_UPPER.to_string();
    let (json, filename) = build_report(&run, ReportFormat::Json);
    assert!(
        json.contains(&format!("\"run_id\": \"{V4_LOWER}\"")),
        "大写 v4 必须归一为小写：{json}"
    );
    assert!(!json.contains(V4_UPPER), "输出不得保留大写形式");
    assert_eq!(filename, format!("adc-report-{V4_LOWER}.json"));
}
