//! V6-R2（契约 §2 T-1..T-6）：目标标识安全投影的永久回归测试。
//!
//! 约束（契约 §5）：只使用 `tempfile::tempdir()` 创建的隔离目录与隔离 SQLite
//! 数据库；不读写真实 AppData / history.db；不访问网络。
//!
//! 断言对象一律是**导出的输出字节**（JSON 预览 / Markdown 预览 / 真实保存后
//! 读回的文件），不是 DTO 内部字段——DTO 字段本身正确但渲染泄漏同样会被抓出。

use ai_dev_cockpit_lib::commands::report::{ReportFormat, build_report, save_report_atomic};
use ai_dev_cockpit_lib::commands::report_export::{
    RuleId, SafeTargetLabel, project_report, render_json, render_markdown,
};
use ai_dev_cockpit_lib::diagnostic::rules::{Confidence, Diagnosis, Severity};
use ai_dev_cockpit_lib::diagnostic::types::{
    DiagnosticError, DiagnosticRun, ErrorKind, ProbeAggregate, ProbeSample, ProbeType, RouteMode,
    RunStatus, TargetResult,
};
use ai_dev_cockpit_lib::storage::Storage;

// ---------------------------------------------------------------------------
// 金丝雀语料（契约 §2 表格逐条）
// ---------------------------------------------------------------------------

/// 契约表格中的全部金丝雀，逐个作为 `target_id` 注入。
const CANARY_CORPUS: [&str; 9] = [
    "canary-a1.corp.internal",
    "10.13.37.42",
    "192.168.77.7",
    "fd12:3456:789a::42",
    "api.openai.com\r\n# CANARY_CRLF",
    "# CANARY_H1 [x](http://canary.invalid)",
    "sk-CANARY0000000000",
    "Cookie: CANARY=1",
    "C:\\Users\\CANARY_USER\\",
];

/// 输出中不得出现的片段（不区分大小写；`CANARY` 由 `canary` 覆盖）。
const FORBIDDEN_FRAGMENTS: [&str; 6] = [
    "canary",
    "10.13.37",
    "192.168.77",
    "fd12:",
    "corp.internal",
    "canary_user",
];

const CANARY_ERR_SUMMARY: &str = "CANARY_ERR_SUMMARY";
const CANARY_ERR_DETAIL: &str = "CANARY_ERR_DETAIL";

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

/// 构造一次运行：每个 `target_id` 都进入 `TargetResult::target_id`
/// **以及** `ProbeSample::target_id`（两个通道都要覆盖），错误文本也带金丝雀。
fn run_with_target_ids(target_ids: &[&str]) -> DiagnosticRun {
    let targets = target_ids
        .iter()
        .enumerate()
        .map(|(index, target_id)| {
            let sample = ProbeSample {
                target_id: target_id.to_string(),
                probe_type: ProbeType::Https,
                route_mode: RouteMode::NoAppProxy,
                attempt_no: u32::try_from(index).unwrap_or(0) + 1,
                success: false,
                duration_ms: None,
                http_status: None,
                http_outcome: None,
                error: Some(DiagnosticError {
                    kind: ErrorKind::Timeout,
                    summary: CANARY_ERR_SUMMARY.to_string(),
                    detail: Some(CANARY_ERR_DETAIL.to_string()),
                }),
                tested_at: "2026-10-25T00:00:00Z".to_string(),
            };
            let samples = vec![sample];
            let aggregates = vec![ProbeAggregate::from_samples(ProbeType::Https, &samples)];
            TargetResult {
                target_id: target_id.to_string(),
                samples,
                aggregates,
            }
        })
        .collect();

    DiagnosticRun {
        run_id: "0f0e0d0c-0b0a-4009-8007-060504030201".to_string(),
        started_at: "2026-10-25T00:00:00Z".to_string(),
        finished_at: Some("2026-10-25T00:00:05Z".to_string()),
        status: RunStatus::Completed,
        route_mode: RouteMode::NoAppProxy,
        repeats: 1,
        app_version: "0.1.0".to_string(),
        targets,
    }
}

fn canary_run() -> DiagnosticRun {
    run_with_target_ids(&CANARY_CORPUS)
}

fn whitelisted_run() -> DiagnosticRun {
    run_with_target_ids(&["github"])
}

