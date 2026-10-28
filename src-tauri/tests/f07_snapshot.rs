//! F-07 回归：预览与导出复用同一份服务端快照字节（走生产 SnapshotStore 状态机）。

use ai_dev_cockpit_lib::commands::report::{
    ReportFormat, ReportSnapshot, SnapshotState, SnapshotStore, build_report,
};
use ai_dev_cockpit_lib::diagnostic::types::{
    DiagnosticError, DiagnosticRun, ErrorKind, ProbeAggregate, ProbeSample, ProbeType, RouteMode,
    RunStatus, TargetResult,
};

/// 测试用带假凭证的运行。
fn contaminated_run_for_snapshot() -> DiagnosticRun {
    let sample = |attempt: u32, summary: &str| ProbeSample {
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
            detail: None,
        }),
        tested_at: "2026-10-25T00:00:00Z".to_string(),
    };
    let samples = vec![
        sample(1, "Cookie: FAKE_C1"),
        sample(2, "X-API-Key: FAKE_K2"),
        sample(3, "internal 10.0.0.9"),
        sample(4, "host db.internal.corp.example"),
    ];
    let aggregates = vec![ProbeAggregate::from_samples(ProbeType::Https, &samples)];
    DiagnosticRun {
        run_id: "run-f07".to_string(),
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

/// 模拟 preview IPC 的核心路径：build_report → insert。返回 (预览字节, 文件名, 快照 id)。
fn do_preview(store: &mut SnapshotStore, run: &DiagnosticRun) -> (String, String, String) {
    let (content, filename) = build_report(run, ReportFormat::Json);
    let snapshot_id = format!("snap-{}", store.available_count());
    store
        .insert(ReportSnapshot {
            snapshot_id: snapshot_id.clone(),
            run_id: run.run_id.clone(),
            format: ReportFormat::Json,
            content: std::sync::Arc::from(content.as_str()),
            filename: filename.clone(),
            created_at: std::time::Instant::now(),
            state: SnapshotState::Available,
        })
        .unwrap();
    (content, filename, snapshot_id)
}

#[test]
fn f07_new_snapshot_immediately_consumable_bytes_equal_preview() {
    let mut store = SnapshotStore::default();
    let run = contaminated_run_for_snapshot();
    let (preview_content, _, snapshot_id) = do_preview(&mut store, &run);

    // 新快照**立即**可消费（V3 QA：take 立即失败——已修复）。
    let snapshot = store
        .take(&snapshot_id, &run.run_id, ReportFormat::Json)
        .expect("新快照必须立即可消费");
    assert_eq!(
        &*snapshot.content,
        preview_content.as_str(),
        "导出字节=预览字节（F-07）"
    );
    assert_eq!(snapshot.state, SnapshotState::InFlight);
    store.finish_success(&snapshot_id).expect("成功保存");
    assert_eq!(store.consumed_count(), 1);
}

#[test]
fn f07_snapshot_survives_history_change() {
    let mut store = SnapshotStore::default();
    let run = contaminated_run_for_snapshot();
    let (preview_content, _, snapshot_id) = do_preview(&mut store, &run);

    // 历史数据"变化"：修改**真正导出的结构化字段**（repeats → attempts_total
    // 变化）——重生成报告确定性不同（不依赖 exported_at 毫秒差，无时间竞态）。
    // （V4 QA：旧断言依赖已默认省略的 error.summary，必失败。）
    let mut changed = run.clone();
    changed.repeats = 5;
    let (regenerated, _) = build_report(&changed, ReportFormat::Json);
    assert_ne!(
        regenerated, preview_content,
        "前提：重生成内容不同（结构化字段）"
    );

    let snapshot = store
        .take(&snapshot_id, &run.run_id, ReportFormat::Json)
        .expect("快照仍可消费");
    assert_eq!(
        &*snapshot.content,
        preview_content.as_str(),
        "F-07：导出=预览字节"
    );
    store.finish_success(&snapshot_id).unwrap();
}

#[test]
fn f07_invalid_wrong_run_wrong_format_rejected() {
    let mut store = SnapshotStore::default();
    let run = contaminated_run_for_snapshot();
    let (content, _, snapshot_id) = do_preview(&mut store, &run);

    // 错误 snapshot_id。
    assert!(
        store
            .take("wrong-id", &run.run_id, ReportFormat::Json)
            .is_err()
    );
    // 正确 id 但错误 run_id。
    assert!(
        store
            .take(&snapshot_id, "other-run", ReportFormat::Json)
            .is_err()
    );
    // 正确 id/run 但错误 format。
    assert!(
        store
            .take(&snapshot_id, &run.run_id, ReportFormat::Markdown)
            .is_err()
    );
    // 正确关联 → 可消费；内容字节与预览一致。
    let snapshot = store
        .take(&snapshot_id, &run.run_id, ReportFormat::Json)
        .expect("正确关联应可消费");
    assert_eq!(&*snapshot.content, content.as_str());
    store.finish_success(&snapshot_id).unwrap();
}

#[test]
fn f07_double_consumption_rejected() {
    let mut store = SnapshotStore::default();
    let run = contaminated_run_for_snapshot();
    let (_, _, snapshot_id) = do_preview(&mut store, &run);

    let _snapshot = store
        .take(&snapshot_id, &run.run_id, ReportFormat::Json)
        .expect("第一次 take");
    store.finish_success(&snapshot_id).unwrap();

    // 成功后快照已消费：第二次 take 必须失败。
    assert!(
        store
            .take(&snapshot_id, &run.run_id, ReportFormat::Json)
            .is_err()
    );
}

#[test]
fn f07_cancel_allows_retry_same_bytes() {
    let mut store = SnapshotStore::default();
    let run = contaminated_run_for_snapshot();
    let (preview_content, _, snapshot_id) = do_preview(&mut store, &run);

    // 用户在导出对话框点取消：take 先把快照置为 InFlight，release 回 Available。
    let _snapshot = store
        .take(&snapshot_id, &run.run_id, ReportFormat::Json)
        .expect("take");
    store.release(&snapshot_id).expect("取消应成功");
    // 重试导出：同一份字节再次可消费。
    let snapshot = store
        .take(&snapshot_id, &run.run_id, ReportFormat::Json)
        .expect("取消后可重试");
    assert_eq!(&*snapshot.content, preview_content.as_str());
    store.finish_success(&snapshot_id).unwrap();
}

#[test]
fn f07_write_failure_allows_retry_and_cleans_partial() {
    let mut store = SnapshotStore::default();
    let run = contaminated_run_for_snapshot();
    let (preview_content, _, snapshot_id) = do_preview(&mut store, &run);

    // 第一次写盘失败（不可写路径）→ 快照回 Available + 部分文件清理。
    let bad_path = std::path::Path::new("Z:\\nonexistent\\dir\\report.json");
    let _snapshot = store
        .take(&snapshot_id, &run.run_id, ReportFormat::Json)
        .expect("take");
    store
        .finish_failure(&snapshot_id, Some(bad_path))
        .expect("失败处理应成功");
    assert!(!bad_path.exists(), "部分文件应被清理");

    // 重试：再次 take → 成功写盘到合法路径。
    let snapshot2 = store
        .take(&snapshot_id, &run.run_id, ReportFormat::Json)
        .expect("失败后可重试");
    assert_eq!(
        &*snapshot2.content,
        preview_content.as_str(),
        "重试导出同一份字节"
    );
    let good_path = std::env::temp_dir().join(format!("adc-f07-{}.md", snapshot_id));
    std::fs::write(&good_path, &*snapshot2.content).unwrap();
    let written = std::fs::read(&good_path).unwrap();
    assert_eq!(
        written,
        preview_content.clone().into_bytes(),
        "文件字节=预览字节"
    );
    std::fs::remove_file(&good_path).ok();
    store.finish_success(&snapshot_id).unwrap();
}

#[test]
fn f07_three_snapshots_coexist_independently() {
    let mut store = SnapshotStore::default();
    let runs: Vec<DiagnosticRun> = (0..3)
        .map(|index| {
            let mut run = contaminated_run_for_snapshot();
            run.run_id = format!("run-{index}");
            run
        })
        .collect();
    let mut ids = Vec::new();
    for run in &runs {
        let (_, _, id) = do_preview(&mut store, run);
        ids.push(id);
    }
    assert_eq!(store.available_count(), 3, "三个快照共存");
    for (index, id) in ids.iter().enumerate() {
        let snapshot = store
            .take(id, &format!("run-{index}"), ReportFormat::Json)
            .expect("独立消费");
        assert_eq!(snapshot.run_id, format!("run-{index}"));
        store.finish_success(id).unwrap();
    }
    assert_eq!(store.available_count(), 0);
}

#[test]
fn f07_ttl_expiry_rejects_expired_snapshot() {
    let mut store = SnapshotStore::default();
    let run = contaminated_run_for_snapshot();

    // 直接构造过期快照（created_at 回拨 11 分钟，超过 600s TTL）。
    store
        .insert(ReportSnapshot {
            snapshot_id: "snap-expired".to_string(),
            run_id: run.run_id.clone(),
            format: ReportFormat::Json,
            content: std::sync::Arc::from("expired-content"),
            filename: "f.json".into(),
            created_at: std::time::Instant::now() - std::time::Duration::from_secs(660),
            state: SnapshotState::Available,
        })
        .unwrap();

    let error = store
        .take("snap-expired", &run.run_id, ReportFormat::Json)
        .expect_err("过期快照必须被拒绝");
    assert!(error.message.contains("不存在、已过期或已使用"));
}

#[test]
fn f07_count_limit_enforced() {
    let mut store = SnapshotStore::default();
    for index in 0..10 {
        let mut run = contaminated_run_for_snapshot();
        run.run_id = format!("run-{index}");
        let (content, filename) = build_report(&run, ReportFormat::Json);
        store
            .insert(ReportSnapshot {
                snapshot_id: format!("snap-{index}"),
                run_id: run.run_id.clone(),
                format: ReportFormat::Json,
                content: std::sync::Arc::from(content.as_str()),
                filename,
                created_at: std::time::Instant::now(),
                state: SnapshotState::Available,
            })
            .unwrap();
    }
    assert_eq!(store.available_count(), 8, "数量上限 8");
    assert!(
        store.take("snap-0", "run-0", ReportFormat::Json).is_err(),
        "最旧的被淘汰"
    );
    assert!(
        store.take("snap-9", "run-9", ReportFormat::Json).is_ok(),
        "最新的保留"
    );
}

/// 链路级：预览与保存文件字节级一致（走生产 take + 真实写盘）。
#[test]
fn f07_saved_file_bytes_equal_preview_bytes() {
    let mut store = SnapshotStore::default();
    let run = contaminated_run_for_snapshot();
    let (preview_content, _, snapshot_id) = do_preview(&mut store, &run);

    let snapshot = store
        .take(&snapshot_id, &run.run_id, ReportFormat::Json)
        .expect("take");
    let path = std::env::temp_dir().join(format!("adc-f07-bytes-{}.json", snapshot_id));
    std::fs::write(&path, &*snapshot.content).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert_eq!(
        saved,
        preview_content.clone().into_bytes(),
        "保存字节=预览字节"
    );
    std::fs::remove_file(&path).ok();
    store.finish_success(&snapshot_id).unwrap();
}
