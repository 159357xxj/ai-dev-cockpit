//! 导出与展示脱敏（AGENTS.md：认证材料不得出现在报告/界面）。
//!
//! F-02 V3 架构原则（第三轮 QA 后确立）：
//! 1. **字符串替换规则不是安全边界**——安全导出依赖 report.rs 的结构化白名单
//!    （error.summary/detail 默认不导出）。本模块是纵深防御 + 显示辅助。
//! 2. **绝不用 Unicode 小写转换后的字节索引切割原字符串**（İ U+0130 小写展开
//!    2 字节会致索引错位与 UTF-8 边界 panic）——所有匹配基于原文字节的
//!    ASCII 大小写不敏感窗口（needle 全 ASCII → 命中必在字符边界）。
//! 3. 头遮蔽**不做前置边界豁免**（QA 实测 `9Cookie:`、全角 `：Cookie:` 漏判）；
//!    导出场景宁滥勿漏。
//! 4. 所有扫描位置单调推进，无死循环；panic 面为零（无未验证字节切片）。

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
/// 返回原文字节索引（needle 全 ASCII → 命中必在字符边界）。
/// `from` 超界直接返回 None（不回退——回退会破坏调用方单调推进）。
fn find_ascii_ci(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    let last_start = haystack.len() - needle.len();
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

/// 导出文本脱敏（F-02 V3）。规则：
/// 1. 敏感头（含 RFC 折叠续行；**无前置边界豁免**）：Authorization/Cookie/
///    Set-Cookie/X-API-Key/Api-Key/X-Auth-Token/Proxy-Authorization → 值 `[已隐藏]`；
/// 2. URL userinfo（含多 `@`：authority 内最后一个 `@` 之前全部遮蔽）；
/// 3. 敏感键值（?k=/&k=/"k":/裸 k: 带 ASCII 词边界；含 `\"k\":\"` JSON 转义形态）；
/// 4. 私有 IPv4（含 `:端口` 后缀）；
/// 5. 全部合法 IPv6 字面量（含 `%scope` 后缀）整段遮蔽；
/// 6. 本地用户路径全部出现（任意盘符 + 大小写不敏感）；
/// 7. 内部主机名：含 internal/corp/lan/local 标签或 localhost —— **内部标签
///    优先于白名单后缀**（fail-closed：db.internal.github.com 也遮蔽）。
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
        text = mask_escaped_json_key(&text, key);
    }
    text = mask_private_ips(&text);
    text = mask_all_user_paths(&text);
    text = mask_internal_hosts(&text);
    text
}

/// 头遮蔽（值至行尾，吸收 RFC 折叠续行；**要求**头名后（可含空白）紧跟 `:`——
/// 这同时排除了第三轮 QA 的五种 panic 输入：头名后无冒号的普通文本不再切片）。
fn mask_headers_folded(text: &str, header: &[u8]) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut search = 0usize;
    while let Some(name_start) = find_ascii_ci(bytes, header, search) {
        let mut probe = name_start + header.len();
        while probe < bytes.len() && (bytes[probe] == b' ' || bytes[probe] == b'\t') {
            probe += 1;
        }
        // 必须紧跟冒号——"ordinary error mentions cookie"/"Cookieé"/"x-api-key"
        // 等无冒号形态在此跳过（原实现缺此验证导致 text[colon+1..] 越界 panic）。
        if probe >= bytes.len() || bytes[probe] != b':' {
            search = name_start + 1;
            continue;
        }
        let colon = probe;
        let mut value_end = text
            .get(colon + 1..)
            .and_then(|rest| rest.find('\n').map(|offset| colon + 1 + offset))
            .unwrap_or(text.len());
        // 吸收折叠续行（以空白开头的下一行）。
        while value_end < text.len() {
            let line_start = value_end + 1;
            let Some(first) = text.as_bytes().get(line_start) else {
                break;
            };
            if *first != b' ' && *first != b'\t' {
                break;
            }
            value_end = text[line_start..]
                .find('\n')
                .map(|offset| line_start + offset)
                .unwrap_or(text.len());
        }
        output.push_str(&text[cursor..=colon]);
        output.push_str(" [已隐藏]");
        cursor = value_end;
        search = value_end;
    }
    output.push_str(&text[cursor..]);
    output
}

