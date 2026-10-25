//! 确定性、离线、可解释的故障诊断规则引擎（Phase 4）。
//!
//! 设计约束（PRD P4 / ARCHITECTURE）：
//! - **纯函数**：`diagnose(runs)` 只消费已测量的结构化结果，不做任何网络请求、
//!   不调用任何 AI/云服务、无内部状态、无随机数——相同输入必产生相同顺序与内容；
//! - **证据分层**：每条诊断严格区分 已观测事实（observed_facts）/ 可能解释
//!   （possible_causes，≤3 条）/ 不可推断事项（limitations）；
//! - **confidence 只表示证据充分程度**（evidence_sufficient / possible /
//!   insufficient），不是根因概率；
//! - **确定性**：规则按 R01..R12 固定顺序求值，target 按 id 排序，输出顺序稳定；
//! - **去重与不矛盾**：每 (规则, 目标) 至多一条；每任务级规则（R10/R12）至多一条；
//!   取消样本不计入任何网络失败统计（D-014）；传输失败与 HTTP 非 2xx 分口径；
//! - **禁止推断**：IP 信誉、ISP 封锁、TUN 绕过、账号可登录、节点必然更快、
//!   服务器必然宕机——各规则的 limitations 显式声明（测试扫描守护）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::types::{
    DiagnosticRun, ErrorKind, HttpOutcome, ProbeSample, ProbeType, RouteMode, RunStatus,
};

// ---------------------------------------------------------------------------
// DTO（与前端 types.ts 一一对应，snake_case 线上格式）
// ---------------------------------------------------------------------------

/// 严重程度：仅表示对"完成用户诊断任务"的影响，不是故障根因的概率。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// 中性事实（如 401/403、限流响应）。
    Info,
    /// 值得关注、可能阻碍使用。
    Warning,
    /// 明确阻断（如指定代理完全不可用）。
    Critical,
}

/// 证据充分程度——**不是根因概率**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// 证据充分：事实直接来自完整、一致的测量。
    EvidenceSufficient,
    /// 可能：存在支持证据但不完整（如偶发失败、单次采样）。
    Possible,
    /// 证据不足：需要更多测量才能下结论。
    Insufficient,
}

/// 一条诊断结论。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnosis {
    /// 规则编号（R01..R12），固定前缀便于排序与去重。
    pub rule_id: String,
    /// 核心发现（中文短句）。
    pub title: String,
    pub severity: Severity,
    pub confidence: Confidence,
    /// 已观测到的事实（引用真实测量数据，不解释）。
    pub observed_facts: Vec<String>,
    /// 基于事实的可能解释（≤3 条，非结论）。
    pub possible_causes: Vec<String>,
    /// 建议排查步骤（优先只读操作）。
    pub verification_steps: Vec<String>,
    /// 不可推断事项 / 测量局限。
    pub limitations: Vec<String>,
    pub related_target_ids: Vec<String>,
    pub related_probe_types: Vec<ProbeType>,
}

// ---------------------------------------------------------------------------
// 内部工具
// ---------------------------------------------------------------------------

struct TargetView<'a> {
    dns: Vec<&'a ProbeSample>,
    tcp: Vec<&'a ProbeSample>,
    https: Vec<&'a ProbeSample>,
    https_aggregate: Option<&'a super::types::ProbeAggregate>,
    /// 计划采样数（run.repeats），用于"样本不足"判定。
    planned_https: u32,
}

impl<'a> TargetView<'a> {
    fn from_run(run: &'a DiagnosticRun) -> BTreeMap<String, TargetView<'a>> {
        let mut views: BTreeMap<String, TargetView<'a>> = BTreeMap::new();
        for target in &run.targets {
            let dns: Vec<&ProbeSample> = target
                .samples
                .iter()
                .filter(|sample| sample.probe_type == ProbeType::Dns)
                .collect();
            let tcp: Vec<&ProbeSample> = target
                .samples
                .iter()
                .filter(|sample| sample.probe_type == ProbeType::Tcp)
                .collect();
            let https: Vec<&ProbeSample> = target
                .samples
                .iter()
                .filter(|sample| sample.probe_type == ProbeType::Https)
                .collect();
            let https_aggregate = target
                .aggregates
                .iter()
                .find(|aggregate| aggregate.probe_type == ProbeType::Https);
            views.insert(
                target.target_id.clone(),
                TargetView {
                    dns,
                    tcp,
                    https,
                    https_aggregate,
                    planned_https: u32::from(run.repeats),
                },
            );
        }
        views
    }

    /// https 样本中"已执行且传输失败"的数量（取消不计，D-014）。
    fn https_transport_failures(&self) -> Vec<&'a ProbeSample> {
        self.https
            .iter()
            .copied()
            .filter(|sample| {
                sample
                    .error
                    .as_ref()
                    .is_some_and(|error| error.kind != ErrorKind::Cancelled)
            })
            .collect()
    }