/// 任意规则编号的合成诊断（用于验证引擎文本不被采信、静态文案与引擎一致）。
fn synthetic_diagnosis(rule: RuleId, related_target: &str) -> Diagnosis {
    Diagnosis {
        rule_id: rule.as_str().to_string(),
        title: "CANARY_TITLE_FROM_ENGINE".to_string(),
        severity: Severity::Warning,
        confidence: Confidence::EvidenceSufficient,
        observed_facts: vec!["CANARY_FACT_FROM_ENGINE".to_string()],
        possible_causes: vec!["CANARY_CAUSE_FROM_ENGINE".to_string()],
        verification_steps: vec!["CANARY_STEP_FROM_ENGINE".to_string()],
        limitations: vec!["CANARY_LIMIT_FROM_ENGINE".to_string()],
        related_target_ids: vec![related_target.to_string()],
        related_probe_types: vec![ProbeType::Dns],
    }
}

// ---------------------------------------------------------------------------
// 断言辅助
// ---------------------------------------------------------------------------

fn assert_no_canary(label: &str, output: &str) {
    let lowered = output.to_lowercase();
    for fragment in FORBIDDEN_FRAGMENTS {
        assert!(
            !lowered.contains(fragment),
            "{label} 泄漏金丝雀片段 `{fragment}`"
        );
    }
}

/// 递归遍历 JSON 的全部字符串值：不得含任何控制字符（T-6）。
fn assert_no_control_characters(value: &serde_json::Value, path: &str) {
    match value {
        serde_json::Value::String(text) => assert!(
            !text.chars().any(char::is_control),
            "{path} 含控制字符：{text:?}"
        ),
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                assert_no_control_characters(item, &format!("{path}[{index}]"));
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                assert_no_control_characters(item, &format!("{path}.{key}"));
            }
        }
        _ => {}
    }
}

/// Markdown 不得出现数据来源的标题/链接语法（T-6）。
fn assert_no_markdown_injection(label: &str, markdown: &str) {
    for line in markdown.lines() {
        assert!(
            !line.starts_with("# CANARY"),
            "{label} 出现数据来源的标题行：{line}"
        );
    }
    assert!(
        !markdown.contains("](http"),
        "{label} 出现数据来源的链接语法"
    );
    assert!(
        !markdown.contains("canary.invalid"),
        "{label} 出现金丝雀链接"
    );
}

/// 同一目标必须出现 `occurrences` 次，且 JSON 与 Markdown 使用**同一个**名称。
fn assert_target_name_consistent(json: &str, markdown: &str, label: &str, occurrences: usize) {
    let json_needle = format!("\"target\": \"{label}\"");
    assert_eq!(
        json.matches(&json_needle).count(),
        occurrences,
        "JSON 中目标名 `{label}` 应出现 {occurrences} 次"
    );
    let markdown_needle = format!("### {label}\n");
    assert_eq!(
        markdown.matches(&markdown_needle).count(),
        occurrences,
        "Markdown 中目标名 `{label}` 应出现 {occurrences} 次"
    );
}

// ---------------------------------------------------------------------------
// T-1..T-6：金丝雀语料 → JSON 预览 / Markdown 预览 / 两个真实文件
// ---------------------------------------------------------------------------

