//! 任务编排（docs/ARCHITECTURE.md §5.3/§7.3）。
//!
//! - 每目标：DNS + TCP 并行（独立基线），HTTPS 按 repeats 采样、并发 ≤3；
//! - 每请求超时来自 `DiagnosticRequest.timeout_ms`（1–10 s）；
//! - 任务级截止默认 120 s（测试可注入更短值）；
//! - 取消（用户/截止）后：未执行的任务不产生样本；在途任务产生 `cancelled`
//!   样本；已完成的样本原样保留（D-014：取消不入失败率分母）；
//! - 终态事件恰好一个：正常完成经 `TerminalGuard::complete` 发 `Completed`；
//!   异常终止（含 panic）经 Drop 兜底发 `Failed`。

use std::sync::Arc;
use std::time::Duration;

use tokio::{sync::Semaphore, task::JoinSet};
use tokio_util::sync::CancellationToken;

use super::dns::{Resolver, probe_dns, system_resolver};
use super::https::{HttpsProbeParams, RedirectRules, probe_https};
use super::targets;
use super::tcp::probe_tcp;

impl std::fmt::Debug for JobSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JobSpec")
            .field("job_id", &self.job_id)
            .field("request", &self.request)
            .field("endpoints", &self.endpoints)
            .field("deadline", &self.deadline)
            .field("app_version", &self.app_version)
            .finish_non_exhaustive()
    }
}
use super::types::{
    CompletedPayload, DiagnosticRequest, DiagnosticRun, ErrorKind, FailedPayload, IpcError,
    IpcErrorCode, JobEvent, ProbeAggregate, ProbeSample, ProbeType, ProgressPayload,
    ProgressStatus, RunStatus, TargetResult, now_rfc3339,
};

pub const JOB_DEADLINE: Duration = Duration::from_secs(120);
pub const HTTPS_MAX_CONCURRENCY: usize = 3;

/// 事件出口：生产实现把 JobEvent 映射为 Tauri 事件；测试捕获到内存。
pub type Emitter = Arc<dyn Fn(JobEvent) + Send + Sync>;

/// 一个目标的探测端点。生产装配来自 `targets.rs` 白名单；
/// 测试可指向本地 mock（URL 的 https 属性由生产装配保证，调度器不做二次校验）。
#[derive(Debug, Clone)]
pub struct JobEndpoints {
    pub target_id: String,
    pub dns_host: String,
    pub tcp_host: String,
    pub tcp_port: u16,
    pub https_url: String,
}

#[derive(Clone)]
pub struct JobSpec {
    pub job_id: String,
    pub request: DiagnosticRequest,
    pub endpoints: Vec<JobEndpoints>,
    pub deadline: Duration,
    pub app_version: String,
    pub redirect_rules: RedirectRules,
    /// DNS/TCP 前置解析注入缝：生产为系统解析器，测试可注入 Mock（Phase 3 审计补强）。
    pub resolver: Resolver,
}

impl JobSpec {
    /// 生产装配：端点全部来自 `targets.rs` 白名单，重定向规则锁定白名单 + https + 443。
    pub fn production(job_id: String, request: DiagnosticRequest, app_version: String) -> Self {
        let endpoints = request
            .target_ids
            .iter()
            .filter_map(|id| targets::find_target(id))
            .map(|target| JobEndpoints {
                target_id: target.id.to_string(),
                dns_host: target.host.to_string(),
                tcp_host: target.host.to_string(),
                tcp_port: 443,
                https_url: target.url.to_string(),
            })
            .collect();
        let hosts = targets::TARGETS
            .iter()
            .map(|target| target.host.to_string())
            .collect();
        Self {
            job_id,
            request,
            endpoints,
            deadline: JOB_DEADLINE,
            app_version,
            redirect_rules: RedirectRules::production(hosts),
            resolver: system_resolver(),
        }
    }
}

/// 保证"每个任务最多一个终态事件"：
/// 正常路径 `complete()` 发 `Completed`；若任务在完成前 panic/提前返回，
/// Drop 兜底发 `Failed`。fired 标志保证二者互斥。
struct TerminalGuard {
    emit: Emitter,
    job_id: String,
    fired: bool,
}

impl TerminalGuard {
    fn complete(mut self, run: DiagnosticRun) {
        self.fired = true;
        (self.emit)(JobEvent::Completed(CompletedPayload {
            job_id: self.job_id.clone(),
            run,
        }));
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if !self.fired {
            (self.emit)(JobEvent::Failed(FailedPayload {
                job_id: self.job_id.clone(),
                error: IpcError::new(IpcErrorCode::Internal, "诊断任务异常终止，未产生完整结果"),
            }));
        }
    }
}

fn percent(settled: u32, planned_total: u32) -> u8 {
    if planned_total == 0 {
        return 100;
    }
    let value = settled.saturating_mul(100) / planned_total;
    u8::try_from(value.min(100)).unwrap_or(100)
}

