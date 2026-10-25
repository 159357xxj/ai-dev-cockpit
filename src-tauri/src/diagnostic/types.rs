//! IPC 数据契约：请求/响应 DTO、枚举、错误分类与聚合计算。
//!
//! 线上格式约定（docs/ARCHITECTURE.md §5）：
//! - 字段名 snake_case（Rust 默认命名，不经 serde 重命名）；
//! - 枚举变体以 snake_case 字符串序列化；
//! - 时间一律 RFC3339 UTC 字符串，展示层转本地时区。

use serde::{Deserialize, Serialize};

/// 允许的采样次数（PRD：默认 3 次，可选 5 次）。
pub const REPEATS_ALLOWED: [u8; 2] = [3, 5];
pub const TIMEOUT_MIN_MS: u32 = 1000;
pub const TIMEOUT_MAX_MS: u32 = 10_000;
/// 契约默认值：前端表单默认与之对齐；Rust 侧 Phase 2 起作为超时缺省使用。
#[allow(dead_code)]
pub const TIMEOUT_DEFAULT_MS: u32 = 10_000;
pub const MAX_TARGETS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteMode {
    /// HTTP 库默认路径（可能读取环境/系统代理源，实际语义 Phase 3 实测固化）。
    LibraryDefault,
    /// 不配置应用层 HTTP 代理；不承诺绕过 TUN/透明路由。
    NoAppProxy,
    /// 指定 loopback HTTP/SOCKS5/SOCKS5h 本地代理；失败绝不回落。
    ManualLocalProxy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeType {
    Dns,
    Tcp,
    Https,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Cancelled,
    Failed,
}

/// 传输层失败分类。收到 HTTP 响应不是错误（D-007），不会产生该类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    DnsFailure,
    TcpFailure,
    TlsFailure,
    Timeout,
    Cancelled,
    ProxyUnavailable,
    InvalidInput,
    Internal,
    Unknown,
}

/// HTTPS 收到响应时的细分；`success` 仅 2xx 为 true（D-007 / D-011）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpOutcome {
    #[serde(rename = "ok_2xx")]
    Ok2xx,
    #[serde(rename = "redirect_3xx")]
    Redirect3xx,
    #[serde(rename = "auth_required_401_403")]
    AuthRequired401To403,
    #[serde(rename = "proxy_auth_407")]
    ProxyAuth407,
    #[serde(rename = "rate_limited_429")]
    RateLimited429,
    #[serde(rename = "server_error_5xx")]
    ServerError5xx,
    #[serde(rename = "client_error_other_4xx")]
    ClientErrorOther4xx,
}

/// IPC 错误码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorCode {
    InvalidInput,
    NotFound,
    /// 功能尚未实现或当前模式下不可用（Phase 1 命令桩的真实状态）。
    NotAvailable,
    Internal,
    Cancelled,
}

/// IPC 错误（命令 `Err` 分支的结构化载荷）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcError {
    pub code: IpcErrorCode,
    pub message: String,
    pub detail: Option<String>,
}

impl IpcError {
    pub fn new(code: IpcErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(IpcErrorCode::InvalidInput, message)
    }

    pub fn not_available(message: impl Into<String>) -> Self {
        Self::new(IpcErrorCode::NotAvailable, message)
    }
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

impl std::error::Error for IpcError {}

/// 诊断请求（前端 → 后端）。所有字段在 Rust 侧二次校验。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticRequest {
    pub target_ids: Vec<String>,
    pub route_mode: RouteMode,
    pub manual_local_proxy_url: Option<String>,
    /// 仅允许 3 或 5。
    pub repeats: u8,
    /// 单请求超时，1000..=10000 毫秒。
    pub timeout_ms: u32,
}