    /// 特定传输错误的执行样本。
    fn https_transport_failures_of_kind(&self, kind: ErrorKind) -> Vec<&'a ProbeSample> {
        self.https_transport_failures()
            .into_iter()
            .filter(|sample| {
                sample
                    .error
                    .as_ref()
                    .is_some_and(|error| error.kind == kind)
            })
            .collect()
    }

    /// 收到指定 HTTP 结果分类的样本。
    fn https_outcomes(&self, outcome: HttpOutcome) -> Vec<&'a ProbeSample> {
        self.https
            .iter()
            .copied()
            .filter(|sample| sample.http_outcome == Some(outcome))
            .collect()
    }

    fn dns_all_failed(&self) -> bool {
        !self.dns.is_empty() && self.dns.iter().all(|sample| !sample.success)
    }

    fn tcp_all_failed(&self) -> bool {
        !self.tcp.is_empty() && self.tcp.iter().all(|sample| !sample.success)
    }

    fn tcp_all_success(&self) -> bool {
        !self.tcp.is_empty() && self.tcp.iter().all(|sample| sample.success)
    }

    fn dns_all_success(&self) -> bool {
        !self.dns.is_empty() && self.dns.iter().all(|sample| sample.success)
    }
}

/// 部分失败的置信度：全部失败 → 证据充分；部分失败 → 可能（偶发 vs 系统性待区分）。
fn partial_aware_confidence(failed: usize, executed: usize) -> Confidence {
    if failed == executed && executed > 0 {
        Confidence::EvidenceSufficient
    } else {
        Confidence::Possible
    }
}

/// 路由模式的中文标签（与前端 ROUTE_MODE_LABELS 一致）。
fn route_mode_label(mode: RouteMode) -> &'static str {
    match mode {
        RouteMode::LibraryDefault => "库默认（library_default）",
        RouteMode::NoAppProxy => "无应用代理（no_app_proxy）",
        RouteMode::ManualLocalProxy => "手动本地代理（manual_local_proxy）",
    }
}

// ---------------------------------------------------------------------------
// 引擎入口
// ---------------------------------------------------------------------------

/// 对一至多个诊断运行求值。多运行场景（路径对比）：每个 run 对应一种
/// 路由模式、同一目标——R10 跨运行求值，其余规则逐运行求值。
/// 输出顺序：R01..R12 固定顺序；同规则内 target 按字典序。
pub fn diagnose(runs: &[DiagnosticRun]) -> Vec<Diagnosis> {
    let mut out: Vec<Diagnosis> = Vec::new();

    // 逐运行规则（R01..R09、R11、R12），target 按字典序（BTreeMap）。
    for run in runs {
        let views = TargetView::from_run(run);
        for (target_id, view) in &views {
            rule_r01_dns_failure(run, target_id, view, &mut out);
            rule_r02_tcp_failure(run, target_id, view, &mut out);
            rule_r03_tls_failure(run, target_id, view, &mut out);
            rule_r04_https_timeout(run, target_id, view, &mut out);
            rule_r05_http_401_403(run, target_id, view, &mut out);
            rule_r06_http_407(run, target_id, view, &mut out);
            rule_r07_http_429(run, target_id, view, &mut out);
            rule_r08_http_5xx(run, target_id, view, &mut out);
            rule_r09_manual_proxy_unavailable(run, target_id, view, &mut out);
            rule_r11_baseline_https_divergence(run, target_id, view, &mut out);
            rule_r12_insufficient_samples(run, target_id, view, &mut out);
        }
    }

    // 跨运行规则（R10）：需要 ≥2 个运行、同一目标、不同路由模式。
    rule_r10_cross_mode_divergence(runs, &mut out);

    out.sort_by(|left, right| {
        left.rule_id
            .cmp(&right.rule_id)
            .then_with(|| left.related_target_ids.cmp(&right.related_target_ids))
    });
    out
}

