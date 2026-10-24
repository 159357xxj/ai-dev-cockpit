//! 独立 HTTPS 请求测量（docs/ARCHITECTURE.md §7.2/§7.3）。
//!
//! Phase 2 仅实现 `no_app_proxy`（客户端显式不配置应用层代理；TUN/透明路由
//! 仍可能生效，绝不声称"绕过"）。`library_default` 与 `manual_local_proxy`
//! 由命令层拒绝并提示 Phase 3 提供。
//!
//! 请求策略（冻结）：
//! - 仅访问调用方给定的 URL；生产路径由 `targets.rs` 白名单保证 https + 公开地址，
//!   不携带 Cookie/Authorization/任何用户凭证；
//! - HEAD 优先；最终响应 405 时允许**一次** GET 回退；
//! - 响应体最多读取 64 KiB 后丢弃（HEAD 无 body，读取即结束）；
//! - 重定向最多 5 次，且只允许跟随到 `RedirectRules` 允许的目标
//!   （生产：白名单主机 + https + 443）；越界/达上限 → 保留可分类的 3xx 终态；
//! - 不做任何隐式重试；
//! - TLS 证书正常校验，永不绕过；
//! - 收到 HTTP 响应（任意状态码）不是传输层错误（D-007）；
//! - 耗时仅成功样本有值（见 `ProbeSample::duration_ms` 契约）。

use std::collections::HashSet;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use super::types::{
    DiagnosticError, ErrorKind, HttpOutcome, ProbeSample, ProbeType, RouteMode, now_rfc3339,
};

pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const MAX_REDIRECTS: usize = 5;
pub const USER_AGENT: &str = "ai-dev-cockpit/0.1 (+local read-only diagnostic; no credentials)";

/// 重定向跟随规则。生产使用 [`RedirectRules::production`]（白名单主机 + https + 443）；
/// 测试注入本地 mock 服务器规则。
#[derive(Debug, Clone)]
pub struct RedirectRules {
    pub allowed_hosts: HashSet<String>,
    pub allowed_schemes: Vec<String>,
    pub allowed_ports: Vec<u16>,
    pub max_redirects: usize,
}

impl RedirectRules {
    /// 生产规则：仅允许跟随到预设白名单主机，且必须 https + 443。
    pub fn production(hosts: HashSet<String>) -> Self {
        Self {
            allowed_hosts: hosts,
            allowed_schemes: vec!["https".to_string()],
            allowed_ports: vec![443],
            max_redirects: MAX_REDIRECTS,
        }
    }

    fn allows(&self, url: &reqwest::Url) -> bool {
        let scheme_ok = self
            .allowed_schemes
            .iter()
            .any(|scheme| url.scheme().eq_ignore_ascii_case(scheme));
        let host_ok = url.host_str().is_some_and(|host| {
            self.allowed_hosts
                .contains(host.to_ascii_lowercase().as_str())
        });
        let port_ok = self
            .allowed_ports
            .contains(&url.port_or_known_default().unwrap_or(0));
        scheme_ok && host_ok && port_ok
    }
}

pub struct HttpsProbeParams {
    pub target_id: String,
    /// 首个请求的 URL。生产路径由白名单保证 https；测试可指向本地 mock。
    pub url: String,
    pub route_mode: RouteMode,
    pub timeout: Duration,
    pub rules: RedirectRules,
}

fn redirect_policy(rules: RedirectRules) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        // 达到上限：停止跟随，保留可分类的 3xx 终态响应。
        if attempt.previous().len() >= rules.max_redirects {
            return attempt.stop();
        }
        // 越界目标（非白名单主机 / 非 https / 非 443）：同样停止而非报错，
        // 让最终 3xx 进入正常分类通道。
        if rules.allows(attempt.url()) {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

fn error_sample(
    target_id: &str,
    route_mode: RouteMode,
    attempt_no: u32,
    kind: ErrorKind,
    summary: String,
    detail: Option<String>,
) -> ProbeSample {
    ProbeSample {
        target_id: target_id.to_string(),
        probe_type: ProbeType::Https,
        route_mode,
        attempt_no,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: Some(DiagnosticError {
            kind,
            summary,
            detail,
        }),
        tested_at: now_rfc3339(),
    }
}

/// 传输层错误分类（reqwest 错误 → ErrorKind）。
/// DNS 已在 reqwest 内部处理：解析失败会出现在 connect 错误链中，
/// 以错误文本识别；连接被拒 → `tcp_failure`；其余 connect 阶段错误
/// （TLS 握手失败为主）→ `tls_failure`。无法归因时如实返回 `unknown`。
fn classify_transport_error(error: &reqwest::Error) -> (ErrorKind, String) {
    if error.is_timeout() {
        return (
            ErrorKind::Timeout,
            format!("请求在超时窗口内未完成：{error}"),
        );
    }
    if error.is_connect() {
        let mut refused = false;
        let mut dns_like = false;
        let mut chain = vec![error.to_string()];
        let mut source = std::error::Error::source(error);
        while let Some(current) = source {
            let text = current.to_string();
            if text.contains("failed to lookup address information")
                || text.contains("resolves to none")
                || text.to_lowercase().contains("no such host")
            {
                dns_like = true;
            }
            if current
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::ConnectionRefused)
            {
                refused = true;
            }
            chain.push(text);
            source = current.source();
        }
        let kind = if dns_like {
            ErrorKind::DnsFailure
        } else if refused {
            ErrorKind::TcpFailure
        } else {
            ErrorKind::TlsFailure
        };
        return (kind, chain.join(" | "));
    }
    (ErrorKind::Unknown, format!("未归因请求错误：{error}"))
}

