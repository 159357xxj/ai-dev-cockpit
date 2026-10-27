//! V6-R1（契约第 1 节 F-1..F-10）：原子"不替换"保存提交的永久回归测试。
//!
//! 约束（契约 §5）：所有用例只使用 `tempfile::tempdir()` 创建的隔离临时目录，
//! 不读写真实 AppData / history.db，不访问网络。
//!
//! 竞态用例 `save_race_target_created_before_commit` 依赖
//! `cargo test --features test-hooks`（默认构建不包含该钩子）。

use std::path::Path;
use std::sync::{Arc, Barrier};
use std::time::SystemTime;

use ai_dev_cockpit_lib::commands::report::{
    ReportFormat, ReportSnapshot, SaveError, SnapshotState, SnapshotStore, build_report,
    save_report_atomic,
};
use ai_dev_cockpit_lib::diagnostic::types::{
    DiagnosticError, DiagnosticRun, ErrorKind, ProbeAggregate, ProbeSample, ProbeType, RouteMode,
    RunStatus, TargetResult,
};

// ---------------------------------------------------------------------------
// 辅助：临时目录断言、运行夹具、SHA-256
// ---------------------------------------------------------------------------

/// 目录中不得残留任何以 `.adc-tmp-` 开头的临时文件（F-6 / F-7）。
fn assert_no_temp_leftovers(dir: &Path) {
    let leftovers: Vec<String> = std::fs::read_dir(dir)
        .expect("列目录")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".adc-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "不应残留临时文件：{leftovers:?}");
}

/// 目录清单（排序后），用于断言"没有文件被删除、没有多余文件被创建"。
fn sorted_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("列目录")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .expect("读取元数据")
        .modified()
        .expect("读取修改时间")
}

