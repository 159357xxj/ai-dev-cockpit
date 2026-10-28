//! V6-R6：导出 DTO 的**库结构驱动**金丝雀审计。
//!
//! 目标：不再依赖"记得给某个字段写用例"，而是让数据库结构自己决定测什么——
//! 枚举隔离 SQLite 中所有表的所有 TEXT 列，逐列把数据改成金丝雀，然后走
//! 生产链路 `Storage::get_run → build_report → save_report_atomic`，检查
//! JSON/Markdown 预览与落盘文件是否泄漏。
//!
//! 每轮只接受三种结果：
//! - `READ_REJECTED`：`get_run` 返回错误（该列是结构/枚举约束的一部分）；
//! - `BUILD_REJECTED`：报告构建返回明确错误（当前 `build_report` 设计上不可失败，
//!   因此该分支为 0——导出采用"白名单替换"而非报错，见测试末尾汇总）；
//! - `CLEAN`：导出成功且四份输出都不含 `CANARY`（不区分大小写）。
//!
//! 约束：仅使用 `tempfile::tempdir()` 中的隔离数据库与隔离输出目录。

use std::path::Path;

use ai_dev_cockpit_lib::commands::report::{ReportFormat, build_report, save_report_atomic};
use ai_dev_cockpit_lib::diagnostic::types::{
    DiagnosticError, DiagnosticRun, ErrorKind, HttpOutcome, ProbeAggregate, ProbeSample, ProbeType,
    RouteMode, RunStatus, TargetResult,
};
use ai_dev_cockpit_lib::storage::Storage;

/// 基线运行的固定 id（合法 UUID v4，两边一致）。
const BASELINE_RUN_ID: &str = "0f0e0d0c-0b0a-4009-8007-060504030201";
/// 金丝雀标记片段（大小写不敏感匹配）。
const CANARY: &str = "CANARY";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    ReadRejected,
    /// 当前 `build_report` 不会失败（不可失败签名），保留该类别以对齐审计口径。
    #[allow(dead_code)]
    BuildRejected,
    Clean,
}

impl Outcome {
    fn label(self) -> &'static str {
        match self {
            Outcome::ReadRejected => "READ_REJECTED",
            Outcome::BuildRejected => "BUILD_REJECTED",
            Outcome::Clean => "CLEAN",
        }
    }
}

// ---------------------------------------------------------------------------
// 基线数据：尽量填满所有可选字段
// ---------------------------------------------------------------------------

/// 构造一条"中性"样本；调用点用结构体更新语法覆盖需要变化的字段
/// （避免十参数助手触发 `clippy::too_many_arguments`）。
fn sample(
    target_id: &str,
    probe_type: ProbeType,
    route_mode: RouteMode,
    attempt_no: u32,
    tested_at: &str,
) -> ProbeSample {
    ProbeSample {
        target_id: target_id.to_string(),
        probe_type,
        route_mode,
        attempt_no,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: None,
        tested_at: tested_at.to_string(),
    }
}

fn baseline_run() -> DiagnosticRun {
    let github_samples = vec![
        ProbeSample {
            success: true,
            duration_ms: Some(12),
            ..sample(
                "github",
                ProbeType::Dns,
                RouteMode::ManualLocalProxy,
                1,
                "2026-10-25T10:00:01Z",
            )
        },
        ProbeSample {
            success: true,
            duration_ms: Some(31),
            ..sample(
                "github",
                ProbeType::Tcp,
                RouteMode::ManualLocalProxy,
                1,
                "2026-10-25T10:00:02Z",
            )
        },
        ProbeSample {
            success: true,
            duration_ms: Some(210),
            http_status: Some(200),
            http_outcome: Some(HttpOutcome::Ok2xx),
            ..sample(
                "github",
                ProbeType::Https,
                RouteMode::ManualLocalProxy,
                1,
                "2026-10-25T10:00:03Z",
            )
        },
        ProbeSample {
            duration_ms: Some(180),
            http_status: Some(401),
            http_outcome: Some(HttpOutcome::AuthRequired401To403),
            ..sample(
                "github",
                ProbeType::Https,
                RouteMode::ManualLocalProxy,
                2,
                "2026-10-25T10:00:04Z",
            )
        },
    ];
    let openai_samples = vec![
        ProbeSample {
            error: Some(DiagnosticError {
                kind: ErrorKind::Timeout,
                summary: "请求超时（本地代理 127.0.0.1:7890 无响应）".to_string(),
                detail: Some("cfg=C:\\Users\\baseline\\clash.yaml".to_string()),
            }),
            ..sample(
                "openai-api",
                ProbeType::Https,
                RouteMode::NoAppProxy,
                1,
                "2026-10-25T10:00:05Z",
            )
        },
        ProbeSample {
            error: Some(DiagnosticError {
                kind: ErrorKind::DnsFailure,
                summary: "no such host".to_string(),
                detail: None,
            }),
            ..sample(
                "openai-api",
                ProbeType::Dns,
                RouteMode::NoAppProxy,
                1,
                "2026-10-25T10:00:06Z",
            )
        },
    ];

    let targets = vec![
        TargetResult {
            target_id: "github".to_string(),
            aggregates: vec![
                ProbeAggregate::from_samples(ProbeType::Dns, &github_samples),
                ProbeAggregate::from_samples(ProbeType::Tcp, &github_samples),
                ProbeAggregate::from_samples(ProbeType::Https, &github_samples),
            ],
            samples: github_samples,
        },
        TargetResult {
            target_id: "openai-api".to_string(),
            aggregates: vec![
                ProbeAggregate::from_samples(ProbeType::Dns, &openai_samples),
                ProbeAggregate::from_samples(ProbeType::Https, &openai_samples),
            ],
            samples: openai_samples,
        },
    ];

    DiagnosticRun {
        run_id: BASELINE_RUN_ID.to_string(),
        started_at: "2026-10-25T10:00:00Z".to_string(),
        finished_at: Some("2026-10-25T10:00:09Z".to_string()),
        status: RunStatus::Completed,
        route_mode: RouteMode::ManualLocalProxy,
        repeats: 3,
        app_version: "0.1.0".to_string(),
        targets,
    }
}