#[test]
fn export_canary_corpus_json_md_preview_and_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let run = canary_run();

    let (json_preview, json_name) = build_report(&run, ReportFormat::Json);
    let (md_preview, md_name) = build_report(&run, ReportFormat::Markdown);

    let json_path = dir.path().join(&json_name);
    let md_path = dir.path().join(&md_name);
    save_report_atomic(&json_path, &json_preview).expect("保存 JSON 报告");
    save_report_atomic(&md_path, &md_preview).expect("保存 Markdown 报告");
    let json_file = std::fs::read_to_string(&json_path).expect("读回 JSON 报告");
    let md_file = std::fs::read_to_string(&md_path).expect("读回 Markdown 报告");

    // 预览与文件必须是同一份字节（F-8 的投影侧对应项）。
    assert_eq!(json_preview, json_file, "JSON 预览与保存文件必须一致");
    assert_eq!(md_preview, md_file, "Markdown 预览与保存文件必须一致");

    for (label, output) in [
        ("JSON 预览", json_preview.as_str()),
        ("Markdown 预览", md_preview.as_str()),
        ("JSON 文件", json_file.as_str()),
        ("Markdown 文件", md_file.as_str()),
    ] {
        assert_no_canary(label, output);
        assert!(
            output.contains("unknown-target"),
            "{label} 应把未知目标显示为 unknown-target"
        );
    }
    // JSON 不得再出现原始 target_id 字段名（Markdown 本就没有字段名）。
    assert!(
        !json_preview.contains("\"target_id\""),
        "不得导出 target_id 字段"
    );
    assert!(
        !json_file.contains("\"target_id\""),
        "不得导出 target_id 字段"
    );

    // 反证（防止用例空转）：未经投影的引擎标题**确实**携带金丝雀，
    // 也就是说上面"输出里没有金丝雀"是投影生效的结果，而不是夹具没喂进去。
    let raw_diagnoses = ai_dev_cockpit_lib::diagnostic::rules::diagnose(std::slice::from_ref(&run));
    let raw_titles = raw_diagnoses
        .iter()
        .map(|diagnosis| diagnosis.title.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        raw_titles.to_lowercase().contains("canary")
            && raw_titles.to_lowercase().contains("corp.internal"),
        "引擎标题应携带原始 target_id，否则本用例没有覆盖投影：{raw_titles}"
    );

    // 同一目标在 JSON 与 Markdown 中的名称完全一致（T-4）。
    assert_target_name_consistent(
        &json_preview,
        &md_preview,
        "unknown-target",
        CANARY_CORPUS.len(),
    );

    // JSON：所有字符串值不含控制字符（T-6）。
    let parsed: serde_json::Value = serde_json::from_str(&json_preview).expect("JSON 可解析");
    assert_no_control_characters(&parsed, "$");

    // Markdown：无数据来源的标题/链接/HTML/代码块语法（T-6）。
    assert_no_markdown_injection("Markdown 预览", &md_preview);
    assert_no_markdown_injection("Markdown 文件", &md_file);
}

// ---------------------------------------------------------------------------
// T-1..T-6：经隔离 SQLite 历史库的同一语料
// ---------------------------------------------------------------------------

#[test]
fn export_canary_from_history_db() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("history.db");
    let storage = Storage::open(&db_path).expect("打开隔离数据库");
    let run = canary_run();
    storage.save_run(&run).expect("写入隔离数据库");

    let loaded = storage.get_run(&run.run_id).expect("从数据库读回运行");
    let (json_preview, json_name) = build_report(&loaded, ReportFormat::Json);
    let (md_preview, md_name) = build_report(&loaded, ReportFormat::Markdown);

    let json_path = dir.path().join(&json_name);
    let md_path = dir.path().join(&md_name);
    save_report_atomic(&json_path, &json_preview).expect("保存 JSON 报告");
    save_report_atomic(&md_path, &md_preview).expect("保存 Markdown 报告");
    let json_file = std::fs::read_to_string(&json_path).expect("读回 JSON 报告");
    let md_file = std::fs::read_to_string(&md_path).expect("读回 Markdown 报告");

    for (label, output) in [
        ("数据库→JSON 预览", json_preview.as_str()),
        ("数据库→Markdown 预览", md_preview.as_str()),
        ("数据库→JSON 文件", json_file.as_str()),
        ("数据库→Markdown 文件", md_file.as_str()),
    ] {
        assert_no_canary(label, output);
        assert!(
            output.contains("unknown-target"),
            "{label} 应显示 unknown-target"
        );
    }
    assert_target_name_consistent(
        &json_preview,
        &md_preview,
        "unknown-target",
        CANARY_CORPUS.len(),
    );

    let parsed: serde_json::Value = serde_json::from_str(&json_preview).expect("JSON 可解析");
    assert_no_control_characters(&parsed, "$");
    assert_no_markdown_injection("数据库→Markdown 预览", &md_preview);
}

// ---------------------------------------------------------------------------
// T-1：白名单目标必须显示其标签（防止"全部变成 unknown-target"的假通过）
// ---------------------------------------------------------------------------

#[test]
fn whitelisted_target_keeps_label() {
    let run = whitelisted_run();
    let (json, _) = build_report(&run, ReportFormat::Json);
    let (markdown, _) = build_report(&run, ReportFormat::Markdown);

    assert!(json.contains("\"target\": \"GitHub\""), "{json}");
    assert!(markdown.contains("### GitHub\n"), "{markdown}");
    assert!(!json.contains("unknown-target"), "白名单目标不应被遮蔽");
    assert!(
        !json.contains("\"target_id\""),
        "JSON 不得再出现原始 target_id 字段名"
    );
    assert!(
        !json.contains("github"),
        "原始 target id 不应出现在导出内容中"
    );
    // 诊断标题同样使用标签，而不是引擎标题里的原始 id。
    assert!(json.contains("GitHub：HTTPS 请求超时"), "{json}");
}

