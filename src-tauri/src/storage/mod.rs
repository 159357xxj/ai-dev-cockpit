//! SQLite 存储层（Phase 5）：runs / results / settings 三表 + 版本化迁移。
//!
//! 边界（PRD §6 / AGENTS.md）：
//! - 后端独占数据库写入；前端只能经白名单 IPC，绝不执行任意 SQL；
//! - 全部查询参数化（SQL 注入面为零）；
//! - 时间一律 UTC RFC3339 存储；
//! - 事务写入保证 "completed 状态的 run 必含全部已采样本"；
//! - 保留策略：默认 30 天或 500 个 run（settings 表可覆盖），启动与每次写入后执行。

pub mod migrations;
pub mod models;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use self::models::{RunRow, SampleRow, result_row_from_sample, run_row_from_run};
use crate::diagnostic::types::{
    DiagnosticRun, IpcError, IpcErrorCode, ProbeAggregate, ProbeSample, ProbeType, RouteMode,
    TargetResult,
};

pub const DEFAULT_RETENTION_MAX_RUNS: u32 = 500;
pub const DEFAULT_RETENTION_MAX_AGE_DAYS: u32 = 30;

/// 线程安全的 SQLite 句柄（命令层共享）。
pub struct Storage {
    connection: Mutex<Connection>,
}

impl Storage {
    /// 打开（或创建）数据库并迁移到最新版本。
    pub fn open(path: &Path) -> Result<Self, String> {
        let connection =
            Connection::open(path).map_err(|error| format!("打开数据库失败：{error}"))?;
        Self::initialize(connection)
    }

    /// 内存数据库（测试用）。
    pub fn open_in_memory() -> Result<Self, String> {
        let connection = Connection::open_in_memory().map_err(|error| format!("{error}"))?;
        Self::initialize(connection)
    }