impl DiagnosticRequest {
    /// 校验规则与 docs/ARCHITECTURE.md §5.5 一致。
    ///
    /// 手动代理模式仅在此做"必须携带地址"的存在性检查；URL 语法与 loopback
    /// 约束由命令层调用 `security::validation::validate_manual_proxy_url` 完成。
    pub fn validate(&self) -> Result<(), String> {
        if self.target_ids.is_empty() {
            return Err("至少选择一个诊断目标".to_string());
        }
        if self.target_ids.len() > MAX_TARGETS {
            return Err(format!("诊断目标最多 {MAX_TARGETS} 个"));
        }
        // 重复目标会让样本归属与聚合口径产生歧义，直接拒绝。
        let mut seen = std::collections::HashSet::new();
        for id in &self.target_ids {
            if !seen.insert(id.as_str()) {
                return Err(format!("诊断目标重复：{id}"));
            }
            if super::targets::find_target(id).is_none() {
                return Err(format!("未知诊断目标：{id}"));
            }
        }
        if !REPEATS_ALLOWED.contains(&self.repeats) {
            return Err(format!("采样次数仅允许 {REPEATS_ALLOWED:?}"));
        }
        if !(TIMEOUT_MIN_MS..=TIMEOUT_MAX_MS).contains(&self.timeout_ms) {
            return Err(format!(
                "单请求超时须在 {TIMEOUT_MIN_MS}..={TIMEOUT_MAX_MS} 毫秒"
            ));
        }
        match self.route_mode {
            RouteMode::ManualLocalProxy => {
                if self
                    .manual_local_proxy_url
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
                {
                    return Err("手动代理模式必须提供本地代理地址".to_string());
                }
            }
            _ => {
                if self.manual_local_proxy_url.is_some() {
                    return Err("仅手动代理模式允许携带代理地址".to_string());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticError {
    pub kind: ErrorKind,
    pub summary: String,
    pub detail: Option<String>,
}

/// 一次探测 = 一条样本（docs/ARCHITECTURE.md §5.4）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeSample {
    pub target_id: String,
    pub probe_type: ProbeType,
    pub route_mode: RouteMode,
    pub attempt_no: u32,
    /// https：仅 2xx 为 true；dns/tcp：连通/解析成功即 true。
    pub success: bool,
    /// 耗时口径（D-017）：
    /// - https：只要收到 HTTP 响应（任意状态码，含 401/403/5xx）即保留真实耗时；
    /// - dns/tcp：成功时保留耗时；
    /// - 传输失败/超时/取消一律 None——未完成的测量不得进入耗时分布。
    pub duration_ms: Option<u64>,
    pub http_status: Option<u16>,
    pub http_outcome: Option<HttpOutcome>,
    /// 仅传输层失败；收到 HTTP 响应不产生 error。
    pub error: Option<DiagnosticError>,
    /// RFC3339 UTC。
    pub tested_at: String,
}

impl ProbeSample {
    /// 已取消样本：未产生网络结论，`duration` 恒为 None（D-014：不进入失败率分母）。
    pub fn cancelled(
        target_id: &str,
        probe_type: ProbeType,
        route_mode: RouteMode,
        attempt_no: u32,
    ) -> Self {
        Self {
            target_id: target_id.to_string(),
            probe_type,
            route_mode,
            attempt_no,
            success: false,
            duration_ms: None,
            http_status: None,
            http_outcome: None,
            error: Some(DiagnosticError {
                kind: ErrorKind::Cancelled,
                summary: "任务已取消，本次样本未产生结论".to_string(),
                detail: None,
            }),
            tested_at: now_rfc3339(),
        }
    }
}

/// 每目标 × 每探测类型的聚合（D-011 口径 + D-014 取消口径）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeAggregate {
    pub probe_type: ProbeType,
    /// 全部样本数（含已取消）。
    pub attempts_total: u32,
    /// 用户取消 / 任务截止未完成的样本：单独计数，不进入任何失败率的分母（D-014）。
    pub cancelled_count: u32,
    /// 执行完成且成功（https 仅 2xx；dns/tcp 连通/解析成功）。
    pub success_count: u32,
    /// 传输层失败：执行完成但未收到任何 HTTP 响应（不含已取消样本）。
    pub transport_failure_count: u32,
    /// 收到 HTTP 响应的样本数（任意状态码；仅 https 可能大于 0）。
    pub http_response_count: u32,
    /// (executed_total - success_count) / executed_total，PRD 总览口径；分母不含已取消样本。
    pub request_failure_rate: f64,
    pub duration_min_ms: Option<u64>,
    pub duration_median_ms: Option<u64>,
    pub duration_max_ms: Option<u64>,
}

impl ProbeAggregate {
    /// 将同一 (target_id, probe_type) 的样本聚合。
    ///
    /// 逐样本互斥分类（D-011 + D-014）：
    /// - `error.kind == cancelled` → 已取消（独立计数）；
    /// - 其他 `error` → 传输失败（未收到 HTTP 响应）；
    /// - 有 `http_status` → 收到响应（`success` 仅当 2xx）；
    /// - 其余（dns/tcp 成功）→ 成功；
    /// - 防御分支（无 error、无响应又标记失败）→ 归入传输失败。
    ///
    /// 恒等式：`attempts_total = success_count + transport_failure_count
    ///          + non_2xx_count + cancelled_count`（non_2xx = http_response_count - success_count）。
    ///
    /// 耗时分布只包含 `duration_ms` 非空的样本；超时/取消样本不进入。
    pub fn from_samples(probe_type: ProbeType, samples: &[ProbeSample]) -> Self {
        let attempts_total = samples.len() as u32;
        let mut cancelled_count = 0u32;
        let mut success_count = 0u32;
        let mut transport_failure_count = 0u32;
        let mut http_response_count = 0u32;
        let mut durations: Vec<u64> = Vec::new();

        for sample in samples {
            if let Some(duration) = sample.duration_ms {
                durations.push(duration);
            }
            if sample
                .error
                .as_ref()
                .is_some_and(|error| error.kind == ErrorKind::Cancelled)
            {
                cancelled_count += 1;
            } else if sample.error.is_some() {
                transport_failure_count += 1;
            } else if sample.http_status.is_some() {
                http_response_count += 1;
                if sample.success {
                    success_count += 1;
                }
            } else if sample.success {
                success_count += 1;
            } else {
                transport_failure_count += 1;
            }
        }

        durations.sort_unstable();
        let (min, median, max) = match durations.len() {
            0 => (None, None, None),
            n if n % 2 == 1 => (
                durations.first().copied(),
                Some(durations[n / 2]),
                durations.last().copied(),
            ),
            // 偶数个样本：中位数取中间两值的均值（整除，向下取整）。
            n => (
                durations.first().copied(),
                Some((durations[n / 2 - 1] + durations[n / 2]) / 2),
                durations.last().copied(),
            ),
        };

        let executed_total = attempts_total - cancelled_count;
        let request_failure_rate = if executed_total == 0 {
            0.0
        } else {
            f64::from(executed_total - success_count) / f64::from(executed_total)
        };

        Self {
            probe_type,
            attempts_total,
            cancelled_count,
            success_count,
            transport_failure_count,
            http_response_count,
            request_failure_rate,
            duration_min_ms: min,
            duration_median_ms: median,
            duration_max_ms: max,
        }
    }

    /// 实际执行（未取消）的样本数；失败率口径的分母。
    pub fn executed_total(&self) -> u32 {
        self.attempts_total - self.cancelled_count
    }

    /// 传输失败率 = transport_failure_count / executed_total（D-014：取消样本不入分母）。
    /// 全部样本被取消时返回 None（口径不适用）。
    pub fn transport_failure_rate(&self) -> Option<f64> {
        let executed = self.executed_total();
        if executed == 0 {
            None
        } else {
            Some(f64::from(self.transport_failure_count) / f64::from(executed))
        }
    }

    /// 非 2xx 响应比例 = non_2xx / http_response_count，分母是"收到响应"的样本。
    /// dns/tcp 或无响应样本时返回 None（该口径不适用）。
    pub fn non_2xx_response_ratio(&self) -> Option<f64> {
        if self.http_response_count == 0 {
            return None;
        }
        let non_2xx = self.http_response_count - self.success_count;
        Some(f64::from(non_2xx) / f64::from(self.http_response_count))
    }
}

/// 一个目标的完整结果：样本 + 各探测类型聚合。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetResult {
    pub target_id: String,
    pub samples: Vec<ProbeSample>,
    pub aggregates: Vec<ProbeAggregate>,
}

/// 一次诊断运行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticRun {
    pub run_id: String,
    /// RFC3339 UTC。
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: RunStatus,
    pub route_mode: RouteMode,
    pub repeats: u8,
    pub app_version: String,
    pub targets: Vec<TargetResult>,
}

/// 进度事件状态：任务开始 / 单个样本完成。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStatus {
    Running,
    SampleFinished,
}

