//! 导出与展示脱敏（AGENTS.md：认证材料不得出现在报告/界面）。
//!
//! F-02 V2 架构原则：
//! - **绝不用 Unicode 小写转换后的字节索引切割原字符串**（İ(U+0130) 小写展开为
//!   2 字节会致索引错位、部分泄漏与潜在 panic）——所有匹配基于原文字节的
//!   ASCII 大小写不敏感窗口（needle 全为 ASCII，命中边界必为 UTF-8 字符边界）；
//! - 所有扫描位置推进，无死循环；
//! - 无法可靠净化的长自由文本在导出层保守截断（见 report.rs 的字段白名单）。

/// 代理字符串展示脱敏（UI 快照卡用；按 `;` 分段，userinfo/查询遮蔽）。
pub fn redact_proxy_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed
        .split(';')
        .map(|segment| {
            let segment = segment.trim();
            let without_query = match segment.split_once('?') {
                Some((base, _)) => format!("{base}?[已隐藏]"),
                None => segment.to_string(),
            };
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

// ---------------------------------------------------------------------------
// ASCII 大小写不敏感字节匹配（安全索引的根基）
// ---------------------------------------------------------------------------

/// 在 `haystack[from..]` 中查找 `needle`（ASCII 大小写不敏感）。
/// 返回**原文字节索引**（needle 为 ASCII，边界必为 UTF-8 字符边界）。
fn find_ascii_ci(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    let last_start = haystack.len() - needle.len();
    // 起点超界（已越过最后可能匹配位）→ 无匹配。禁止回退钳制：
    // 回退会破坏调用方的单调推进并造成 cursor>start 的反向切片 panic。
    if from > last_start {
        return None;
    }
    let mut position = from;
    while position <= last_start {
        if haystack[position..position + needle.len()].eq_ignore_ascii_case(needle) {
            return Some(position);
        }
        position += 1;
    }
    None
}

/// 导出文本脱敏（F-02 V2 重写）。规则：
/// 1. 敏感头（含 RFC 折叠续行）：Authorization/Cookie/Set-Cookie/X-API-Key/
///    Api-Key/X-Auth-Token/Proxy-Authorization → 值整段 `[已隐藏]`；
/// 2. URL userinfo（含多 `@`：`://` 与 authority 内**最后一个** `@` 之间全为凭证）；
/// 3. 敏感键值（?k=/&k=/"k":/裸 k: 带 ASCII 词边界）；
/// 4. 私有 IPv4（10/127/172.16-31/192.168/169.254）；
/// 5. **全部** IPv6 字面量（保守：诊断白名单无 IPv6 主机；整段遮蔽）；
/// 6. 本地用户路径（`C:\Users\` / `/home/` / `/Users/` 的**全部**出现）；
/// 7. 内部主机名（localhost 与含 internal/corp/lan/local 标签的域名）。
pub fn sanitize_text(raw: &str) -> String {
    let mut text = raw.to_string();
    for header in [
        "authorization",
        "cookie",
        "set-cookie",
        "x-api-key",
        "api-key",
        "x-auth-token",
        "proxy-authorization",
    ] {
        text = mask_headers_folded(&text, header.as_bytes());
    }
    text = mask_url_userinfo(&text);
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
        text = mask_key_pattern(&text, &format!("?{key}="), false);
        text = mask_key_pattern(&text, &format!("&{key}="), false);
        text = mask_key_pattern(&text, &format!("\"{key}\""), true);
        text = mask_key_pattern(&text, &format!("{key}:"), true);
    }
    text = mask_private_ips(&text);
    text = mask_all_user_paths(&text);
    text = mask_internal_hosts(&text);
    text
}

/// 头遮蔽（含折叠续行：以空白开头的后续行并入值）。
fn mask_headers_folded(text: &str, header: &[u8]) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut search = 0usize;
    while let Some(name_start) = find_ascii_ci(bytes, header, search) {
        // 头名后：可含空白，必须跟 ':'。
        let mut probe = name_start + header.len();
        while probe < bytes.len() && (bytes[probe] == b' ' || bytes[probe] == b'\t') {
            probe += 1;
        }
        // F-02 V2：导出场景保守遮蔽——不要求头名前边界（QA 实测：随机前缀
        // `9Cookie:` / 全角冒号 `：Cookie:` 都曾漏判）。仅要求"头名+冒号"形态，
        // 命中即遮蔽值至行尾（误伤面：导出文本中的自定义 `xxxCookie:` 也被
        // 保守遮蔽，可接受）。
        let colon = probe;
        // 值至行尾，再吸收折叠续行（以空格/Tab 开头的行）。
        let mut value_end = text[colon + 1..]
            .find('\n')
            .map(|offset| colon + 1 + offset)
            .unwrap_or(text.len());
        while value_end < text.len() {
            let line_start = value_end + 1;
            if line_start >= text.len() {
                break;
            }
            let first = bytes[line_start];
            if first == b' ' || first == b'\t' {
                value_end = text[line_start..]
                    .find('\n')
                    .map(|offset| line_start + offset)
                    .unwrap_or(text.len());
            } else {
                break;
            }
        }
        output.push_str(&text[cursor..colon + 1]);
        output.push_str(" [已隐藏]");
        cursor = value_end;
        search = value_end;
    }
    output.push_str(&text[cursor..]);
    output
}

