//! AI Dev Cockpit — 开发者网络诊断台（Rust 核心）。
//!
//! 只读诊断工具：所有网络探测、系统信息读取、持久化与脱敏导出都在本进程内完成；
//! 前端仅通过白名单 IPC 调用（docs/ARCHITECTURE.md）。

// diagnostic / platform / security / storage 设为 pub：集成测试直接驱动调度器、
// 探测函数、Windows 代理快照（只读）、脱敏/校验规则与存储层。
// commands 的 build_report/ReportFormat 供导出测试复用（命令面仍仅经 Tauri 白名单暴露）。
pub mod commands;
pub mod diagnostic;
pub mod platform;
pub mod security;
pub mod storage;

use std::sync::{Arc, Mutex};

use diagnostic::backend::{ProbeBackend, ProductionBackend};
use storage::Storage;
use tauri::Manager;

/// 全局应用状态：探测后端 + 历史存储（均经依赖注入组装，D-012）。
pub struct AppState {
    pub backend: Arc<dyn ProbeBackend>,
    /// None = 存储不可用（历史/导出命令返回 internal 错误，其余功能不受影响）。
    pub storage: Option<Arc<Storage>>,
    /// 报告快照管理（F-07：预览与导出复用同一份脱敏字节）。
    pub snapshot_store: Mutex<commands::report::SnapshotStore>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 生产组装点：真实探测后端，端点只来自 targets.rs 白名单。
            // 注入面收敛于此处与测试代码，命令签名不暴露注入接口。
            let version = app.package_info().version.to_string();

            // 历史存储：应用数据目录下 history.db；打开失败不阻断应用（记为不可用）。
            let storage = app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|data_dir| {
                    std::fs::create_dir_all(&data_dir).ok()?;
                    Storage::open(&data_dir.join("history.db")).ok()
                })
                .map(|storage| {
                    // 启动即执行保留策略（30 天 / 500 个 run，settings 可覆盖）。
                    let _ = storage.enforce_retention();
                    Arc::new(storage)
                });

            let backend = match storage.clone() {
                Some(storage) => ProductionBackend::new(version)
                    .with_on_run_complete(commands::report::make_persister(storage)),
                None => ProductionBackend::new(version),
            };
            app.manage(AppState {
                backend: Arc::new(backend),
                storage,
                snapshot_store: Mutex::new(commands::report::SnapshotStore::default()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::get_system_snapshot,
            commands::system::get_proxy_snapshot,
            commands::diagnostics::start_diagnostic,
            commands::diagnostics::cancel_diagnostic,
            commands::diagnostics::diagnose_runs,
            commands::history::list_runs,
            commands::history::get_run,
            commands::history::delete_run,
            commands::history::clear_history,
            commands::history::get_setting,
            commands::history::set_setting,
            commands::report::preview_sanitized_report,
            commands::report::export_sanitized_report,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
