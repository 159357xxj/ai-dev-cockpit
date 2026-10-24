//! 探测后端依赖注入缝（docs/ARCHITECTURE.md §2.1，D-012）。
//!
//! - 生产构建：`lib.rs::run()` 组装 `ProductionBackend`，探测目标只能来自
//!   `targets.rs` 白名单与用户表单合法参数，无任意 URL/端口入口。
//! - 测试：`with_spec_builder` 可把任务端点指向本地 mock 服务器；
//!   `MockBackend` 仅在测试代码中构造。
//! - Tauri 命令签名与前端 IPC 面不暴露注入接口。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use super::scheduler::{Emitter, JobSpec, execute_job};
use super::types::{CancelOutcome, DiagnosticRequest, IpcError, IpcErrorCode, JobHandle};

/// 事件出口容器：生产实现把 JobEvent 映射为 Tauri 事件；测试捕获到内存。
pub struct JobContext {
    pub emit: Emitter,
}

pub trait ProbeBackend: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &'static str;
    /// 启动诊断任务。请求校验由命令层完成；进度与终态经 `ctx.emit` 推送，
    /// 终态事件恰好一个（`scheduler::TerminalGuard` 保证）。
    fn start_job(&self, request: DiagnosticRequest, ctx: JobContext)
    -> Result<JobHandle, IpcError>;
    /// 取消任务（幂等）；任务不存在或已结束返回 `cancelled=false`。
    fn cancel_job(&self, job_id: &str) -> CancelOutcome;
}

/// 生产实现：真实 DNS/TCP/HTTPS 探测。
///
/// `spec_builder` 默认用 [`JobSpec::production`]（白名单端点）；
/// 测试通过 [`ProductionBackend::with_spec_builder`] 注入指向本地 mock 的端点，
/// 从而在不联网的前提下走完整的任务/事件/取消路径。
pub struct ProductionBackend {
    app_version: String,
    jobs: Mutex<HashMap<String, JobEntry>>,
    spec_builder: Arc<dyn Fn(String, DiagnosticRequest, String) -> JobSpec + Send + Sync>,
}

impl std::fmt::Debug for ProductionBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProductionBackend")
            .field("app_version", &self.app_version)
            .field("active_jobs", &self.jobs.lock().map(|jobs| jobs.len()))
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct JobEntry {
    cancel: CancellationToken,
    done: Arc<AtomicBool>,
}

impl ProductionBackend {
    pub fn new(app_version: String) -> Self {
        Self::with_spec_builder(app_version, Arc::new(JobSpec::production))
    }

    pub fn with_spec_builder(
        app_version: String,
        spec_builder: Arc<dyn Fn(String, DiagnosticRequest, String) -> JobSpec + Send + Sync>,
    ) -> Self {
        Self {
            app_version,
            jobs: Mutex::new(HashMap::new()),
            spec_builder,
        }
    }
}

impl ProbeBackend for ProductionBackend {
    fn name(&self) -> &'static str {
        "production"
    }

    fn start_job(
        &self,
        request: DiagnosticRequest,
        ctx: JobContext,
    ) -> Result<JobHandle, IpcError> {
        // 必须在 Tokio 运行时上下文中（Tauri async 命令满足；防御性检查避免 panic）。
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(IpcError::new(
                IpcErrorCode::Internal,
                "缺少 Tokio 运行时上下文，无法启动诊断任务",
            ));
        }

        let job_id = uuid::Uuid::new_v4().to_string();
        let cancel = CancellationToken::new();
        let done = Arc::new(AtomicBool::new(false));
        {
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            // 清理已结束任务，防止映射无限增长。
            jobs.retain(|_, entry| !entry.done.load(Ordering::Relaxed));
            jobs.insert(
                job_id.clone(),
                JobEntry {
                    cancel: cancel.clone(),
                    done: done.clone(),
                },
            );
        }

        let spec = (self.spec_builder)(job_id.clone(), request, self.app_version.clone());
        let emit = ctx.emit;
        let done_flag = done.clone();
        tokio::task::spawn(async move {
            let _run = execute_job(spec, cancel, emit).await;
            done_flag.store(true, Ordering::Relaxed);
        });

        Ok(JobHandle { job_id })
    }

    fn cancel_job(&self, job_id: &str) -> CancelOutcome {
        let jobs = self
            .jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match jobs.get(job_id) {
            Some(entry) => {
                if entry.done.load(Ordering::Relaxed) || entry.cancel.is_cancelled() {
                    CancelOutcome { cancelled: false }
                } else {
                    entry.cancel.cancel();
                    CancelOutcome { cancelled: true }
                }
            }
            None => CancelOutcome { cancelled: false },
        }
    }
}

/// Phase 1 桩：不产生任何网络请求，`start_job` 如实返回 `not_available`。
/// 生产装配已换用 `ProductionBackend`；本实现保留给命令层单元测试与降级路径。
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct StubBackend;

#[allow(dead_code)]
impl StubBackend {
    pub fn new() -> Self {
        Self
    }
}

