//! 诊断任务命令（Phase 2：校验真实执行，探测经注入后端异步运行，事件推送）。

use std::sync::Arc;

use tauri::{AppHandle, Emitter as _, State};

use crate::AppState;
use crate::diagnostic::backend::{JobContext, ProbeBackend};
use crate::diagnostic::scheduler::Emitter;
use crate::diagnostic::types::{
    CancelOutcome, DiagnosticRequest, IpcError, JobEvent, JobHandle, RouteMode,
};
use crate::security::validation::validate_manual_proxy_url;

/// 把任务事件映射为 Tauri 全局事件（前端在 start 之前订阅即可无竞争）。
fn production_emitter(app: AppHandle) -> Emitter {
    Arc::new(move |event| {
        let result = match event {
            JobEvent::Progress(payload) => app.emit("diagnostic://progress", payload),
            JobEvent::Completed(payload) => app.emit("diagnostic://completed", payload),
            JobEvent::Failed(payload) => app.emit("diagnostic://failed", payload),
        };
        if let Err(error) = result {
            eprintln!("诊断事件推送失败：{error}");
        }
    })
}

/// 校验请求并启动诊断任务。
///
/// 校验（目标白名单/次数/超时/代理地址规则）真实执行；探测任务由注入的
/// `ProbeBackend` 异步运行，进度与终态经事件推送。
#[tauri::command(rename_all = "snake_case")]
pub async fn start_diagnostic(
    request: DiagnosticRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<JobHandle, IpcError> {
    let emitter = production_emitter(app);
    start_diagnostic_impl(request, state.backend.as_ref(), emitter)
}

/// 实现核心（与 Tauri 解耦，便于单元测试）。
pub fn start_diagnostic_impl(
    request: DiagnosticRequest,
    backend: &dyn ProbeBackend,
    emitter: Emitter,
) -> Result<JobHandle, IpcError> {
    request.validate().map_err(IpcError::invalid_input)?;
    // Phase 2 仅支持 no_app_proxy；其余模式显式禁用（Phase 3 提供）。
    if request.route_mode != RouteMode::NoAppProxy {
        return Err(IpcError::not_available(
            "该连接模式将在 Phase 3 提供（当前仅支持 no_app_proxy）",
        ));
    }
    if request.route_mode == RouteMode::ManualLocalProxy {
        let raw = request
            .manual_local_proxy_url
            .as_deref()
            .unwrap_or_default();
        validate_manual_proxy_url(raw).map_err(IpcError::invalid_input)?;
    }
    backend.start_job(request, JobContext { emit: emitter })
}

/// 取消诊断任务（幂等）。
#[tauri::command(rename_all = "snake_case")]
pub fn cancel_diagnostic(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<CancelOutcome, IpcError> {
    if job_id.trim().is_empty() {
        return Err(IpcError::invalid_input("job_id 不能为空"));
    }
    Ok(state.backend.cancel_job(&job_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::backend::StubBackend;
    use crate::diagnostic::types::IpcErrorCode;
    use std::sync::Mutex as StdMutex;

    fn request() -> DiagnosticRequest {
        DiagnosticRequest {
            target_ids: vec!["github".to_string()],
            route_mode: RouteMode::NoAppProxy,
            manual_local_proxy_url: None,
            repeats: 3,
            timeout_ms: 10_000,
        }
    }

    fn sink_emitter() -> (Emitter, Arc<StdMutex<Vec<JobEvent>>>) {
        let events: Arc<StdMutex<Vec<JobEvent>>> = Arc::new(StdMutex::new(Vec::new()));
        let sink = events.clone();
        (
            Arc::new(move |event| sink.lock().unwrap().push(event)),
            events,
        )
    }

    #[test]
    fn impl_rejects_invalid_request_before_backend() {
        let mut invalid = request();
        invalid.repeats = 4;
        let (emitter, events) = sink_emitter();
        let error = start_diagnostic_impl(invalid, &StubBackend::new(), emitter).unwrap_err();
        assert_eq!(error.code, IpcErrorCode::InvalidInput);
        assert!(events.lock().unwrap().is_empty(), "拒绝时不产生事件");
    }

    #[test]
    fn impl_rejects_phase3_modes_with_not_available() {
        let mut req = request();
        req.route_mode = RouteMode::LibraryDefault;
        let (emitter, _) = sink_emitter();
        let error = start_diagnostic_impl(req, &StubBackend::new(), emitter).unwrap_err();
        assert_eq!(error.code, IpcErrorCode::NotAvailable);
        assert!(error.message.contains("Phase 3"));
    }

    #[test]
    fn impl_rejects_unknown_target() {
        let mut req = request();
        req.target_ids = vec!["not-in-whitelist".to_string()];
        let (emitter, _) = sink_emitter();
        let error = start_diagnostic_impl(req, &StubBackend::new(), emitter).unwrap_err();
        assert_eq!(error.code, IpcErrorCode::InvalidInput);
    }

    #[test]
    fn impl_forwards_to_backend_when_valid() {
        // StubBackend 的 start_job 返回 not_available——即"校验通过后交给后端"。
        let (emitter, _) = sink_emitter();
        let error = start_diagnostic_impl(request(), &StubBackend::new(), emitter).unwrap_err();
        assert_eq!(error.code, IpcErrorCode::NotAvailable);
    }
}