fn sample_run(run_id: &str) -> DiagnosticRun {
    let sample = ProbeSample {
        target_id: "github".to_string(),
        probe_type: ProbeType::Https,
        route_mode: RouteMode::NoAppProxy,
        attempt_no: 1,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: Some(DiagnosticError {
            kind: ErrorKind::Timeout,
            summary: "连接超时".to_string(),
            detail: None,
        }),
        tested_at: "2026-10-25T00:00:00Z".to_string(),
    };
    let samples = vec![sample];
    let aggregates = vec![ProbeAggregate::from_samples(ProbeType::Https, &samples)];
    DiagnosticRun {
        run_id: run_id.to_string(),
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

#[cfg(windows)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

#[cfg(unix)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// 测试内置 SHA-256（不引入额外依赖：Cargo.toml 在本轮只允许加入 tempfile）。
/// 正确性由 `sha256_helper_matches_known_vectors` 用标准测试向量自证。
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];

    let mut state: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];

    let mut message = data.to_vec();
    let bit_length = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());

    for chunk in message.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (index, word) in schedule.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                chunk[index * 4],
                chunk[index * 4 + 1],
                chunk[index * 4 + 2],
                chunk[index * 4 + 3],
            ]);
        }
        for index in 16..64 {
            let prev15 = schedule[index - 15];
            let prev2 = schedule[index - 2];
            let s0 = prev15.rotate_right(7) ^ prev15.rotate_right(18) ^ (prev15 >> 3);
            let s1 = prev2.rotate_right(17) ^ prev2.rotate_right(19) ^ (prev2 >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }

        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        let mut e = state[4];
        let mut f = state[5];
        let mut g = state[6];
        let mut h = state[7];

        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    let mut digest = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 自证：内置 SHA-256 与标准测试向量一致，后续 SHA-256 断言才有意义。
#[test]
fn sha256_helper_matches_known_vectors() {
    assert_eq!(
        hex(&sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&sha256(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

// ---------------------------------------------------------------------------
// F-2：目标上已存在对象时拒绝，且字节 / 长度 / 修改时间不变
// ---------------------------------------------------------------------------

#[test]
fn save_refuses_existing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("existing.json");
    std::fs::write(&target, b"ORIGINAL").expect("预置用户文件");
    let before_len = std::fs::metadata(&target).expect("元数据").len();
    let before_mtime = modified(&target);

    let error = save_report_atomic(&target, "NEW CONTENT THAT MUST NOT LAND")
        .expect_err("目标已存在必须拒绝保存");
    assert!(
        matches!(&error, SaveError::TargetExists),
        "应返回 TargetExists，实际 {error:?}"
    );

    assert_eq!(
        std::fs::read(&target).expect("读回"),
        b"ORIGINAL",
        "原文件字节不得改变"
    );
    let after = std::fs::metadata(&target).expect("元数据");
    assert_eq!(after.len(), before_len, "长度不得改变");
    assert_eq!(
        after.modified().expect("修改时间"),
        before_mtime,
        "修改时间不得改变"
    );
    assert_no_temp_leftovers(dir.path());
}

#[test]
fn save_refuses_existing_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("dir-target");
    std::fs::create_dir(&target).expect("预置目录");

    let error = save_report_atomic(&target, "X").expect_err("目标是目录时必须拒绝");
    assert!(
        matches!(&error, SaveError::TargetExists),
        "应返回 TargetExists，实际 {error:?}"
    );
    assert!(target.is_dir(), "目录必须仍然存在");
    assert_no_temp_leftovers(dir.path());
}

/// F-2 的符号链接分支：创建符号链接需要 SeCreateSymbolicLinkPrivilege
/// （Windows 开发者模式或提权）。该特权缺失时无法构造前提，因此本用例默认
/// `ignored`——普通 `cargo test` 会显示为 ignored 而不是 passed，杜绝
/// 「无权限提前返回」造成的假通过。提权/开发者模式下用 `cargo test --ignored` 执行。
#[test]
#[ignore = "requires SeCreateSymbolicLinkPrivilege; run with --ignored in elevated shell"]
fn save_refuses_existing_symlink() {
    let dir = tempfile::tempdir().expect("tempdir");
    let victim = dir.path().join("victim.txt");
    std::fs::write(&victim, b"VICTIM").expect("预置被指向文件");
    let link = dir.path().join("link.json");

    create_file_symlink(&victim, &link).expect(
        "创建符号链接失败：需要 SeCreateSymbolicLinkPrivilege（请在提权 shell 或开启开发者模式后用 `cargo test --ignored` 运行）",
    );

    let before_len = std::fs::metadata(&victim).expect("元数据").len();
    let before_mtime = modified(&victim);

    let error = save_report_atomic(&link, "OURS").expect_err("符号链接作为目标必须拒绝");
    assert!(
        matches!(&error, SaveError::TargetExists),
        "应返回 TargetExists，实际 {error:?}"
    );
    let link_meta = std::fs::symlink_metadata(&link).expect("链接必须仍然存在");
    assert!(
        link_meta.file_type().is_symlink(),
        "目标不得被替换为普通文件"
    );
    assert_eq!(
        std::fs::read(&victim).expect("读回"),
        b"VICTIM",
        "被指向文件内容不得改变"
    );
    assert_eq!(
        std::fs::metadata(&victim).expect("元数据").len(),
        before_len
    );
    assert_eq!(modified(&victim), before_mtime, "被指向文件 mtime 不得改变");
    assert_no_temp_leftovers(dir.path());

    // 悬空符号链接：`exists()` 为 false，因此必须由提交步骤（而非预检查）
    // 拒绝——绝不能顺着链接把内容写到别处或新建同名文件。
    let dangling = dir.path().join("dangling.json");
    create_file_symlink(&dir.path().join("no-such-victim.txt"), &dangling)
        .expect("创建悬空符号链接失败：需要 SeCreateSymbolicLinkPrivilege");
    let error = save_report_atomic(&dangling, "OURS").expect_err("悬空符号链接必须拒绝");
    assert!(
        matches!(&error, SaveError::TargetExists),
        "悬空链接应返回 TargetExists，实际 {error:?}"
    );
    assert!(
        std::fs::symlink_metadata(&dangling)
            .expect("悬空链接必须仍然存在")
            .file_type()
            .is_symlink(),
        "悬空链接不得被替换"
    );
    assert!(
        !dir.path().join("no-such-victim.txt").exists(),
        "不得顺着悬空链接创建文件"
    );
    assert_no_temp_leftovers(dir.path());
}

/// F-2 的 junction 分支：junction（directory reparse point）的创建**不需要**
/// SeCreateSymbolicLinkPrivilege，因此本机可以实际执行——这是"符号链接用例
/// 因特权缺失被跳过"时的补充证据。若 mklink 不可用则明确跳过。
#[cfg(windows)]
#[test]
fn save_refuses_existing_junction() {
    let dir = tempfile::tempdir().expect("tempdir");
    let real_dir = dir.path().join("real-dir");
    std::fs::create_dir(&real_dir).expect("预置真实目录");
    let junction = dir.path().join("junction-target");

    // 注意：路径作为独立 argv 传给 Command，由 std 负责必要时的引号转义；
    // 手工加引号会被 cmd 二次解析成非法路径。
    let created = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&real_dir)
        .status();
    match created {
        Ok(status) if status.success() => {}
        other => {
            println!("SKIPPED: cannot create junction ({other:?})");
            return;
        }
    }

    let before = sorted_names(&real_dir);
    let error = save_report_atomic(&junction, "X").expect_err("junction 作为目标必须拒绝");
    assert!(
        matches!(&error, SaveError::TargetExists),
        "应返回 TargetExists，实际 {error:?}"
    );
    assert!(junction.exists(), "junction 必须仍然存在");
    assert!(
        junction.is_dir(),
        "junction 不得被替换为普通文件（reparse point 必须保留）"
    );
    assert_eq!(
        sorted_names(&real_dir),
        before,
        "junction 指向的真实目录内容不得改变"
    );
    assert_no_temp_leftovers(dir.path());

    // 显式移除 junction 本身（不递归进入其指向的真实目录）。
    let _ = std::fs::remove_dir(&junction);
}

// ---------------------------------------------------------------------------
// F-3：提交前目标被创建（需要 test-hooks）
// ---------------------------------------------------------------------------
#[cfg(feature = "test-hooks")]
#[test]
fn save_race_target_created_before_commit() {
    use ai_dev_cockpit_lib::commands::report::test_hooks;

    /// 无论断言是否 panic，都要清除全局钩子，避免污染同进程并行运行的用例。
    struct HookReset;
    impl Drop for HookReset {
        fn drop(&mut self) {
            test_hooks::set_before_commit(None);
        }
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("race.json");
    let hooked_target = target.clone();
    test_hooks::set_before_commit(Some(Arc::new(move |path: &Path| {
        // 只干预本用例的目标路径（其它用例可能同时在别的目录保存）。
        if path == hooked_target.as_path() {
            std::fs::write(path, b"COMPETITOR").expect("竞争者写入");
        }
    })));
    let _reset = HookReset;

    let error = save_report_atomic(&target, "OURS").expect_err("提交前目标被创建必须失败");
    assert!(
        matches!(&error, SaveError::TargetExists),
        "应返回 TargetExists，实际 {error:?}"
    );
    assert_eq!(
        std::fs::read(&target).expect("读回"),
        b"COMPETITOR",
        "竞争者写入的内容必须保持完整"
    );
    assert_no_temp_leftovers(dir.path());
}

// ---------------------------------------------------------------------------
// F-4：8 线程屏障同步并发，恰好 1 个成功
// ---------------------------------------------------------------------------

#[test]
fn save_8_concurrent_exactly_one_wins() {
    const ROUNDS: usize = 200;
    const THREADS: usize = 8;

    let dir = tempfile::tempdir().expect("tempdir");
    for round in 0..ROUNDS {
        let target = dir.path().join(format!("race-{round:03}.json"));
        let barrier = Arc::new(Barrier::new(THREADS));
        let mut handles = Vec::with_capacity(THREADS);

        for thread_id in 0..THREADS {
            let target = target.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                let payload = format!("PAYLOAD-{thread_id}-{round}");
                barrier.wait();
                let outcome = save_report_atomic(&target, &payload);
                (payload, outcome)
            }));
        }

        let results: Vec<(String, Result<(), SaveError>)> = handles
            .into_iter()
            .map(|handle| handle.join().expect("线程不得 panic"))
            .collect();

        let winners: Vec<&String> = results
            .iter()
            .filter(|(_, outcome)| outcome.is_ok())
            .map(|(payload, _)| payload)
            .collect();
        let target_exists = results
            .iter()
            .filter(|(_, outcome)| matches!(outcome, Err(SaveError::TargetExists)))
            .count();

        assert_eq!(
            winners.len(),
            1,
            "第 {round} 轮：必须恰好 1 个成功，实际成功 {} 个",
            winners.len()
        );
        assert_eq!(
            target_exists,
            THREADS - 1,
            "第 {round} 轮：其余 {} 个必须是 TargetExists",
            THREADS - 1
        );

        let written = std::fs::read(&target).expect("读回最终文件");
        assert_eq!(
            hex(&sha256(&written)),
            hex(&sha256(winners[0].as_bytes())),
            "第 {round} 轮：最终文件 SHA-256 必须等于获胜线程载荷"
        );
        assert_no_temp_leftovers(dir.path());
    }
}

