//! 诊断任务命令（Phase 2：校验真实执行，探测经注入后端异步运行，事件推送）。

use std::sync::Arc;

use tauri::{AppHandle, Emitter as _, State};

use crate::AppState;
use crate::diagnostic::backend::{JobContext, ProbeBackend};
use crate::diagnostic::rules::{Diagnosis, diagnose};
use crate::diagnostic::scheduler::Emitter;
use crate::diagnostic::types::{
    CancelOutcome, DiagnosticRequest, DiagnosticRun, IpcError, JobEvent, JobHandle, RouteMode,
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
    // F-01 修复：三种合法路由模式（no_app_proxy / library_default / manual_local_proxy）
    // 均可启动（Phase 3 已实现底层路径）。保留全部参数校验：
    // - 目标白名单 / 次数 / 超时（request.validate）；
    // - manual_local_proxy 的 loopback/端口/协议严格校验（无凭证、无路径/查询）。
    if request.route_mode == RouteMode::ManualLocalProxy {
        let raw = request
            .manual_local_proxy_url
            .as_deref()
            .unwrap_or_default();
        validate_manual_proxy_url(raw).map_err(IpcError::invalid_input)?;
    }
    backend.start_job(request, JobContext { emit: emitter })
}

/// 规则诊断：对一至多个已完成/已取消的运行求值（纯函数、离线、确定性，
/// 无网络请求、无 AI 服务）。输入来自前端已持有的运行结果。
#[tauri::command(rename_all = "snake_case")]
pub fn diagnose_runs(runs: Vec<DiagnosticRun>) -> Result<Vec<Diagnosis>, IpcError> {
    if runs.len() > 16 {
        return Err(IpcError::invalid_input("单次诊断最多支持 16 个运行"));
    }
    Ok(diagnose(&runs))
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

    /// F-01 回归：三种合法模式都必须越过门控到达后端（Stub 以 not_available
    /// 应答代表"校验通过并转发"）；非法代理仍 invalid_input。
    #[test]
    fn impl_accepts_all_three_route_modes() {
        for mode in [
            RouteMode::NoAppProxy,
            RouteMode::LibraryDefault,
            RouteMode::ManualLocalProxy,
        ] {
            let mut req = request();
            req.route_mode = mode;
            if mode == RouteMode::ManualLocalProxy {
                req.manual_local_proxy_url = Some("socks5h://127.0.0.1:7890".to_string());
            }
            let (emitter, events) = sink_emitter();
            let error = start_diagnostic_impl(req, &StubBackend::new(), emitter).unwrap_err();
            // Stub 后端在接收请求后返回 not_available（非 invalid_input）＝已过门控。
            assert_eq!(
                error.code,
                IpcErrorCode::NotAvailable,
                "模式 {mode:?} 应到达后端，实际被入口拒绝：{error}"
            );
            // 事件不泄漏：入口拒绝才会产生空事件；此处后端拒答同样无事件。
            assert!(events.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn impl_rejects_invalid_manual_proxy_with_invalid_input() {
        for bad in [
            "http://proxy.example.com:8080",   // 非回环
            "http://user:pass@127.0.0.1:7890", // 凭证
            "http://127.0.0.1:7890/path",      // 路径
            "ftp://127.0.0.1:21",              // 协议
        ] {
            let mut req = request();
            req.route_mode = RouteMode::ManualLocalProxy;
            req.manual_local_proxy_url = Some(bad.to_string());
            let (emitter, _) = sink_emitter();
            let error = start_diagnostic_impl(req, &StubBackend::new(), emitter).unwrap_err();
            assert_eq!(error.code, IpcErrorCode::InvalidInput, "应拒绝 {bad}");
        }
    }

    /// F-01：library_default 不允许携带代理地址（与请求校验一致）。
    #[test]
    fn impl_rejects_proxy_url_on_non_manual_mode() {
        let mut req = request();
        req.route_mode = RouteMode::LibraryDefault;
        req.manual_local_proxy_url = Some("http://127.0.0.1:7890".to_string());
        let (emitter, _) = sink_emitter();
        let error = start_diagnostic_impl(req, &StubBackend::new(), emitter).unwrap_err();
        assert_eq!(error.code, IpcErrorCode::InvalidInput);
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
