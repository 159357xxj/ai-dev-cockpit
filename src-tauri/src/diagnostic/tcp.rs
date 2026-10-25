//! 独立 TCP connect 测量（docs/ARCHITECTURE.md §7.1）。
//!
//! 先做一次**不计时**的前置解析（解析失败归类 `dns_failure`，与 TCP 连接结论
//! 分离），随后对解析出的地址逐个做计时 connect——测得的是纯 TCP 建连耗时，
//! **不是 ICMP RTT、不叫 Ping**。IPv4/IPv6：按解析顺序尝试全部地址，
//! 任一成功即成功。

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

use super::types::{DiagnosticError, ErrorKind, ProbeSample, ProbeType, RouteMode, now_rfc3339};

/// 对 `host:port` 做一次独立 TCP connect 测量。
pub async fn probe_tcp(
    target_id: &str,
    host: &str,
    port: u16,
    route_mode: RouteMode,
    timeout: Duration,
    cancel: CancellationToken,
    resolver: &super::dns::Resolver,
) -> ProbeSample {
    // 前置解析不计时：让 TCP 耗时只包含建连本身。
    let addrs: Vec<SocketAddr> = match resolver(host.to_string(), port).await {
        Ok(addrs) => addrs,
        Err(error) => {
            return failure_sample(
                target_id,
                host,
                port,
                route_mode,
                ErrorKind::DnsFailure,
                format!("TCP 探测的前置解析失败：{error}"),
                None,
            );
        }
    };
    if addrs.is_empty() {
        return failure_sample(
            target_id,
            host,
            port,
            route_mode,
            ErrorKind::DnsFailure,
            "前置解析结果为空".to_string(),
            None,
        );
    }

    let started = Instant::now();
    let addr_count = addrs.len();
    let mut last_error = String::new();
    for addr in addrs {
        let connected = tokio::select! {
            _ = cancel.cancelled() => {
                return cancelled_sample(target_id, host, port, route_mode);
            }
            connected = tokio::time::timeout(timeout, TcpStream::connect(addr)) => connected,
        };
        match connected {
            // 整个 connect 窗口耗尽：不再尝试后续地址。
            Err(_) => {
                return failure_sample(
                    target_id,
                    host,
                    port,
                    route_mode,
                    ErrorKind::Timeout,
                    format!("TCP connect 在 {} ms 内未完成", timeout.as_millis()),
                    None,
                );
            }
            Ok(Err(error)) => {
                last_error = format!("{addr}: {error}");
            }
            Ok(Ok(_stream)) => {
                let duration_ms = Some(started.elapsed().as_millis() as u64);
                return ProbeSample {
                    target_id: target_id.to_string(),
                    probe_type: ProbeType::Tcp,
                    route_mode,
                    attempt_no: 1,
                    success: true,
                    duration_ms,
                    http_status: None,
                    http_outcome: None,
                    error: None,
                    tested_at: now_rfc3339(),
                };
            }
        }
    }

    failure_sample(
        target_id,
        host,
        port,
        route_mode,
        ErrorKind::TcpFailure,
        format!("无法建立 TCP 连接（尝试了 {addr_count} 个地址）"),
        Some(last_error),
    )
}

fn cancelled_sample(target_id: &str, host: &str, port: u16, route_mode: RouteMode) -> ProbeSample {
    ProbeSample {
        target_id: target_id.to_string(),
        probe_type: ProbeType::Tcp,
        route_mode,
        attempt_no: 1,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: Some(DiagnosticError {
            kind: ErrorKind::Cancelled,
            summary: "任务已取消，TCP 连接未完成".to_string(),
            detail: Some(format!("{host}:{port}")),
        }),
        tested_at: now_rfc3339(),
    }
}

fn failure_sample(
    target_id: &str,
    host: &str,
    port: u16,
    route_mode: RouteMode,
    kind: ErrorKind,
    summary: String,
    detail: Option<String>,
) -> ProbeSample {
    ProbeSample {
        target_id: target_id.to_string(),
        probe_type: ProbeType::Tcp,
        route_mode,
        attempt_no: 1,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: Some(DiagnosticError {
            kind,
            summary,
            detail: detail.or(Some(format!("{host}:{port}"))),
        }),
        tested_at: now_rfc3339(),
    }
}