    fn initialize(mut connection: Connection) -> Result<Self, String> {
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| format!("WAL 设置失败：{error}"))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| format!("外键设置失败：{error}"))?;
        migrations::apply(&mut connection)?;
        // F-07：清理历史版本经 settings 表传递快照 id 的遗留键（一次性、幂等；
        // 快照现已纯内存管理，不再写 settings）。
        let _ = connection.execute(
            "DELETE FROM settings WHERE key LIKE 'report.snapshot.%'",
            [],
        );
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, String>,
    ) -> Result<T, IpcError> {
        let connection = self
            .connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        operation(&connection).map_err(|message| {
            IpcError::new(IpcErrorCode::Internal, format!("存储操作失败：{message}"))
        })
    }

    // ---- 写入：run + samples 单事务 ----

    /// 事务写入一次运行及其全部样本；随后执行保留策略。
    pub fn save_run(&self, run: &DiagnosticRun) -> Result<(), IpcError> {
        self.with_connection(|connection| {
            let transaction = connection
                .unchecked_transaction()
                .map_err(|error| error.to_string())?;
            let run_row = run_row_from_run(run);
            transaction
                .execute(
                    "INSERT OR REPLACE INTO runs (id, started_at, finished_at, status, route_mode, repeats, app_version, notes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        run_row.id,
                        run_row.started_at,
                        run_row.finished_at,
                        run_row.status,
                        run_row.route_mode,
                        run_row.repeats,
                        run_row.app_version,
                        run_row.notes,
                    ],
                )
                .map_err(|error| error.to_string())?;
            // 样本整体替换（同 run 重写时保持幂等）。
            transaction
                .execute("DELETE FROM results WHERE run_id = ?1", [&run_row.id])
                .map_err(|error| error.to_string())?;
            for sample in run.targets.iter().flat_map(|target| target.samples.iter()) {
                let row = result_row_from_sample(&run_row.id, sample);
                transaction
                    .execute(
                        "INSERT INTO results (run_id, target_id, probe_type, route_mode, attempt_no,
                            success, duration_ms, http_status, http_outcome, error_kind, error_summary, error_detail, tested_at)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                        rusqlite::params![
                            row.run_id,
                            row.target_id,
                            row.probe_type,
                            row.route_mode,
                            row.attempt_no,
                            row.success,
                            row.duration_ms.map(|value| value as i64),
                            row.http_status,
                            row.http_outcome,
                            row.error_kind,
                            row.error_summary,
                            row.error_detail,
                            row.tested_at,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
            }
            transaction
                .commit()
                .map_err(|error| error.to_string())
        })?;
        self.enforce_retention()
    }

    // ---- 读取 ----

    /// 查询运行摘要（参数化过滤，最新在前，limit 上限 500）。
    pub fn list_runs(
        &self,
        target_id: Option<&str>,
        route_mode: Option<RouteMode>,
        limit: u32,
    ) -> Result<Vec<RunSummary>, IpcError> {
        let limit = limit.clamp(1, 500);
        self.with_connection(|connection| {
            let mut sql = String::from(
                "SELECT r.id, r.started_at, r.finished_at, r.status, r.route_mode, r.repeats, r.app_version
                 FROM runs r",
            );
            if target_id.is_some() {
                sql.push_str(
                    " WHERE r.id IN (SELECT DISTINCT run_id FROM results WHERE target_id = ?1)",
                );
            }
            if route_mode.is_some() {
                sql.push_str(if target_id.is_some() {
                    " AND r.route_mode = ?2"
                } else {
                    " WHERE r.route_mode = ?2"
                });
            }
            sql.push_str(" ORDER BY r.started_at DESC, r.id DESC LIMIT ?3");

            let mode_string = route_mode
                .map(|mode| serde_json::to_string(&mode).unwrap().trim_matches('"').to_string());
            let mut statement = connection
                .prepare(&sql)
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(
                    rusqlite::params![
                        target_id.unwrap_or(""),
                        mode_string.clone().unwrap_or_default(),
                        limit,
                    ],
                    |row| {
                        Ok(RunRow {
                            id: row.get(0)?,
                            started_at: row.get(1)?,
                            finished_at: row.get(2)?,
                            status: row.get(3)?,
                            route_mode: row.get(4)?,
                            repeats: row.get(5)?,
                            app_version: row.get(6)?,
                            notes: None,
                        })
                    },
                )
                .map_err(|error| error.to_string())?;

            let mut summaries = Vec::new();
            for row in rows {
                let run_row = row.map_err(|error| error.to_string())?;
                summaries.push(Self::summary_for(connection, &run_row)?);
            }
            Ok(summaries)
        })
    }

    /// 组装单个 run 的摘要统计（目标列表 / 样本数 / 成功数 / 传输失败数 / HTTPS 中位耗时）。
    fn summary_for(connection: &Connection, run_row: &RunRow) -> Result<RunSummary, String> {
        let targets: Vec<String> = {
            let mut statement = connection
                .prepare(
                    "SELECT DISTINCT target_id FROM results WHERE run_id = ?1 ORDER BY target_id",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([&run_row.id], |row| row.get::<_, String>(0))
                .map_err(|error| error.to_string())?;
            rows.collect::<Result<_, _>>()
                .map_err(|error| error.to_string())?
        };
        let (sample_count, success_count, transport_failure_count): (u32, u32, u32) = {
            let mut statement = connection
                .prepare(
                    "SELECT COUNT(*),
                            COALESCE(SUM(success), 0),
                            COALESCE(SUM(CASE WHEN error_kind IS NOT NULL AND error_kind != 'cancelled' THEN 1 ELSE 0 END), 0)
                     FROM results WHERE run_id = ?1",
                )
                .map_err(|error| error.to_string())?;
            statement
                .query_row([&run_row.id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .map_err(|error| error.to_string())?
        };
        let https_median_ms: Option<u64> = {
            let mut statement = connection
                .prepare(
                    // 中位耗时：仅"成功且收到响应"的 HTTPS 样本（与 D-017 展示口径一致）。
                    "SELECT duration_ms FROM results
                     WHERE run_id = ?1 AND probe_type = 'https' AND success = 1 AND duration_ms IS NOT NULL
                     ORDER BY duration_ms",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([&run_row.id], |row| row.get::<_, Option<i64>>(0))
                .map_err(|error| error.to_string())?;
            let durations: Vec<u64> = rows
                .collect::<Result<Vec<Option<i64>>, _>>()
                .map_err(|error| error.to_string())?
                .into_iter()
                .flatten()
                .map(|value| value as u64)
                .collect();
            match durations.len() {
                0 => None,
                n if n % 2 == 1 => Some(durations[n / 2]),
                n => Some((durations[n / 2 - 1] + durations[n / 2]) / 2),
            }
        };
        Ok(RunSummary {
            run_id: run_row.id.clone(),
            started_at: run_row.started_at.clone(),
            finished_at: run_row.finished_at.clone(),
            status: run_row.status.clone(),
            route_mode: run_row.route_mode.clone(),
            repeats: run_row.repeats,
            app_version: run_row.app_version.clone(),
            target_ids: targets,
            sample_count,
            success_count,
            transport_failure_count,
            https_median_ms,
        })
    }

    /// 读取完整运行（含样本与重算聚合）。
    pub fn get_run(&self, run_id: &str) -> Result<DiagnosticRun, IpcError> {
        self.with_connection(|connection| {
            let run_row = connection
                .query_row(
                    "SELECT id, started_at, finished_at, status, route_mode, repeats, app_version, notes
                     FROM runs WHERE id = ?1",
                    [run_id],
                    |row| {
                        Ok(RunRow {
                            id: row.get(0)?,
                            started_at: row.get(1)?,
                            finished_at: row.get(2)?,
                            status: row.get(3)?,
                            route_mode: row.get(4)?,
                            repeats: row.get(5)?,
                            app_version: row.get(6)?,
                            notes: row.get(7)?,
                        })
                    },
                )
                .map_err(|_| format!("运行不存在：{run_id}"))?;

            let mut statement = connection
                .prepare(
                    "SELECT target_id, probe_type, route_mode, attempt_no, success, duration_ms,
                            http_status, http_outcome, error_kind, error_summary, error_detail, tested_at
                     FROM results WHERE run_id = ?1 ORDER BY target_id, probe_type, attempt_no",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([run_id], |row| {
                    Ok(SampleRow {
                        target_id: row.get(0)?,
                        probe_type: row.get(1)?,
                        route_mode: row.get(2)?,
                        attempt_no: row.get(3)?,
                        success: row.get(4)?,
                        duration_ms: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                        http_status: row.get(6)?,
                        http_outcome: row.get(7)?,
                        error_kind: row.get(8)?,
                        error_summary: row.get(9)?,
                        error_detail: row.get(10)?,
                        tested_at: row.get(11)?,
                    })
                })
                .map_err(|error| error.to_string())?;

            let mut by_target: std::collections::BTreeMap<String, Vec<ProbeSample>> =
                std::collections::BTreeMap::new();
            for row in rows {
                let row = row.map_err(|error| error.to_string())?;
                let sample = row
                    .to_sample()
                    .map_err(|error| format!("样本反序列化失败：{error}"))?;
                by_target
                    .entry(sample.target_id.clone())
                    .or_default()
                    .push(sample);
            }

            let targets: Vec<TargetResult> = by_target
                .into_iter()
                .map(|(target_id, samples)| {
                    let mut aggregates = Vec::new();
                    for probe_type in [ProbeType::Dns, ProbeType::Tcp, ProbeType::Https] {
                        let subset: Vec<ProbeSample> = samples
                            .iter()
                            .filter(|sample| sample.probe_type == probe_type)
                            .cloned()
                            .collect();
                        if !subset.is_empty() {
                            aggregates.push(ProbeAggregate::from_samples(probe_type, &subset));
                        }
                    }
                    TargetResult {
                        target_id,
                        samples,
                        aggregates,
                    }
                })
                .collect();

            run_row
                .to_run(targets)
                .map_err(|error| format!("运行反序列化失败：{error}"))
        })
    }

    // ---- 删除 ----

    pub fn delete_run(&self, run_id: &str) -> Result<bool, IpcError> {
        self.with_connection(|connection| {
            let deleted = connection
                .execute("DELETE FROM runs WHERE id = ?1", [run_id])
                .map_err(|error| error.to_string())?;
            Ok(deleted > 0)
        })
    }

    pub fn clear_history(&self) -> Result<u32, IpcError> {
        self.with_connection(|connection| {
            let deleted = connection
                .execute("DELETE FROM results", [])
                .map_err(|error| error.to_string())? as u32;
            connection
                .execute("DELETE FROM runs", [])
                .map_err(|error| error.to_string())?;
            Ok(deleted)
        })
    }

    pub fn run_count(&self) -> Result<u32, IpcError> {
        self.with_connection(|connection| {
            let count: u32 = connection
                .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            Ok(count)
        })
    }

    // ---- 设置（仅白名单键；不存储任何凭证） ----

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, IpcError> {
        self.with_connection(|connection| {
            let value = connection
                .query_row(
                    "SELECT value_json FROM settings WHERE key = ?1",
                    [key],
                    |row| row.get::<_, String>(0),
                )
                .ok();
            Ok(value)
        })
    }

    /// 删除设置键（内部使用：导出后清理一次性快照 id 记录）。
    pub fn delete_setting(&self, key: &str) -> Result<(), IpcError> {
        self.with_connection(|connection| {
            connection
                .execute("DELETE FROM settings WHERE key = ?1", [key])
                .map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    pub fn set_setting(&self, key: &str, value_json: &str) -> Result<(), IpcError> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
                    [key, value_json],
                )
                .map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    // ---- 保留策略 ----

    /// 30 天或 max_runs（settings 可覆盖）：先按时间裁剪，再按数量裁剪（最新优先保留）。
    pub fn enforce_retention(&self) -> Result<(), IpcError> {
        let max_age_days = self
            .retention_setting("retention.max_age_days")
            .unwrap_or(DEFAULT_RETENTION_MAX_AGE_DAYS);
        let max_runs = self
            .retention_setting("retention.max_runs")
            .unwrap_or(DEFAULT_RETENTION_MAX_RUNS);
        self.with_connection(|connection| {
            let cutoff = (chrono::Utc::now() - chrono::Duration::days(i64::from(max_age_days)))
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            connection
                .execute("DELETE FROM runs WHERE started_at < ?1", [&cutoff])
                .map_err(|error| error.to_string())?;
            connection
                .execute(
                    "DELETE FROM runs WHERE id IN (
                        SELECT id FROM runs ORDER BY started_at DESC, id DESC LIMIT -1 OFFSET ?1
                    )",
                    [i64::from(max_runs)],
                )
                .map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    fn retention_setting(&self, key: &str) -> Option<u32> {
        self.get_setting(key)
            .ok()
            .flatten()
            .and_then(|value| serde_json::from_str::<u32>(&value).ok())
    }
}

/// 运行摘要（list_runs 的返回；字段与前端 types.ts 一一对应）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RunSummary {
    pub run_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub route_mode: String,
    pub repeats: u32,
    pub app_version: String,
    pub target_ids: Vec<String>,
    pub sample_count: u32,
    pub success_count: u32,
    pub transport_failure_count: u32,
    pub https_median_ms: Option<u64>,
}