fn classify_status(status: u16) -> (bool, HttpOutcome) {
    match status {
        200..=299 => (true, HttpOutcome::Ok2xx),
        300..=399 => (false, HttpOutcome::Redirect3xx),
        401 | 403 => (false, HttpOutcome::AuthRequired401To403),
        407 => (false, HttpOutcome::ProxyAuth407),
        429 => (false, HttpOutcome::RateLimited429),
        500..=599 => (false, HttpOutcome::ServerError5xx),
        400..=499 => (false, HttpOutcome::ClientErrorOther4xx),
        _ => (false, HttpOutcome::ClientErrorOther4xx),
    }
}

/// 执行一次 HTTPS 测量。
pub async fn probe_https(params: &HttpsProbeParams, cancel: CancellationToken) -> ProbeSample {
    let attempt_no = 1;
    if params.route_mode != RouteMode::NoAppProxy {
        return error_sample(
            &params.target_id,
            params.route_mode,
            attempt_no,
            ErrorKind::Internal,
            "该连接模式未在本阶段支持（命令层应已拦截）".to_string(),
            None,
        );
    }

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(params.timeout)
        .connect_timeout(params.timeout)
        .redirect(redirect_policy(params.rules.clone()))
        .no_proxy()
        .build();
    let client = match client {
        Ok(client) => client,
        Err(error) => {
            return error_sample(
                &params.target_id,
                params.route_mode,
                attempt_no,
                ErrorKind::Internal,
                format!("HTTP 客户端构建失败：{error}"),
                None,
            );
        }
    };

    let started = Instant::now();
    let mut used_get_fallback = false;
    let response = {
        // HEAD 优先。
        let head = tokio::select! {
            _ = cancel.cancelled() => {
                return ProbeSample::cancelled(&params.target_id, ProbeType::Https, params.route_mode, attempt_no);
            }
            sent = client.head(&params.url).send() => sent,
        };
        match head {
            Ok(response) => response,
            Err(error) => {
                let (kind, detail) = classify_transport_error(&error);
                return error_sample(
                    &params.target_id,
                    params.route_mode,
                    attempt_no,
                    kind,
                    "HTTPS 请求未获得响应".to_string(),
                    Some(detail),
                );
            }
        }
    };

    // 最终响应 405 → 允许一次 GET 回退（不重试其他状态）。
    let response = if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
        used_get_fallback = true;
        let get = tokio::select! {
            _ = cancel.cancelled() => {
                return ProbeSample::cancelled(&params.target_id, ProbeType::Https, params.route_mode, attempt_no);
            }
            sent = client.get(&params.url).send() => sent,
        };
        match get {
            Ok(response) => response,
            Err(error) => {
                let (kind, detail) = classify_transport_error(&error);
                return error_sample(
                    &params.target_id,
                    params.route_mode,
                    attempt_no,
                    kind,
                    "HEAD 405 后的 GET 回退未获得响应".to_string(),
                    Some(detail),
                );
            }
        }
    } else {
        response
    };

    // 读取响应体（上限 MAX_BODY_BYTES；HEAD 响应无 body，读取立即结束）。
    let mut response = response;
    let mut total_read = 0usize;
    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => break, // 响应已获得：按已取得的状态分类，中止读体
            read = response.chunk() => read,
        };
        match chunk {
            Ok(Some(bytes)) => {
                total_read += bytes.len();
                if total_read >= MAX_BODY_BYTES {
                    break; // 达到读取上限，丢弃剩余内容
                }
            }
            Ok(None) => break,
            Err(_) => break, // 读体中断不影响"已收到响应"这一事实
        }
    }

    let status = response.status().as_u16();
    let (success, outcome) = classify_status(status);
    let duration_ms = if success {
        Some(started.elapsed().as_millis() as u64)
    } else {
        None
    };

    // used_get_fallback 目前仅约束行为本身（回退一次、读体有上限）；
    // 其信息随 Phase 4 规则引擎需要时再进入契约字段，避免现在膨胀。
    let _ = used_get_fallback;

    ProbeSample {
        target_id: params.target_id.clone(),
        probe_type: ProbeType::Https,
        route_mode: params.route_mode,
        attempt_no,
        success,
        duration_ms,
        http_status: Some(status),
        http_outcome: Some(outcome),
        error: None,
        tested_at: now_rfc3339(),
    }
}
