/**
 * 前后端共享 DTO —— 与 src-tauri/src/diagnostic/types.rs 一一对应。
 * 线上格式：字段名与枚举字面量一律 snake_case（docs/ARCHITECTURE.md §5，D-003/D-013）。
 */

export type RouteMode = 'library_default' | 'no_app_proxy' | 'manual_local_proxy';

export type ProbeType = 'dns' | 'tcp' | 'https';

export type RunStatus = 'running' | 'completed' | 'cancelled' | 'failed';

/** 传输层失败分类；收到 HTTP 响应不属于错误（D-007）。 */
export type ErrorKind =
  | 'dns_failure'
  | 'tcp_failure'
  | 'tls_failure'
  | 'timeout'
  | 'cancelled'
  | 'proxy_unavailable'
  | 'invalid_input'
  | 'internal'
  | 'unknown';

/** HTTPS 收到响应时的细分；success 仅 2xx（D-007/D-011）。 */
export type HttpOutcome =
  | 'ok_2xx'
  | 'redirect_3xx'
  | 'auth_required_401_403'
  | 'proxy_auth_407'
  | 'rate_limited_429'
  | 'server_error_5xx'
  | 'client_error_other_4xx';

export type IpcErrorCode =
  | 'invalid_input'
  | 'not_found'
  | 'not_available'
  | 'internal'
  | 'cancelled';

export interface IpcError {
  code: IpcErrorCode;
  message: string;
  detail?: string | null;
}

export interface DiagnosticError {
  kind: ErrorKind;
  summary: string;
  detail?: string | null;
}

export interface ProbeSample {
  target_id: string;
  probe_type: ProbeType;
  route_mode: RouteMode;
  attempt_no: number;
  success: boolean;
  duration_ms: number | null;
  http_status: number | null;
  http_outcome: HttpOutcome | null;
  error: DiagnosticError | null;
  /** RFC3339 UTC。 */
  tested_at: string;
}

export interface ProbeAggregate {
  probe_type: ProbeType;
  /** 全部样本数（含已取消）。 */
  attempts_total: number;
  /** 取消/截止未完成的样本：不进入失败率分母（D-014）。 */
  cancelled_count: number;
  success_count: number;
  transport_failure_count: number;
  http_response_count: number;
  request_failure_rate: number;
  duration_min_ms: number | null;
  duration_median_ms: number | null;
  duration_max_ms: number | null;
}

export interface TargetResult {
  target_id: string;
  samples: ProbeSample[];
  aggregates: ProbeAggregate[];
}

export interface DiagnosticRun {
  run_id: string;
  started_at: string;
  finished_at: string | null;
  status: RunStatus;
  route_mode: RouteMode;
  repeats: number;
  app_version: string;
  targets: TargetResult[];
}

export interface DiagnosticRequest {
  target_ids: string[];
  route_mode: RouteMode;
  manual_local_proxy_url: string | null;
  repeats: number;
  timeout_ms: number;
}

export interface JobHandle {
  job_id: string;
}

export interface CancelOutcome {
  cancelled: boolean;
}

/** diagnostic://progress 事件载荷。 */
export type ProgressStatus = 'running' | 'sample_finished';

export interface ProgressPayload {
  job_id: string;
  percent: number;
  status: ProgressStatus;
  message: string | null;
  sample: ProbeSample | null;
}

/** diagnostic://completed 事件载荷（含已取消状态，run.status 区分）。 */
export interface CompletedPayload {
  job_id: string;
  run: DiagnosticRun;
}

/** diagnostic://failed 事件载荷：任务级失败。 */
export interface FailedPayload {
  job_id: string;
  error: IpcError;
}

/** Windows 代理配置快照（三个来源分别标注；Phase 3）。 */
export interface WindowsUserProxyInfo {
  source: string;
  captured_at: string;
  /** "ok" | "unknown"；unknown 时各字段 None（读取失败绝不伪造"未启用"）。 */
  status: string;
  read_error: string | null;
  auto_detect: boolean | null;
  pac_url: string | null;
  manual_proxy: string | null;
  proxy_bypass: string | null;
}

export interface WinHttpProxyInfo {
  source: string;
  captured_at: string;
  status: string;
  read_error: string | null;
  access_type: string | null;
  proxy: string | null;
  bypass: string | null;
}

export interface EnvVarInfo {
  name: string;
  present: boolean;
  value_redacted: string | null;
  note: string;
}