// ---------------------------------------------------------------------------
// 结构枚举与 JSON 处理
// ---------------------------------------------------------------------------

/// 枚举所有表的所有 TEXT 列（不写死列名，新增列自动纳入）。
fn text_columns(connection: &rusqlite::Connection) -> Vec<(String, String)> {
    let mut statement = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .expect("查询表清单");
    let tables: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("读取表清单")
        .collect::<Result<Vec<_>, _>>()
        .expect("收集表名");

    let mut columns = Vec::new();
    for table in tables {
        let mut info = connection
            .prepare(&format!("PRAGMA table_info(\"{table}\")"))
            .expect("PRAGMA table_info");
        let rows = info
            .query_map([], |row| {
                let name: String = row.get(1)?;
                let kind: String = row.get(2)?;
                Ok((name, kind))
            })
            .expect("读取列信息");
        for row in rows {
            let (name, kind) = row.expect("列信息行");
            if kind.eq_ignore_ascii_case("TEXT") {
                columns.push((table.clone(), name));
            }
        }
    }
    columns
}

/// 读取该列的一个非空样本值，用于判断它是不是 JSON 列。
fn column_sample(connection: &rusqlite::Connection, table: &str, column: &str) -> Option<String> {
    connection
        .query_row(
            &format!("SELECT \"{column}\" FROM \"{table}\" WHERE \"{column}\" IS NOT NULL LIMIT 1"),
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
}

/// 把所有 JSON **字符串叶子值**替换成标记，保持结构与键名不变。
fn replace_string_leaves(value: &serde_json::Value, marker: &str) -> serde_json::Value {
    match value {
        serde_json::Value::String(_) => serde_json::Value::String(marker.to_string()),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| replace_string_leaves(item, marker))
                .collect(),
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), replace_string_leaves(item, marker)))
                .collect(),
        ),
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------
// 单轮：复制基线库 → UPDATE 一列 → 生产读取/构建/保存 → 检查四份输出
// ---------------------------------------------------------------------------