fn push(out: &mut Vec<Diagnosis>, diagnosis: Diagnosis) {
    out.push(diagnosis);
}

// ---------------------------------------------------------------------------
// 规则实现（R01..R12）
// ---------------------------------------------------------------------------

/// R01 DNS 失败：独立 DNS 基线解析失败。
fn rule_r01_dns_failure(
    _run: &DiagnosticRun,
    target_id: &str,
    view: &TargetView<'_>,
    out: &mut Vec<Diagnosis>,
) {
    if !view.dns_all_failed() {
        return;
    }
    // 事实合并：HTTPS 样本若也出现 dns_failure，归并到同一条，避免重复结论。
    let https_dns_failures = view.https_transport_failures_of_kind(ErrorKind::DnsFailure);
    let mut facts = vec![format!(
        "独立 DNS 测量未能解析目标域名（{} 次采样全部失败）",
        view.dns.len()
    )];
    if !https_dns_failures.is_empty() {
        facts.push(format!(
            "HTTPS 采样中另有 {} 次同样在域名解析阶段失败",
            https_dns_failures.len()
        ));
    }
    push(
        out,
        Diagnosis {
            rule_id: "R01".to_string(),
            title: format!("目标 {target_id} 的域名解析失败"),
            severity: Severity::Warning,
            confidence: Confidence::EvidenceSufficient,
            observed_facts: facts,
            possible_causes: vec![
                "本机解析策略（hosts、DoH、企业 DNS 策略）拦截或改写了该域名".to_string(),
                "当前网络的 DNS 服务器无响应或对该域名返回失败".to_string(),
                "该域名在当前网络环境下不可解析".to_string(),
            ],
            verification_steps: vec![
                "用系统命令 nslookup 对同一域名做对比解析（只读）".to_string(),
                "检查本机 hosts 文件与 DoH/代理客户端的 DNS 设置".to_string(),
                "更换 DNS 服务器后重测（仅修改本工具之外的配置需自行确认）".to_string(),
            ],
            limitations: vec![
                "本测量代表系统解析器的完整解析过程，不是原始 DNS 报文 RTT".to_string(),
                "不代表 HTTPS 请求实际经历的解析路径（缓存/连接复用可能不同）".to_string(),
                "不能据此推断域名被 ISP 封锁或 DNS 污染".to_string(),
            ],
            related_target_ids: vec![target_id.to_string()],
            related_probe_types: vec![ProbeType::Dns],
        },
    );
}

/// R02 TCP 失败：独立 TCP 基线连接失败。
fn rule_r02_tcp_failure(
    _run: &DiagnosticRun,
    target_id: &str,
    view: &TargetView<'_>,
    out: &mut Vec<Diagnosis>,
) {
    if !view.tcp_all_failed() {
        return;
    }
    push(
        out,
        Diagnosis {
            rule_id: "R02".to_string(),
            title: format!("到目标 {target_id} 443 端口的 TCP 连接建立失败"),
            severity: Severity::Warning,
            confidence: Confidence::EvidenceSufficient,
            observed_facts: vec![format!(
                "独立 TCP 测量未能建立到目标 443 端口的连接（{} 次采样全部失败）",
                view.tcp.len()
            )],
            possible_causes: vec![
                "本机防火墙或安全软件拦截了出站连接".to_string(),
                "当前网络到目标 443 端口的路径不可达".to_string(),
                "目标服务的 443 端口未开放或拒绝来自该网络的连接".to_string(),
            ],
            verification_steps: vec![
                "检查本机防火墙/安全软件的出站规则（只读查看）".to_string(),
                "切换网络（如手机热点）后重测对比".to_string(),
                "对其它白名单目标做同样测量，判断是否普遍性问题".to_string(),
            ],
            limitations: vec![
                "这是独立 TCP connect 测量（非 ICMP Ping），耗时口径不同".to_string(),
                "不能据此断言对方服务器宕机或端口被 ISP 封锁".to_string(),
            ],
            related_target_ids: vec![target_id.to_string()],
            related_probe_types: vec![ProbeType::Tcp],
        },
    );
}

