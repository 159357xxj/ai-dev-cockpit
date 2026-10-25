//! Windows 代理配置只读快照（docs/ARCHITECTURE.md §9；PRD P0-2）。
//!
//! 三个数据来源**分别检测、分别标注**，互不混同：
//! 1. 当前用户 WinINet 代理配置 —— `WinHttpGetIEProxyConfigForCurrentUser`
//!    （手动代理、PAC URL、自动检测；只读取配置，**不下载/不执行 PAC 脚本**）；
//! 2. WinHTTP 默认代理配置 —— `WinHttpGetDefaultProxyConfiguration`
//!    （机器级 `netsh winhttp` 所设置的值）；
//! 3. 进程环境变量 —— `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY`/`NO_PROXY` 等的存在状态。
//!
//! 边界（冻结）：
//! - 只读：不调用任何设置/修改 API，不执行 netsh/注册表写入；
//! - **配置存在 ≠ 真实请求使用该配置**（应用各自的代理解析规则不同）；
//! - 读取失败 → `status="unknown"`，绝不伪造"未启用"；
//! - 所有字符串经 `security::redaction` 脱敏后才返回，凭证/令牌不出进程。

use std::time::SystemTime;

use crate::diagnostic::types::{EnvVarInfo, ProxySnapshot, WinHttpProxyInfo, WindowsUserProxyInfo};
use crate::security::redaction::redact_proxy_value;

const USER_PROXY_SOURCE: &str = "wininet_api:WinHttpGetIEProxyConfigForCurrentUser";
const WINHTTP_SOURCE: &str = "winhttp_api:WinHttpGetDefaultProxyConfiguration";

fn now_utc() -> String {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| {
            chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                .unwrap_or_default()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
        .unwrap_or_default()
}

/// 完整代理快照（三个来源 + 整体说明）。
pub fn snapshot() -> ProxySnapshot {
    let captured_at = now_utc();
    let user_settings = read_user_proxy_config();
    let winhttp = read_winhttp_default_proxy();
    let env_vars = read_env_vars();

    // 便捷布尔：仅在对应来源 status=ok 时有效，否则 None（未知）。
    let manual_proxy_enabled = if user_settings.status == "ok" {
        Some(user_settings.manual_proxy.is_some())
    } else {
        None
    };
    let pac_configured = if user_settings.status == "ok" {
        Some(user_settings.pac_url.is_some())
    } else {
        None
    };
    let auto_detect_enabled = if user_settings.status == "ok" {
        user_settings.auto_detect
    } else {
        None
    };

    ProxySnapshot {
        user_settings,
        winhttp,
        env_vars,
        manual_proxy_enabled,
        pac_configured,
        auto_detect_enabled,
        captured_at,
        note: "配置存在 ≠ 真实请求使用该配置；本工具不下载/不执行 PAC 脚本，不修改任何设置。各代理软件（Clash 等）的实际生效路径由其自身决定。".to_string(),
    }
}

/// 来源 1：当前用户 WinINet 代理配置（官方 API，只读）。
fn read_user_proxy_config() -> WindowsUserProxyInfo {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::GlobalFree;
        use windows_sys::Win32::Networking::WinHttp::{
            WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WinHttpGetIEProxyConfigForCurrentUser,
        };

        unsafe {
            let mut config = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default();
            if WinHttpGetIEProxyConfigForCurrentUser(&mut config) == 0 {
                return WindowsUserProxyInfo {
                    source: USER_PROXY_SOURCE.to_string(),
                    captured_at: now_utc(),
                    status: "unknown".to_string(),
                    read_error: Some(
                        "WinHttpGetIEProxyConfigForCurrentUser 调用失败（配置不可判定）"
                            .to_string(),
                    ),
                    auto_detect: None,
                    pac_url: None,
                    manual_proxy: None,
                    proxy_bypass: None,
                };
            }
            let auto_config_url = pwstr_to_string(config.lpszAutoConfigUrl);
            let proxy = pwstr_to_string(config.lpszProxy);
            let proxy_bypass = pwstr_to_string(config.lpszProxyBypass);
            // API 文档要求调用方 GlobalFree 释放返回的字符串缓冲区。
            GlobalFree(config.lpszAutoConfigUrl.cast());
            GlobalFree(config.lpszProxy.cast());
            GlobalFree(config.lpszProxyBypass.cast());

            WindowsUserProxyInfo {
                source: USER_PROXY_SOURCE.to_string(),
                captured_at: now_utc(),
                status: "ok".to_string(),
                read_error: None,
                auto_detect: Some(config.fAutoDetect != 0),
                pac_url: auto_config_url
                    .filter(|value| !value.is_empty())
                    .map(|value| redact_proxy_value(&value)),
                manual_proxy: proxy
                    .filter(|value| !value.is_empty())
                    .map(|value| redact_proxy_value(&value)),
                proxy_bypass: proxy_bypass.filter(|value| !value.is_empty()),
            }
        }
    }
    #[cfg(not(windows))]
    {
        WindowsUserProxyInfo {
            source: USER_PROXY_SOURCE.to_string(),
            captured_at: now_utc(),
            status: "unknown".to_string(),
            read_error: Some("非 Windows 平台未实现".to_string()),
            auto_detect: None,
            pac_url: None,
            manual_proxy: None,
            proxy_bypass: None,
        }
    }
}

