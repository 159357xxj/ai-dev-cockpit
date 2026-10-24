//! 预设诊断目标白名单（docs/ARCHITECTURE.md §4）。
//!
//! 探测目标的唯一来源：生产构建不接受白名单之外的任何主机/端口输入；
//! 请求不携带任何凭证，仅访问公开 HTTPS 地址。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub host: &'static str,
    /// 仅允许 https:// 的公开地址。
    pub url: &'static str,
}

pub const TARGETS: &[TargetSpec] = &[
    TargetSpec {
        id: "chatgpt",
        label: "ChatGPT",
        host: "chatgpt.com",
        url: "https://chatgpt.com/",
    },
    TargetSpec {
        id: "openai-api",
        label: "OpenAI API",
        host: "api.openai.com",
        url: "https://api.openai.com/",
    },
    TargetSpec {
        id: "openai-auth",
        label: "OpenAI 登录",
        host: "auth.openai.com",
        url: "https://auth.openai.com/",
    },
    TargetSpec {
        id: "claude",
        label: "Claude",
        host: "claude.ai",
        url: "https://claude.ai/",
    },
    TargetSpec {
        id: "github",
        label: "GitHub",
        host: "github.com",
        url: "https://github.com/",
    },
    TargetSpec {
        id: "zcode",
        label: "ZCode",
        host: "zcode.z.ai",
        url: "https://zcode.z.ai/",
    },
];

pub fn find_target(id: &str) -> Option<&'static TargetSpec> {
    TARGETS.iter().find(|target| target.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_six_unique_targets() {
        assert_eq!(TARGETS.len(), 6);
        let mut ids: Vec<&str> = TARGETS.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), TARGETS.len(), "target id 必须唯一");
    }

    #[test]
    fn all_targets_are_https_public_urls() {
        for target in TARGETS {
            assert!(
                target.url.starts_with("https://"),
                "{} 必须是 https 地址",
                target.id
            );
            assert!(target.url.contains(target.host));
            assert!(!target.url.contains('@'), "{} 不得携带 userinfo", target.id);
        }
    }

    #[test]
    fn find_target_matches_registry() {
        assert!(find_target("github").is_some());
        assert!(find_target("openai-api").is_some());
        assert!(find_target("nonexistent").is_none());
        assert!(find_target("").is_none());
    }
}