/// R03 TLS 失败（部分失败 → possible）。
fn rule_r03_tls_failure(
    _run: &DiagnosticRun,
    target_id: &str,
    view: &TargetView<'_>,
    out: &mut Vec<Diagnosis>,
) {
    let failures = view.https_transport_failures_of_kind(ErrorKind::TlsFailure);
    if failures.is_empty() {
        return;
    }
    let executed = view.https_aggregate.map_or(view.https.len(), |aggregate| {
        aggregate.executed_total() as usize
    });
    let confidence = partial_aware_confidence(failures.len(), executed);
    push(
        out,
        Diagnosis {
            rule_id: "R03".to_string(),
            title: format!(
                "目标 {target_id} 的 HTTPS 请求在 TLS 握手阶段失败（{}/{}）",
                failures.len(),
                executed
            ),
            severity: Severity::Warning,
            confidence,
            observed_facts: vec![format!(
                "{} 次已执行的 HTTPS 采样中有 {} 次未完成 TLS 握手",
                executed,
                failures.len()
            )],
            possible_causes: vec![
                "网络路径上存在对 TLS 握手的干扰（中间设备重置/阻断）".to_string(),
                "代理路径的隧道质量问题（若经代理）".to_string(),
                "证书验证失败（本工具始终执行完整证书校验）".to_string(),
            ],
            verification_steps: vec![
                "用浏览器访问同一目标，对比是否同样失败".to_string(),
                "在路径对比中切换不同模式，观察 TLS 失败是否随路径变化".to_string(),
                "检查本机系统时间是否正确（影响证书有效期判断）".to_string(),
            ],
            limitations: vec![
                "不能区分干扰来自网络中间设备还是目标侧".to_string(),
                "偶发失败与系统性失败需更多采样才能区分（见置信标记）".to_string(),
            ],
            related_target_ids: vec![target_id.to_string()],
            related_probe_types: vec![ProbeType::Https],
        },
    );
}

/// R04 HTTPS 超时（部分失败 → possible）。
fn rule_r04_https_timeout(
    _run: &DiagnosticRun,
    target_id: &str,
    view: &TargetView<'_>,
    out: &mut Vec<Diagnosis>,
) {
    let failures = view.https_transport_failures_of_kind(ErrorKind::Timeout);
    if failures.is_empty() {
        return;
    }
    let executed = view.https_aggregate.map_or(view.https.len(), |aggregate| {
        aggregate.executed_total() as usize
    });
    let confidence = partial_aware_confidence(failures.len(), executed);
    push(
        out,
        Diagnosis {
            rule_id: "R04".to_string(),
            title: format!(
                "目标 {target_id} 的 HTTPS 请求超时（{}/{}）",
                failures.len(),
                executed
            ),
            severity: Severity::Warning,
            confidence,
            observed_facts: vec![format!(
                "{} 次已执行的 HTTPS 采样中有 {} 次在 {} 毫秒超时窗口内未完成",
                executed,
                failures.len(),
                failures
                    .first()
                    .and_then(|sample| sample.error.as_ref())
                    .map(|error| error.summary.clone())
                    .unwrap_or_else(|| "限定".to_string())
            )],
            possible_causes: vec![
                "网络路径丢包或拥塞导致响应缓慢".to_string(),
                "目标服务响应缓慢（含 CDN/网关）".to_string(),
                "代理路径缓慢（若经代理）".to_string(),
            ],
            verification_steps: vec![
                "提高单请求超时后重测，观察是否完成".to_string(),
                "对比其它目标的超时情况，判断是否普遍".to_string(),
                "在路径对比中切换模式，定位慢在哪条路径".to_string(),
            ],
            limitations: vec![
                "不能定位丢包或拥塞发生的位置".to_string(),
                "不能据此断言对方服务器宕机".to_string(),
            ],
            related_target_ids: vec![target_id.to_string()],
            related_probe_types: vec![ProbeType::Https],
        },
    );
}

