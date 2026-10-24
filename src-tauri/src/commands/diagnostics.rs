//! 诊断任务命令（Phase 1：校验真实执行，探测经注入后端的桩完成）。

use crate::AppState;
use crate::diagnostic::types::{CancelOutcome, DiagnosticRequest, IpcError, JobHandle, RouteMode};
use crate::security::validation::validate_manual_proxy_url;
use tauri::State;

/// 校验请求并启动诊断。
///
/// Phase 1：请求校验（目标白名单/次数/超时/代理地址规则）真实执行；
/// 探测调用注入的 `ProbeBackend`（当前为 StubBackend，返回 not_available）。
/// Phase 2 替换为 ProductionBackend 后，本命令签名与校验逻辑不变。
#[tauri::command(rename_all = "snake_case")]
pub fn start_diagnostic(
    request: DiagnosticRequest,
    state: State<'_, AppState>,
) -> Result<JobHandle, IpcError> {
    request.validate().map_err(IpcError::invalid_input)?;
    if request.route_mode == RouteMode::ManualLocalProxy {
        let raw = request
            .manual_local_proxy_url
            .as_deref()
            .unwrap_or_default();
        validate_manual_proxy_url(raw).map_err(IpcError::invalid_input)?;
    }
    let run = state.backend.probe(&request)?;
    // Phase 2：调度器返回独立 job_id 并异步推送进度事件；
    // 当前桩以 run_id 作为句柄，仅用于保持契约形状。
    Ok(JobHandle { job_id: run.run_id })
}

/// 取消诊断任务。Phase 1 无运行中任务，Phase 2 接入调度器取消令牌。
#[tauri::command(rename_all = "snake_case")]
pub fn cancel_diagnostic(
    job_id: String,
    _state: State<'_, AppState>,
) -> Result<CancelOutcome, IpcError> {
    if job_id.trim().is_empty() {
        return Err(IpcError::invalid_input("job_id 不能为空"));
    }
    Ok(CancelOutcome { cancelled: false })
}