#[test]
fn target_label_mapping_is_exact_match_only() {
    assert_eq!(SafeTargetLabel::from_target_id("github").as_str(), "GitHub");
    assert_eq!(
        SafeTargetLabel::from_target_id("GitHub").as_str(),
        "unknown-target",
        "大小写不同不得命中白名单"
    );
    assert_eq!(
        SafeTargetLabel::from_target_id(" github").as_str(),
        "unknown-target"
    );
    assert_eq!(
        SafeTargetLabel::from_target_id("git").as_str(),
        "unknown-target",
        "前缀不得命中白名单"
    );
    assert_eq!(
        SafeTargetLabel::from_target_id("").as_str(),
        "unknown-target"
    );
}

#[test]
fn rule_id_mapping_is_exact_match_only() {
    assert_eq!(RuleId::from_raw("R01"), RuleId::R01);
    assert_eq!(RuleId::from_raw("R12"), RuleId::R12);
    for raw in ["r01", " R01", "R01 ", "R010", "R13", ""] {
        assert_eq!(
            RuleId::from_raw(raw),
            RuleId::Unknown,
            "`{raw}` 不得被识别为已知规则"
        );
    }
    assert_eq!(RuleId::Unknown.as_str(), "unknown-rule");
}

// ---------------------------------------------------------------------------
// T-2 / T-3：引擎的自由文本一律不被采信
// ---------------------------------------------------------------------------

#[test]
fn engine_free_text_is_not_projected() {
    let run = whitelisted_run();
    let forged = synthetic_diagnosis(RuleId::R01, "github");
    let report = project_report(&run, &[forged]);

    let json = render_json(&report);
    let markdown = render_markdown(&report);

    // 引擎的 title / observed_facts / causes / steps / limitations 全部被丢弃，
    // 报告文案来自静态模板。
    for output in [&json, &markdown] {
        assert!(!output.contains("CANARY"), "引擎自由文本不得进入报告");
    }
    assert!(json.contains("\"rule_id\": \"R01\""));
    assert!(json.contains("GitHub：域名解析失败"), "{json}");
    // 标题由 rule_id + 安全目标名重新构造：结果与 diagnosis_title 一致。
    assert!(json.contains(&format!(
        "\"title\": \"{}\"",
        ai_dev_cockpit_lib::commands::report_export::diagnosis_title(
            RuleId::R01,
            SafeTargetLabel::from_target_id("github")
        )
    )));
}

// ---------------------------------------------------------------------------
// 报告静态文案必须与规则引擎文案逐字一致（防止报告与引擎悄悄漂移）
// ---------------------------------------------------------------------------

#[test]
fn export_static_copy_mirrors_engine_copy() {
    const RULES_SOURCE: &str = include_str!("../src/diagnostic/rules.rs");
    let run = whitelisted_run();

    for rule in RuleId::ALL {
        let report = project_report(&run, &[synthetic_diagnosis(rule, "github")]);
        let exported = &report.diagnoses[0];
        assert_eq!(exported.rule_id, rule);
        assert!(
            !exported.possible_causes.is_empty(),
            "{rule:?} 缺少可能原因文案"
        );
        assert!(
            !exported.verification_steps.is_empty(),
            "{rule:?} 缺少建议步骤文案"
        );
        assert!(!exported.limitations.is_empty(), "{rule:?} 缺少局限文案");

        for text in exported
            .possible_causes
            .iter()
            .chain(exported.verification_steps)
            .chain(exported.limitations)
        {
            assert!(
                RULES_SOURCE.contains(text),
                "{rule:?} 的报告文案与 rules.rs 不一致：{text}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T-5：错误自由文本从未被导出（无字段承载）
// ---------------------------------------------------------------------------

#[test]
fn error_free_text_never_reaches_export() {
    let run = canary_run();
    let json = build_report(&run, ReportFormat::Json).0;
    let markdown = build_report(&run, ReportFormat::Markdown).0;

    for output in [&json, &markdown] {
        assert!(
            !output.contains(CANARY_ERR_SUMMARY),
            "error.summary 不得导出"
        );
        assert!(!output.contains(CANARY_ERR_DETAIL), "error.detail 不得导出");
        assert!(!output.contains("summary"), "不得出现 summary 字段");
        assert!(!output.contains("detail"), "不得出现 detail 字段");
    }
    // 结构化错误类别仍然导出（可用性不为安全让路）。
    assert!(json.contains("\"error_kind\": \"timeout\""), "{json}");
}
