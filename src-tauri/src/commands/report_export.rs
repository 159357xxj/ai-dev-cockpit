//! 报告导出的安全投影与渲染（V6-R2，契约 §2 T-1..T-6）。
//!
//! 硬约束（复核时按这几条看即可）：
//!
//! 1. **渲染器只接收 `ExportReport`**（T-2）：`render_json` / `render_markdown`
//!    的参数类型就是 DTO 本身，预览与导出都经由它，不存在解析内部
//!    `DiagnosticRun` / `Diagnosis` 的第二条渲染路径。
//! 2. **DTO 中没有裸 `String` 字段**：文本只可能是
//!    - `SafeTargetLabel`（白名单 `id → &'static str` 映射，T-1）；
//!    - `&'static str` 静态文案（含 `REPORT_NOTE` 与逐规则文案表）；
//!    - 本模块内唯一构造入口生成的 newtype：`SafeRunId`（规范化 run_id）、
//!      `SafeTimestamp`（chrono 规范化的 UTC 时间戳）、`SafeAppVersion`
//!      （受限字符集版本号）、`SafeDiagnosisTitle`（只由 `diagnosis_title`
//!      生成）。
//! 3. **诊断标题不经引擎**（T-3）：`diagnosis_title(rule_id, label)` 用静态
//!    模板重新拼装，引擎的 `title` / `observed_facts` 自由文本一律不进入
//!    DTO；只有 `rule_id` / `severity` / `confidence` / `related_*` 与逐规则
//!    静态文案被采用。
//! 4. **不导出错误自由文本**（T-5）：样本只保留 `error_kind` 枚举，
//!    `error.summary` / `error.detail` 没有任何字段承载。
//! 5. **不存在动态文本进入渲染**（T-6）：所有插入渲染器的内容都已是
//!    `&'static str` 或受限字符集的规范化结果，CR/LF 与控制字符无法到达
//!    输出；因此也不需要（也不再使用）Markdown 转义函数。

use serde::Serialize;

use crate::diagnostic::rules::{Confidence, Diagnosis, Severity};
use crate::diagnostic::targets::{TARGETS, TargetSpec};
use crate::diagnostic::types::{
    DiagnosticRun, ErrorKind, HttpOutcome, ProbeSample, ProbeType, RouteMode, RunStatus,
};

/// 非白名单目标的固定占位符（T-1）。
const UNKNOWN_TARGET: &str = "unknown-target";

/// 目标白名单的唯一来源（`diagnostic::targets::TARGETS`）。
const WHITELIST: &[TargetSpec] = TARGETS;

// ---------------------------------------------------------------------------
// 安全文本类型：全部字段都是 newtype，构造入口在本模块内唯一
// ---------------------------------------------------------------------------

/// 安全目标名（T-1）：只能由白名单的固定映射构造，不实现 `From<String>` /
/// `From<&str>`，因此渲染层无法凭空产生目标名。比对为**完全相等**，
/// 不做 trim、大小写转换或前缀匹配。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SafeTargetLabel(&'static str);