/// R05 HTTP 401/403：中性事实，绝不判定业务可用性。
fn rule_r05_http_401_403(
    _run: &DiagnosticRun,
    target_id: &str,
    view: &TargetView<'_>,
    out: &mut Vec<Diagnosis>,
) {
    let responses: Vec<&ProbeSample> = view.https_outcomes(HttpOutcome::AuthRequired401To403);
    if responses.is_empty() {
        return;
    }
    push(
        out,
        Diagnosis {
            rule_id: "R05".to_string(),
            title: format!("目标 {target_id} 返回了 HTTP 401/403 响应"),
            severity: Severity::Info,
            confidence: Confidence::EvidenceSufficient,
            observed_facts: vec![format!(
                "{} 个 HTTPS 采样收到了 401/403 状态码（网络与 TLS 均已完成）",
                responses.len()
            )],
            possible_causes: vec![
                "目标对未携带凭证的请求返回的正常响应（本工具从不发送凭证）".to_string(),
                "目标或其网络边缘对该来源启用了访问控制".to_string(),
            ],
            verification_steps: vec![
                "用浏览器访问同一地址，对比响应状态（只读）".to_string(),
                "确认本工具未携带任何 Cookie/令牌（设计如此）".to_string(),
            ],
            limitations: vec![
                "401/403 ≠ 服务不可用，也 ≠ 服务可用：只说明 HTTP 层可达且有响应".to_string(),
                "不能区分响应来自目标服务本身还是网络边缘设备".to_string(),
                "不能据此推断账号能否正常登录或业务是否正常".to_string(),
            ],
            related_target_ids: vec![target_id.to_string()],
            related_probe_types: vec![ProbeType::Https],
        },
    );
}

/// R06 HTTP 407：代理要求认证。
fn rule_r06_http_407(
    _run: &DiagnosticRun,
    target_id: &str,
    view: &TargetView<'_>,
    out: &mut Vec<Diagnosis>,
) {
    let responses = view.https_outcomes(HttpOutcome::ProxyAuth407);
    if responses.is_empty() {
        return;
    }
    push(
        out,
        Diagnosis {
            rule_id: "R06".to_string(),
            title: format!("访问目标 {target_id} 时代理要求认证（HTTP 407）"),
            severity: Severity::Warning,
            confidence: Confidence::EvidenceSufficient,
            observed_facts: vec![format!(
                "{} 个 HTTPS 采样收到了代理的 407 认证要求",
                responses.len()
            )],
            possible_causes: vec![
                "本地代理客户端开启了认证要求".to_string(),
                "代理服务器配置了访问控制策略".to_string(),
            ],
            verification_steps: vec![
                "检查本机代理客户端的认证设置（只读查看）".to_string(),
                "用 no_app_proxy 模式对比是否绕开了该代理".to_string(),
            ],
            limitations: vec![
                "本工具不发送任何代理凭证，也不会自动通过认证".to_string(),
                "407 是代理侧要求，与目标服务本身无关".to_string(),
            ],
            related_target_ids: vec![target_id.to_string()],
            related_probe_types: vec![ProbeType::Https],
        },
    );
}

/// R07 HTTP 429：限流。
fn rule_r07_http_429(
    _run: &DiagnosticRun,
    target_id: &str,
    view: &TargetView<'_>,
    out: &mut Vec<Diagnosis>,
) {
    let responses = view.https_outcomes(HttpOutcome::RateLimited429);
    if responses.is_empty() {
        return;
    }
    push(
        out,
        Diagnosis {
            rule_id: "R07".to_string(),
            title: format!("目标 {target_id} 返回了 HTTP 429 限流响应"),
            severity: Severity::Info,
            confidence: Confidence::EvidenceSufficient,
            observed_facts: vec![format!(
                "{} 个 HTTPS 采样收到了 429（服务端限流）",
                responses.len()
            )],
            possible_causes: vec![
                "目标服务对请求频率或并发做了限制".to_string(),
                "当前出口 IP 的共享使用者触发了限流阈值".to_string(),
            ],
            verification_steps: vec![
                "稍后降低采样频率重试".to_string(),
                "在路径对比中观察限流是否只在特定路径出现".to_string(),
            ],
            limitations: vec![
                "不能推断出口 IP 的信誉状况".to_string(),
                "限流原因只有目标服务方可以确认".to_string(),
            ],
            related_target_ids: vec![target_id.to_string()],
            related_probe_types: vec![ProbeType::Https],
        },
    );
}

