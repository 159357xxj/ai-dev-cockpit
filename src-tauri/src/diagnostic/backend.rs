//! 探测后端依赖注入缝（docs/ARCHITECTURE.md §2.1，D-012）。
//!
//! - 生产构建：`lib.rs::run()` 组装 `ProductionBackend`（Phase 2 起实现），
//!   其探测目标只能来自 `targets.rs` 白名单与用户表单合法参数，无任意 URL/端口入口。
//! - 测试：`MockBackend` 等实现仅在测试代码中构造，可指向本地 mock 服务器。
//! - Tauri 命令签名与前端 IPC 面不暴露注入接口。

use super::types::{DiagnosticRequest, DiagnosticRun, IpcError};

/// 探测后端。Phase 1 为最小形态：同步返回整体结果；
/// Phase 2 将演进为基于调度器的异步任务模型（job_id + 进度事件 + 取消）。
pub trait ProbeBackend: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &'static str;
    fn probe(&self, request: &DiagnosticRequest) -> Result<DiagnosticRun, IpcError>;
}

/// Phase 1 桩：命令层会真实执行请求校验，探测本身返回 `not_available`。
#[derive(Debug, Default)]
pub struct StubBackend;

impl StubBackend {
    pub fn new() -> Self {
        Self
    }
}

impl ProbeBackend for StubBackend {
    fn name(&self) -> &'static str {
        "stub"
    }

    fn probe(&self, _request: &DiagnosticRequest) -> Result<DiagnosticRun, IpcError> {
        Err(IpcError::not_available(
            "真实网络探测将在 Phase 2 提供；当前为界面桩，不产生任何网络请求",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::types::{IpcErrorCode, RouteMode, RunStatus};

    fn request() -> DiagnosticRequest {
        DiagnosticRequest {
            target_ids: vec!["github".to_string()],
            route_mode: RouteMode::NoAppProxy,
            manual_local_proxy_url: None,
            repeats: 3,
            timeout_ms: 10_000,
        }
    }

    /// 测试专用后端：演示测试代码可以注入任意实现（D-012 注入缝烟测）。
    #[derive(Debug)]
    struct MockBackend {
        fail_with: Option<IpcError>,
    }

    impl ProbeBackend for MockBackend {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn probe(&self, _request: &DiagnosticRequest) -> Result<DiagnosticRun, IpcError> {
            if let Some(error) = &self.fail_with {
                return Err(error.clone());
            }
            Ok(DiagnosticRun {
                run_id: "mock-run-1".to_string(),
                started_at: "2026-10-24T00:00:00Z".to_string(),
                finished_at: Some("2026-10-24T00:00:05Z".to_string()),
                status: RunStatus::Completed,
                route_mode: RouteMode::NoAppProxy,
                repeats: 3,
                app_version: "0.1.0".to_string(),
                targets: Vec::new(),
            })
        }
    }

    #[test]
    fn stub_backend_reports_not_available() {
        let error = StubBackend::new().probe(&request()).unwrap_err();
        assert_eq!(error.code, IpcErrorCode::NotAvailable);
    }

    #[test]
    fn backend_is_injectable_for_tests() {
        let backend: std::sync::Arc<dyn ProbeBackend> =
            std::sync::Arc::new(MockBackend { fail_with: None });
        assert_eq!(backend.name(), "mock");
        let run = backend.probe(&request()).expect("mock 后端应成功");
        assert_eq!(run.run_id, "mock-run-1");
        assert_eq!(run.status, RunStatus::Completed);

        let failing: std::sync::Arc<dyn ProbeBackend> = std::sync::Arc::new(MockBackend {
            fail_with: Some(IpcError::new(IpcErrorCode::Internal, "mock 故障")),
        });
        assert_eq!(
            failing.probe(&request()).unwrap_err().code,
            IpcErrorCode::Internal
        );
    }
}