/// `diagnostic://progress` 事件载荷。样本完成时携带该样本本体，
/// 前端无需轮询即可实时呈现（终态以 completed/failed 事件为准）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    pub job_id: String,
    /// 0..=100，按"已完成样本数 / 计划样本数"计算。
    pub percent: u8,
    pub status: ProgressStatus,
    pub message: Option<String>,
    pub sample: Option<ProbeSample>,
}

/// `diagnostic://completed` 事件载荷：终态运行结果（含已取消状态）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedPayload {
    pub job_id: String,
    pub run: DiagnosticRun,
}

/// `diagnostic://failed` 事件载荷：任务级失败（未产生可归因样本的内部错误）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedPayload {
    pub job_id: String,
    pub error: IpcError,
}

/// 任务事件。`Completed` 是包含已取消在内的正常终态（run.status 区分）；
/// `Failed` 是任务级失败。终态事件的"恰好一个"保证由
/// `scheduler::TerminalGuard` 实现：正常完成发 Completed，异常终止（含 panic）
/// 经 Drop 兜底发 Failed，二者互斥且必居其一。
#[derive(Debug, Clone)]
pub enum JobEvent {
    Progress(ProgressPayload),
    Completed(CompletedPayload),
    Failed(FailedPayload),
}

/// 当前 UTC 时间的 RFC3339 字符串（毫秒精度，Z 后缀）。
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// `start_diagnostic` 的返回句柄；Phase 2 起用于进度事件与取消。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobHandle {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelOutcome {
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterSummary {
    pub name: String,
    pub adapter_type: Option<String>,
    pub status_up: bool,
    /// 展示允许；导出时默认移除（脱敏在 Rust 导出层执行）。
    pub ipv4: Option<String>,
}

