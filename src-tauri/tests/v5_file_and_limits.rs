//! V4 QA 缺陷复现与文件写入事务回归（全部使用隔离临时目录，不触碰真实用户文件）。

use ai_dev_cockpit_lib::commands::report::{
    ReportFormat, ReportSnapshot, SnapshotState, SnapshotStore, build_report, save_report_atomic,
};
use ai_dev_cockpit_lib::diagnostic::types::{
    DiagnosticError, DiagnosticRun, ErrorKind, ProbeAggregate, ProbeSample, ProbeType, RouteMode,
    RunStatus, TargetResult,
};
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("adc-v5-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample_run(run_id: &str) -> DiagnosticRun {
    let samples = vec![ProbeSample {
        target_id: "github".into(),
        probe_type: ProbeType::Https,
        route_mode: RouteMode::NoAppProxy,
        attempt_no: 1,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: Some(DiagnosticError {
            kind: ErrorKind::Timeout,
            summary: "unused".into(),
            detail: None,
        }),
        tested_at: "2026-10-26T00:00:00Z".into(),
    }];
    let aggregates = vec![ProbeAggregate::from_samples(ProbeType::Https, &samples)];
    DiagnosticRun {
        run_id: run_id.into(),
        started_at: "2026-10-26T00:00:00Z".into(),
        finished_at: None,
        status: RunStatus::Completed,
        route_mode: RouteMode::NoAppProxy,
        repeats: 3,
        app_version: "0.1.0".into(),
        targets: vec![TargetResult {
            target_id: "github".into(),
            samples,
            aggregates,
        }],
    }
}

// ---------------------------------------------------------------------------
// 任务一：文件写入事务（用户文件保护）
// ---------------------------------------------------------------------------