/// R08 HTTP 5xx：服务端错误响应（部分失败 → possible）。
fn rule_r08_http_5xx(
    _run: &DiagnosticRun,
    target_id: &str,
    view: &TargetView<'_>,
    out: &mut Vec<Diagnosis>,
) {
    let responses = view.https_outcomes(HttpOutcome::ServerError5xx);
    if responses.is_empty() {
        return;
    }
    let executed = view.https_aggregate.map_or(view.https.len(), |aggregate| {
        aggregate.executed_total() as usize
    });
    let confidence = partial_aware_confidence(responses.len(), executed);
    push(
        out,
        Diagnosis {
            rule_id: "R08".to_string(),
            title: format!(
                "目标 {target_id} 返回了 HTTP 5xx 服务端错误（{}/{}）",
                responses.len(),
                executed
            ),
            severity: Severity::Warning,
            confidence,
            observed_facts: vec![format!(
                "{} 次已执行采样中有 {} 次收到 5xx 状态码（含状态码明细）",
                executed,
                responses.len()
            )],
            possible_causes: vec![
                "目标服务内部错误".to_string(),
                "目标侧网关/CDN 出错".to_string(),
            ],
            verification_steps: vec![
                "稍后重试，观察是否持续".to_string(),
                "用浏览器访问同一目标对比".to_string(),
            ],
            limitations: vec![
                "不能断言对方服务器宕机，也不能推断其故障根因".to_string(),
                "偶发与持续性需更多采样区分（见置信标记）".to_string(),
            ],
            related_target_ids: vec![target_id.to_string()],
            related_probe_types: vec![ProbeType::Https],
        },
    );
}

/// R09 本地指定代理不可用（manual_local_proxy 运行中传输失败 → 绝不回落）。
fn rule_r09_manual_proxy_unavailable(
    run: &DiagnosticRun,
    target_id: &str,
    view: &TargetView<'_>,
    out: &mut Vec<Diagnosis>,
) {
    if run.route_mode != super::types::RouteMode::ManualLocalProxy {
        return;
    }
    let failures: Vec<&ProbeSample> = view
        .https_transport_failures()
        .into_iter()
        .filter(|sample| {
            sample
                .error
                .as_ref()
                .is_some_and(|error| error.kind == ErrorKind::ProxyUnavailable)
        })
        .collect();
    if failures.is_empty() {
        return;
    }
    let executed = view.https_aggregate.map_or(view.https.len(), |aggregate| {
        aggregate.executed_total() as usize
    });
    let confidence = partial_aware_confidence(failures.len(), executed);
    push(
        out,
        Diagnosis {
            rule_id: "R09".to_string(),
            title: format!("指定的本地代理在访问 {target_id} 时不可用"),
            severity: Severity::Critical,
            confidence,
            observed_facts: vec![format!(
                "manual_local_proxy 模式下 {} 次已执行采样中有 {} 次无法建立到指定代理的连接",
                executed,
                failures.len()
            )],
            possible_causes: vec![
                "代理客户端未运行或监听端口与指定不一致".to_string(),
                "代理协议不匹配（http 与 socks5 混用）".to_string(),
                "代理客户端拒绝了连接（访问控制）".to_string(),
            ],
            verification_steps: vec![
                "确认代理客户端正在运行并核对监听端口（只读查看）".to_string(),
                "核对协议前缀（http/socks5/socks5h）与客户端实际协议一致".to_string(),
                "用 no_app_proxy 模式单独测量目标基线（不经过该代理）".to_string(),
            ],
            limitations: vec![
                "本工具绝不回落直连：这些失败样本不代表目标的直连状态".to_string(),
                "不能推断代理客户端的具体配置细节".to_string(),
            ],
            related_target_ids: vec![target_id.to_string()],
            related_probe_types: vec![ProbeType::Https],
        },
    );
}

