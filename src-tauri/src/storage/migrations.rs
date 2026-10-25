//! 版本化迁移：只向前，幂等（user_version 记录当前版本）。

use rusqlite::Connection;

/// 迁移清单：按序追加，禁止修改已发布条目。
const MIGRATIONS: &[&str] = &[
    // v1：初始三表 + 索引（PRD §6）。
    "CREATE TABLE runs (
        id TEXT PRIMARY KEY,
        started_at TEXT NOT NULL,
        finished_at TEXT,
        status TEXT NOT NULL,
        route_mode TEXT NOT NULL,
        repeats INTEGER NOT NULL,
        app_version TEXT NOT NULL,
        notes TEXT
    );
    CREATE TABLE results (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
        target_id TEXT NOT NULL,
        probe_type TEXT NOT NULL,
        route_mode TEXT NOT NULL,
        attempt_no INTEGER NOT NULL,
        success INTEGER NOT NULL,
        duration_ms INTEGER,
        http_status INTEGER,
        http_outcome TEXT,
        error_kind TEXT,
        error_summary TEXT,
        error_detail TEXT,
        tested_at TEXT NOT NULL
    );
    CREATE INDEX idx_results_run ON results(run_id);
    CREATE INDEX idx_results_target ON results(target_id);
    CREATE INDEX idx_runs_started_at ON runs(started_at);
    CREATE TABLE settings (
        key TEXT PRIMARY KEY,
        value_json TEXT NOT NULL
    );",
];

/// 应用所有未执行的迁移；重复调用幂等。
pub fn apply(connection: &mut Connection) -> Result<(), String> {
    let current: u32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| format!("读取 user_version 失败：{error}"))?;

    for (index, script) in MIGRATIONS.iter().enumerate() {
        let version = (index + 1) as u32;
        if version <= current {
            continue;
        }
        connection
            .execute_batch(script)
            .map_err(|error| format!("迁移 v{version} 失败：{error}"))?;
        connection
            .pragma_update(None, "user_version", version)
            .map_err(|error| format!("写入 user_version={version} 失败：{error}"))?;
    }
    Ok(())
}