fn run_case(
    baseline: &Path,
    root: &Path,
    index: usize,
    table: &str,
    column: &str,
    injected: &str,
) -> Outcome {
    let db = root.join(format!("round-{index:03}.db"));
    std::fs::copy(baseline, &db).expect("复制基线数据库");

    {
        let connection = rusqlite::Connection::open(&db).expect("打开数据库副本");
        // 审计目的是"库里已有脏数据时导出会不会泄漏"，因此关闭外键约束以允许
        // 单独篡改任意列（生产读取侧不做外键修复，也不需要）。
        connection
            .execute_batch("PRAGMA foreign_keys = OFF;")
            .expect("关闭外键约束");
        connection
            .execute(
                &format!("UPDATE \"{table}\" SET \"{column}\" = ?1"),
                [injected],
            )
            .expect("写入金丝雀");
    }

    let storage = Storage::open(&db).expect("打开副本存储");
    let run = match storage.get_run(BASELINE_RUN_ID) {
        Ok(run) => run,
        Err(_) => return Outcome::ReadRejected,
    };
    drop(storage);

    let (json_preview, json_name) = build_report(&run, ReportFormat::Json);
    let (md_preview, md_name) = build_report(&run, ReportFormat::Markdown);

    let out = root.join(format!("out-{index:03}"));
    std::fs::create_dir_all(&out).expect("创建输出目录");
    let json_path = out.join(&json_name);
    let md_path = out.join(&md_name);
    save_report_atomic(&json_path, &json_preview).expect("保存 JSON 报告");
    save_report_atomic(&md_path, &md_preview).expect("保存 Markdown 报告");
    let json_file = std::fs::read_to_string(&json_path).expect("读回 JSON 报告");
    let md_file = std::fs::read_to_string(&md_path).expect("读回 Markdown 报告");

    for (label, output) in [
        ("JSON 预览", &json_preview),
        ("Markdown 预览", &md_preview),
        ("JSON 文件", &json_file),
        ("Markdown 文件", &md_file),
    ] {
        assert!(
            !output.to_lowercase().contains(&CANARY.to_lowercase()),
            "表 {table} 列 {column} 的金丝雀进入了{label}（既未拒绝也未净化）"
        );
    }
    // 预览与文件必须是同一份字节（F-8 的上游对应项）。
    assert_eq!(json_preview, json_file, "{table}.{column}: JSON 预览≠文件");
    assert_eq!(md_preview, md_file, "{table}.{column}: Markdown 预览≠文件");

    Outcome::Clean
}

// ---------------------------------------------------------------------------
// 主审计
// ---------------------------------------------------------------------------

#[test]
fn export_dto_column_canary_audit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let baseline = dir.path().join("baseline.db");

    // 1) 用正式 Storage API 写入一条字段尽量填满的基线记录（含一条 settings 记录，
    //    使 settings 表的两个 TEXT 列也有真实数据可替换）。
    {
        let storage = Storage::open(&baseline).expect("打开基线数据库");
        storage.save_run(&baseline_run()).expect("写入基线运行");
        storage
            .set_setting(
                "retention.max_runs",
                r#"{"max_runs":500,"max_age_days":30,"origin":"baseline"}"#,
            )
            .expect("写入基线设置");
    }

    // 2) 枚举所有表的所有 TEXT 列。
    let connection = rusqlite::Connection::open(&baseline).expect("打开基线库以枚举结构");
    let columns = text_columns(&connection);
    assert!(!columns.is_empty(), "应至少枚举到一个 TEXT 列");

    // 3) 逐列（以及 JSON 列的额外一轮）做金丝雀注入。
    let mut results: Vec<(String, Outcome)> = Vec::new();
    let mut index = 0usize;
    for (table, column) in &columns {
        index += 1;
        let marker = format!("{CANARY}_{table}_{column}");
        let outcome = run_case(&baseline, dir.path(), index, table, column, &marker);
        results.push((format!("{table}.{column}"), outcome));

        // 4) 如果该列存的是 JSON，再额外做一轮：字符串叶子值全部替换。
        let Some(raw) = column_sample(&connection, table, column) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        if !(parsed.is_object() || parsed.is_array()) {
            continue;
        }
        index += 1;
        let json_marker = format!("{CANARY}_{table}_{column}_JSON");
        let mutated = replace_string_leaves(&parsed, &json_marker);
        let serialized = serde_json::to_string(&mutated).expect("序列化变异 JSON");
        let outcome = run_case(&baseline, dir.path(), index, table, column, &serialized);
        results.push((format!("{table}.{column} (JSON)"), outcome));
    }

    // 5) 汇总：每个 表.列 一行。
    println!(
        "---- V6-R6 导出字段列级金丝雀汇总（共 {} 轮）----",
        results.len()
    );
    for (name, outcome) in &results {
        println!("{name} => {}", outcome.label());
    }
    let read_rejected = results
        .iter()
        .filter(|(_, outcome)| *outcome == Outcome::ReadRejected)
        .count();
    let build_rejected = results
        .iter()
        .filter(|(_, outcome)| *outcome == Outcome::BuildRejected)
        .count();
    let clean = results
        .iter()
        .filter(|(_, outcome)| *outcome == Outcome::Clean)
        .count();
    println!(
        "---- 合计：CLEAN {clean} / READ_REJECTED {read_rejected} / BUILD_REJECTED {build_rejected} ----"
    );
    println!(
        "---- 说明：build_report 为不可失败签名，导出采用白名单替换而非报错，故 BUILD_REJECTED 恒为 0 ----"
    );

    assert!(
        results
            .iter()
            .all(|(_, outcome)| *outcome != Outcome::BuildRejected),
        "BUILD_REJECTED 出现即说明构建签名发生变化，需人工确认"
    );
}

