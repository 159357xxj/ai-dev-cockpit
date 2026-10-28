# AI Dev Cockpit Phase 6 最终发布验收报告

验收日期：2026-10-25（Asia/Shanghai）  
验收范围：本地只读审计、前后端自动化质量基线、候选安装包元信息、隔离临时数据库与假凭证导出探针。  
未执行：安装、升级、卸载、真实桌面启动、真实公网诊断、GitHub 推送或 Release。

## 发布判定：FAIL

当前版本**不具备发布条件**。Rust/前端自动化基线通过，但确认存在两个阻断问题：桌面 IPC 入口拒绝 `library_default` 和 `manual_local_proxy`；JSON 导出未遮蔽 Cookie、API Key 头、私有 IP 和敏感主机名。另有 WAL 备份说明不足及发布文档陈旧等问题。安装、真实 Release UI、跨版本升级、卸载和 GitHub CI 均未在本轮执行，不能作为通过项。

## 1. 测试环境与操作系统

| 项目 | 实际观察 |
|---|---|
| OS | Windows 11 家庭中文版，23H2，10.0.22631，x64 |
| Node.js / npm | v24.19.0 / 11.17.0 |
| Rust / Cargo | rustc 1.97.1 / cargo 1.97.1，stable MSVC 工具链 |
| OpenSSL | 3.5.9；全量 Rust 测试复跑时仅对当前测试进程设置 `OPENSSL_CONF=E:\Miniconda3\Library\ssl\openssl.cnf` |
| 环境隔离 | 不是干净 Windows 虚拟机。当前用户应用数据目录已有 SQLite 文件；未启动应用、未打开数据库内容、未复制或改写该目录 |

该主机的 `%APPDATA%\com.aidevcockpit.app` 下观察到 `history.db`（4,096 字节）、`history.db-shm`（32,768 字节）和 `history.db-wal`（177,192 字节）；当前没有发现运行中的 `ai-dev-cockpit` 进程。因为程序启动时会自动执行保留策略（见 `src-tauri/src/lib.rs`），启动应用可能迁移或清理既有记录。本轮因此没有对当前用户配置文件进行安装或启动测试。

## 2. 项目版本与 Git 状态