/// URL userinfo：对每个 `://`，取 authority（到下一个 `/ ? #` 或空白）内**最后一个**
/// `@`，其前全部视为凭证遮蔽（天然覆盖多 `@`）。
fn mask_url_userinfo(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut search = 0usize;
    while let Some(scheme_start) = find_ascii_ci(bytes, b"://", search) {
        let authority_start = scheme_start + 3;
        let authority_end = text[authority_start..]
            .find(['/', '?', '#', ' ', '\t', '\r', '\n', '"'])
            .map(|offset| authority_start + offset)
            .unwrap_or(text.len());
        let authority = &text[authority_start..authority_end];
        if let Some(at_offset) = authority.rfind('@')
            && at_offset > 0
        {
            output.push_str(&text[cursor..authority_start]);
            output.push_str("[凭证已隐藏]@");
            cursor = authority_start + at_offset + 1;
        }
        search = authority_end.max(scheme_start + 3);
    }
    output.push_str(&text[cursor..]);
    output
}

/// 敏感键值遮蔽。`word_boundary=true` 表示 `"k"` / `k:` 形态需 ASCII 词边界；
/// 查询参数形态（?k=/&k=）不需要。
fn mask_key_pattern(text: &str, pattern: &str, word_boundary: bool) -> String {
    let needle = pattern.as_bytes();
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut search = 0usize;
    while let Some(start) = find_ascii_ci(bytes, needle, search) {
        let after_pattern = start + needle.len();
        // JSON 键形态："k" 后值起点为冒号后；裸 k: / ?k= / &k= 直接是值起点。
        let (value_start, valid_form) = if needle.first() == Some(&b'"') {
            match text[after_pattern..].find(':') {
                Some(offset) => (after_pattern + offset + 1, true),
                None => (after_pattern, false),
            }
        } else {
            (after_pattern, true)
        };
        // 词边界：模式首字符为键名（字母）时，其前一字节不得为 ASCII 字母数字/_。
        let boundary_ok = if word_boundary {
            let first = needle[0];
            if first == b'"' {
                true
            } else {
                start == 0
                    || !(bytes[start - 1].is_ascii_alphanumeric()
                        || bytes[start - 1] == b'_'
                        || bytes[start - 1] == b'-')
            }
        } else {
            true
        };
        if !valid_form || !boundary_ok {
            search = start + 1;
            continue;
        }
        let trimmed_start =
            value_start + (text[value_start..].len() - text[value_start..].trim_start().len());
        // 已遮蔽值跳过（位置推进防死循环）。
        if text[trimmed_start..].starts_with("[已隐藏]") {
            search = trimmed_start;
            continue;
        }
        let value_end = text[trimmed_start..]
            .find(['&', '"', '\'', ' ', '}', '\n', '\r', ','])
            .map(|offset| trimmed_start + offset)
            .unwrap_or(text.len());
        output.push_str(&text[cursor..start]);
        output.push_str(pattern);
        output.push_str("[已隐藏]");
        cursor = value_end;
        search = value_end;
    }
    output.push_str(&text[cursor..]);
    output
}

