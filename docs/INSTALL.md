# 安装说明（Windows 10/11）

## 系统要求

| 项目 | 要求 |
|---|---|
| 操作系统 | Windows 10 1809+ 或 Windows 11（x64） |
| WebView2 Runtime | 需要（Win10/11 通常已内置；安装包会按需引导安装） |
| 管理员权限 | 安装向导是否请求管理员权限未通过 Gate B 实测（见 `docs/GATE_B_WAIVER.md`） |
| 网络连接 | 安装时可能需要（WebView2 引导器）；应用本身离线可用 |

## 安装步骤

1. 从[官方 Releases 页面](https://github.com/159357xxj/ai-dev-cockpit-releases/releases)获取 `AI-Dev-Cockpit_<版本>_x64-setup.exe`。
2. 核对文件 SHA-256 校验值（发布页会附 `SHA256SUMS.txt`；
   本地验证：`certutil -hashfile <文件> SHA256`）。
3. 双击运行安装程序，选择安装语言（简体中文/English），按向导完成安装。
4. 从开始菜单启动 **AI Dev Cockpit**。

> 安装包未签名，Windows 可能显示 SmartScreen 提示。请只从官方发布页下载，并先核对 SHA-256。

## 数据位置

| 数据 | 位置 |
|---|---|
| 历史数据库 | Rust 通过 Tauri `app_data_dir()` 在应用数据目录下创建 `history.db`（SQLite，含 WAL；应用标识 `com.aidevcockpit.app`；安装版实际路径未经 Gate B 验证） |
| 导出报告 | 用户在"另存为"对话框中选择的任意位置 |
| 卸载影响 | 设计上历史数据库位于独立的用户应用数据目录；真实卸载与重装后的数据读取未通过 Gate B 验证 |

## 升级

设计上，直接运行新版安装包覆盖安装时会执行向前兼容的版本化迁移并保留历史数据；此行为尚未在真实升级中验证。

导出报告时，如果所选文件已存在，应用会拒绝保存并提示换名，不会覆盖原文件。

### 升级前备份（重要）

应用使用 SQLite WAL 模式：最近写入可能仍在 `history.db-wal` 中、尚未合并进主库。
**只复制 `history.db` 一个文件不构成可靠备份**。可靠的备份方式（任选其一）：

1. **推荐：先退出应用**（确保进程完全结束，任务管理器无 `ai-dev-cockpit.exe`），
   再把 `history.db`、`history.db-wal`、`history.db-shm` **三个文件一起复制**。
2. 使用任意 SQLite 工具执行 Backup API（如 `sqlite3 history.db ".backup backup.db"`），
   可在应用运行时安全进行。
3. 应用完全关闭后，`history.db-wal` 若已自动合并（checkpoint），仅备份 `history.db` 即可——
   但请以"三文件一起复制"为默认做法，不必判断合并状态。

## 常见问题

- **启动报 WebView2 缺失**：安装 [WebView2 Evergreen Runtime]
  (https://developer.microsoft.com/microsoft-edge/webview2/) 后重试。
- **诊断全部失败**：检查网络与代理客户端状态；工具不修改任何系统网络设置。
- **SmartScreen 提示**：安装包未签名。只从官方发布页下载并核对 SHA-256；源码当前为私有仓库。