/// R10 不同路径结果不一致（跨运行：同目标、不同路由模式）。
fn rule_r10_cross_mode_divergence(runs: &[DiagnosticRun], out: &mut Vec<Diagnosis>) {
    if runs.len() < 2 {
        return;
    }
    // 目标 → (模式, 成功数, 传输失败数, 执行数)。BTreeMap 保证确定性。
    let mut per_target: BTreeMap<String, Vec<(RouteMode, u32, u32, u32)>> = BTreeMap::new();
    for run in runs {
        // 取消任务不参与路径一致性判断（结果不完整）。
        if run.status != RunStatus::Completed {
            continue;
        }
        for target in &run.targets {
            let Some(aggregate) = target
                .aggregates
                .iter()
                .find(|aggregate| aggregate.probe_type == ProbeType::Https)
            else {
                continue;
            };
            per_target
                .entry(target.target_id.clone())
                .or_default()
                .push((
                    run.route_mode,
                    aggregate.success_count,
                    aggregate.transport_failure_count,
                    aggregate.executed_total(),
                ));
        }
    }

    for (target_id, entries) in &per_target {
        if entries.len() < 2 {
            continue;
        }
        // 参与比较的运行必须覆盖至少两种不同 route_mode：
        // 相同模式的不同运行（如重复测量）不构成"跨路径"差异。
        let has_two_modes = entries
            .iter()
            .any(|(mode, ..)| entries.iter().any(|(other, ..)| other != mode));
        if !has_two_modes {
            continue;
        }
        // 触发条件（专项修复，基于 transport_failure_count 区分口径）：
        // 某模式全部成功（executed>0 且 success==executed），而另一模式存在
        // **真实传输失败**（未收到任何 HTTP 响应）。非 2xx 响应是"收到了响应"
        // （D-007/D-011），不计入传输失败，也不触发本规则。
        let all_success_somewhere = entries
            .iter()
            .any(|(_, success, _, executed)| *executed > 0 && success == executed);
        let transport_failure_somewhere = entries.iter().any(|(_, _, failures, _)| *failures > 0);
        if !(all_success_somewhere && transport_failure_somewhere) {
            continue;
        }
        let facts: Vec<String> = entries
            .iter()
            .map(|(mode, success, failures, executed)| {
                format!(
                    "模式 {}：{}/{} 成功，传输失败 {}",
                    route_mode_label(*mode),
                    success,
                    executed,
                    failures
                )
            })
            .collect();
        push(
            out,
            Diagnosis {
                rule_id: "R10".to_string(),
                title: format!("目标 {target_id} 在不同请求路径下结果不一致"),
                severity: Severity::Info,
                confidence: Confidence::Possible,
                observed_facts: facts,
                possible_causes: vec![
                    "各模式经过的网络路径不同（代理 vs 直连，或不同代理节点）".to_string(),
                    "library_default 的代理选择受本机系统配置影响（见对比页说明）".to_string(),
                    "路径质量随时间波动，同一批次的先后执行也会引入差异".to_string(),
                ],
                verification_steps: vec![
                    "在相近时间窗口重复对比多次，观察差异是否稳定".to_string(),
                    "核对各模式说明，确认其语义差异（含 TUN/透明代理边界）".to_string(),
                    "分别查看各模式的完整采样记录".to_string(),
                ],
                limitations: vec![
                    "中位数排序不构成节点性能排名，也不代表某路径必然更快".to_string(),
                    "不能推断任何出口 IP 的信誉状况".to_string(),
                    "no_app_proxy 不等同于真实直连（TUN/透明代理仍可能生效）".to_string(),
                ],
                related_target_ids: vec![target_id.clone()],
                related_probe_types: vec![ProbeType::Https],
            },
        );
    }
}