// ---------------------------------------------------------------------------
// F-6 / F-7：失败只清理本事务的临时文件
// ---------------------------------------------------------------------------

#[test]
fn save_failure_cleans_only_own_temp() {
    let dir = tempfile::tempdir().expect("tempdir");
    let existing = dir.path().join("existing.json");
    std::fs::write(&existing, b"KEEP").expect("预置用户文件");
    let subdir = dir.path().join("subdir");
    std::fs::create_dir(&subdir).expect("预置目录");
    let fresh = dir.path().join("fresh.json");

    // 三种事务：目标已存在文件 / 目标是目录 / 正常新建。
    let error = save_report_atomic(&existing, "X").expect_err("已存在文件必须拒绝");
    assert!(matches!(&error, SaveError::TargetExists), "实际 {error:?}");
    let error = save_report_atomic(&subdir, "X").expect_err("目录目标必须拒绝");
    assert!(matches!(&error, SaveError::TargetExists), "实际 {error:?}");
    save_report_atomic(&fresh, "FRESH").expect("新文件应保存成功");

    // 目录清单精确匹配：没有残留临时文件，也没有任何既有对象被删除/改名。
    assert_eq!(
        sorted_names(dir.path()),
        vec![
            "existing.json".to_string(),
            "fresh.json".to_string(),
            "subdir".to_string()
        ],
        "除新增 fresh.json 外，目录内容必须完全不变"
    );
    assert_eq!(
        std::fs::read(&existing).expect("读回"),
        b"KEEP",
        "既有文件不得被删除或改动"
    );
    assert_eq!(std::fs::read(&fresh).expect("读回"), b"FRESH");
    assert_no_temp_leftovers(dir.path());
}