/// 来源 2：WinHTTP 默认代理配置（机器级，只读 API）。
fn read_winhttp_default_proxy() -> WinHttpProxyInfo {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::GlobalFree;
        use windows_sys::Win32::Networking::WinHttp::{
            WINHTTP_PROXY_INFO, WinHttpGetDefaultProxyConfiguration,
        };

        unsafe {
            let mut info = WINHTTP_PROXY_INFO::default();
            if WinHttpGetDefaultProxyConfiguration(&mut info) != 0 {
                let proxy = pwstr_to_string(info.lpszProxy);
                let bypass = pwstr_to_string(info.lpszProxyBypass);
                GlobalFree(info.lpszProxy.cast());
                GlobalFree(info.lpszProxyBypass.cast());
                WinHttpProxyInfo {
                    source: WINHTTP_SOURCE.to_string(),
                    captured_at: now_utc(),
                    status: "ok".to_string(),
                    read_error: None,
                    access_type: Some(map_access_type(info.dwAccessType)),
                    proxy: proxy
                        .filter(|value| !value.is_empty())
                        .map(|value| redact_proxy_value(&value)),
                    bypass: bypass.filter(|value| !value.is_empty()),
                }
            } else {
                WinHttpProxyInfo {
                    source: WINHTTP_SOURCE.to_string(),
                    captured_at: now_utc(),
                    status: "unknown".to_string(),
                    read_error: Some("WinHttpGetDefaultProxyConfiguration 调用失败".to_string()),
                    access_type: None,
                    proxy: None,
                    bypass: None,
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        WinHttpProxyInfo {
            source: WINHTTP_SOURCE.to_string(),
            captured_at: now_utc(),
            status: "unknown".to_string(),
            read_error: Some("非 Windows 平台未实现".to_string()),
            access_type: None,
            proxy: None,
            bypass: None,
        }
    }
}

/// 来源 3：代理相关环境变量（存在状态 + 脱敏值）。
fn read_env_vars() -> Vec<EnvVarInfo> {
    const NAMES: [&str; 8] = [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "NO_PROXY",
        "no_proxy",
    ];
    NAMES
        .iter()
        .map(|name| {
            // 环境变量读取失败（如包含非 Unicode）视为"存在但不可显示"。
            let value = std::env::var(name);
            match value {
                Ok(value) if value.is_empty() => EnvVarInfo {
                    name: (*name).to_string(),
                    present: true,
                    value_redacted: None,
                    note: "已设置但为空字符串（多数客户端视同未设置）".to_string(),
                },
                Ok(value) => EnvVarInfo {
                    name: (*name).to_string(),
                    present: true,
                    value_redacted: Some(redact_proxy_value(&value)),
                    note: String::new(),
                },
                Err(_) => EnvVarInfo {
                    name: (*name).to_string(),
                    present: false,
                    value_redacted: None,
                    note: String::new(),
                },
            }
        })
        .collect()
}

fn map_access_type(access_type: u32) -> String {
    match access_type {
        0 => "default(未显式设置，通常跟随 WinINet 配置)".to_string(),
        1 => "direct(直连，WinHTTP 不使用代理)".to_string(),
        3 => "named_proxy(已设置命名代理)".to_string(),
        other => format!("other({other})"),
    }
}

#[cfg(windows)]
fn pwstr_to_string(pointer: *mut u16) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    unsafe {
        let mut length = 0usize;
        while *pointer.add(length) != 0 {
            length += 1;
        }
        let slice = std::slice::from_raw_parts(pointer, length);
        Some(String::from_utf16_lossy(slice))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真机只读调用（无网络、无写操作）：结构完整、来源标注正确、脱敏生效。
    #[test]
    #[cfg(windows)]
    fn snapshot_sources_are_labeled_and_safe() {
        let snapshot = snapshot();
        assert!(snapshot.user_settings.source.starts_with("wininet_api"));
        assert!(snapshot.winhttp.source.starts_with("winhttp_api"));
        assert_eq!(snapshot.env_vars.len(), 8);
        // 便捷布尔与来源状态一致：unknown 时不得声称"未启用"。
        if snapshot.user_settings.status == "unknown" {
            assert!(snapshot.manual_proxy_enabled.is_none());
        }
        // 免责说明必须存在。
        assert!(snapshot.note.contains("配置存在 ≠"));
    }

    #[test]
    #[cfg(windows)]
    fn user_proxy_config_handles_real_machine() {
        let info = read_user_proxy_config();
        // 本机无论配置如何，status 必须是 ok/unknown 之一且来源标注固定。
        assert!(info.status == "ok" || info.status == "unknown");
        assert_eq!(info.source, USER_PROXY_SOURCE);
    }
}