/// 补充用例：`runs.id` 列在列级审计中会因"按原 id 查找失败"而 READ_REJECTED，
/// 本用例改按金丝雀 id 读取，证明 run_id 本身即使来自数据库也不会被原样导出。
#[test]
fn run_id_canary_in_database_is_not_exported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("run-id-canary.db");
    let canary_run_id = "CANARY_runs_id";

    {
        let storage = Storage::open(&db).expect("打开数据库");
        let mut run = baseline_run();
        run.run_id = canary_run_id.to_string();
        storage.save_run(&run).expect("写入金丝雀 run_id 记录");
    }

    let storage = Storage::open(&db).expect("重新打开数据库");
    let loaded = storage
        .get_run(canary_run_id)
        .expect("按金丝雀 id 读取运行");
    let (json_preview, json_name) = build_report(&loaded, ReportFormat::Json);
    let (md_preview, md_name) = build_report(&loaded, ReportFormat::Markdown);

    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).expect("创建输出目录");
    let json_path = out.join(&json_name);
    let md_path = out.join(&md_name);
    save_report_atomic(&json_path, &json_preview).expect("保存 JSON");
    save_report_atomic(&md_path, &md_preview).expect("保存 Markdown");
    let json_file = std::fs::read_to_string(&json_path).expect("读回 JSON");
    let md_file = std::fs::read_to_string(&md_path).expect("读回 Markdown");

    for (label, output) in [
        ("JSON 预览", &json_preview),
        ("Markdown 预览", &md_preview),
        ("JSON 文件", &json_file),
        ("Markdown 文件", &md_file),
    ] {
        assert!(
            !output.to_lowercase().contains(&CANARY.to_lowercase()),
            "run_id 金丝雀进入了{label}"
        );
        assert!(
            output.contains("invalid-run-id"),
            "{label} 应把非法 run_id 显示为 invalid-run-id"
        );
    }
    assert_eq!(json_preview, json_file);
    assert_eq!(md_preview, md_file);
    println!("runs.id（按金丝雀 id 读取）=> CLEAN（run_id 归一为 invalid-run-id）");
}

/// V7-T2-APP-VERSION 的直接回归：Codex 的三个最小复现值都满足旧的
/// "字符集 + 长度"规则，现在必须被版本白名单拒绝。
#[test]
fn app_version_whitelist_rejects_v7_canaries() {
    let current_version = env!("CARGO_PKG_VERSION");

    for canary in [
        "sk-CANARY0000000000",
        "canary-a1.corp.internal",
        "10.13.37.42",
    ] {
        let mut run = baseline_run();
        run.app_version = canary.to_string();

        let (json, json_name) = build_report(&run, ReportFormat::Json);
        let (md, md_name) = build_report(&run, ReportFormat::Markdown);

        let dir = tempfile::tempdir().expect("tempdir");
        let json_path = dir.path().join(&json_name);
        let md_path = dir.path().join(&md_name);
        save_report_atomic(&json_path, &json).expect("保存 JSON");
        save_report_atomic(&md_path, &md).expect("保存 Markdown");
        let json_file = std::fs::read_to_string(&json_path).expect("读回 JSON");
        let md_file = std::fs::read_to_string(&md_path).expect("读回 Markdown");

        for (label, output) in [
            ("JSON 预览", &json),
            ("Markdown 预览", &md),
            ("JSON 文件", &json_file),
            ("Markdown 文件", &md_file),
        ] {
            assert!(
                !output.to_lowercase().contains("canary"),
                "{label} 泄漏 app_version 金丝雀：{canary}"
            );
            assert!(!output.contains("sk-"), "{label} 泄漏假凭证前缀");
            assert!(!output.contains("10.13.37"), "{label} 泄漏私有地址");
            // 元数据版本固定为编译期常量。
            assert!(
                output.contains(current_version),
                "{label} 应显示编译期版本 {current_version}"
            );
        }
        // 记录中的版本号经白名单落到固定占位符（仅 JSON 携带运行级版本）。
        assert!(
            json.contains("\"app_version\": \"unknown-version\""),
            "运行记录的未知版本应显示 unknown-version：{json}"
        );
        assert_eq!(json, json_file, "预览与文件必须一致");
        assert_eq!(md, md_file, "预览与文件必须一致");
    }
    println!(
        "app_version（V7 三个最小复现值）=> CLEAN（metadata 用编译期常量，记录值 → unknown-version）"
    );
}
