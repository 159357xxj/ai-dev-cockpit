//! 系统信息命令（Phase 1：类型化桩，诚实返回 not_available，不假装有数据）。

use crate::diagnostic::types::{IpcError, SystemSnapshot};

/// 后续阶段返回真实本机网络摘要（platform/windows + diagnostic）。
#[tauri::command(rename_all = "snake_case")]
pub fn get_system_snapshot() -> Result<SystemSnapshot, IpcError> {
    Err(IpcError::not_available("系统快照将在后续阶段提供"))
}
