//! 输入校验（docs/ARCHITECTURE.md §5.5，D-005）。
//!
//! 手动本地代理仅允许 loopback 的 http/socks5/socks5h，禁止认证信息、
//! 路径、查询参数与锚点；校验失败必须显式报错，绝不静默回落。

use url::Url;

pub const PROXY_SCHEMES: [&str; 3] = ["http", "socks5", "socks5h"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedProxy {
    /// 归一化后的代理 URL（scheme://host:port，无凭证/路径/查询）。
    pub normalized: String,
    pub scheme: String,
    pub port: u16,
}

/// 校验用户输入的手动本地代理地址。
///
/// 规则：
/// - scheme ∈ {http, socks5, socks5h}（大小写不敏感）；
/// - host ∈ {127.0.0.1, localhost, [::1]}；
/// - 端口必填且 1..=65535；
/// - 禁止 userinfo（用户名/密码）；
/// - 禁止 path/query/fragment。
pub fn validate_manual_proxy_url(raw: &str) -> Result<ValidatedProxy, String> {
    let trimmed = raw.trim();
    let parsed = Url::parse(trimmed).map_err(|error| format!("代理地址无法解析：{error}"))?;

    let scheme = parsed.scheme().to_ascii_lowercase();
    if !PROXY_SCHEMES.contains(&scheme.as_str()) {
        return Err(format!("代理协议仅支持 {PROXY_SCHEMES:?}，收到：{scheme}"));
    }

    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("代理地址不允许携带用户名或密码".to_string());
    }

    let port = parsed
        .port()
        .ok_or_else(|| "代理地址必须显式指定端口（1..=65535）".to_string())?;
    if port == 0 {
        return Err("代理端口不能为 0".to_string());
    }

    match parsed.host() {
        Some(url::Host::Ipv4(ip)) if ip.octets() == [127, 0, 0, 1] => {}
        Some(url::Host::Domain(domain)) if domain.eq_ignore_ascii_case("localhost") => {}
        Some(url::Host::Ipv6(ip)) if ip == std::net::Ipv6Addr::LOCALHOST => {}
        other => {
            return Err(format!(
                "代理主机仅允许 127.0.0.1 / localhost / [::1]，收到：{other:?}"
            ));
        }
    }

    if !parsed.path().is_empty() && parsed.path() != "/" {
        return Err("代理地址不允许携带路径".to_string());
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("代理地址不允许携带查询参数或锚点".to_string());
    }

    let host_display = match parsed.host() {
        Some(url::Host::Ipv6(_)) => "[::1]".to_string(),
        Some(url::Host::Ipv4(ip)) => ip.to_string(),
        Some(url::Host::Domain(domain)) => domain.to_string(),
        None => return Err("代理地址缺少主机".to_string()),
    };

    Ok(ValidatedProxy {
        normalized: format!("{scheme}://{host_display}:{port}"),
        scheme,
        port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_loopback_proxies() {
        for (raw, scheme, port) in [
            ("http://127.0.0.1:7890", "http", 7890u16),
            ("socks5://localhost:1080", "socks5", 1080),
            ("socks5h://[::1]:7897", "socks5h", 7897),
            ("HTTP://127.0.0.1:7890", "http", 7890),
            ("http://127.0.0.1:7890/", "http", 7890),
        ] {
            let validated = validate_manual_proxy_url(raw)
                .unwrap_or_else(|error| panic!("{raw} 应当合法：{error}"));
            assert_eq!(validated.scheme, scheme);
            assert_eq!(validated.port, port);
        }
        assert_eq!(
            validate_manual_proxy_url("socks5://localhost:1080")
                .unwrap()
                .normalized,
            "socks5://localhost:1080"
        );
        assert_eq!(
            validate_manual_proxy_url("socks5h://[::1]:7897")
                .unwrap()
                .normalized,
            "socks5h://[::1]:7897"
        );
    }

    #[test]
    fn rejects_non_loopback_hosts() {
        for raw in [
            "http://proxy.example.com:7890",
            "http://192.168.1.10:7890",
            "http://0.0.0.0:7890",
            "socks5://::1:1080",
        ] {
            assert!(validate_manual_proxy_url(raw).is_err(), "{raw} 应被拒绝");
        }
    }

    #[test]
    fn rejects_credentials_paths_and_junk() {
        for raw in [
            "http://user:pass@127.0.0.1:7890",
            "http://token@127.0.0.1:7890",
            "http://127.0.0.1:7890/api",
            "http://127.0.0.1:7890?token=abc",
            "http://127.0.0.1:7890#frag",
            "ftp://127.0.0.1:21",
            "https://127.0.0.1:7890",
            "http://127.0.0.1",
            "127.0.0.1:7890",
            "not a url",
            "",
            "   ",
        ] {
            assert!(validate_manual_proxy_url(raw).is_err(), "{raw:?} 应被拒绝");
        }
    }
}
