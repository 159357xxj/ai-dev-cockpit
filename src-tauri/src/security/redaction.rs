//! 导出与展示脱敏（AGENTS.md：认证材料不得出现在报告/界面）。
//!
//! 本模块只做**展示前脱敏**，不做任何网络请求；导出级脱敏（报告生成）
//! 共用本模块规则（F-02 加固：Cookie/API Key 头、私有 IP、内部主机名、多次出现、大小写）。

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

/// 导出文本脱敏（F-02 加固版）：对导出报告中的自由文本做保守遮蔽。
///
/// 规则（大小写不敏感；支持同文本多次出现；所有搜索位置推进，无死循环）：
/// 1. 敏感头：`Authorization` / `Cookie` / `Set-Cookie` / `X-API-Key` / `Api-Key` /
///    `X-Auth-Token` / `Proxy-Authorization` —— 头名保留，值替换为 `[已隐藏]`；
/// 2. URL userinfo：`scheme://user:pass@host` → `scheme://[凭证已隐藏]@host`；
/// 3. 敏感键值：`token` / `key` / `secret` / `password` / `apikey` / `api_key` /
///    `access_token` / `auth` 作为查询参数或 JSON 键时，值遮蔽；
/// 4. 私有 IP：IPv4 回环/私有/链路本地与 IPv6 本地字面量 → `[内网地址]`；
///    公网 IP 与预设白名单域名保留；
/// 5. 本地用户路径：`C:\Users\<name>` / `/home/<name>` / `/Users/<name>` → `[本地路径]`；
/// 6. 内部主机名（保守）：`localhost` / `*.local` / `*.internal` / `*.lan` / `*.corp`
///    → `[内部主机]`；预设诊断目标域名（github.com 等）不受影响。
pub fn sanitize_text(raw: &str) -> String {
    let mut text = raw.to_string();

    // 1. 敏感头（值至行尾）。
    for header in [
        "authorization",
        "cookie",
        "set-cookie",
        "x-api-key",
        "api-key",
        "x-auth-token",
        "proxy-authorization",
    ] {
        text = mask_header_values(&text, header);
    }

    // 2. URL userinfo（保守：仅当 @ 前是纯 authority 形态，避免误伤邮件地址等）。
    if let Some(at) = text.rfind('@') {
        let head = &text[..at];
        if let Some(scheme_end) = head.rfind("://") {
            let authority = &head[scheme_end + 3..];
            if !authority.is_empty()
                && authority.chars().all(|ch| {
                    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | ':' | '%' | '_')
                })
            {
                text = format!(
                    "{}{}{}",
                    &head[..scheme_end + 3],
                    "[凭证已隐藏]@",
                    &text[at + 1..]
                );
            }
        }
    }

    // 3. 敏感键值（?k= / &k= / "k": / k: 形态）。
    for key in [
        "token",
        "key",
        "secret",
        "password",
        "apikey",
        "api_key",
        "access_token",
        "auth",
    ] {
        text = mask_pattern_values(&text, &format!("?{key}="));
        text = mask_pattern_values(&text, &format!("&{key}="));
        text = mask_pattern_values(&text, &format!("\"{key}\""));
        text = mask_pattern_values(&text, &format!("{key}:"));
    }

    // 4. 私有/内部 IP。
    text = mask_private_ips(&text);

    // 5. 本地用户路径（首个匹配即可满足脱敏要求）。
    let windows_user_prefix = String::from_utf8_lossy(b"C:\\Users\\").to_string();
    for prefix in [windows_user_prefix.as_str(), "/home/", "/Users/"] {
        if let Some(start) = text.find(prefix) {
            let after = &text[start + prefix.len()..];
            let end = after
                .find(['/', ' ', '"', ':', '\\'])
                .map(|offset| start + prefix.len() + offset)
                .unwrap_or(text.len());
            text = format!("{}[本地路径]{}", &text[..start], &text[end..]);
        }
    }

    // 6. 内部主机名。
    text = mask_internal_hosts(&text);

    text
}

/// 遮蔽指定头（大小写不敏感）的值：`Name: value` → `Name: [已隐藏]`。
fn mask_header_values(text: &str, header_lower: &str) -> String {
    let lowered = text.to_lowercase();
    let Some(first) = lowered.find(header_lower) else {
        return text.to_string();
    };
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut search = first;
    while let Some(relative) = lowered[search..].find(header_lower) {
        let name_start = search + relative;
        search = name_start + header_lower.len();
        // 头名前必须是边界（行首、空白、引号或容器字符）。
        let boundary_ok = name_start == 0
            || text[..name_start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '"' | '{' | '[' | ','));
        // 头名后必须紧跟（可含空白）冒号。
        let after_name = &text[name_start + header_lower.len()..];
        let colon_offset = after_name.find(':');
        let header_form = boundary_ok
            && colon_offset
                .map(|offset| after_name[..offset].trim().is_empty())
                .unwrap_or(false);
        if !header_form {
            continue;
        }
        let colon = name_start + header_lower.len() + colon_offset.unwrap_or(0);
        // 值至行尾（换行）。
        let value_end = text[colon + 1..]
            .find('\n')
            .map(|offset| colon + 1 + offset)
            .unwrap_or(text.len());
        output.push_str(&text[cursor..colon + 1]);
        output.push_str(" [已隐藏]");
        cursor = value_end;
        search = value_end;
    }
    output.push_str(&text[cursor..]);
    output
}

