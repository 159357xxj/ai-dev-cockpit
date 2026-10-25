//! 独立 DNS 查询测量（docs/ARCHITECTURE.md §7.1）。
//!
//! 使用系统标准解析器（`tokio::net::lookup_host`）对目标域名做完整解析。
//! 测得的耗时是"系统解析器完成一次解析的总时长"（含缓存、hosts、DoH 等
//! 系统行为的影响），**不是原始 DNS 报文 RTT**——展示层必须如实标注。
//! 这是独立于 HTTPS 请求的基线测量，不代表 HTTPS 实际经历的解析路径。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use super::types::{DiagnosticError, ErrorKind, ProbeSample, ProbeType, RouteMode, now_rfc3339};

/// 解析函数注入缝（Phase 3 审计补强）：
/// 生产用 [`system_resolver`]（系统标准解析器）；测试注入 Mock 解析器，
/// 使 `dns_failure` 分类可离线确定性验证（本机运营商解析器会通配解析，
/// 真实解析失败无法离线构造）。
pub type ResolveFuture =
    Pin<Box<dyn Future<Output = std::io::Result<Vec<std::net::SocketAddr>>> + Send>>;
pub type Resolver = Arc<dyn Fn(String, u16) -> ResolveFuture + Send + Sync>;

/// 生产解析器：`tokio::net::lookup_host`（系统标准解析器，含缓存/hosts/DoH 等系统行为）。
pub fn system_resolver() -> Resolver {
    Arc::new(|host, port| {
        Box::pin(async move {
            tokio::net::lookup_host((host.as_str(), port))
                .await
                .map(Iterator::collect)
        })
    })
}

/// 对 `host` 做一次独立解析测量。
///
/// 结果语义：
/// - 成功 → `success=true`，`duration_ms` 为解析耗时；
/// - 解析失败（NXDOMAIN/服务器错误）→ `dns_failure`，`duration_ms` 记录到失败为止的耗时；
/// - 超时 / 取消 → `timeout` / `cancelled`，`duration_ms=None`。
pub async fn probe_dns(
    target_id: &str,
    host: &str,
    route_mode: RouteMode,
    timeout: Duration,
    cancel: CancellationToken,
    resolver: &Resolver,
) -> ProbeSample {
    let started = Instant::now();

    let outcome = tokio::select! {
        _ = cancel.cancelled() => Err((ErrorKind::Cancelled, "任务已取消，DNS 查询未完成".to_string())),
        resolved = tokio::time::timeout(timeout, resolver(host.to_string(), 443u16)) => {
            match resolved {
                Err(_) => Err((
                    ErrorKind::Timeout,
                    format!("DNS 解析在 {} ms 内未完成", timeout.as_millis()),
                )),
                Ok(Err(error)) => Err((ErrorKind::DnsFailure, format!("DNS 解析失败：{error}"))),
                Ok(Ok(addrs)) => {
                    if !addrs.is_empty() {
                        Ok(())
                    } else {
                        Err((ErrorKind::DnsFailure, "域名解析结果为空".to_string()))
                    }
                }
            }
        }
    };

    let elapsed = started.elapsed();
    let (success, error) = match outcome {
        Ok(()) => (true, None),
        Err((kind, summary)) => (
            false,
            Some(DiagnosticError {
                kind,
                summary,
                detail: Some(format!("host={host}")),
            }),
        ),
    };
    // 耗时仅成功样本有值（见 ProbeSample::duration_ms 契约）。
    let duration_ms = if success {
        Some(elapsed.as_millis() as u64)
    } else {
        None
    };

    ProbeSample {
        target_id: target_id.to_string(),
        probe_type: ProbeType::Dns,
        route_mode,
        attempt_no: 1,
        success,
        duration_ms,
        http_status: None,
        http_outcome: None,
        error,
        tested_at: now_rfc3339(),
    }
}