/// 本机网络/系统快照（只读、非敏感摘要，Phase 2 起提供真实数据）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub os_version: String,
    pub arch: String,
    pub app_version: String,
    pub adapters: Vec<AdapterSummary>,
    pub dns_servers: Vec<String>,
    /// RFC3339 UTC。
    pub captured_at: String,
}

/// 来源 1：当前用户 WinINet 代理配置（Phase 3 实装）。
/// `status="ok"` 时各 Option 有意义（None=确认未配置）；
/// `status="unknown"` 时全部 Option=None——**读取失败绝不伪造"未启用"**。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsUserProxyInfo {
    /// 数据来源标签（如 `wininet_api:WinHttpGetIEProxyConfigForCurrentUser`）。
    pub source: String,
    /// RFC3339 UTC。
    pub captured_at: String,
    /// "ok" | "unknown"。
    pub status: String,
    pub read_error: Option<String>,
    /// 自动检测（AutoDetect）状态。
    pub auto_detect: Option<bool>,
    /// PAC 自动配置 URL（脱敏后；仅展示，本工具不下载/不执行脚本）。
    pub pac_url: Option<String>,
    /// 手动代理服务器（脱敏后，可能为 per-protocol 格式）。
    pub manual_proxy: Option<String>,
    /// 代理例外列表。
    pub proxy_bypass: Option<String>,
}

/// 来源 2：WinHTTP 默认代理配置（机器级，`netsh winhttp` 所设置的值）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WinHttpProxyInfo {
    pub source: String,
    /// RFC3339 UTC。
    pub captured_at: String,
    /// "ok" | "unknown"。
    pub status: String,
    pub read_error: Option<String>,
    /// "default" | "direct" | "named_proxy" | "other(n)"。
    pub access_type: Option<String>,
    pub proxy: Option<String>,
    pub bypass: Option<String>,
}

/// 来源 3：代理相关环境变量（存在状态 + 脱敏值）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarInfo {
    pub name: String,
    pub present: bool,
    /// Some = 已设置（脱敏值）；None = 未设置或为空。
    pub value_redacted: Option<String>,
    pub note: String,
}

