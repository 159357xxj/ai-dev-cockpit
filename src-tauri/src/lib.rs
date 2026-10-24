//! AI Dev Cockpit — 开发者网络诊断台（Rust 核心）。
//!
//! 只读诊断工具：所有网络探测、系统信息读取、持久化与脱敏导出都在本进程内完成；
//! 前端仅通过白名单 IPC 调用（docs/ARCHITECTURE.md）。

// diagnostic 设为 pub：集成测试（tests/probes_e2e.rs）直接驱动调度器与探测函数；
// commands / security 保持私有，命令面仅经 Tauri 白名单暴露。
mod commands;
pub mod diagnostic;
mod security;

use std::sync::Arc;

use diagnostic::backend::{ProbeBackend, ProductionBackend};
use tauri::Manager;

/// 全局应用状态：探测后端经依赖注入组装（docs/ARCHITECTURE.md §2.1，D-012）。
pub struct AppState {
    pub backend: Arc<dyn ProbeBackend>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 生产组装点：真实探测后端，端点只来自 targets.rs 白名单。
            // 注入面收敛于此处与测试代码，命令签名不暴露注入接口。
            let version = app.package_info().version.to_string();
            app.manage(AppState {
                backend: Arc::new(ProductionBackend::new(version)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::get_system_snapshot,
            commands::diagnostics::start_diagnostic,
            commands::diagnostics::cancel_diagnostic,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
