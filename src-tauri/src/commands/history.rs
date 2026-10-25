//! 历史记录命令（Phase 5）：列表 / 详情 / 删除 / 清空 / 设置（仅白名单键）。

use tauri::State;

use crate::AppState;
use crate::diagnostic::types::{IpcError, IpcErrorCode, RouteMode};
use crate::storage::{RunSummary, Storage};

fn storage<'a>(state: &'a State<'_, AppState>) -> Result<&'a Storage, IpcError> {
    state
        .storage
        .as_deref()
        .ok_or_else(|| IpcError::new(IpcErrorCode::Internal, "历史存储不可用"))
}

/// 查询参数（与前端 RunQuery 一一对应）。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RunQuery {
    pub target_id: Option<String>,
    pub route_mode: Option<RouteMode>,
    pub limit: Option<u32>,
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_runs(query: RunQuery, state: State<'_, AppState>) -> Result<Vec<RunSummary>, IpcError> {
    storage(&state)?.list_runs(
        query.target_id.as_deref(),
        query.route_mode,
        query.limit.unwrap_or(100),
    )
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_run(
    run_id: String,
    state: State<'_, AppState>,
) -> Result<crate::diagnostic::types::DiagnosticRun, IpcError> {
    if run_id.trim().is_empty() {
        return Err(IpcError::invalid_input("run_id 不能为空"));
    }
    storage(&state)?.get_run(&run_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_run(run_id: String, state: State<'_, AppState>) -> Result<bool, IpcError> {
    storage(&state)?.delete_run(&run_id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn clear_history(state: State<'_, AppState>) -> Result<u32, IpcError> {
    storage(&state)?.clear_history()
}

/// 设置键白名单（PRD：settings 表不得存储凭证）。
const ALLOWED_SETTING_KEYS: [&str; 2] = ["retention.max_runs", "retention.max_age_days"];

#[tauri::command(rename_all = "snake_case")]
pub fn get_setting(key: String, state: State<'_, AppState>) -> Result<Option<String>, IpcError> {
    if !ALLOWED_SETTING_KEYS.contains(&key.as_str()) {
        return Err(IpcError::invalid_input("不支持的设置键"));
    }
    storage(&state)?.get_setting(&key)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_setting(
    key: String,
    value_json: String,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    if !ALLOWED_SETTING_KEYS.contains(&key.as_str()) {
        return Err(IpcError::invalid_input("不支持的设置键"));
    }
    storage(&state)?.set_setting(&key, &value_json)
}