impl SafeTargetLabel {
    /// `target_id → &'static str` 白名单映射；任何非白名单值一律
    /// `unknown-target`（fail-closed）。
    pub fn from_target_id(raw: &str) -> Self {
        match WHITELIST.iter().find(|spec| spec.id == raw) {
            Some(spec) => SafeTargetLabel(spec.label),
            None => SafeTargetLabel(UNKNOWN_TARGET),
        }
    }

    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

/// 规范化 run_id：由 UUID 形态校验生成，非法值退化为固定占位符。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SafeRunId(String);

impl SafeRunId {
    pub fn from_raw(raw: &str) -> Self {
        Self(normalize_run_id(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 规范化时间戳：chrono 解析为 RFC3339 后统一输出 UTC 毫秒精度；
/// 非法值退化为固定占位符。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SafeTimestamp(String);

impl SafeTimestamp {
    pub fn from_raw(raw: &str) -> Self {
        Self(normalize_timestamp(raw))
    }

    /// 导出时刻（本地生成的 UTC 时间，与其它时间戳同一规范化路径）。
    pub fn now() -> Self {
        Self::from_raw(&crate::diagnostic::types::now_rfc3339())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 受限字符集的版本号：仅数字/字母/`.`/`+`/`-`，长度 ≤ 32；否则 `unknown`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SafeAppVersion(String);

impl SafeAppVersion {
    pub fn from_raw(raw: &str) -> Self {
        Self(normalize_app_version(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 诊断标题：唯一的构造入口是 [`SafeDiagnosisTitle::from_rule`]（即
/// `diagnosis_title` 的静态模板），引擎原始标题无法进入 DTO（T-3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SafeDiagnosisTitle(String);

impl SafeDiagnosisTitle {
    fn from_rule(rule: RuleId, target: SafeTargetLabel) -> Self {
        Self(diagnosis_title(rule, target))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// 规则编号与逐规则静态文案
// ---------------------------------------------------------------------------

/// 规则编号（R01..R12）的封闭枚举：任何未知字符串都落到 `Unknown`，
/// 不进入 DTO 原文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleId {
    R01,
    R02,
    R03,
    R04,
    R05,
    R06,
    R07,
    R08,
    R09,
    R10,
    R11,
    R12,
    Unknown,
}

impl RuleId {
    /// 完全相等匹配（不做 trim / 大小写转换）。
    pub fn from_raw(raw: &str) -> Self {
        match raw {
            "R01" => RuleId::R01,
            "R02" => RuleId::R02,
            "R03" => RuleId::R03,
            "R04" => RuleId::R04,
            "R05" => RuleId::R05,
            "R06" => RuleId::R06,
            "R07" => RuleId::R07,
            "R08" => RuleId::R08,
            "R09" => RuleId::R09,
            "R10" => RuleId::R10,
            "R11" => RuleId::R11,
            "R12" => RuleId::R12,
            _ => RuleId::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RuleId::R01 => "R01",
            RuleId::R02 => "R02",
            RuleId::R03 => "R03",
            RuleId::R04 => "R04",
            RuleId::R05 => "R05",
            RuleId::R06 => "R06",
            RuleId::R07 => "R07",
            RuleId::R08 => "R08",
            RuleId::R09 => "R09",
            RuleId::R10 => "R10",
            RuleId::R11 => "R11",
            RuleId::R12 => "R12",
            RuleId::Unknown => "unknown-rule",
        }
    }

    /// 全部已知规则（供扫描测试遍历）。
    pub const ALL: [RuleId; 12] = [
        RuleId::R01,
        RuleId::R02,
        RuleId::R03,
        RuleId::R04,
        RuleId::R05,
        RuleId::R06,
        RuleId::R07,
        RuleId::R08,
        RuleId::R09,
        RuleId::R10,
        RuleId::R11,
        RuleId::R12,
    ];
}

impl Serialize for RuleId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// 逐规则静态文案（可能原因 / 建议步骤 / 局限）。
///
/// 内容逐字来自 `diagnostic/rules.rs` 中对应规则的 `possible_causes` /
/// `verification_steps` / `limitations` 字面量；`v6_target_projection` 的
/// `export_static_copy_mirrors_engine_copy` 用源码扫描守护两者一致
/// （防止报告文案与引擎文案悄悄漂移）。
#[derive(Debug, Clone, Copy)]
pub struct RuleProfile {
    pub possible_causes: &'static [&'static str],
    pub verification_steps: &'static [&'static str],
    pub limitations: &'static [&'static str],
}

const EMPTY_PROFILE: RuleProfile = RuleProfile {
    possible_causes: &[],
    verification_steps: &[],
    limitations: &[],
};

/// 诊断标题的**唯一**生成方式（T-3）：静态模板 + 安全目标名。
/// 不使用引擎 `Diagnosis::title` 的任何内容。
pub fn diagnosis_title(rule: RuleId, target: SafeTargetLabel) -> String {
    let label = target.as_str();
    match rule {
        RuleId::R01 => format!("{label}：域名解析失败"),
        RuleId::R02 => format!("{label}：443 端口 TCP 连接建立失败"),
        RuleId::R03 => format!("{label}：HTTPS 的 TLS 握手失败"),
        RuleId::R04 => format!("{label}：HTTPS 请求超时"),
        RuleId::R05 => format!("{label}：返回 HTTP 401/403（收到响应，业务能力未验证）"),
        RuleId::R06 => format!("{label}：代理要求认证（HTTP 407）"),
        RuleId::R07 => format!("{label}：返回 HTTP 429 限流响应"),
        RuleId::R08 => format!("{label}：返回 HTTP 5xx 服务端错误"),
        RuleId::R09 => format!("{label}：指定的本地代理不可用"),
        RuleId::R10 => format!("{label}：不同请求路径下结果不一致"),
        RuleId::R11 => format!("{label}：DNS 与 TCP 基线正常但 HTTPS 全部失败"),
        RuleId::R12 => format!("{label}：样本不完整"),
        RuleId::Unknown => format!("{label}：未识别的诊断规则"),
    }
}

fn rule_profile(rule: RuleId) -> &'static RuleProfile {
    match rule {
        RuleId::R01 => &RuleProfile {
            possible_causes: &[
                "本机解析策略（hosts、DoH、企业 DNS 策略）拦截或改写了该域名",
                "当前网络的 DNS 服务器无响应或对该域名返回失败",
                "该域名在当前网络环境下不可解析",
            ],
            verification_steps: &[
                "用系统命令 nslookup 对同一域名做对比解析（只读）",
                "检查本机 hosts 文件与 DoH/代理客户端的 DNS 设置",
                "更换 DNS 服务器后重测（仅修改本工具之外的配置需自行确认）",
            ],
            limitations: &[
                "本测量代表系统解析器的完整解析过程，不是原始 DNS 报文 RTT",
                "不代表 HTTPS 请求实际经历的解析路径（缓存/连接复用可能不同）",
                "不能据此推断域名被 ISP 封锁或 DNS 污染",
            ],
        },
        RuleId::R02 => &RuleProfile {
            possible_causes: &[
                "本机防火墙或安全软件拦截了出站连接",
                "当前网络到目标 443 端口的路径不可达",
                "目标服务的 443 端口未开放或拒绝来自该网络的连接",
            ],
            verification_steps: &[
                "检查本机防火墙/安全软件的出站规则（只读查看）",
                "切换网络（如手机热点）后重测对比",
                "对其它白名单目标做同样测量，判断是否普遍性问题",
            ],
            limitations: &[
                "这是独立 TCP connect 测量（非 ICMP Ping），耗时口径不同",
                "不能据此断言对方服务器宕机或端口被 ISP 封锁",
            ],
        },
        RuleId::R03 => &RuleProfile {
            possible_causes: &[
                "网络路径上存在对 TLS 握手的干扰（中间设备重置/阻断）",
                "代理路径的隧道质量问题（若经代理）",
                "证书验证失败（本工具始终执行完整证书校验）",
            ],
            verification_steps: &[
                "用浏览器访问同一目标，对比是否同样失败",
                "在路径对比中切换不同模式，观察 TLS 失败是否随路径变化",
                "检查本机系统时间是否正确（影响证书有效期判断）",
            ],
            limitations: &[
                "不能区分干扰来自网络中间设备还是目标侧",
                "偶发失败与系统性失败需更多采样才能区分（见置信标记）",
            ],
        },
        RuleId::R04 => &RuleProfile {
            possible_causes: &[
                "网络路径丢包或拥塞导致响应缓慢",
                "目标服务响应缓慢（含 CDN/网关）",
                "代理路径缓慢（若经代理）",
            ],
            verification_steps: &[
                "提高单请求超时后重测，观察是否完成",
                "对比其它目标的超时情况，判断是否普遍",
                "在路径对比中切换模式，定位慢在哪条路径",
            ],
            limitations: &["不能定位丢包或拥塞发生的位置", "不能据此断言对方服务器宕机"],
        },
        RuleId::R05 => &RuleProfile {
            possible_causes: &[
                "目标对未携带凭证的请求返回的正常响应（本工具从不发送凭证）",
                "目标或其网络边缘对该来源启用了访问控制",
            ],
            verification_steps: &[
                "用浏览器访问同一地址，对比响应状态（只读）",
                "确认本工具未携带任何 Cookie/令牌（设计如此）",
            ],
            limitations: &[
                "401/403 ≠ 服务不可用，也 ≠ 服务可用：只说明 HTTP 层可达且有响应",
                "不能区分响应来自目标服务本身还是网络边缘设备",
                "不能据此推断账号能否正常登录或业务是否正常",
            ],
        },
        RuleId::R06 => &RuleProfile {
            possible_causes: &[
                "本地代理客户端开启了认证要求",
                "代理服务器配置了访问控制策略",
            ],
            verification_steps: &[
                "检查本机代理客户端的认证设置（只读查看）",
                "用 no_app_proxy 模式对比是否绕开了该代理",
            ],
            limitations: &[
                "本工具不发送任何代理凭证，也不会自动通过认证",
                "407 是代理侧要求，与目标服务本身无关",
            ],
        },
        RuleId::R07 => &RuleProfile {
            possible_causes: &[
                "目标服务对请求频率或并发做了限制",
                "当前出口 IP 的共享使用者触发了限流阈值",
            ],
            verification_steps: &[
                "稍后降低采样频率重试",
                "在路径对比中观察限流是否只在特定路径出现",
            ],
            limitations: &[
                "不能推断出口 IP 的信誉状况",
                "限流原因只有目标服务方可以确认",
            ],
        },
        RuleId::R08 => &RuleProfile {
            possible_causes: &["目标服务内部错误", "目标侧网关/CDN 出错"],
            verification_steps: &["稍后重试，观察是否持续", "用浏览器访问同一目标对比"],
            limitations: &[
                "不能断言对方服务器宕机，也不能推断其故障根因",
                "偶发与持续性需更多采样区分（见置信标记）",
            ],
        },
        RuleId::R09 => &RuleProfile {
            possible_causes: &[
                "代理客户端未运行或监听端口与指定不一致",
                "代理协议不匹配（http 与 socks5 混用）",
                "代理客户端拒绝了连接（访问控制）",
            ],
            verification_steps: &[
                "确认代理客户端正在运行并核对监听端口（只读查看）",
                "核对协议前缀（http/socks5/socks5h）与客户端实际协议一致",
                "用 no_app_proxy 模式单独测量目标基线（不经过该代理）",
            ],
            limitations: &[
                "本工具绝不回落直连：这些失败样本不代表目标的直连状态",
                "不能推断代理客户端的具体配置细节",
            ],
        },
        RuleId::R10 => &RuleProfile {
            possible_causes: &[
                "各模式经过的网络路径不同（代理 vs 直连，或不同代理节点）",
                "library_default 的代理选择受本机系统配置影响（见对比页说明）",
                "路径质量随时间波动，同一批次的先后执行也会引入差异",
            ],
            verification_steps: &[
                "在相近时间窗口重复对比多次，观察差异是否稳定",
                "核对各模式说明，确认其语义差异（含 TUN/透明代理边界）",
                "分别查看各模式的完整采样记录",
            ],
            limitations: &[
                "中位数排序不构成节点性能排名，也不代表某路径必然更快",
                "不能推断任何出口 IP 的信誉状况",
                "no_app_proxy 不等同于真实直连（TUN/透明代理仍可能生效）",
            ],
        },
        RuleId::R11 => &RuleProfile {
            possible_causes: &[
                "TLS 层在该路径上被干扰或阻断",
                "HTTPS 所经的应用层代理路径存在问题（与基线路径不同）",
                "目标 HTTPS 服务异常",
            ],
            verification_steps: &[
                "用浏览器访问同一目标，对比 HTTPS 行为",
                "在路径对比中切换模式，定位问题在哪条路径",
                "检查代理客户端日志（若 HTTPS 经代理，只读查看）",
            ],
            limitations: &[
                "三次测量相互独立：不能把 DNS/TCP 结论强行归因到 HTTPS 路径",
                "不能据此断言服务故障或网络封锁",
            ],
        },
        RuleId::R12 => &RuleProfile {
            possible_causes: &["用户主动取消了任务", "任务达到全局截止时间"],
            verification_steps: &[
                "重新运行诊断以获得完整采样",
                "如持续超时，先调低采样次数或检查网络",
            ],
            limitations: &[
                "取消与未执行的样本不代表网络失败，不计入任何失败率（D-014）",
                "已有样本的结论仍然有效，但样本量不支持长期稳定性判断",
                "3 或 5 次采样不能断言服务长期稳定性",
            ],
        },
        RuleId::Unknown => &EMPTY_PROFILE,
    }
}

// ---------------------------------------------------------------------------
// 导出 DTO（T-2）：字段类型受限，无裸 String
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ExportReport {
    pub meta: ExportMeta,
    pub run: ExportRun,
    pub diagnoses: Vec<ExportDiagnosis>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportMeta {
    pub app_version: SafeAppVersion,
    pub exported_at: SafeTimestamp,
    /// 静态说明文案（常量）。
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportRun {
    pub run_id: SafeRunId,
    pub started_at: SafeTimestamp,
    pub finished_at: Option<SafeTimestamp>,
    pub status: RunStatus,
    pub route_mode: RouteMode,
    pub repeats: u8,
    pub app_version: SafeAppVersion,
    pub targets: Vec<ExportTarget>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportTarget {
    /// 值是安全标签（如 `GitHub`），不是原始 id；JSON 与 Markdown 共用
    /// 同一个字段，因此两处名称必然一致（T-4）。
    pub target: SafeTargetLabel,
    pub aggregates: Vec<ExportAggregate>,
    pub samples: Vec<ExportSample>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportAggregate {
    pub probe_type: ProbeType,
    pub attempts_total: u32,
    pub cancelled_count: u32,
    pub success_count: u32,
    pub transport_failure_count: u32,
    pub http_response_count: u32,
    pub request_failure_rate: f64,
    pub duration_min_ms: Option<u64>,
    pub duration_median_ms: Option<u64>,
    pub duration_max_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportSample {
    pub probe_type: ProbeType,
    pub route_mode: RouteMode,
    pub attempt_no: u32,
    pub success: bool,
    pub duration_ms: Option<u64>,
    pub http_status: Option<u16>,
    pub http_outcome: Option<HttpOutcome>,
    /// 只导出结构化错误类别；`error.summary` / `error.detail` 不导出（T-5）。
    pub error_kind: Option<ErrorKind>,
    pub tested_at: SafeTimestamp,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportDiagnosis {
    pub rule_id: RuleId,
    pub title: SafeDiagnosisTitle,
    pub severity: Severity,
    pub confidence: Confidence,
    pub possible_causes: &'static [&'static str],
    pub verification_steps: &'static [&'static str],
    pub limitations: &'static [&'static str],
    pub related_targets: Vec<SafeTargetLabel>,
    pub related_probe_types: Vec<ProbeType>,
}

// ---------------------------------------------------------------------------
// 投影：内部对象 → DTO（唯一的转换入口）
// ---------------------------------------------------------------------------

impl ExportAggregate {
    fn from_internal(aggregate: &crate::diagnostic::types::ProbeAggregate) -> Self {
        Self {
            probe_type: aggregate.probe_type,
            attempts_total: aggregate.attempts_total,
            cancelled_count: aggregate.cancelled_count,
            success_count: aggregate.success_count,
            transport_failure_count: aggregate.transport_failure_count,
            http_response_count: aggregate.http_response_count,
            request_failure_rate: aggregate.request_failure_rate,
            duration_min_ms: aggregate.duration_min_ms,
            duration_median_ms: aggregate.duration_median_ms,
            duration_max_ms: aggregate.duration_max_ms,
        }
    }
}

impl ExportSample {
    /// 注意：**不使用** `ProbeSample::target_id`——样本里的目标字段同样可能
    /// 被污染，目标名只取所属 `TargetResult` 的白名单映射结果。
    fn from_internal(sample: &ProbeSample) -> Self {
        Self {
            probe_type: sample.probe_type,
            route_mode: sample.route_mode,
            attempt_no: sample.attempt_no,
            success: sample.success,
            duration_ms: sample.duration_ms,
            http_status: sample.http_status,
            http_outcome: sample.http_outcome,
            error_kind: sample.error.as_ref().map(|error| error.kind),
            tested_at: SafeTimestamp::from_raw(&sample.tested_at),
        }
    }
}

impl ExportDiagnosis {
    fn from_internal(diagnosis: &Diagnosis) -> Self {
        let rule = RuleId::from_raw(&diagnosis.rule_id);
        // 标题只需要一个目标：逐目标规则（R01..R09、R11、R12）的
        // related_target_ids 恰好一个；缺失时退化为 unknown-target。
        let primary = diagnosis
            .related_target_ids
            .first()
            .map(String::as_str)
            .unwrap_or_default();
        let profile = rule_profile(rule);
        Self {
            rule_id: rule,
            title: SafeDiagnosisTitle::from_rule(rule, SafeTargetLabel::from_target_id(primary)),
            severity: diagnosis.severity,
            confidence: diagnosis.confidence,
            possible_causes: profile.possible_causes,
            verification_steps: profile.verification_steps,
            limitations: profile.limitations,
            related_targets: diagnosis
                .related_target_ids
                .iter()
                .map(|id| SafeTargetLabel::from_target_id(id))
                .collect(),
            related_probe_types: diagnosis.related_probe_types.clone(),
        }
    }
}

/// 把一次运行 + 其诊断结论投影为导出 DTO。这是渲染前**唯一**的转换点。
pub fn project_report(run: &DiagnosticRun, diagnoses: &[Diagnosis]) -> ExportReport {
    ExportReport {
        meta: ExportMeta {
            app_version: SafeAppVersion::from_raw(&run.app_version),
            exported_at: SafeTimestamp::now(),
            note: super::report::REPORT_NOTE,
        },
        run: ExportRun {
            run_id: SafeRunId::from_raw(&run.run_id),
            started_at: SafeTimestamp::from_raw(&run.started_at),
            finished_at: run.finished_at.as_deref().map(SafeTimestamp::from_raw),
            status: run.status,
            route_mode: run.route_mode,
            repeats: run.repeats,
            app_version: SafeAppVersion::from_raw(&run.app_version),
            targets: run
                .targets
                .iter()
                .map(|target| ExportTarget {
                    target: SafeTargetLabel::from_target_id(&target.target_id),
                    aggregates: target
                        .aggregates
                        .iter()
                        .map(ExportAggregate::from_internal)
                        .collect(),
                    samples: target
                        .samples
                        .iter()
                        .map(ExportSample::from_internal)
                        .collect(),
                })
                .collect(),
        },
        diagnoses: diagnoses
            .iter()
            .map(ExportDiagnosis::from_internal)
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 渲染器（T-2：只接收 ExportReport）
// ---------------------------------------------------------------------------

pub fn render_json(report: &ExportReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_default()
}

pub fn render_markdown(report: &ExportReport) -> String {
    let mut markdown = String::new();
    markdown.push_str("# AI Dev Cockpit 诊断报告\n\n");
    markdown.push_str(&format!("- 运行 ID：{}\n", report.run.run_id.as_str()));
    markdown.push_str(&format!(
        "- 开始时间（UTC）：{}\n",
        report.run.started_at.as_str()
    ));
    if let Some(finished) = report.run.finished_at.as_ref() {
        markdown.push_str(&format!("- 结束时间（UTC）：{}\n", finished.as_str()));
    }
    markdown.push_str(&format!("- 状态：{:?}\n", report.run.status));
    markdown.push_str(&format!("- 路由模式：{:?}\n", report.run.route_mode));
    markdown.push_str(&format!("- 采样次数：{}\n", report.run.repeats));
    markdown.push_str(&format!(
        "- 应用版本：{}\n",
        report.meta.app_version.as_str()
    ));
    markdown.push_str(&format!("\n> {}\n\n## 目标统计\n\n", report.meta.note));

    for target in &report.run.targets {
        markdown.push_str(&format!("### {}\n\n", target.target.as_str()));
        for aggregate in &target.aggregates {
            markdown.push_str(&format!(
                "- {:?}：样本 {}，成功 {}，传输失败 {}，收到响应 {}，中位 {}\n",
                aggregate.probe_type,
                aggregate.attempts_total,
                aggregate.success_count,
                aggregate.transport_failure_count,
                aggregate.http_response_count,
                fmt_millis(aggregate.duration_median_ms),
            ));
        }
        markdown.push('\n');
    }

    markdown.push_str("## 诊断结论\n\n");
    if report.diagnoses.is_empty() {
        markdown.push_str("无触发规则（测量均在正常范围或样本不足以诊断）。\n");
    }
    for diagnosis in &report.diagnoses {
        markdown.push_str(&format!(
            "### {} {}（置信：{:?}，严重度：{:?}）\n\n",
            diagnosis.rule_id.as_str(),
            diagnosis.title.as_str(),
            diagnosis.confidence,
            diagnosis.severity
        ));
        for cause in diagnosis.possible_causes {
            markdown.push_str(&format!("- 可能原因：{cause}\n"));
        }
        for step in diagnosis.verification_steps {
            markdown.push_str(&format!("- 建议步骤：{step}\n"));
        }
        for limitation in diagnosis.limitations {
            markdown.push_str(&format!("- 局限：{limitation}\n"));
        }
        markdown.push('\n');
    }
    markdown
}

/// 渲染辅助：把可空毫秒数格式化为文本（只拼接数值与静态单位）。
fn fmt_millis(value: Option<u64>) -> String {
    value.map_or_else(|| "—".to_string(), |millis| format!("{millis} ms"))
}

// ---------------------------------------------------------------------------
// 规范化实现
// ---------------------------------------------------------------------------

/// run_id 规范化（V6-R3 / U-1、U-2）：必须是**UUID v4**（`Version::Random`
/// 且 `Variant::RFC4122`）的规范 36 字符形式。
///
/// - `uuid::Uuid::parse_str` 负责结构性解析；
/// - `get_version() == Some(Version::Random)` 排除 v1/v3/v5/v7 等其它版本；
/// - `get_variant() == Variant::RFC4122` 排除错误 variant（如第 20 位为 `c`）；
/// - `raw.len() == 36` 排除花括号形式（38）、`urn:uuid:` 前缀（45）与
///   无连字符形式（32）——`uuid` 的解析器接受这些写法，长度校验把它们挡在外面。
///
/// 不合法的输入统一输出固定占位符 `invalid-run-id`（不回显原值、不使导出失败）。
fn normalize_run_id(raw: &str) -> String {
    match uuid::Uuid::parse_str(raw) {
        Ok(parsed)
            if parsed.get_version() == Some(uuid::Version::Random)
                && parsed.get_variant() == uuid::Variant::RFC4122
                && raw.len() == 36 =>
        {
            parsed.hyphenated().to_string()
        }
        _ => "invalid-run-id".to_string(),
    }
}

/// 时间戳规范化：必须可被 chrono 解析为 RFC3339；统一输出 UTC 毫秒精度。
fn normalize_timestamp(raw: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|parsed| parsed.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(|_| "invalid-timestamp".to_string())
}

/// 版本号规范化：仅接受短且受限的字符集（数字/字母/`.`/`+`/`-`）。
fn normalize_app_version(raw: &str) -> String {
    let valid = !raw.is_empty()
        && raw.len() <= 32
        && raw
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '+' | '-'));
    if valid {
        raw.to_string()
    } else {
        "unknown".to_string()
    }
}