/// URL userinfo：对每个 `://`，authority（至下一个 `/ ? # 空白 引号`）内
/// **最后一个** `@` 之前全部视为凭证（覆盖任意层数的多 `@`）。
fn mask_url_userinfo(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut search = 0usize;
    while let Some(scheme_start) = find_ascii_ci(bytes, b"://", search) {
        let authority_start = scheme_start + 3;
        let authority_end = text[authority_start..]
            .find(['/', '?', '#', ' ', '\t', '\r', '\n', '"'])
            // 相对偏移必须加回绝对坐标（原实现漏加导致反向切片 panic）。
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
        search = authority_end.max(authority_start);
    }
    output.push_str(&text[cursor..]);
    output
}

/// 敏感键值遮蔽。`word_boundary`：键名前不得为 ASCII 字母数字/_/-。
/// 值边界：`& " ' 空白 } , 换行`；已遮蔽值跳过（位置单调推进）。
fn mask_key_pattern(text: &str, pattern: &str, word_boundary: bool) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut search = 0usize;
    while let Some(start) = find_ascii_ci(bytes, pattern.as_bytes(), search) {
        let value_start = start + pattern.len();
        if word_boundary && start > 0 {
            let prev = bytes[start - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-' {
                search = start + 1;
                continue;
            }
        }
        if text[value_start..].starts_with("[已隐藏]") {
            search = value_start;
            continue;
        }
        let value_end = text[value_start..]
            .find(['&', '"', '\'', ' ', '}', '\n', '\r', ','])
            // 相对偏移必须加回绝对坐标（原实现漏加导致 cursor 越界 panic）。
            .map(|offset| value_start + offset)
            .unwrap_or(text.len());
        output.push_str(&text[cursor..value_start]);
        output.push_str("[已隐藏]");
        cursor = value_end;
        search = cursor;
    }
    output.push_str(&text[cursor..]);
    output
}

/// JSON 转义键值（F-02 V3 QA："token":"QA_ESC_13F" 的值原样留存）：
/// 匹配 `\"key\":\"` 形态，值遮蔽至下一个 `\"`。
fn mask_escaped_json_key(text: &str, key: &str) -> String {
    // 原文中的字节序列：反斜杠 + 引号 + key + 反斜杠 + 引号 + 冒号 + 反斜杠 + 引号。
    let needle = format!("\\\"{key}\\\":\\\"");
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut search = 0usize;
    while let Some(start) = find_ascii_ci(bytes, needle.as_bytes(), search) {
        let value_start = start + needle.len();
        if text[value_start..].starts_with("[已隐藏]") {
            search = value_start;
            continue;
        }
        let value_end = text[value_start..]
            .find('\\')
            .map(|offset| value_start + offset)
            .unwrap_or(text.len());
        output.push_str(&text[cursor..value_start]);
        output.push_str("[已隐藏]");
        cursor = value_end;
        search = cursor;
    }
    output.push_str(&text[cursor..]);
    output
}

/// 私有 IPv4（含 `:端口`）与全部合法 IPv6 字面量（含 `%scope`）。
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
            // 带端口的私有 IPv4（QA V3）：`192.168.77.4:8443` —— 旧边界把
            // `:` 当作 IP 字符后缀而不遮蔽。扩展：`:端口数字` 一并遮蔽。
            if !boundary_after
                && bytes[end] == b':'
                && bytes.get(end + 1).is_some_and(|b| b.is_ascii_digit())
            {
                let mut port_end = end + 1;
                while port_end < bytes.len() && bytes[port_end].is_ascii_digit() {
                    port_end += 1;
                }
                // 端口后必须是干净边界。
                if port_end == bytes.len() || !is_ip_char(bytes[port_end]) {
                    output.push_str(&text[cursor..index]);
                    output.push_str("[内网地址]");
                    cursor = port_end;
                    index = port_end;
                    continue;
                }
            }
        }
        // 全部合法 IPv6 字面量（含 %scope）整段遮蔽。
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

/// IPv6 字面量（含 %scope 后缀）：扫描 [0-9a-fA-F:.%] 连续串，要求 ≥2 个 ':'
/// 且 '%' 前的部分是合法 IPv6。时间串 `12:34:56` 被 Ipv6Addr 解析天然拒绝。
fn scan_ipv6_token(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut end = start;
    while end < bytes.len()
        && (bytes[end].is_ascii_hexdigit() || bytes[end] == b':' || bytes[end] == b'.')
    {
        end += 1;
    }
    // 吸收 %scope（后跟 ASCII 字母数字）。
    if end < bytes.len() && bytes[end] == b'%' {
        let mut scope_end = end + 1;
        while scope_end < bytes.len()
            && (bytes[scope_end].is_ascii_alphanumeric() || bytes[scope_end] == b'_')
        {
            scope_end += 1;
        }
        end = scope_end;
    }
    let token = &text[start..end];
    if token.matches(':').count() < 2 {
        return None;
    }
    let base = token.split('%').next().unwrap_or(token);
    if base.parse::<std::net::Ipv6Addr>().is_ok() {
        Some(end)
    } else {
        None
    }
}

