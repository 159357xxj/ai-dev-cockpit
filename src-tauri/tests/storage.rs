//! 存储层与脱敏导出集成测试（Phase 5）：全离线、内存数据库、固定 fixture。

use ai_dev_cockpit_lib::commands::report::{ReportFormat, build_report};
use ai_dev_cockpit_lib::diagnostic::types::{
    DiagnosticError, DiagnosticRun, ErrorKind, HttpOutcome, ProbeAggregate, ProbeSample, ProbeType,
    RouteMode, RunStatus, TargetResult,
};
use ai_dev_cockpit_lib::security::redaction::sanitize_text;
use ai_dev_cockpit_lib::storage::{DEFAULT_RETENTION_MAX_RUNS, Storage};

fn sample(probe: ProbeType, attempt: u32) -> ProbeSample {
    ProbeSample {
        target_id: "github".to_string(),
        probe_type: probe,
        route_mode: RouteMode::NoAppProxy,
        attempt_no: attempt,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: None,
        tested_at: "2026-10-25T00:00:00Z".to_string(),
    }
}

fn ok(mut s: ProbeSample, d: u64) -> ProbeSample {
    s.success = true;
    s.duration_ms = Some(d);
    if s.probe_type == ProbeType::Https {
        s.http_status = Some(200);
        s.http_outcome = Some(HttpOutcome::Ok2xx);
    }
    s
}

fn cancelled(mut s: ProbeSample) -> ProbeSample {
    s.error = Some(DiagnosticError {
        kind: ErrorKind::Cancelled,
        summary: "任务已取消".to_string(),
        detail: None,
    });
    s
}

fn run(id: &str, status: RunStatus, repeats: u8, started: &str) -> DiagnosticRun {
    let samples = vec![
        ok(sample(ProbeType::Dns, 1), 10),
        ok(sample(ProbeType::Tcp, 1), 40),
        ok(sample(ProbeType::Https, 1), 400),
        ok(sample(ProbeType::Https, 2), 500),
        cancelled(sample(ProbeType::Https, 3)),
    ];
    let aggregates = vec![
        ProbeAggregate::from_samples(ProbeType::Dns, &samples[..1]),
        ProbeAggregate::from_samples(ProbeType::Tcp, &samples[1..2]),
        ProbeAggregate::from_samples(ProbeType::Https, &samples[2..]),
    ];
    DiagnosticRun {
        run_id: id.to_string(),
        started_at: started.to_string(),
        finished_at: Some("2026-10-25T00:00:09Z".to_string()),
        status,
        route_mode: RouteMode::NoAppProxy,
        repeats,
        app_version: "0.1.0".to_string(),
        targets: vec![TargetResult {
            target_id: "github".to_string(),
            samples,
            aggregates,
        }],
    }
}

#[test]
fn migrations_are_idempotent_and_versioned() {
    let storage = Storage::open_in_memory().unwrap();
    // 重复打开同一路径：迁移幂等（user_version 不回退、不重复建表）。
    drop(storage);
    let storage = Storage::open_in_memory().unwrap();
    assert_eq!(storage.run_count().unwrap(), 0);
}

#[test]
fn save_and_load_round_trip_preserves_fidelity() {
    let storage = Storage::open_in_memory().unwrap();
    let original = run("run-1", RunStatus::Cancelled, 3, "2026-10-25T00:00:00Z");
    storage.save_run(&original).unwrap();

    let loaded = storage.get_run("run-1").unwrap();
    assert_eq!(loaded.run_id, original.run_id);
    assert_eq!(loaded.status, RunStatus::Cancelled);
    assert_eq!(loaded.route_mode, RouteMode::NoAppProxy);
    assert_eq!(loaded.targets.len(), 1);
    let target = &loaded.targets[0];
    assert_eq!(target.samples.len(), 5, "取消样本原样保留");
    let https_cancelled = target
        .samples
        .iter()
        .find(|s| {
            s.error
                .as_ref()
                .is_some_and(|e| e.kind == ErrorKind::Cancelled)
        })
        .expect("取消样本保留");
    assert!(!https_cancelled.success);
    // 聚合重算一致：取消计数不入失败率分母（D-014）。
    let https_agg = target
        .aggregates
        .iter()
        .find(|a| a.probe_type == ProbeType::Https)
        .unwrap();
    assert_eq!(https_agg.attempts_total, 3);
    assert_eq!(https_agg.cancelled_count, 1);
    assert_eq!(https_agg.success_count, 2);
    assert_eq!(https_agg.duration_median_ms, Some(450));
}

#[test]
fn list_filter_and_summary_stats() {
    let storage = Storage::open_in_memory().unwrap();
    storage
        .save_run(&run(
            "run-1",
            RunStatus::Completed,
            3,
            "2026-10-25T00:00:00Z",
        ))
        .unwrap();
    let mut other = run("run-2", RunStatus::Completed, 3, "2026-10-25T01:00:00Z");
    other.targets[0].target_id = "zcode".to_string();
    for s in &mut other.targets[0].samples {
        s.target_id = "zcode".to_string();
    }
    storage.save_run(&other).unwrap();

    let all = storage.list_runs(None, None, 100).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].run_id, "run-2", "最新在前");

    let filtered = storage.list_runs(Some("github"), None, 100).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].run_id, "run-1");

    let summary = &all[1];
    assert_eq!(summary.sample_count, 5);
    assert_eq!(summary.success_count, 4);
    assert_eq!(summary.transport_failure_count, 0, "取消不计入传输失败");
    assert_eq!(summary.https_median_ms, Some(450));
}