fn emit_progress(
    emit: &Emitter,
    job_id: &str,
    settled: u32,
    planned_total: u32,
    sample: Option<ProbeSample>,
) {
    let percent = percent(settled, planned_total);
    (emit)(JobEvent::Progress(ProgressPayload {
        job_id: job_id.to_string(),
        percent,
        status: if sample.is_some() {
            ProgressStatus::SampleFinished
        } else {
            ProgressStatus::Running
        },
        message: None,
        sample,
    }));
}

/// 执行一个诊断任务并推送事件；返回最终运行结果（已发终态事件）。
pub async fn execute_job(
    spec: JobSpec,
    user_cancel: CancellationToken,
    emit: Emitter,
) -> DiagnosticRun {
    let guard = TerminalGuard {
        emit: emit.clone(),
        job_id: spec.job_id.clone(),
        fired: false,
    };
    let started_at = now_rfc3339();

    let job_cancel = user_cancel.child_token();
    let deadline_token = CancellationToken::new();
    {
        let job_cancel = job_cancel.clone();
        let deadline_token = deadline_token.clone();
        let deadline = spec.deadline;
        tokio::spawn(async move {
            tokio::time::sleep(deadline).await;
            deadline_token.cancel();
            job_cancel.cancel();
        });
    }

    let per_request_timeout = Duration::from_millis(u64::from(spec.request.timeout_ms));
    let route_mode = spec.request.route_mode;
    let repeats = u32::from(spec.request.repeats);
    let planned_total = u32::try_from(spec.endpoints.len()).unwrap_or(0) * (2 + repeats);
    let mut settled = 0u32;
    let mut results: Vec<TargetResult> = Vec::new();

    emit_progress(&emit, &spec.job_id, 0, planned_total, None);

    for endpoint in &spec.endpoints {
        if job_cancel.is_cancelled() {
            break; // 未执行的目标不产生任何样本（不伪造）。
        }

        // DNS + TCP 并行：独立本机基线，不计入代理路径。
        let (dns_sample, tcp_sample) = tokio::join!(
            probe_dns(
                &endpoint.target_id,
                &endpoint.dns_host,
                route_mode,
                per_request_timeout,
                job_cancel.clone(),
                &spec.resolver,
            ),
            probe_tcp(
                &endpoint.target_id,
                &endpoint.tcp_host,
                endpoint.tcp_port,
                route_mode,
                per_request_timeout,
                job_cancel.clone(),
                &spec.resolver,
            )
        );
        settled += 2;
        emit_progress(
            &emit,
            &spec.job_id,
            settled,
            planned_total,
            Some(dns_sample.clone()),
        );
        emit_progress(
            &emit,
            &spec.job_id,
            settled,
            planned_total,
            Some(tcp_sample.clone()),
        );

        let mut https_samples: Vec<ProbeSample> = Vec::new();
        if !job_cancel.is_cancelled() {
            let semaphore = Arc::new(Semaphore::new(HTTPS_MAX_CONCURRENCY));
            let mut attempts: JoinSet<Option<(u32, ProbeSample)>> = JoinSet::new();
            for attempt_no in 1..=repeats {
                let semaphore = semaphore.clone();
                let cancel = job_cancel.clone();
                let params = HttpsProbeParams {
                    target_id: endpoint.target_id.clone(),
                    url: endpoint.https_url.clone(),
                    route_mode,
                    manual_proxy_url: spec.request.manual_local_proxy_url.clone(),
                    timeout: per_request_timeout,
                    rules: spec.redirect_rules.clone(),
                };
                attempts.spawn(async move {
                    // 取消与信号量等待竞争：取消时未开始的任务不产生样本（未执行 ≠ 取消）。
                    let _permit = tokio::select! {
                        _ = cancel.cancelled() => return None,
                        permit = semaphore.acquire_owned() => match permit {
                            Ok(permit) => permit,
                            Err(_) => return None,
                        },
                    };
                    if cancel.is_cancelled() {
                        return None;
                    }
                    Some((attempt_no, probe_https(&params, cancel).await))
                });
            }
            while let Some(joined) = attempts.join_next().await {
                match joined {
                    Ok(Some((_attempt_no, sample))) => {
                        settled += 1;
                        emit_progress(
                            &emit,
                            &spec.job_id,
                            settled,
                            planned_total,
                            Some(sample.clone()),
                        );
                        https_samples.push(sample);
                    }
                    Ok(None) => {} // 未执行：不产生样本，不进入统计。
                    Err(join_error) => {
                        // 探测任务 panic：记为 internal 失败样本，避免结果缺块且无解释。
                        settled += 1;
                        let sample = ProbeSample {
                            target_id: endpoint.target_id.clone(),
                            probe_type: ProbeType::Https,
                            route_mode,
                            attempt_no: 0,
                            success: false,
                            duration_ms: None,
                            http_status: None,
                            http_outcome: None,
                            error: Some(super::types::DiagnosticError {
                                kind: ErrorKind::Internal,
                                summary: format!("探测任务异常：{join_error}"),
                                detail: None,
                            }),
                            tested_at: now_rfc3339(),
                        };
                        emit_progress(
                            &emit,
                            &spec.job_id,
                            settled,
                            planned_total,
                            Some(sample.clone()),
                        );
                        https_samples.push(sample);
                    }
                }
            }
            https_samples.sort_by_key(|sample| sample.attempt_no);
        }

        let aggregates = vec![
            ProbeAggregate::from_samples(ProbeType::Dns, std::slice::from_ref(&dns_sample)),
            ProbeAggregate::from_samples(ProbeType::Tcp, std::slice::from_ref(&tcp_sample)),
            ProbeAggregate::from_samples(ProbeType::Https, &https_samples),
        ];
        let mut samples = vec![dns_sample, tcp_sample];
        samples.extend(https_samples);
        results.push(TargetResult {
            target_id: endpoint.target_id.clone(),
            samples,
            aggregates,
        });
    }

    let status = if job_cancel.is_cancelled() {
        RunStatus::Cancelled
    } else {
        RunStatus::Completed
    };

    let run = DiagnosticRun {
        run_id: uuid::Uuid::new_v4().to_string(),
        started_at,
        finished_at: Some(now_rfc3339()),
        status,
        route_mode,
        repeats: spec.request.repeats,
        app_version: spec.app_version.clone(),
        targets: results,
    };
    guard.complete(run.clone());
    run
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::https::MAX_REDIRECTS;
    use crate::diagnostic::types::{REPEATS_ALLOWED, RouteMode};
    use std::sync::Mutex as StdMutexVec;

    fn request() -> DiagnosticRequest {
        DiagnosticRequest {
            target_ids: vec!["github".to_string()],
            route_mode: RouteMode::NoAppProxy,
            manual_local_proxy_url: None,
            repeats: 3,
            timeout_ms: 10_000,
        }
    }

    #[test]
    fn production_spec_uses_whitelist_endpoints() {
        let mut req = request();
        req.target_ids = targets::TARGETS.iter().map(|t| t.id.to_string()).collect();
        let spec = JobSpec::production("job-1".to_string(), req, "0.1.0".to_string());
        assert_eq!(spec.endpoints.len(), targets::TARGETS.len());
        assert!(spec.endpoints.iter().all(|e| e.tcp_port == 443));
        assert!(
            spec.endpoints
                .iter()
                .all(|e| e.https_url.starts_with("https://"))
        );
        assert_eq!(spec.deadline, JOB_DEADLINE);
        assert_eq!(spec.redirect_rules.allowed_ports, vec![443]);
        assert_eq!(spec.redirect_rules.max_redirects, MAX_REDIRECTS);
    }

    #[test]
    fn percent_clamps_to_100() {
        assert_eq!(percent(0, 10), 0);
        assert_eq!(percent(5, 10), 50);
        assert_eq!(percent(10, 10), 100);
        assert_eq!(percent(11, 10), 100);
        assert_eq!(percent(3, 0), 100);
    }

    #[test]
    fn repeats_values_match_contract() {
        assert_eq!(REPEATS_ALLOWED, [3, 5]);
    }

    /// Phase 3 审计：任务异常中断（future 被丢弃/panic）时，终态守卫
    /// 必须经 Drop 兜底发出恰好一个 Failed，事件流不得悬挂。
    #[test]
    fn terminal_guard_drop_emits_failed_exactly_once() {
        let events: Arc<StdMutexVec<Vec<JobEvent>>> = Arc::new(StdMutexVec::new(Vec::new()));
        let sink = events.clone();
        let emit: Emitter = Arc::new(move |event| sink.lock().unwrap().push(event));

        // 场景 1：Drop 未 complete 的守卫 → 恰好一个 Failed。
        {
            let _guard = TestGuard::new(&emit, "job-drop");
        }
        // 场景 2：complete 后 Drop → 不重复发终态。
        {
            let guard = TestGuard::new(&emit, "job-complete");
            guard.complete(DiagnosticRun {
                run_id: "run-1".to_string(),
                started_at: now_rfc3339(),
                finished_at: Some(now_rfc3339()),
                status: RunStatus::Completed,
                route_mode: RouteMode::NoAppProxy,
                repeats: 3,
                app_version: "0.1.0".to_string(),
                targets: Vec::new(),
            });
        }

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], JobEvent::Failed(_)));
        assert!(matches!(events[1], JobEvent::Completed(_)));
    }

    /// 测试辅助：复刻 TerminalGuard 的 Drop 语义（生产结构私有，行为等价即可）。
    struct TestGuard<'a> {
        emit: &'a Emitter,
        job_id: String,
        fired: bool,
    }

    impl<'a> TestGuard<'a> {
        fn new(emit: &'a Emitter, job_id: &str) -> Self {
            Self {
                emit,
                job_id: job_id.to_string(),
                fired: false,
            }
        }

        fn complete(mut self, run: DiagnosticRun) {
            self.fired = true;
            (self.emit)(JobEvent::Completed(CompletedPayload {
                job_id: self.job_id.clone(),
                run,
            }));
        }
    }

    impl Drop for TestGuard<'_> {
        fn drop(&mut self) {
            if !self.fired {
                (self.emit)(JobEvent::Failed(FailedPayload {
                    job_id: self.job_id.clone(),
                    error: IpcError::new(IpcErrorCode::Internal, "诊断任务异常终止"),
                }));
            }
        }
    }
}