/// 本地用户路径全部出现：任意盘符（ASCII 字母）+ `:\users\`（ASCII 大小写
/// 不敏感）及 `/home/`、`/Users/`。
fn mask_all_user_paths(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut index = 0usize;
    // `X:\users\` 需要 index..=index+8 共 9 字节；循环条件必须保证 index+8 可访问
    // （原 `index + 8 <= len` 在 index+8 == len 时越界——"C:\Users" panic 根因）。
    while index + 9 <= bytes.len() {
        // Windows：X:\users\（X 为任意 ASCII 字母；\u 大小写不敏感）。
        let is_win = bytes[index].is_ascii_alphabetic()
            && bytes[index + 1] == b':'
            && bytes[index + 2] == b'\\'
            && bytes[index + 3].eq_ignore_ascii_case(&b'u')
            && bytes[index + 4].eq_ignore_ascii_case(&b's')
            && bytes[index + 5].eq_ignore_ascii_case(&b'e')
            && bytes[index + 6].eq_ignore_ascii_case(&b'r')
            && bytes[index + 7].eq_ignore_ascii_case(&b's')
            && bytes[index + 8] == b'\\'
            && (index == 0 || !bytes[index - 1].is_ascii_alphanumeric());
        let unix_home = &bytes[index..index + 6] == b"/home/";
        let mac_users = index + 7 <= bytes.len() && &bytes[index..index + 7] == b"/Users/";
        if is_win || unix_home || mac_users {
            let prefix_len = if is_win {
                9
            } else if unix_home {
                6
            } else {
                7
            };
            output.push_str(&text[cursor..index]);
            output.push_str("[本地路径]");
            // 跳过用户名（至下一个分隔符）。
            let after = index + prefix_len;
            let mut skip = after;
            while skip < bytes.len()
                && !matches!(
                    bytes[skip],
                    b'/' | b'\\' | b' ' | b'"' | b'\'' | b':' | b'\n' | b'\r'
                )
            {
                skip += 1;
            }
            cursor = skip;
            index = skip;
            continue;
        }
        index += 1;
    }
    output.push_str(&text[cursor..]);
    output
}

/// 内部/未知主机名（QA V3：db.internal.github.com 因白名单后缀误放行）。
/// **fail-closed**：token 含 internal/corp/lan/local 标签或 localhost → 遮蔽；
/// 否则仅当 token 本身或其父域命中白名单（`suffix == token` 或
/// `token.ends_with("." + suffix)`）时保留——其余未知主机一律遮蔽。
/// IP 字面量（v4/v6）不在此处理（由 IP 规则负责，公网 IP 保留）。
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
            // IP 字面量跳过（v4 由私有 IP 规则处理、公网保留；v6 整段已遮蔽）。
            let looks_like_ipv4 = token.split('.').count() == 4
                && token
                    .split('.')
                    .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
            let looks_like_ipv6 = token.contains("::") || token.matches(':').count() >= 2;
            if looks_like_ipv4 || looks_like_ipv6 {
                continue;
            }
            // 含连字符或全数字标签的不是域名（如标识符、版本号）——跳过。
            let has_alpha_label = token
                .split('.')
                .any(|label| label.chars().any(|c| c.is_ascii_alphabetic()));
            if !token.contains('.') || !has_alpha_label {
                continue;
            }
            let internal = token.eq_ignore_ascii_case("localhost")
                || token.split('.').any(|label| {
                    matches!(
                        label.to_ascii_lowercase().as_str(),
                        "internal" | "corp" | "lan" | "local"
                    )
                });
            let whitelisted = !internal
                && WHITELIST.iter().any(|suffix| {
                    token.eq_ignore_ascii_case(suffix)
                        || token.len() > suffix.len()
                            && token[..token.len() - suffix.len()].ends_with('.')
                            && token[token.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
                });
            if !whitelisted {
                // 保守：非白名单主机一律遮蔽（诊断白名单目标在正常流程中由
                // targets.rs 产生，不受影响；合成/未知 ID fail-closed）。
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
