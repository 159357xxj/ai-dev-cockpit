//! 系统信息命令。

use crate::diagnostic::types::{IpcError, ProxySnapshot, SystemSnapshot};

/// 后续阶段返回真实本机网络摘要（platform/windows + diagnostic）。
#[tauri::command(rename_all = "snake_case")]
pub fn get_system_snapshot() -> Result<SystemSnapshot, IpcError> {
    Err(IpcError::not_available("系统快照将在后续阶段提供"))
}

/// Windows 代理配置只读快照（Phase 3：三个来源分别标注，全量脱敏）。
/// 纯本地读取，无网络请求，不修改任何设置。
#[tauri::command(rename_all = "snake_case")]
pub fn get_proxy_snapshot() -> Result<ProxySnapshot, IpcError> {
    Ok(crate::platform::windows::proxy::snapshot())
}