// ---------------------------------------------------------------------------
// F-8：保存字节 == 预览字节
// ---------------------------------------------------------------------------

#[test]
fn saved_bytes_equal_preview_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let run = sample_run("0f0e0d0c-0b0a-4009-8007-060504030201");

    for (format, file_name) in [
        (ReportFormat::Json, "preview.json"),
        (ReportFormat::Markdown, "preview.md"),
    ] {
        let (preview, _filename) = build_report(&run, format);
        let path = dir.path().join(file_name);
        save_report_atomic(&path, &preview).expect("保存成功");
        let saved = std::fs::read(&path).expect("读回保存文件");
        assert_eq!(saved.len(), preview.len(), "{format:?}: 长度必须一致");
        assert_eq!(
            hex(&sha256(&saved)),
            hex(&sha256(preview.as_bytes())),
            "{format:?}: SHA-256 必须与预览字节一致"
        );
    }
    assert_no_temp_leftovers(dir.path());
}

// ---------------------------------------------------------------------------
// F-9：TargetExists 后快照仍可复用（换名重试，字节不变）
// ---------------------------------------------------------------------------

#[test]
fn snapshot_retry_after_target_exists() {
    let dir = tempfile::tempdir().expect("tempdir");
    let run = sample_run("0f0e0d0c-0b0a-4009-8007-060504030201");
    let (preview, filename) = build_report(&run, ReportFormat::Json);

    let mut store = SnapshotStore::default();
    store
        .insert(ReportSnapshot {
            snapshot_id: "snap-retry".to_string(),
            run_id: run.run_id.clone(),
            format: ReportFormat::Json,
            content: std::sync::Arc::from(preview.as_str()),
            filename,
            created_at: std::time::Instant::now(),
            state: SnapshotState::Available,
        })
        .expect("插入快照");

    let taken = store
        .take("snap-retry", &run.run_id, ReportFormat::Json)
        .expect("取用快照");
    assert_eq!(taken.state, SnapshotState::InFlight);

    // 第一次：目标已存在 → TargetExists（IPC 层同样会调用 finish_failure）。
    let occupied = dir.path().join("occupied.json");
    std::fs::write(&occupied, b"USER DATA").expect("预置用户文件");
    let error = save_report_atomic(&occupied, &taken.content).expect_err("已存在必须拒绝");
    assert!(matches!(&error, SaveError::TargetExists), "实际 {error:?}");
    store
        .finish_failure("snap-retry", None)
        .expect("状态回退应成功");

    assert_eq!(store.available_count(), 1, "快照必须回到 Available 可重试");
    assert_eq!(store.consumed_count(), 0, "失败不得计为已消费");

    // 第二次：换名保存成功，字节仍与首次预览一致。
    let retry = store
        .take("snap-retry", &run.run_id, ReportFormat::Json)
        .expect("重试取用快照");
    let renamed = dir.path().join("renamed.json");
    save_report_atomic(&renamed, &retry.content).expect("换名后应保存成功");
    assert_eq!(
        std::fs::read(&renamed).expect("读回"),
        preview.as_bytes(),
        "重试保存的字节必须与预览一致"
    );
    store.finish_success("snap-retry").expect("标记成功");

    assert_eq!(store.available_count(), 0, "成功后快照失效（Consumed）");
    assert_eq!(store.consumed_count(), 1);
    assert_eq!(
        std::fs::read(&occupied).expect("读回"),
        b"USER DATA",
        "用户既有文件必须保持原样"
    );
    assert_no_temp_leftovers(dir.path());
}