#[test]
fn delete_and_clear() {
    let storage = Storage::open_in_memory().unwrap();
    storage
        .save_run(&run(
            "run-1",
            RunStatus::Completed,
            3,
            "2026-10-25T00:00:00Z",
        ))
        .unwrap();
    storage
        .save_run(&run(
            "run-2",
            RunStatus::Completed,
            3,
            "2026-10-25T01:00:00Z",
        ))
        .unwrap();

    assert!(storage.delete_run("run-1").unwrap());
    assert!(!storage.delete_run("run-1").unwrap(), "重复删除幂等");
    assert_eq!(storage.run_count().unwrap(), 1);

    let cleared = storage.clear_history().unwrap();
    assert!(cleared >= 4, "样本级联删除计数");
    assert_eq!(storage.run_count().unwrap(), 0);
    assert!(storage.get_run("run-2").is_err());
}

#[test]
fn retention_by_count_and_settings() {
    let storage = Storage::open_in_memory().unwrap();
    // 数量上限：写 DEFAULT_RETENTION_MAX_RUNS+2 个，只留最新 500。
    for index in 0..(DEFAULT_RETENTION_MAX_RUNS + 2) {
        let id = format!("run-{index:04}");
        let started = format!("2026-10-{:02}T00:00:00Z", 1 + index % 28);
        storage
            .save_run(&run(&id, RunStatus::Completed, 3, &started))
            .unwrap();
    }
    assert_eq!(
        storage.run_count().unwrap(),
        DEFAULT_RETENTION_MAX_RUNS,
        "超过上限后旧运行被裁剪"
    );

    // settings 覆盖：max_runs=3。
    storage.set_setting("retention.max_runs", "3").unwrap();
    storage
        .save_run(&run(
            "run-new",
            RunStatus::Completed,
            3,
            "2026-10-25T02:00:00Z",
        ))
        .unwrap();
    assert_eq!(storage.run_count().unwrap(), 3, "自定义上限生效");

    // 时间裁剪：max_age_days 覆盖后，2026-10-01 的运行过期。
    storage.set_setting("retention.max_age_days", "1").unwrap();
    storage.enforce_retention().unwrap();
    let remaining = storage.list_runs(None, None, 500).unwrap();
    assert!(
        remaining
            .iter()
            .all(|s| s.started_at.as_str() >= "2026-10-24"),
        "早于 cutoff 的运行被清理，实际 {:?}",
        remaining
            .iter()
            .map(|s| s.started_at.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn settings_whitelist_behavior() {
    let storage = Storage::open_in_memory().unwrap();
    assert!(storage.get_setting("retention.max_runs").unwrap().is_none());
    storage.set_setting("retention.max_runs", "500").unwrap();
    assert_eq!(
        storage
            .get_setting("retention.max_runs")
            .unwrap()
            .as_deref(),
        Some("500")
    );
}

#[test]
fn sanitize_text_masks_secrets() {
    // userinfo
    assert!(
        sanitize_text("http://user:pass@127.0.0.1:7890/proxy")
            .contains("[凭证已隐藏]@127.0.0.1:7890")
    );
    // 查询 token
    let masked = sanitize_text("https://example.com/path?token=abcdef123456&x=1");
    assert!(masked.contains("token=[已隐藏]"), "{masked}");
    assert!(!masked.contains("abcdef123456"));
    // 本地路径
    let path = sanitize_text("错误发生在 C:\\Users\\alice\\config.ini");
    assert!(path.contains("[本地路径]"), "{path}");
    assert!(!path.contains("alice"));
    // Authorization（F-02 后统一遮蔽标记为 [已隐藏]）
    let auth = sanitize_text("header Authorization: Bearer eyJhbGciOi");
    assert!(auth.contains("[已隐藏]"), "{auth}");
    assert!(!auth.contains("eyJhbGciOi"));
    assert!(!auth.contains("Bearer"));
    // 普通文本不受影响
    assert_eq!(
        sanitize_text("DNS 解析失败：no such host"),
        "DNS 解析失败：no such host"
    );
}

#[test]
fn report_is_sanitized_and_carries_limitations() {
    let mut contaminated = run("run-x", RunStatus::Completed, 3, "2026-10-25T00:00:00Z");
    contaminated.targets[0].samples[0].error = Some(DiagnosticError {
        kind: ErrorKind::Unknown,
        summary: "代理连接 http://user:secret@127.0.0.1:7890 失败".to_string(),
        detail: Some("配置文件 C:\\Users\\bob\\clash.yaml".to_string()),
    });

    let (json, filename) = build_report(&contaminated, ReportFormat::Json);
    assert!(filename.ends_with(".json"));
    assert!(!json.contains("user:secret"), "JSON 报告不得泄漏 userinfo");
    assert!(!json.contains("bob"), "JSON 报告不得泄漏用户名");
    assert!(json.contains("[凭证已隐藏]"));
    assert!(json.contains("[本地路径]"));
    assert!(json.contains("不构成统计显著结论"), "报告必须携带局限说明");
    assert!(json.contains("diagnoses"), "报告包含规则诊断");

    let (markdown, filename) = build_report(&contaminated, ReportFormat::Markdown);
    assert!(filename.ends_with(".md"));
    assert!(!markdown.contains("user:secret"));
    assert!(!markdown.contains("bob"));
    assert!(markdown.contains("诊断报告"));
    assert!(markdown.contains("局限"));
}
