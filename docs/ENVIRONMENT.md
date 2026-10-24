# 环境核查表（Phase 0）

核查日期：2026-10-24｜机器：Windows 11 x64｜核查方式：全部只读命令 + 一次临时目录内的无依赖构建验证
状态定义：**已验证** = 命令真实执行且输出符合预期；**未验证** = 本阶段无法或不适合验证，需后续阶段补验。

## 1. 结论摘要

**Phase 1（Tauri 2 脚手架）的全部系统前置条件均已满足，无缺失依赖。**
唯一注意点：Windows SDK 安装在 `E:\Windows Kits\10`（非 C 盘默认路径），链接测试已实证可用。

## 2. 逐项核查

| # | 组件 | 要求 | 实测结果 | 执行的命令 | 状态 |
|---|---|---|---|---|---|
| 1 | Windows | 10/11 | Windows 11 23H2（10.0.22631.5189） | `cmd //c ver` | 已验证 |
| 2 | Git | 可用 | 2.54.0.windows.1 | `git --version` | 已验证 |
| 3 | Git 提交身份 | 必需 | 原为空；已按用户确认配置仓库级 `user.name=159357xxj`、`user.email=159357xxj@users.noreply.github.com`（仅写入本仓库 .git/config，未改全局） | `git config --get user.name` / `git config --get user.email` | 已验证 |
| 4 | Node.js | LTS | v24.19.0（x64） | `node --version`；`node -p "process.arch"` | 已验证 |
| 5 | npm | 随 Node | 11.17.0 | `npm --version` | 已验证 |
| 6 | Rust 工具链 | stable-msvc | rustc 1.97.1 / cargo 1.97.1，active: `stable-x86_64-pc-windows-msvc` | `rustc --version`；`cargo --version`；`rustup show` | 已验证 |
| 7 | MSVC C++ Build Tools | 必需 | VS 2022 BuildTools，含 x86/x64 VC 工具集 | `vswhere.exe -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath` → `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools` | 已验证 |
| 8 | Windows SDK | Rust 链接必需 | 安装于 `E:\Windows Kits\10\`（注册表 `HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots → KitsRoot10`），含 10.0.22000 / 10.0.22621 / 10.0.26100 三版 Include/Lib。**注意：不在 C 盘默认路径，初查 C 盘目录时易误判为缺失** | `reg query 'HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots' /v KitsRoot10`；`ls E:/Windows Kits/10/{Include,Lib}` | 已验证 |
| 9 | Rust 真实构建/链接 | 必需 | 临时目录内 `cargo new` + `cargo build`（无依赖 hello-world）成功，生成 `linktest.exe`，退出码 0，耗时 28.6s；测试后已清理 | `cargo new linktest --bin --vcs none`；`cargo build`（位于 `%TEMP%\msvc-link-test`） | 已验证 |
| 10 | WebView2 Runtime | 必需 | 155.0.4283.62，由 `C:\Program Files (x86)\Microsoft\EdgeWebView\Application\` 文件系统确认。标准注册表键（HKLM/HKCU `EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`）未命中，属检测路径异常而非缺失，已记录 | `reg query`（标准键）；`ls "C:/Program Files (x86)/Microsoft/EdgeWebView/Application"` | 已验证（附注） |
| 11 | Tauri CLI | 项目本地即可 | 未全局安装（`tauri --version` → command not found）；Phase 1 通过 npm 项目依赖 `@tauri-apps/cli` 提供，符合预期 | `tauri --version` | 已验证（不缺失） |

## 3. 本阶段未验证项（按计划属于后续阶段）

| 项目 | 原因 | 计划验证阶段 |
|---|---|---|
| `tauri dev` / `tauri build` 真实运行 | Phase 0 无工程代码 | Phase 1 / Phase 6 |
| `npm run lint / typecheck / test` | Phase 0 无前端代码，**记为"不适用"而非"通过"** | Phase 1 起 |
| `cargo fmt --check / clippy / test` | Phase 0 无 Rust 工程代码，同上 | Phase 2 起 |
| 公网预设目标（chatgpt.com 等）真实探测 | 仅人工检查项，自动化测试不依赖公网 | Phase 2（人工） |
| NSIS 安装包 | 属发布范畴 | Phase 6 |

## 4. 对后续阶段的约束提醒

1. 本环境 Node 24 / Rust 1.97 较新：依赖版本一律以锁文件固定，升级需显式提交。
2. WebView2 检测路径异常（注册表标准键缺失）说明：本机 EdgeUpdate 注册表布局与常见文档不同，Phase 6 打包 NSIS 时需复核 bootstrap 逻辑，不能以"注册表键存在"作为唯一判据。
3. Windows SDK 位于 E 盘：若未来 CI 或其他机器构建失败，优先核对 `KitsRoot10` 注册表值与安装盘位。
