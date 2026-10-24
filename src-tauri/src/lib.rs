//! AI Dev Cockpit — 开发者网络诊断台（Rust 核心）。
//!
//! 只读诊断工具：所有网络探测、系统信息读取、持久化与脱敏导出都在本进程内完成；
//! 前端仅通过白名单 IPC 调用（docs/ARCHITECTURE.md）。

mod commands;
mod diagnostic;
mod security;

use std::sync::Arc;

use diagnostic::backend::{ProbeBackend, StubBackend};

/// 全局应用状态：探测后端经依赖注入组装（docs/ARCHITECTURE.md §2.1，D-012）。
pub struct AppState {
    pub backend: Arc<dyn ProbeBackend>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 生产组装点：Phase 2 将替换为 ProductionBackend（白名单目标 + 真实探测）。
    // 注入面收敛于此处与测试代码，命令签名不暴露注入接口。
    let state = AppState {
        backend: Arc::new(StubBackend::new()),
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::system::get_system_snapshot,
            commands::diagnostics::start_diagnostic,
            commands::diagnostics::cancel_diagnostic,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