| 项目 | 结果 |
|---|---|
| 真实仓库根目录 | `E:\ai-dev-cockpit`（由 `git rev-parse --show-toplevel` 确认） |
| 分支 / HEAD | `main` / `a22a92913126b3d4ee58945b4a95d2cff6282265` |
| 提交 | `Phase 6: 质量审计、原生另存为、体积优化、CI 与 NSIS 安装包`，提交时间 2026-10-25 18:23:40 +08:00 |
| 远端差异 | 本地 `main` 比本地记录的 `origin/main` **领先 2 个提交**；没有推送或创建 Release |
| 工作区 | 验收开始前干净；临时假凭证/SQLite 探针已移除，未保留产品源码改动 |
| 锁文件 | `package-lock.json`（187,438 字节）和 `src-tauri/Cargo.lock`（134,825 字节）均存在 |
| 版本 | `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 均为 `0.1.0` |
| 许可证 | `LICENSE` 存在，MIT License |

## 3. 候选安装包完整性

| 项目 | 实际结果 |
|---|---|
| NSIS 安装包 | `E:\ai-dev-cockpit\src-tauri\target\release\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe` |
| 大小 | 2,779,239 字节（2.65 MiB） |
| SHA-256（`Get-FileHash -Algorithm SHA256`） | `2AF65668A7481ACA6DF6E3D5C198A60364AB0AEA3FDA3457BBB74A5966E14EBA` |
| 与既有报告比对 | 与既有哈希一致 |
| 安装器文件/产品版本 | 0.1.0 / 0.1.0 |
| 安装器 PE machine | `0x014C`（32 位 NSIS 启动器）；候选应用本体 `ai-dev-cockpit.exe` 为 `0x8664`（x64） |
| 安装器及应用签名 | 均为 `NotSigned`；`docs/INSTALL.md` 有 SmartScreen 说明 |

安装器和应用本体文件时间分别为 18:18:18、18:18:19，早于 `a22a929` 提交时间约 5 分钟。关键运行时代码文件时间早于该构建，但没有本轮从 `a22a929` 重建 NSIS 包或可验证的构建证明；因此严格的 commit-to-artifact 来源对应关系为 **NOT TESTED**。本轮没有覆盖候选安装包。

## 4. 自动化代码质量测试

| 命令 | 退出码 | 实际结果 |
|---|---:|---|
| `npm run lint` | 0 | ESLint 完成，无输出错误 |
| `npm run typecheck` | 0 | `tsc --noEmit` 完成 |
| `npm run test` | 0 | 9 个测试文件通过，35 tests passed |
| `npm run build` | 0 | Vite 8.3.5；2,630 modules transformed；主 JS 431.20 kB，图表 chunk 351.39 kB；完成，无构建警告 |
| `cargo fmt --check`（`src-tauri`） | 0 | 无格式差异 |
| `cargo clippy --all-targets -- -D warnings`（`src-tauri`） | 0 | 完成，无 warning |
| 首次 `cargo test`（`src-tauri`） | 1 | 3 个 TLS mock 用例因 OpenSSL 默认配置路径 `C:\Program Files\Common Files\ssl\openssl.cnf` 不存在而失败；不是证书校验逻辑失败 |
| `OPENSSL_CONF=... cargo test`（仅进程环境变量，`src-tauri`） | 0 | 全部 110 项通过、0 失败；3 个公网用例 ignored；Doc-tests 0 项。分组：39 unit + 19 probes E2E + 13 proxy modes + 31 rules engine + 8 storage |
| `npm ls --depth=0` | 0 | 顶层依赖均已解析，无 invalid/extraneous 项 |
| `cargo tree --locked --depth 1`（`src-tauri`） | 0 | 锁定依赖树解析成功 |
| `git diff --check` | 0 | 无空白错误 |

依赖锁定版本示例：Reqwest 0.12.28、Tauri 2.12.5、rusqlite 0.41.0、Vite 8.3.5、Vitest 5.0.5。未执行外部漏洞数据库审计。成功运行的构建、Clippy 和 Cargo 测试输出中没有观察到编译警告。

**命令契约发现：FAIL。** `src-tauri/src/commands/diagnostics.rs:51-55` 仍保留 Phase 2 限制：只允许 `no_app_proxy`，其他模式返回 `not_available`，提示“将在 Phase 3 提供”。因此 `library_default` 和 `manual_local_proxy` 即使通过前端表单和底层网络实现，也会在应用 IPC 层被拒绝；`validate_manual_proxy_url` 的后续手动代理分支也无法到达。已有 unit test `impl_rejects_phase3_modes_with_not_available` 正在确认这项限制。前后端字段命名测试存在，但未进行真实 Tauri WebView IPC 往返。

## 5. Windows 安装与运行

**NOT TESTED — 等待授权。** 本轮没有执行安装器、安装向导、卸载器或应用启动，因此以下均未真实验证：

- 安装器启动、语言选择、默认安装路径、是否请求管理员权限；
- 开始菜单快捷方式、安装后启动、主窗口渲染、白屏情况、主题切换；
- 关闭窗口后的残留进程、安装版 SQLite 目录访问；
- 干净 Windows 10/11、断网首启和 WebView2 引导安装行为。

当前用户已有数据库/WAL/SHM 文件，且启动钩子会应用保留策略。未获得安装/启动授权，也未创建可靠数据库备份，所以没有在当前配置文件上继续测试。环境不是干净 Windows 环境。

## 6. 核心功能验收

| 项目 | 结论 | 证据与边界 |
|---|---|---|
| DNS、TCP 443、HTTPS 独立探测；HTTP 非 2xx 中性分类；重定向/超时/取消/采样 | PASS（离线 Rust 集成测试） | `probes_e2e` 19 项通过，覆盖 mock HTTP、TLS、拒连、超时、取消、并发、401/403/407/429/5xx 等；未在 Release 桌面 UI 执行 |
| 3/5 次采样与结果聚合 | PASS（自动化层） | Rust 类型/聚合与 E2E 测试通过；Release 表单与运行体验 NOT TESTED |
| WinINet / WinHTTP / 环境变量只读快照 | PASS（本机 Rust 测试） | `user_proxy_config_handles_real_machine` 和快照来源测试通过；真实 UI 展示 NOT TESTED |
| `no_app_proxy` | PASS（底层测试）；Release UI NOT TESTED | 测试确认不使用应用层 proxy 配置；不代表绕过 TUN/透明代理 |
| `library_default`、`manual_local_proxy` 应用路径 | **FAIL** | IPC 层在启动任务前拒绝这两种模式，见 `diagnostics.rs:51-55` |
| 手动代理不可用时 `proxy_unavailable` 且无静默回落 | PASS（底层测试） | `dead_manual_proxy_never_falls_back_to_direct` 通过；正式 UI 路径因 IPC 限制不可验收 |
| R10/R11、HTTP 非 2xx 不等于传输失败、事实/解释/局限 | PASS（规则自动化） | `rules_engine` 31 项通过，含 R10/R11 回归用例；用户界面诊断卡 NOT TESTED |
| TUN 免责声明 | NOT TESTED（Release UI） | 仅检查文档/源码，无桌面运行证据 |
| 本轮实际公网诊断 | NOT TESTED | `probes_public` 的 3 项测试明确 ignored；本轮没有扩大公网测试范围 |
| 应用启动无自动公网检测、无未知后台请求 | NOT TESTED（运行态） | 静态代码路径未见启动时调用 `start_diagnostic`；启动会读取本地系统快照/SQLite。没有对 Release 进程做网络流量或后台进程观察 |

## 7. SQLite 持久化与升级

| 项目 | 结论 | 依据 |
|---|---|---|
| 迁移、读写往返、删除/清空、保留策略 | PASS（自动化） | `storage` 8 项通过：迁移版本/幂等、读写、删除清空、按数量与设置的保留策略；使用内存数据库 |
| 磁盘数据库关闭重开及重复写入幂等 | PASS（本轮隔离临时探针） | 在系统临时目录建立 DB，写入同一 run 两次、关闭并重开后仍为 1 条 run/5 个样本；临时目录随测试清理 |
| 当前用户历史实际自动保存及 UI 重启后显示 | NOT TESTED | 未在当前用户 DB 写入或启动应用 |
| 真实旧版本到 0.1.0 跨版本升级 | NOT TESTED | 没有安装旧版本，也没有用同版本覆盖冒充升级；本轮没有执行升级 |
| WAL 备份说明 | **FAIL（文档风险）** | `docs/INSTALL.md:34` 仅建议备份 `history.db`，未说明停止写入/SQLite Backup API/配套 WAL。当前配置文件中确有非空 `history.db-wal` 和 `history.db-shm`；单独复制主库不构成可靠备份 |

## 8. 脱敏与导出安全

本轮用临时集成探针把**假凭证**分别放入错误摘要，再检查 JSON 和 Markdown 报告。JSON 会序列化完整运行对象（`src-tauri/src/commands/report.rs:68-74`），其摘要/详情经过 `sanitize_text`（同文件 45-52 行）。探针结果：

| 假数据类别 | JSON | Markdown |
|---|---|---|
| Authorization 头、userinfo 代理密码、`?token=`、Windows `C:\Users\...` 用户目录 | PASS：值被遮蔽 | PASS：测试文本未泄漏 |
| Cookie 头、`X-API-Key` 头、私有 IP、敏感主机名 | **FAIL：原样出现在 JSON 导出** | PASS：当前 Markdown 不包含错误摘要字段 |

临时探针结果为 1 项导出安全测试失败，类别为 `Cookie/Json`、`API key header/Json`、`private address/Json`、`sensitive host/Json`。探针文件已移除，没有保留源码改动。`sanitize_text` 目前只显式处理若干 query key、本地目录和 Authorization 文本，没有 Cookie/API Key 头或主机/IP 遮蔽规则（`src-tauri/src/security/redaction.rs:57-130`）。这与本次要求的 JSON 脱敏验收不符，属于发布阻断问题。

以下原生文件操作只做了源码审阅，均为 **NOT TESTED**：另存为对话框、用户取消、目录权限不足、同名文件处理、预览与实际保存字节一致、导出文件在 Windows 上打开。没有写入任何报告到个人目录。

## 9. 隐私与安全审计

| 审计项 | 结论 | 证据/限制 |
|---|---|---|
| Tauri capability | PASS（静态） | `src-tauri/capabilities/default.json` 仅 `core:default`；未配置 shell/fs/http 扩展权限 |
| CSP | PASS（静态） | `script-src 'self'`；无远程脚本源；`connect-src` 为本地 IPC 来源；`style-src 'unsafe-inline'` 存在 |
| Rust IPC 输入校验 | PASS（自动化/静态） | 目标白名单、次数/超时与 loopback 代理验证有测试；但诊断命令的过期路由门控导致功能 FAIL |
| HTTPS 目标与重定向 | PASS（自动化/静态） | 生产目标来自白名单；E2E 验证越界重定向不跟随 |
| TLS 证书校验 | PASS（自动化） | `tls_validation_still_rejects_without_trusted_root` 通过；未发现关闭证书验证配置 |
| 代理隐式回落 | PASS（底层自动化） | 手动代理死端口无直连回落测试通过；应用层路由模式入口仍 FAIL |
| 敏感信息硬编码扫描 | PASS（有限静态扫描） | 高置信度 token/私钥/带 userinfo URL 模式未在生产代码部分发现；不等同于外部 Secret 扫描器审计 |
| Release Demo 模式 | NOT TESTED（运行态） | 源码使用 `import.meta.env.DEV` 作为 `DEMO_AVAILABLE` 并在正式模式拒绝开启；但当前生产 JS bundle 仍包含 `demo-run-1` 至 `demo-run-5` 标识，未用正式桌面运行确认不可进入历史 |
| 启动网络与未知后台流量 | NOT TESTED（运行态） | 静态入口未见自动诊断；未抓取 Release 进程网络或系统进程树 |

## 10. 卸载及数据保留

**NOT TESTED — 等待授权。** 未执行卸载、程序文件清理、开始菜单快捷方式清理、卸载后的后台进程检查或重装后读取保留数据。未删除或修改任何用户历史数据。

## 11. GitHub CI

实际状态：**NOT TESTED — 等待推送后实际运行**。本地分支领先 `origin/main` 两个提交，本轮未推送、未创建 PR/Release，也没有 CI run。

静态 YAML 审阅：

- `windows-latest` runner 配置存在；Node 22 与 stable Rust 工具链配置存在；
- 前端包含 `npm ci`、lint、typecheck、test、build；Rust 包含 fmt、Clippy `-D warnings`、test；
- NSIS 构建只在 `workflow_dispatch` 时执行，并上传 artifact；未配置自动 GitHub Release；
- 未发现 workflow 中硬编码 Secret；
- **FAIL（权限配置）**：workflow 未声明最小 `permissions`（例如 `contents: read`），当前实际 token 权限取决于仓库级默认设置，不能据此确认满足最小权限要求。

## 12. 问题清单与尚未完成的人工验证

| ID | 严重程度 | 问题 | 发布影响 |
|---|---|---|---|
| F-01 | 高 / P1 | IPC 入口仍禁用 `library_default` 与 `manual_local_proxy`，并把它们标记为 Phase 3 未实现 | 代理快照以外的两种核心 HTTPS 路径无法从正式 UI 启动；阻断发布 |
| F-02 | 高 / P1 | JSON 导出假凭证测试发现 Cookie、API Key 头、私有 IP、敏感主机名会泄漏 | 不满足导出隐私验收；阻断发布 |
| F-03 | 高 / P1 | 安装文档的 WAL 备份说明只提 `history.db` | 用户可能漏掉 WAL 中尚未 checkpoint 的数据；升级前必须修正文档并采用可靠备份方法 |
| F-04 | 中 / P2 | README 仍把 P3/P4/P5 列为未实现、测试数仍写 22；CHANGELOG 声称“诊断均为本地操作”且“运行过程零网络请求”；ARCHITECTURE 仍把已修复的 R11 标作待复核 | 发布文档与代码/既有测试记录不一致；需更新后再发 |
| F-05 | 中 / P2 | CI workflow 未声明 token 最小权限 | 不能从 YAML 证明权限最小化；需明确权限并在推送后实际跑 CI |
| F-06 | 待确认 | 候选安装器比 HEAD 提交时间早约 5 分钟，没有本轮 commit-to-artifact 构建证明 | 需从最终提交重建并记录最终 SHA-256，或提供可信构建来源证据 |

尚未完成且必须保持 **NOT TESTED** 的人工项目：Release 安装器运行及权限/路径/快捷方式；正式 UI 首屏、主题、真实诊断/取消/恢复；三种模式真实 UI（其中两种目前已知被 IPC 拒绝）；TUN 免责声明展示；原生另存为取消/权限/覆盖/打开文件；当前用户真实历史重启；真实旧版本升级；卸载和数据保留；断网安装启动；应用进程网络观察；推送后 GitHub CI。

## 验收结论依据

- 前后端自动化质量基线本轮实际执行并通过；Rust 首次测试受 OpenSSL 配置路径影响失败，设置进程级配置后完整复跑通过。
- 离线低层代理和规则测试通过，但真实应用 IPC 代码明确拒绝两种请求模式，不能用低层通过替代桌面验收。
- 临时假数据经 JSON 报告生成器验证，确认四类敏感信息泄漏。
- 安装、升级、卸载和当前用户数据相关测试没有执行；不能宣称这些门槛通过。
- 由于 F-01 与 F-02 是已确认的功能/隐私阻断问题，最终状态判为 **FAIL**，而不是因自动化测试通过而判为可发布。