/// R11 独立 DNS/TCP 基线正常，但 HTTPS 全部未获得响应（PRD §8.3 核心场景）。
fn rule_r11_baseline_https_divergence(
    _run: &DiagnosticRun,
    target_id: &str,
    view: &TargetView<'_>,
    out: &mut Vec<Diagnosis>,
) {
    if !(view.dns_all_success() && view.tcp_all_success()) {
        return;
    }
    let transport_failures = view.https_transport_failures();
    let executed = view.https_aggregate.map_or(view.https.len(), |aggregate| {
        aggregate.executed_total() as usize
    });
    // 触发条件（专项修复）：基线正常，且**全部已执行样本都是传输失败**——
    // 即没有任何样本收到 HTTP 响应（含非 2xx）。只要存在收到响应的样本
    // （200 或 401/403/5xx），"均未获得 HTTP 响应"的结论就不成立，
    // 非 2xx 事实由 R05/R07/R08 等响应类规则覆盖（D-007/D-011/D-017）。
    if executed == 0 || transport_failures.len() != executed {
        return;
    }
    push(
        out,
        Diagnosis {
            rule_id: "R11".to_string(),
            title: format!("目标 {target_id} 的 DNS 与 TCP 基线正常，但 HTTPS 请求全部失败"),
            severity: Severity::Warning,
            confidence: Confidence::Possible,
            observed_facts: vec![
                format!(
                    "独立基线：DNS 解析成功、TCP 443 建连成功（{} 次测量）",
                    view.tcp.len()
                ),
                format!("{executed} 次 HTTPS 采样均未获得 HTTP 响应（全部为传输层失败）"),
            ],
            possible_causes: vec![
                "TLS 层在该路径上被干扰或阻断".to_string(),
                "HTTPS 所经的应用层代理路径存在问题（与基线路径不同）".to_string(),
                "目标 HTTPS 服务异常".to_string(),
            ],
            verification_steps: vec![
                "用浏览器访问同一目标，对比 HTTPS 行为".to_string(),
                "在路径对比中切换模式，定位问题在哪条路径".to_string(),
                "检查代理客户端日志（若 HTTPS 经代理，只读查看）".to_string(),
            ],
            limitations: vec![
                "三次测量相互独立：不能把 DNS/TCP 结论强行归因到 HTTPS 路径".to_string(),
                "不能据此断言服务故障或网络封锁".to_string(),
            ],
            related_target_ids: vec![target_id.to_string()],
            related_probe_types: vec![ProbeType::Dns, ProbeType::Tcp, ProbeType::Https],
        },
    );
}

/// R12 样本不足或任务取消：明示证据边界（insufficient）。
fn rule_r12_insufficient_samples(
    run: &DiagnosticRun,
    target_id: &str,
    view: &TargetView<'_>,
    out: &mut Vec<Diagnosis>,
) {
    let aggregate = view.https_aggregate;
    let cancelled = aggregate.map_or(0, |item| item.cancelled_count);
    let missing = view.planned_https.saturating_sub(view.https.len() as u32);
    let run_cancelled = run.status == RunStatus::Cancelled;

    if !run_cancelled && cancelled == 0 && missing == 0 {
        return;
    }
    let mut facts = Vec::new();
    if run_cancelled {
        facts.push("任务已被取消或达到截止时间".to_string());
    }
    if cancelled > 0 {
        facts.push(format!("{cancelled} 个已开始的采样被中断（记为取消）"));
    }
    if missing > 0 {
        facts.push(format!("{missing} 个计划采样未执行"));
    }
    push(
        out,
        Diagnosis {
            rule_id: "R12".to_string(),
            title: format!(
                "目标 {target_id} 的样本不完整（取消 {} 个、未执行 {} 个）",
                cancelled, missing
            ),
            severity: Severity::Info,
            confidence: Confidence::Insufficient,
            observed_facts: facts,
            possible_causes: vec![
                "用户主动取消了任务".to_string(),
                "任务达到全局截止时间".to_string(),
            ],
            verification_steps: vec![
                "重新运行诊断以获得完整采样".to_string(),
                "如持续超时，先调低采样次数或检查网络".to_string(),
            ],
            limitations: vec![
                "取消与未执行的样本不代表网络失败，不计入任何失败率（D-014）".to_string(),
                "已有样本的结论仍然有效，但样本量不支持长期稳定性判断".to_string(),
                "3 或 5 次采样不能断言服务长期稳定性".to_string(),
            ],
            related_target_ids: vec![target_id.to_string()],
            related_probe_types: vec![ProbeType::Https],
        },
    );
}