/// Windows 代理配置只读快照：三个来源分别检测、分别标注（Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxySnapshot {
    pub user_settings: WindowsUserProxyInfo,
    pub winhttp: WinHttpProxyInfo,
    pub env_vars: Vec<EnvVarInfo>,
    /// 便捷布尔：仅对应来源 status=ok 时有效，否则 None（未知）。
    pub manual_proxy_enabled: Option<bool>,
    pub pac_configured: Option<bool>,
    pub auto_detect_enabled: Option<bool>,
    /// RFC3339 UTC。
    pub captured_at: String,
    /// 免责说明（配置存在 ≠ 真实请求使用；PAC 不下载不执行）。
    pub note: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::targets;

    fn http_sample(
        attempt: u32,
        status: Option<u16>,
        success: bool,
        duration_ms: Option<u64>,
    ) -> ProbeSample {
        ProbeSample {
            target_id: "github".to_string(),
            probe_type: ProbeType::Https,
            route_mode: RouteMode::NoAppProxy,
            attempt_no: attempt,
            success,
            duration_ms,
            http_status: status,
            http_outcome: status.map(|code| match code {
                200..=299 => HttpOutcome::Ok2xx,
                300..=399 => HttpOutcome::Redirect3xx,
                401 | 403 => HttpOutcome::AuthRequired401To403,
                407 => HttpOutcome::ProxyAuth407,
                429 => HttpOutcome::RateLimited429,
                500..=599 => HttpOutcome::ServerError5xx,
                _ => HttpOutcome::ClientErrorOther4xx,
            }),
            error: None,
            tested_at: "2026-10-24T00:00:00Z".to_string(),
        }
    }

    fn transport_sample(attempt: u32, kind: ErrorKind, duration_ms: Option<u64>) -> ProbeSample {
        ProbeSample {
            target_id: "github".to_string(),
            probe_type: ProbeType::Https,
            route_mode: RouteMode::NoAppProxy,
            attempt_no: attempt,
            success: false,
            duration_ms,
            http_status: None,
            http_outcome: None,
            error: Some(DiagnosticError {
                kind,
                summary: "模拟传输失败".to_string(),
                detail: None,
            }),
            tested_at: "2026-10-24T00:00:00Z".to_string(),
        }
    }

    fn valid_request() -> DiagnosticRequest {
        DiagnosticRequest {
            target_ids: vec!["github".to_string()],
            route_mode: RouteMode::NoAppProxy,
            manual_local_proxy_url: None,
            repeats: 3,
            timeout_ms: TIMEOUT_DEFAULT_MS,
        }
    }

    #[test]
    fn diagnostic_request_round_trips_snake_case() {
        // PRD §5 的样例输入必须原样解析。
        let raw = r#"{"target_ids":["openai-api","github"],"route_mode":"no_app_proxy","manual_local_proxy_url":null,"repeats":3,"timeout_ms":10000}"#;
        let request: DiagnosticRequest = serde_json::from_str(raw).expect("解析 PRD 样例");
        assert_eq!(request.route_mode, RouteMode::NoAppProxy);
        assert_eq!(request.repeats, 3);
        assert_eq!(request.timeout_ms, 10_000);
        assert!(request.manual_local_proxy_url.is_none());

        let out = serde_json::to_string(&request).expect("序列化");
        assert!(out.contains("\"route_mode\":\"no_app_proxy\""));
        assert!(out.contains("\"target_ids\""));
        assert!(out.contains("\"timeout_ms\""));
    }

    #[test]
    fn enum_literals_are_snake_case() {
        assert_eq!(
            serde_json::to_string(&RouteMode::NoAppProxy).unwrap(),
            "\"no_app_proxy\""
        );
        assert_eq!(
            serde_json::to_string(&RouteMode::ManualLocalProxy).unwrap(),
            "\"manual_local_proxy\""
        );
        assert_eq!(
            serde_json::to_string(&ProbeType::Https).unwrap(),
            "\"https\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorKind::DnsFailure).unwrap(),
            "\"dns_failure\""
        );
        assert_eq!(
            serde_json::to_string(&IpcErrorCode::NotAvailable).unwrap(),
            "\"not_available\""
        );
        assert_eq!(
            serde_json::to_string(&HttpOutcome::Ok2xx).unwrap(),
            "\"ok_2xx\""
        );
        assert_eq!(
            serde_json::to_string(&HttpOutcome::AuthRequired401To403).unwrap(),
            "\"auth_required_401_403\""
        );
        assert_eq!(
            serde_json::to_string(&RunStatus::Cancelled).unwrap(),
            "\"cancelled\""
        );
    }

    #[test]
    fn ipc_error_serializes_structured_payload() {
        let error = IpcError::not_available("测试");
        let out = serde_json::to_string(&error).unwrap();
        assert!(out.contains("\"code\":\"not_available\""));
        assert!(out.contains("\"message\":\"测试\""));
    }

    #[test]
    fn request_validation_rejects_bad_input() {
        let mut request = valid_request();

        request.repeats = 4;
        assert!(request.validate().is_err(), "repeats 仅允许 3/5");

        request = valid_request();
        request.timeout_ms = 20_000;
        assert!(request.validate().is_err(), "超时上限 10000");

        request = valid_request();
        request.timeout_ms = 900;
        assert!(request.validate().is_err(), "超时下限 1000");

        request = valid_request();
        request.target_ids.clear();
        assert!(request.validate().is_err(), "目标不能为空");

        request = valid_request();
        request.target_ids = vec!["unknown-target".to_string()];
        assert!(request.validate().is_err(), "未知目标");

        request = valid_request();
        request.target_ids = vec!["github".to_string(), "github".to_string()];
        assert!(request.validate().is_err(), "重复目标");

        request = valid_request();
        request.route_mode = RouteMode::ManualLocalProxy;
        request.manual_local_proxy_url = None;
        assert!(request.validate().is_err(), "手动模式必须带地址");

        request = valid_request();
        request.manual_local_proxy_url = Some("http://127.0.0.1:7890".to_string());
        assert!(request.validate().is_err(), "非手动模式不得带地址");
    }

    #[test]
    fn request_validation_accepts_valid_input() {
        let mut request = valid_request();
        assert!(request.validate().is_ok());

        request.target_ids = targets::TARGETS.iter().map(|t| t.id.to_string()).collect();
        assert!(request.validate().is_ok(), "全部 6 个目标合法");

        request.route_mode = RouteMode::ManualLocalProxy;
        request.manual_local_proxy_url = Some("socks5h://127.0.0.1:7890".to_string());
        assert!(request.validate().is_ok());

        request.repeats = 5;
        request.timeout_ms = TIMEOUT_MIN_MS;
        assert!(request.validate().is_ok());
    }

    #[test]
    fn aggregate_splits_transport_failures_from_http_responses() {
        // 5 个 https 样本：2×200 成功、1×401（保留真实耗时，D-017）、1×TCP 失败、1×超时。
        let samples = vec![
            http_sample(1, Some(200), true, Some(100)),
            http_sample(2, Some(200), true, Some(200)),
            http_sample(3, Some(401), false, Some(300)),
            transport_sample(4, ErrorKind::TcpFailure, None),
            transport_sample(5, ErrorKind::Timeout, None),
        ];
        let aggregate = ProbeAggregate::from_samples(ProbeType::Https, &samples);

        assert_eq!(aggregate.attempts_total, 5);
        assert_eq!(aggregate.success_count, 2);
        assert_eq!(aggregate.http_response_count, 3, "401 也是收到响应");
        assert_eq!(aggregate.transport_failure_count, 2);
        assert!((aggregate.request_failure_rate - 0.6).abs() < 1e-9);
        assert!((aggregate.transport_failure_rate().unwrap() - 0.4).abs() < 1e-9);
        // 非 2xx 比例 = (3-2)/3，分母是收到响应的样本（D-011）。
        let ratio = aggregate.non_2xx_response_ratio().expect("https 有响应");
        assert!((ratio - 1.0 / 3.0).abs() < 1e-9);

        // 耗时分布含"收到响应"的样本（2xx 与 401，D-017）；
        // 传输失败/超时/取消无耗时，不进入分布。
        assert_eq!(aggregate.duration_min_ms, Some(100));
        assert_eq!(aggregate.duration_median_ms, Some(200));
        assert_eq!(aggregate.duration_max_ms, Some(300));
    }

    #[test]
    fn aggregate_median_even_takes_mean_of_middle_two() {
        let samples: Vec<ProbeSample> = [10u64, 20, 30, 40]
            .iter()
            .enumerate()
            .map(|(i, d)| http_sample(i as u32 + 1, Some(200), true, Some(*d)))
            .collect();
        let aggregate = ProbeAggregate::from_samples(ProbeType::Https, &samples);
        assert_eq!(aggregate.duration_median_ms, Some(25));
        assert_eq!(aggregate.duration_min_ms, Some(10));
        assert_eq!(aggregate.duration_max_ms, Some(40));
    }

    #[test]
    fn dns_tcp_samples_do_not_count_as_http_responses() {
        let dns_ok = ProbeSample {
            target_id: "github".to_string(),
            probe_type: ProbeType::Dns,
            route_mode: RouteMode::NoAppProxy,
            attempt_no: 1,
            success: true,
            duration_ms: Some(12),
            http_status: None,
            http_outcome: None,
            error: None,
            tested_at: "2026-10-24T00:00:00Z".to_string(),
        };
        let dns_fail = transport_sample(2, ErrorKind::DnsFailure, None);
        let aggregate = ProbeAggregate::from_samples(ProbeType::Dns, &[dns_ok, dns_fail]);

        assert_eq!(aggregate.attempts_total, 2);
        assert_eq!(aggregate.success_count, 1);
        assert_eq!(aggregate.http_response_count, 0);
        assert_eq!(aggregate.transport_failure_count, 1);
        assert!(aggregate.non_2xx_response_ratio().is_none());
    }

    #[test]
    fn aggregate_excludes_cancelled_from_failure_rates() {
        // D-014：取消样本单独计数，不进入失败率分母。
        // 4 个 https 样本：1×200 成功、1×TCP 失败、2×用户取消。
        let samples = vec![
            http_sample(1, Some(200), true, Some(100)),
            transport_sample(2, ErrorKind::TcpFailure, None),
            transport_sample(3, ErrorKind::Cancelled, None),
            transport_sample(4, ErrorKind::Cancelled, None),
        ];
        let aggregate = ProbeAggregate::from_samples(ProbeType::Https, &samples);

        assert_eq!(aggregate.attempts_total, 4);
        assert_eq!(aggregate.cancelled_count, 2);
        assert_eq!(aggregate.executed_total(), 2);
        assert_eq!(aggregate.transport_failure_count, 1);
        assert!((aggregate.transport_failure_rate().unwrap() - 0.5).abs() < 1e-9);
        assert!((aggregate.request_failure_rate - 0.5).abs() < 1e-9);
        // 恒等式：4 = 1 成功 + 1 传输失败 + 0 非2xx + 2 取消。
        let non_2xx = aggregate.http_response_count - aggregate.success_count;
        assert_eq!(
            aggregate.success_count
                + aggregate.transport_failure_count
                + non_2xx
                + aggregate.cancelled_count,
            aggregate.attempts_total
        );
    }

    #[test]
    fn aggregate_handles_all_cancelled() {
        let samples = vec![
            transport_sample(1, ErrorKind::Cancelled, None),
            transport_sample(2, ErrorKind::Cancelled, None),
        ];
        let aggregate = ProbeAggregate::from_samples(ProbeType::Https, &samples);
        assert_eq!(aggregate.cancelled_count, 2);
        assert_eq!(aggregate.executed_total(), 0);
        assert!(aggregate.transport_failure_rate().is_none());
        assert_eq!(aggregate.request_failure_rate, 0.0);
    }

    #[test]
    fn progress_payload_serializes_snake_case() {
        let payload = ProgressPayload {
            job_id: "job-1".to_string(),
            percent: 40,
            status: ProgressStatus::SampleFinished,
            message: None,
            sample: None,
        };
        let out = serde_json::to_string(&payload).unwrap();
        assert!(out.contains("\"job_id\":\"job-1\""));
        assert!(out.contains("\"status\":\"sample_finished\""));
    }

    #[test]
    fn now_rfc3339_is_utc_zulu() {
        let stamp = now_rfc3339();
        assert!(stamp.ends_with('Z'), "应为 Z 后缀 UTC：{stamp}");
        assert!(stamp.contains('T'));
    }

    #[test]
    fn aggregate_handles_empty_input() {
        let aggregate = ProbeAggregate::from_samples(ProbeType::Https, &[]);
        assert_eq!(aggregate.attempts_total, 0);
        assert_eq!(aggregate.request_failure_rate, 0.0);
        assert_eq!(aggregate.duration_min_ms, None);
        assert!(aggregate.non_2xx_response_ratio().is_none());
    }
}
