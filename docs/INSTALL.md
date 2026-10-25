# 安装说明（Windows 10/11）

## 系统要求

| 项目 | 要求 |
|---|---|
| 操作系统 | Windows 10 1809+ 或 Windows 11（x64） |
| WebView2 Runtime | 需要（Win10/11 通常已内置；安装包会按需引导安装） |
| 管理员权限 | **不需要**（按当前用户安装） |
| 网络连接 | 安装时可能需要（WebView2 引导器）；应用本身离线可用 |

## 安装步骤

1. 从 Release 页面或构建产物中获取 `AI Dev Cockpit_<版本>_x64-setup.exe`。
2. 核对文件 SHA-256 校验值（发布页会附 `SHA256SUMS.txt`；
   本地验证：`certutil -hashfile <文件> SHA256`）。
3. 双击运行安装程序，选择安装语言（简体中文/English），按向导完成安装。
4. 从开始菜单启动 **AI Dev Cockpit**。

> 未签名构建会触发 Windows SmartScreen 提示：点击"更多信息"→"仍要运行"。
> 如需消除提示，可自行对安装包与主程序做代码签名。

## 数据位置

| 数据 | 位置 |
|---|---|
| 历史数据库 | `%APPDATA%\com.aidevcockpit.app\history.db`（SQLite，含 WAL） |
| 导出报告 | 用户在"另存为"对话框中选择的任意位置 |
| 卸载影响 | 卸载程序仅移除应用文件；**历史数据库保留**在上述位置，可手动删除 |

## 升级

直接运行新版安装包覆盖安装；数据库结构使用向前兼容的版本化迁移，
历史数据自动保留。升级前建议备份 `history.db`。

## 常见问题

- **启动报 WebView2 缺失**：安装 [WebView2 Evergreen Runtime]
  (https://developer.microsoft.com/microsoft-edge/webview2/) 后重试。
- **诊断全部失败**：检查网络与代理客户端状态；工具不修改任何系统网络设置。
- **杀毒软件报警（未签名）**：本工具为开源构建，可自行从源码验证或编译。