/// 私有 IPv4 + 全部 IPv6 字面量（原文索引解析）。
fn mask_private_ips(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        let boundary_before = index == 0 || !is_ip_char(bytes[index - 1]);
        let byte = bytes[index];
        if byte.is_ascii_digit()
            && boundary_before
            && let Some((end, octets)) = parse_ipv4_at(text, index)
        {
            let boundary_after = end == bytes.len() || !is_ip_char(bytes[end]);
            if boundary_after && is_private_v4(octets) {
                output.push_str(&text[cursor..index]);
                output.push_str("[内网地址]");
                cursor = end;
                index = end;
                continue;
            }
        }
        // IPv6：候选 token = 连续 [0-9a-fA-F:.] 且 ≥2 个 ':'；Ipv6Addr 解析器
        // 天然拒绝时间串（12:34:56）等形态。保守：**全部**合法 IPv6 整段遮蔽。
        if (byte.is_ascii_hexdigit() || byte == b':')
            && boundary_before
            && let Some(end) = scan_ipv6_token(text, index)
        {
            output.push_str(&text[cursor..index]);
            output.push_str("[内网地址]");
            cursor = end;
            index = end;
            continue;
        }
        index += 1;
    }
    output.push_str(&text[cursor..]);
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

/// 从 index 起扫描最长 [0-9a-fA-F:.] 串；若其为合法 IPv6 地址且边界干净 → 返回结束位置。
fn scan_ipv6_token(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut end = start;
    while end < bytes.len()
        && (bytes[end].is_ascii_hexdigit() || bytes[end] == b':' || bytes[end] == b'.')
    {
        end += 1;
    }
    // 后边界：紧邻 ip 字符（但非本类字符）则整体放弃（更长 token 的一部分）。
    if end < bytes.len() && is_ip_char(bytes[end]) && !bytes[end].is_ascii_hexdigit() {
        return None;
    }
    let token = &text[start..end];
    if token.matches(':').count() < 2 {
        return None;
    }
    if token.parse::<std::net::Ipv6Addr>().is_ok() {
        Some(end)
    } else {
        None
    }
}

/// 本地用户路径的**全部**出现（QA：第二个及后续路径此前泄漏）。
fn mask_all_user_paths(text: &str) -> String {
    let windows_prefix = String::from_utf8_lossy(b"C:\\Users\\").to_string();
    let mut output = text.to_string();
    for prefix in [windows_prefix.as_str(), "/home/", "/Users/"] {
        while let Some(start) = output.find(prefix) {
            let after = &output[start + prefix.len()..];
            let end = after
                .find(['/', ' ', '"', ':', '\\'])
                .map(|offset| start + prefix.len() + offset)
                .unwrap_or(output.len());
            output = format!("{}[本地路径]{}", &output[..start], &output[end..]);
        }
    }
    output
}

/// 内部主机名（原文索引 token 扫描，ASCII 大小写不敏感比较）。
fn mask_internal_hosts(text: &str) -> String {
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
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_alphanumeric() {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric()
                    || bytes[index] == b'.'
                    || bytes[index] == b'-')
            {
                index += 1;
            }
            let token = &text[start..index];
            let whitelisted = WHITELIST.iter().any(|suffix| {
                token.eq_ignore_ascii_case(suffix)
                    || token
                        .len()
                        .checked_sub(suffix.len() + 1)
                        .map(|at| {
                            token.as_bytes()[at] == b'.'
                                && token[at + 1..].eq_ignore_ascii_case(suffix)
                        })
                        .unwrap_or(false)
            });
            let internal = token.eq_ignore_ascii_case("localhost")
                || [".local", ".internal", ".lan", ".corp"]
                    .iter()
                    .any(|suffix| {
                        token.len() > suffix.len()
                            && token[token.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
                    })
                || (token.contains('.')
                    && token.split('.').any(|label| {
                        matches!(
                            label.to_ascii_lowercase().as_str(),
                            "internal" | "corp" | "lan" | "local"
                        )
                    }));
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
}