impl ProbeBackend for StubBackend {
    fn name(&self) -> &'static str {
        "stub"
    }

    fn start_job(
        &self,
        _request: DiagnosticRequest,
        _ctx: JobContext,
    ) -> Result<JobHandle, IpcError> {
        Err(IpcError::not_available(
            "当前后端为测试桩，不产生任何网络请求",
        ))
    }

    fn cancel_job(&self, _job_id: &str) -> CancelOutcome {
        CancelOutcome { cancelled: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::types::{JobEvent, ProbeType, RouteMode, RunStatus};
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

    fn silent_emitter() -> Emitter {
        Arc::new(|_event| {})
    }

    #[derive(Debug)]
    struct MockBackend {
        fail_with: Option<IpcError>,
        started: StdMutex<Vec<String>>,
    }

    impl ProbeBackend for MockBackend {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn start_job(
            &self,
            _request: DiagnosticRequest,
            _ctx: JobContext,
        ) -> Result<JobHandle, IpcError> {
            if let Some(error) = &self.fail_with {
                return Err(error.clone());
            }
            let job_id = format!("mock-{}", self.started.lock().unwrap().len());
            self.started.lock().unwrap().push(job_id.clone());
            Ok(JobHandle { job_id })
        }

        fn cancel_job(&self, job_id: &str) -> CancelOutcome {
            CancelOutcome {
                cancelled: !job_id.is_empty(),
            }
        }
    }

    #[test]
    fn stub_backend_reports_not_available() {
        let error = StubBackend::new()
            .start_job(
                request(),
                JobContext {
                    emit: silent_emitter(),
                },
            )
            .unwrap_err();
        assert_eq!(error.code, IpcErrorCode::NotAvailable);
        assert!(!StubBackend::new().cancel_job("any").cancelled);
    }

    #[test]
    fn backend_is_injectable_for_tests() {
        let backend: Arc<dyn ProbeBackend> = Arc::new(MockBackend {
            fail_with: None,
            started: StdMutex::new(Vec::new()),
        });
        assert_eq!(backend.name(), "mock");
        let handle = backend
            .start_job(
                request(),
                JobContext {
                    emit: silent_emitter(),
                },
            )
            .expect("mock 后端应成功");
        assert!(handle.job_id.starts_with("mock-"));
        assert!(backend.cancel_job(&handle.job_id).cancelled);
    }

    fn offline_spec_builder(
        deadline_ms: u64,
    ) -> Arc<dyn Fn(String, DiagnosticRequest, String) -> JobSpec + Send + Sync> {
        Arc::new(move |job_id, request, version| {
            let mut spec = JobSpec::production(job_id, request, version);
            for endpoint in &mut spec.endpoints {
                endpoint.dns_host = "127.0.0.1".to_string();
                endpoint.tcp_host = "127.0.0.1".to_string();
                endpoint.tcp_port = 1;
                endpoint.https_url = "http://127.0.0.1:1/".to_string();
            }
            spec.deadline = std::time::Duration::from_millis(deadline_ms);
            spec
        })
    }

    /// ProductionBackend：取消幂等 + 已结束任务返回 cancelled=false + 未知任务 false。
    #[tokio::test]
    async fn production_backend_cancel_is_idempotent() {
        let backend =
            ProductionBackend::with_spec_builder("0.1.0".to_string(), offline_spec_builder(300));
        let handle = backend
            .start_job(
                request(),
                JobContext {
                    emit: silent_emitter(),
                },
            )
            .expect("启动成功");

        // 立即取消 → true；重复取消 → false（幂等，不破坏状态）。
        assert!(backend.cancel_job(&handle.job_id).cancelled);
        assert!(!backend.cancel_job(&handle.job_id).cancelled);
        // 未知任务 → false。
        assert!(!backend.cancel_job("no-such-job").cancelled);

        // 等任务结束后再取消 → false。
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        assert!(!backend.cancel_job(&handle.job_id).cancelled);
    }

    /// 终态事件恰好一个：正常完成（含取消）发 Completed，且首个事件是进度。
    #[tokio::test]
    async fn production_backend_emits_exactly_one_terminal_event() {
        let events: Arc<StdMutex<Vec<JobEvent>>> = Arc::new(StdMutex::new(Vec::new()));
        let sink = events.clone();
        let emitter: Emitter = Arc::new(move |event| sink.lock().unwrap().push(event));

        let backend =
            ProductionBackend::with_spec_builder("0.1.0".to_string(), offline_spec_builder(200));
        let handle = backend
            .start_job(request(), JobContext { emit: emitter })
            .expect("启动成功");

        // 等待终态（最多 5s）。
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let terminal_count = events
                .lock()
                .unwrap()
                .iter()
                .filter(|event| matches!(event, JobEvent::Completed(_) | JobEvent::Failed(_)))
                .count();
            if terminal_count == 1 || std::time::Instant::now() > deadline {
                assert_eq!(terminal_count, 1, "终态事件必须恰好一个");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        let events = events.lock().unwrap();
        let first = events.first().expect("至少有初始进度事件");
        assert!(matches!(first, JobEvent::Progress(_)));
        let terminal = events
            .iter()
            .find(|event| matches!(event, JobEvent::Completed(_) | JobEvent::Failed(_)))
            .expect("存在终态");
        if let JobEvent::Completed(payload) = terminal {
            assert_eq!(payload.job_id, handle.job_id);
            assert_eq!(payload.run.status, RunStatus::Cancelled);
            assert!(!payload.run.run_id.is_empty());
            // 取消任务：样本只能来自实际执行的部分。
            // HTTPS 指向 127.0.0.1:1（拒连）不得成功；DNS 解析 127.0.0.1 成功是正确行为。
            let https_all_failed = payload
                .run
                .targets
                .iter()
                .flat_map(|target| &target.samples)
                .filter(|sample| sample.probe_type == ProbeType::Https)
                .all(|sample| !sample.success);
            assert!(https_all_failed, "拒连目标的 HTTPS 样本不得成功");
        } else {
            panic!("正常完成的任务应发 Completed 而非 Failed");
        }
    }
}