export interface ProxySnapshot {
  user_settings: WindowsUserProxyInfo;
  winhttp: WinHttpProxyInfo;
  env_vars: EnvVarInfo[];
  manual_proxy_enabled: boolean | null;
  pac_configured: boolean | null;
  auto_detect_enabled: boolean | null;
  captured_at: string;
  note: string;
}

/** 规则诊断结论（Phase 4，Rust rules.rs 一一对应）。 */
export type Severity = 'info' | 'warning' | 'critical';

/** 证据充分程度——不是根因概率。 */
export type Confidence = 'evidence_sufficient' | 'possible' | 'insufficient';

export interface Diagnosis {
  rule_id: string;
  title: string;
  severity: Severity;
  confidence: Confidence;
  observed_facts: string[];
  possible_causes: string[];
  verification_steps: string[];
  limitations: string[];
  related_target_ids: string[];
  related_probe_types: ProbeType[];
}

/** 历史记录（Phase 5，Rust storage::RunSummary 一一对应）。 */
export interface RunSummary {
  run_id: string;
  started_at: string;
  finished_at: string | null;
  status: string;
  route_mode: string;
  repeats: number;
  app_version: string;
  target_ids: string[];
  sample_count: number;
  success_count: number;
  transport_failure_count: number;
  https_median_ms: number | null;
}

export interface RunQuery {
  target_id?: string | null;
  route_mode?: RouteMode | null;
  limit?: number | null;
}

export type ReportFormat = 'json' | 'markdown';

export interface ReportPreview {
  run_id: string;
  format: ReportFormat;
  content: string;
  filename: string;
  note: string;
  /** 服务端不可变快照 id（F-07）：导出时透传，复用同一份脱敏字节。 */
  snapshot_id: string;
}

export interface ExportResult {
  run_id: string;
  format: ReportFormat;
  path: string;
  size_bytes: number;
}

export interface AdapterSummary {
  name: string;
  adapter_type: string | null;
  status_up: boolean;
  ipv4: string | null;
}

export interface SystemSnapshot {
  os_version: string;
  arch: string;
  app_version: string;
  adapters: AdapterSummary[];
  dns_servers: string[];
  captured_at: string;
}

export interface TargetSpec {
  id: string;
  label: string;
  host: string;
  url: string;
}

/** 校验基线（与 Rust 侧常量一致）。 */
export const REPEATS_ALLOWED = [3, 5] as const;
export const TIMEOUT_MIN_MS = 1000;
export const TIMEOUT_MAX_MS = 10000;
export const TIMEOUT_DEFAULT_MS = 10000;
export const MAX_TARGETS = 6;

/**
 * 预设目标（仅用于渲染表单）。
 * 注意：探测目标的最终裁决在后端 targets.rs 白名单——本清单仅是展示副本，
 * 两者不一致时以后端校验为准（未知目标会被拒绝）。
 */
export const PRESET_TARGETS: TargetSpec[] = [
  { id: 'chatgpt', label: 'ChatGPT', host: 'chatgpt.com', url: 'https://chatgpt.com/' },
  { id: 'openai-api', label: 'OpenAI API', host: 'api.openai.com', url: 'https://api.openai.com/' },
  { id: 'openai-auth', label: 'OpenAI 登录', host: 'auth.openai.com', url: 'https://auth.openai.com/' },
  { id: 'claude', label: 'Claude', host: 'claude.ai', url: 'https://claude.ai/' },
  { id: 'github', label: 'GitHub', host: 'github.com', url: 'https://github.com/' },
  { id: 'zcode', label: 'ZCode', host: 'zcode.z.ai', url: 'https://zcode.z.ai/' },
];

export const ROUTE_MODE_LABELS: Record<RouteMode, string> = {
  library_default: '库默认（library_default）',
  no_app_proxy: '无应用代理（no_app_proxy）',
  manual_local_proxy: '手动本地代理（manual_local_proxy）',
};

export const ERROR_KIND_LABELS: Record<ErrorKind, string> = {
  dns_failure: 'DNS 解析失败',
  tcp_failure: 'TCP 连接失败',
  tls_failure: 'TLS 握手失败',
  timeout: '超时（未收到响应）',
  cancelled: '已取消',
  proxy_unavailable: '指定代理不可用',
  invalid_input: '请求参数不合法',
  internal: '工具内部错误',
  unknown: '原因未明',
};