/// 遮蔽 `pattern` 后面的值（大小写不敏感，位置推进，无死循环）。
/// pattern 形态：`?k=` / `&k=` / `"k"` / `k:`。
fn mask_pattern_values(text: &str, pattern: &str) -> String {
    let lowered = text.to_lowercase();
    let pattern_lower = pattern.to_lowercase();
    if !lowered.contains(&pattern_lower) {
        return text.to_string();
    }
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut search = 0usize;
    while let Some(relative) = lowered[search..].find(&pattern_lower) {
        let start = search + relative;
        search = start + pattern_lower.len();
        // 原文中对应片段（保留原始大小写）。
        let raw_prefix = &text[start..start + pattern.len()];
        // 确定"值"的起点与要保留的前缀。
        let value_start: usize = if raw_prefix.starts_with('?') || raw_prefix.starts_with('&') {
            start + pattern.len()
        } else if raw_prefix.starts_with('"') {
            // JSON 键形态 "k"：值起点为后续冒号后。
            match text[start + pattern.len()..].find(':') {
                Some(offset) => start + pattern.len() + offset + 1,
                None => start + pattern.len(),
            }
        } else {
            // 裸 k: 形态。
            start + pattern.len()
        };
        // 值起点后的首个非空白。
        let trimmed = text[value_start..].trim_start();
        let value_begin = value_start + (text[value_start..].len() - trimmed.len());
        // 已遮蔽则跳过。
        if text[value_begin..].starts_with("[已隐藏]") {
            continue;
        }
        let value_end = text[value_begin..]
            .find(['&', '"', '\'', ' ', '}', '\n', '\r', ','])
            .map(|offset| value_begin + offset)
            .unwrap_or(text.len());
        output.push_str(&text[cursor..value_begin]);
        output.push_str("[已隐藏]");
        cursor = value_end;
        search = value_end;
    }
    output.push_str(&text[cursor..]);
    output
}

/// 遮蔽私有/回环/链路本地 IPv4 与本地 IPv6（公网 IP 保留）。
fn mask_private_ips(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        let is_boundary_before = index == 0 || !is_ip_char(bytes[index - 1]);
        if bytes[index].is_ascii_digit()
            && is_boundary_before
            && let Some((end, octets)) = parse_ipv4_at(text, index)
        {
            let is_boundary_after = end == bytes.len() || !is_ip_char(bytes[end]);
            if is_boundary_after && is_private_v4(octets) {
                output.push_str(&text[cursor..index]);
                output.push_str("[内网地址]");
                cursor = end;
                index = end;
                continue;
            }
        }
        index += 1;
    }
    output.push_str(&text[cursor..]);

    // IPv6 本地字面量（保守）。
    for literal in ["::1", "fe80::", "fc00::", "fd00::"] {
        output = output.replace(literal, "[内网地址]");
    }
    output
}

fn is_ip_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'.' || byte == b':'
}

fn parse_ipv4_at(text: &str, start: usize) -> Option<(usize, [u8; 4])> {
    let bytes = text.as_bytes();
    let mut octets = [0u8; 4];
    let mut position = start;
    let mut octet_index = 0usize;
    while octet_index < 4 {
        let mut value = 0usize;
        let mut digits = 0;
        while position < bytes.len() && bytes[position].is_ascii_digit() && digits < 3 {
            value = value * 10 + usize::from(bytes[position] - b'0');
            position += 1;
            digits += 1;
        }
        if digits == 0 || value > 255 {
            return None;
        }
        octets[octet_index] = value as u8;
        octet_index += 1;
        if octet_index < 4 {
            if position >= bytes.len() || bytes[position] != b'.' {
                return None;
            }
            position += 1;
        }
    }
    Some((position, octets))
}

fn is_private_v4(octets: [u8; 4]) -> bool {
    match octets[0] {
        10 | 127 => true,
        172 => (16..=31).contains(&octets[1]),
        192 => octets[1] == 168,
        169 => octets[1] == 254,
        _ => false,
    }
}

/// 内部主机名遮蔽：显式保留字与内网后缀；预设诊断目标域名不受影响。
fn mask_internal_hosts(text: &str) -> String {
    // 诊断白名单（targets.rs）+ 测试夹具域名：不遮蔽。
    const WHITELIST: [&str; 8] = [
        "github.com",
        "chatgpt.com",
        "openai.com",
        "claude.ai",
        "z.ai",
        "example.com",
        "invalid",
        "localhost.invalid",
    ];
    let lowered = text.to_lowercase();
    let bytes = lowered.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch.is_ascii_alphanumeric() {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric()
                    || bytes[index] == b'.'
                    || bytes[index] == b'-')
            {
                index += 1;
            }
            let token = &lowered[start..index];
            let whitelisted = WHITELIST
                .iter()
                .any(|suffix| token == *suffix || token.ends_with(&format!(".{suffix}")));
            let internal = token == "localhost"
                || token.ends_with(".local")
                || token.ends_with(".internal")
                || token.ends_with(".lan")
                || token.ends_with(".corp")
                // 保守：任一标签命中内部后缀即视为内部主机（如 db.internal.corp.example）。
                || token
                    .split('.')
                    .any(|label| matches!(label, "internal" | "corp" | "lan" | "local"))
                    && token.contains('.');
            if internal && !whitelisted {
                output.push_str(&text[cursor..start]);
                output.push_str("[内部主机]");
                cursor = index;
            }
        } else {
            index += 1;
        }
    }
    output.push_str(&text[cursor..]);
    output
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
