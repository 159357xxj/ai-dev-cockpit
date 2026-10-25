//! 导出与展示脱敏（AGENTS.md：认证材料不得出现在报告/界面）。
//!
//! 本模块只做**展示前脱敏**，不做任何网络请求；导出级脱敏（报告生成）
//! 在 Phase 5 扩展，共用本模块规则。

/// 代理字符串展示脱敏规则：
/// - 按 `;` 分段（Windows ProxyServer 的 per-protocol 格式逐段处理）；
/// - 每段含 `@` 时：凭证部分替换为 `[凭证已隐藏]`，仅保留 `@` 之后内容；
/// - 含 `?` 查询串时：整段查询替换为 `?[已隐藏]`；
/// - 其余内容（host:port、scheme://host:port）原样保留。
///
/// 示例：
/// - `user:pass@127.0.0.1:7890` → `[凭证已隐藏]@127.0.0.1:7890`
/// - `http://127.0.0.1:7890` → 原样
/// - `http=u:p@proxy:8080;https=proxy:8443` → `http=[凭证已隐藏]@proxy:8080;https=proxy:8443`
/// - `host:7890?token=abc` → `host:7890?[已隐藏]`
pub fn redact_proxy_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed
        .split(';')
        .map(|segment| {
            let segment = segment.trim();
            // 查询串整段遮蔽（可能携带 token）。
            let without_query = match segment.split_once('?') {
                Some((base, _)) => format!("{base}?[已隐藏]"),
                None => segment.to_string(),
            };
            // userinfo 遮蔽：凭证位于最近的 "://" 或 '=' 之后、最后一个 '@' 之前；
            // scheme（http://）与 per-protocol 前缀（http=）必须保留。
            match without_query.rfind('@') {
                None => without_query,
                Some(at) => {
                    let before = &without_query[..at];
                    let keep_prefix_len = before
                        .rfind("://")
                        .map(|index| index + 3)
                        .or_else(|| before.rfind('=').map(|index| index + 1))
                        .unwrap_or(0);
                    format!(
                        "{}[凭证已隐藏]@{}",
                        &before[..keep_prefix_len],
                        &without_query[at + 1..]
                    )
                }
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(test)]
mod tests {
    use super::redact_proxy_value;

    #[test]
    fn masks_userinfo_and_keeps_host() {
        assert_eq!(
            redact_proxy_value("user:pass@127.0.0.1:7890"),
            "[凭证已隐藏]@127.0.0.1:7890"
        );
        assert_eq!(
            redact_proxy_value("http://token@proxy.local:8080"),
            "http://[凭证已隐藏]@proxy.local:8080"
        );
    }

    #[test]
    fn keeps_plain_values() {
        assert_eq!(redact_proxy_value("127.0.0.1:7890"), "127.0.0.1:7890");
        assert_eq!(
            redact_proxy_value("http://127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert_eq!(redact_proxy_value("proxy.local"), "proxy.local");
    }

    #[test]
    fn handles_per_protocol_and_query() {
        assert_eq!(
            redact_proxy_value("http=u:p@proxy:8080;https=proxy:8443"),
            "http=[凭证已隐藏]@proxy:8080;https=proxy:8443"
        );
        assert_eq!(
            redact_proxy_value("127.0.0.1:7890?token=abc"),
            "127.0.0.1:7890?[已隐藏]"
        );
    }

    #[test]
    fn empty_and_whitespace_are_stable() {
        assert_eq!(redact_proxy_value(""), "");
        assert_eq!(redact_proxy_value("   "), "");
    }
}