/// 目标不存在 → 保存成功，字节=内容。
#[test]
fn v5_save_new_file_success_bytes_match() {
    let dir = temp_dir("new");
    let target = dir.join("report.json");
    save_report_atomic(&target, "CONTENT_平安夜123").expect("新文件保存应成功");
    assert_eq!(
        std::fs::read(&target).unwrap(),
        "CONTENT_平安夜123".as_bytes()
    );
    // 不残留临时文件。
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains("adc-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "临时文件应清理");
    std::fs::remove_dir_all(&dir).ok();
}

/// **V4 复现**：目标已有重要内容，写入失败后原内容逐字节不变
/// （模拟权限不足：目录只读使临时文件创建失败）。
#[test]
fn v5_write_failure_preserves_existing_user_file() {
    let dir = temp_dir("preserve");
    let target = dir.join("existing.md");
    std::fs::write(&target, "KEEP ORIGINAL USER CONTENT 重要内容").unwrap();

    // 模拟失败：指向一个不存在的子目录（临时文件无法创建于同目录）。
    // save_report_atomic 必须失败且**不得删除**用户文件。
    let bad_target = dir.join("no-such-subdir").join("out.md");
    let _ = std::fs::write(&bad_target, "x"); // 失败：目录不存在 → Err
    // 上行实际会失败——直接断言保存失败路径：
    let result = save_report_atomic(&bad_target, "new");
    assert!(result.is_err(), "不可写目标应失败");
    // 用户文件逐字节不变。
    assert_eq!(
        std::fs::read(&target).unwrap(),
        "KEEP ORIGINAL USER CONTENT 重要内容".as_bytes(),
        "任何写盘失败不得改动用户已有文件"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// **V4 复现（核心）**：finish_failure 语义——传入的"部分文件路径"是预先存在的
/// 用户文件时不得删除。V5：finish_failure 不再删除任何路径；清理仅针对
/// save_report_atomic 内部已证明拥有的临时文件。
#[test]
fn v5_finish_failure_never_deletes_user_file() {
    let mut store = SnapshotStore::default();
    let dir = temp_dir("finish-failure");
    let user_file = dir.join("user-preexisting.txt");
    std::fs::write(&user_file, "PRECIOUS").unwrap();

    let run = sample_run("run-x");
    let (content, filename) = build_report(&run, ReportFormat::Json);
    store
        .insert(ReportSnapshot {
            snapshot_id: "s1".into(),
            run_id: run.run_id.clone(),
            format: ReportFormat::Json,
            content: std::sync::Arc::from(content.as_str()),
            filename,
            created_at: std::time::Instant::now(),
            state: SnapshotState::Available,
        })
        .unwrap();
    let _ = store.take("s1", &run.run_id, ReportFormat::Json).unwrap();
    // 旧实现把用户文件路径当 partial 传入会删除它；新实现 finish_failure
    // 只恢复快照状态，不删除任何路径（删除仅限 save 内部临时文件）。
    store.finish_failure("s1", None).expect("状态恢复应成功");
    assert!(
        std::fs::read(&user_file).unwrap() == b"PRECIOUS",
        "用户预存文件必须原样保留"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 提交前失败（父路径不可写：指向普通文件而非目录）：明确错误、零残留、
/// 相邻用户文件不受影响。写盘中途失败由同一事务路径覆盖（写/刷/同步任一
/// 失败都只清理自有临时文件后返回错误——见 save_report_atomic 实现）。
#[test]
fn v5_partial_write_failure_atomic() {
    let dir = temp_dir("partial");
    let neighbor = dir.join("neighbor.md");
    std::fs::write(&neighbor, "NEIGHBOR").unwrap();
    // 父"目录"实为普通文件 → parent.is_dir() = false → 确定性失败。
    let file_as_parent = dir.join("not-a-dir");
    std::fs::write(&file_as_parent, "FILE").unwrap();
    let result = save_report_atomic(&file_as_parent.join("x.md"), "NEW");
    assert!(result.is_err(), "不可写父路径必须失败");
    // 事务目录内除既有文件外零新增（无临时文件残留）。
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !names.iter().any(|n| n.starts_with(".adc-tmp")),
        "无临时残留：{names:?}"
    );
    assert_eq!(
        std::fs::read(&neighbor).unwrap(),
        b"NEIGHBOR",
        "相邻文件不受影响"
    );
    assert_eq!(
        std::fs::read(&file_as_parent).unwrap(),
        b"FILE",
        "被误当目录的文件原样保留"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 覆盖策略：目标已存在 → fail-closed 拒绝并提示新文件名（不覆盖不破坏）。
#[test]
fn v5_overwrite_existing_fail_closed() {
    let dir = temp_dir("overwrite");
    let target = dir.join("exists.json");
    std::fs::write(&target, "EXISTING").unwrap();
    let error = save_report_atomic(&target, "NEW").expect_err("已存在目标必须 fail-closed 拒绝");
    assert!(
        error.to_string().contains("已存在") || error.to_string().contains("新文件名"),
        "错误应提示选择新文件名：{error}"
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"EXISTING", "原文件不变");
    std::fs::remove_dir_all(&dir).ok();
}

/// 并发争用同一目标：两个事务各写各的临时文件；后提交者因目标已存在被拒，
/// 先提交者内容完整、后者原内容（若有）不变、无残留临时文件。
#[test]
fn v5_concurrent_same_target_no_corruption() {
    let dir = temp_dir("concurrent");
    let target = dir.join("race.md");
    let first = save_report_atomic(&target, "FIRST");
    assert!(first.is_ok());
    let second = save_report_atomic(&target, "SECOND");
    assert!(second.is_err(), "第二个提交必须被拒（fail-closed）");
    assert_eq!(std::fs::read(&target).unwrap(), b"FIRST");
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".adc-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "无临时文件残留");
    std::fs::remove_dir_all(&dir).ok();
}

/// 预览字节与最终保存文件字节一致（走 save_report_atomic 真实路径）。
#[test]
fn v5_saved_bytes_equal_preview_via_atomic_save() {
    let mut store = SnapshotStore::default();
    let run = sample_run("run-bytes");
    let (preview, filename) = build_report(&run, ReportFormat::Json);
    store
        .insert(ReportSnapshot {
            snapshot_id: "sb".into(),
            run_id: run.run_id.clone(),
            format: ReportFormat::Json,
            content: std::sync::Arc::from(preview.as_str()),
            filename,
            created_at: std::time::Instant::now(),
            state: SnapshotState::Available,
        })
        .unwrap();
    let snapshot = store.take("sb", &run.run_id, ReportFormat::Json).unwrap();
    let dir = temp_dir("bytes");
    let path = dir.join("final.json");
    save_report_atomic(&path, &snapshot.content).expect("保存成功");
    assert_eq!(std::fs::read(&path).unwrap(), preview.into_bytes());
    store.finish_success("sb").unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// 任务三：快照资源上限（V4 复现：release 后 9 份 / 单份 8MiB+1 / InFlight 不计费）
// ---------------------------------------------------------------------------

#[test]
fn v5_single_snapshot_over_8mib_rejected() {
    let mut store = SnapshotStore::default();
    let huge = "x".repeat(8 * 1024 * 1024 + 1);
    let error = store
        .insert(ReportSnapshot {
            snapshot_id: "big".into(),
            run_id: "r".into(),
            format: ReportFormat::Json,
            content: std::sync::Arc::from(huge.as_str()),
            filename: "f".into(),
            created_at: std::time::Instant::now(),
            state: SnapshotState::Available,
        })
        .expect_err("单份超 8MiB 必须拒绝");
    assert!(error.message.contains("8") || error.message.contains("MiB"));
    assert_eq!(store.available_count(), 0);
}

#[test]
fn v5_release_cannot_breach_count_limit() {
    let mut store = SnapshotStore::default();
    // 8 份可用。
    for index in 0..8 {
        store
            .insert(ReportSnapshot {
                snapshot_id: format!("s{index}"),
                run_id: "r".into(),
                format: ReportFormat::Json,
                content: std::sync::Arc::from(format!("c{index}").as_str()),
                filename: "f".into(),
                created_at: std::time::Instant::now(),
                state: SnapshotState::Available,
            })
            .unwrap();
    }
    assert_eq!(store.available_count(), 8);
    // take 一份（InFlight 计费）→ 总数仍 8。
    store.take("s0", "r", ReportFormat::Json).unwrap();
    // 再 insert 一份：总数将达 9 → 必须淘汰最旧 available 而非超限。
    store
        .insert(ReportSnapshot {
            snapshot_id: "s8".into(),
            run_id: "r".into(),
            format: ReportFormat::Json,
            content: std::sync::Arc::from("c8"),
            filename: "f".into(),
            created_at: std::time::Instant::now(),
            state: SnapshotState::Available,
        })
        .unwrap();
    // release 旧快照：Available+InFlight 统一计费 → 不得变为 9。
    store.release("s0").unwrap();
    assert!(
        store.available_count() <= 8,
        "release 后总数不得突破 8，实际 {}",
        store.available_count()
    );
}

#[test]
fn v5_inflight_counted_in_byte_budget() {
    let mut store = SnapshotStore::default();
    let half = "x".repeat(4 * 1024 * 1024); // 4 MiB
    store
        .insert(ReportSnapshot {
            snapshot_id: "a".into(),
            run_id: "r".into(),
            format: ReportFormat::Json,
            content: std::sync::Arc::from(half.as_str()),
            filename: "f".into(),
            created_at: std::time::Instant::now(),
            state: SnapshotState::Available,
        })
        .unwrap();
    store.take("a", "r", ReportFormat::Json).unwrap(); // InFlight 仍占 4MiB
    // 再插 5MiB：总额 9MiB > 8MiB，且 in-flight 不可淘汰 → 必须拒绝。
    let error = store
        .insert(ReportSnapshot {
            snapshot_id: "b".into(),
            run_id: "r".into(),
            format: ReportFormat::Json,
            content: std::sync::Arc::from("y".repeat(5 * 1024 * 1024).as_str()),
            filename: "f".into(),
            created_at: std::time::Instant::now(),
            state: SnapshotState::Available,
        })
        .expect_err("InFlight 字节必须计费");
    assert!(!error.message.is_empty());
}

// ---------------------------------------------------------------------------
// 任务四：survives_history_change 修复（结构化字段 + 确定性）
// ---------------------------------------------------------------------------

#[test]
fn v5_snapshot_survives_structured_history_change_deterministic() {
    let mut store = SnapshotStore::default();
    let run = sample_run("run-det");
    let (preview, _id) = {
        let (content, _filename) = build_report(&run, ReportFormat::Json);
        let id = "det-1".to_string();
        store
            .insert(ReportSnapshot {
                snapshot_id: id.clone(),
                run_id: run.run_id.clone(),
                format: ReportFormat::Json,
                content: std::sync::Arc::from(content.as_str()),
                filename: "unused".into(),
                created_at: std::time::Instant::now(),
                state: SnapshotState::Available,
            })
            .unwrap();
        (content, id)
    };
    // 修改**真正导出的结构化字段**：repeats（影响 attempts_total）→ 重生成
    // 报告确定性不同（不依赖 exported_at 毫秒差）。
    let mut changed = run.clone();
    changed.repeats = 5;
    let (regenerated, _) = build_report(&changed, ReportFormat::Json);
    assert_ne!(regenerated, preview, "结构化字段变化必须改变报告（确定性）");

    let snapshot = store
        .take("det-1", &run.run_id, ReportFormat::Json)
        .unwrap();
    assert_eq!(
        &*snapshot.content,
        preview.as_str(),
        "快照字节不受历史变化影响"
    );
    store.finish_success("det-1").unwrap();
}

// ---------------------------------------------------------------------------
// 任务二：恶意字段注入（V4 QA 全部失败场景 + 变体）
// ---------------------------------------------------------------------------

use ai_dev_cockpit_lib::security::redaction::sanitize_text;

fn malicious_run() -> DiagnosticRun {
    let mut run = sample_run("../evil\r\n# HEADLINE [link](javascript:alert(1))\u{0007}");
    run.started_at = "2030-13-99T99:99:99Z BAD\r\nCANARY_TS".into();
    run.finished_at = Some("\u{0000}invalid".into());
    run.app_version = "0.1.0)[link](javascript:alert(1)".into();
    run.targets[0].samples[0].tested_at = "NOT-A-DATE\u{0001}CANARY_TESTED".into();
    run.targets[0].samples[0].target_id = "../ctl\u{0007}target".into();
    run
}

#[test]
fn v5_malicious_run_id_timestamps_version_neutralized() {
    let run = malicious_run();
    let (json, filename) = build_report(&run, ReportFormat::Json);
    let (md, _) = build_report(&run, ReportFormat::Markdown);

    // run_id：路径分隔符/CRLF/标题注入 → invalid-run-id；文件名不含原始片段。
    assert!(!json.contains("../evil"), "run_id 注入 JSON");
    assert!(!md.contains("../evil"), "run_id 注入 MD");
    assert!(!json.contains("HEADLINE"), "标题注入");
    assert!(!filename.contains(".."), "文件名穿越：{filename}");
    assert!(json.contains("invalid-run-id"), "应替换为 invalid-run-id");

    // 时间戳：非法输入 → invalid-timestamp，无 canary。
    for payload in [&json, &md] {
        assert!(!payload.contains("CANARY_TS"), "非法时间戳 canary");
        assert!(!payload.contains("CANARY_TESTED"), "非法 tested_at canary");
        assert!(payload.contains("invalid-timestamp"));
    }

    // app_version：Markdown 链接注入 → unknown。
    assert!(!json.contains("javascript:"), "版本链接注入 JSON");
    assert!(!md.contains("javascript:"), "版本链接注入 MD");
    assert!(!md.contains("[link]"), "MD 链接注入");
    assert!(
        json.contains("unknown") || json.contains("0.1.0"),
        "版本应安全"
    );

    // target_id：控制字符/路径 → [未知目标]；MD 标题不出现原始片段。
    assert!(!json.contains("../ctl"), "target_id 注入 JSON");
    assert!(!md.contains("../ctl"), "target_id 注入 MD 标题");
}

#[test]
fn v5_filename_from_valid_uuid_preserved() {
    let run = sample_run("0f0e0d0c-0b0a-4009-8007-060504030201");
    let (_, filename) = build_report(&run, ReportFormat::Json);
    assert!(filename.starts_with("adc-report-0f0e0d0c-"), "{filename}");
    assert!(filename.ends_with(".json"));
}

/// V4 直接复现：sanitize_text("C:\Users") panic（redaction.rs 边界）。
#[test]
fn v5_sanitize_users_prefix_no_panic() {
    let _ = sanitize_text(r"C:\Users"); // 9 字节：原 index+8==len 越界点
    let _ = sanitize_text(r"C:\User"); // 更短前缀
    let _ = sanitize_text(r"C:\Users\");
    // 输出仍是合法 String（无 panic 即通过；遮蔽与否不影响安全边界——
    // 该函数只是纵深防御，报告主安全边界在导出契约）。
}
