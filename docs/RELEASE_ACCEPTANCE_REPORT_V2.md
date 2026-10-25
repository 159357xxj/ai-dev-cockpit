# AI Dev Cockpit — Release Acceptance Report V2

**复验日期：** 2026-10-25（Asia/Shanghai）  
**验收类型：** 独立 Release Candidate 复验  
**最终判定：FAIL**

## 1. 判定摘要

当前版本不具备发布条件。独立构造的假凭证数据在 JSON 和 Markdown 报告中仍有明文泄漏；JSON 预览与随后导出的内容因重新生成 `exported_at` 而不一致。README 与架构 IPC 文档也仍有已实现功能/当前接口方面的矛盾。这些结果与 F-02、F-04 的“已修复”声明不符。

本机自动化质量基线在处理当前 OpenSSL 环境配置后通过；但没有可用的隔离 Windows Sandbox/VM，故未安装、启动或卸载应用，未进行桌面 UI、真实安装版数据库、跨版本升级和真实公网验收。GitHub CI 尚未运行。不得将这些 NOT TESTED 项视为通过。

本轮没有修改产品源码，没有 push、创建 Release、安装/卸载软件、修改系统网络配置，也没有打开或修改当前用户真实数据库。临时独立 QA 测试文件和临时数据库均已删除。

## 2. 环境与候选版本

| 项目 | 实测值 |
|---|---|
| 项目根目录 | `E:\ai-dev-cockpit`（由 Git 仓库定位） |
| 操作系统 | Windows 11 家庭中文版，10.0.22631，Build 22631，64 位 |
| Node / npm | Node v24.19.0 / npm 11.17.0 |
| Rust / Cargo | rustc 1.97.1 / cargo 1.97.1，MSVC x64 |
| OpenSSL | 3.5.9；`openssl.exe` 来自 Miniconda |
| 分支 / HEAD | `main`，`a7821faf4f7b09da0854b5dc16c36e338612d23a` |
| 提交状态 | 相对 `origin/main` ahead 4；Phase 6 未推送 |
| 依赖锁文件 | 根目录 `package-lock.json` 与 `src-tauri/Cargo.lock` 均存在；`npm ls --depth=0` 退出码 0 |
| 依赖检查边界 | Cargo 锁定图可解析；存在常见传递依赖的多版本共存。本轮未运行漏洞数据库审计（如 `npm audit`/`cargo audit`） |

## 3. 安装包完整性与来源

独立重新执行 `npm run tauri build`，退出码 0，Tauri 构建日志显示目标为 x64，生成 1 个 NSIS 包。候选包信息如下：

| 项目 | 实测值 |
|---|---|
| 安装包绝对路径 | `E:\ai-dev-cockpit\src-tauri\target\release\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe` |
| 安装包大小 | 2,784,031 字节（约 2.66 MiB） |
| `Get-FileHash -Algorithm SHA256` | `A0AA79A5CD6656936593F56B26CA7F6B9FD2B67275727688A46DB4A34780B62C` |
| 安装包版本信息 | FileVersion/ProductVersion 均为 `0.1.0` |
| 安装包签名 | `NotSigned` |
| PE 架构 | NSIS 安装器 PE machine `0x014C`（32 位 bootstrap）；包内主程序 PE machine `0x8664`（x64），与 Tauri 日志的 x64 目标相符 |
| 主程序 | `E:\ai-dev-cockpit\src-tauri\target\release\ai-dev-cockpit.exe`，7,418,368 字节，版本 0.1.0，SHA-256 `892180C7F467EC954A8AE4CEBAE2A3188BB47F8D369B5046656676A8FEDEB8D9`，未签名 |

用户提供的 `5624A3BE...506FAD` 在本轮重新构建前已与当时文件相符；重新构建覆盖候选包后，当前哈希为上表的新值，旧值不再代表当前文件。`git diff e400d21..a7821fa` 仅显示新增 `docs/RELEASE_REMEDIATION_REPORT.md`，没有产品源码变更，因此 e400d21 到 a7821fa 之间没有必须重建的代码差异。本轮仍按当前 HEAD 重建，以建立新的本地构建记录。该对应关系有本地构建日志和提交差异支持，但没有签名构建证明或可复现构建证明。

构建成功时仍输出两项提示：bundle identifier 以 `.app` 结尾（Tauri 对 macOS 命名的提示），以及链接器输出被 Rust 记录为 warning；均未导致构建失败。安装包和主程序均未签名，安装后的 SmartScreen 表现未实测；`docs/INSTALL.md` 已明确提醒未签名构建可能触发 SmartScreen。

## 4. 自动化质量基线

以下为本轮真实执行结果。Rust 全量测试第一次未配置 OpenSSL 路径时有 3 项 TLS mock 失败；仅对重跑测试进程设置 `OPENSSL_CONF=C:\Program Files\Git\usr\ssl\openssl.cnf` 后全量通过，没有修改系统环境或 TLS 安全策略。

| 命令 | 退出码 | 实际结果 |
|---|---:|---|
| `npm run lint` | 0 | ESLint 通过 |
| `npm run typecheck` | 0 | TypeScript 检查通过 |
| `npm run test`（隔离重跑） | 0 | 9 个文件通过，35 passed、0 failed |
| `npm run build` | 0 | `tsc` + Vite 生产构建通过，2,630 modules transformed |
| `cargo fmt --check`（`src-tauri`） | 0 | 通过 |
| `cargo clippy --all-targets -- -D warnings`（`src-tauri`） | 0 | 通过，无 Clippy warning |
| `cargo test --quiet`，未设置 `OPENSSL_CONF` | 101 | 3 failed：`no_app_proxy_tls_direct_success_with_trusted_test_cert`、`manual_https_proxy_full_tls_success_with_trusted_test_cert`、`tls_validation_still_rejects_without_trusted_root`；均因 OpenSSL 找不到 `C:\Program Files\Common Files\ssl\openssl.cnf`，不是测试断言失败 |
| `cargo test --quiet`，仅本进程设置上述 `OPENSSL_CONF` | 0 | **119 passed、0 failed、3 ignored**。分组：41 单元、7 导出回归、19 本地 mock E2E、13 代理模式、31 规则、8 存储；3 个公网用例 ignored |
| `npm run tauri build` | 0 | NSIS x64 目标构建成功；见 §3 |
| `npm ls --depth=0` / `cargo tree -d` | 0 / 0 | npm 顶层依赖解析成功；Cargo 显示多个传递依赖版本并存，未发现解析错误 |

前端测试曾在与其它任务并行时有 2 项超时；独立运行及本轮再次单独运行均为 35/35 通过。该超时结果保留记录，不将其隐去。

### 本轮独立 QA 探针（非产品测试，临时文件已删除）

使用独立构造的纯假凭证运行 `build_report`，覆盖 JSON、Markdown、R04 观察事实、错误摘要/详情、大小写、重复项、换行、Cookie、API Key、Authorization、敏感查询参数、userinfo、私有地址、内部主机、Windows 用户目录和异常输入。探针结果：

- **FAIL：** JSON 与 Markdown 都出现假敏感值：折叠 Cookie 行 `QA_FOLDED_COOKIE_11C`、URL 凭证 `QA_URL_PASS_12D`、第二个 Windows 用户名 `qa-two`、多 `@` URL 凭证 `QA_MULTI_PASS_15G`。R04 的 `observed_facts` 也把受污染的错误摘要带入两种报告。
- **FAIL：** Unicode 前缀 `İ?token=QA_UNICODE_16H` 被处理为 `İ?token=Q[已隐藏]`，敏感值首字符 `Q` 仍泄漏；没有崩溃，但不能视为完整脱敏。
- **部分遮蔽：** `fd12:3456:789a::1` 输出为 `fd12:3456:789a[内网地址]`，`fe90::1` 输出为 `fe90[内网地址]`，地址前缀仍可见。
- **PASS：** 本探针中 `Authorization`、常规 Cookie、多次大小写变体、API Key、敏感查询参数、内部主机名和第一个 Windows 用户目录均被遮蔽；允许公开的 `github.com`、`api.openai.com` 和公网 `8.8.8.8` 保留。
- **FAIL：** 两次独立生成 JSON，间隔 15 ms 后比较完整字节，`PREVIEW_EXPORT_BYTES_EQUAL=false`。差异包括重新生成的 `meta.exported_at`（例如 `.811Z` 与 `.830Z`）。预览与导出 IPC 分别重新加载记录并各自调用 `build_report`，所以用户确认的预览不是随后保存的同一份 JSON 内容。
- **PASS：** 独立临时磁盘 SQLite 测试写入、重复写入、完全关闭、重新打开后仍有 1 条历史及 3 个样本；该测试库在系统临时目录创建并在测试结束清理。

以上 QA 探针有预期失败，故不计入产品自动化的 119 项通过数。正常仓库 `cargo test` 复跑是在删除临时探针文件后完成的。

## 5. F-01～F-06 独立复验

| 项目 | 复验状态 | 证据与结论 |
|---|---|---|
| F-01 命令入口路由门控 | **代码与自动化 PASS；桌面 UI NOT TESTED** | `start_diagnostic_impl` 先执行请求白名单/次数/超时/模式校验，再只对手动代理运行严格 loopback URL 校验，之后把合法请求转交 backend（`src-tauri/src/commands/diagnostics.rs:45-66`）。已无旧 Phase 2 模式拒绝门控。入口测试用 Stub 验证三模式均越过校验层、非法代理被拒；不是 `ProductionBackend` 的命令级端到端测试。生产 HTTPS 的本地 mock 代理测试 13/13 通过，其中包括手动死代理返回 `proxy_unavailable` 且不回落直连。桌面 UI/IPC 实际点击未运行。 |
| F-02 导出脱敏 | **FAIL，P1 阻断** | 独立假凭证报告测试在 JSON、Markdown 和 R04 事实中复现泄漏，见 §4。逐字段安全序列化和诊断字段调用 sanitizer 的结构虽存在，但 sanitizer 的覆盖缺口使安全边界仍可绕过。现有 7 项回归全过不代表独立输入已安全。 |
| F-03 SQLite WAL 备份说明 | **文档审查 PASS；真实升级备份 NOT TESTED** | `docs/INSTALL.md` 现在明确说明只复制主库不可靠，提供退出应用后三文件一起复制和 SQLite Backup API 两种可行方法（§“升级前备份”）。本轮没有复制或打开用户数据库，也没有执行应用升级。 |
| F-04 README/CHANGELOG/ARCHITECTURE 一致性 | **FAIL，P2** | README 当前仍把 P3/P4/P5 列作“未实现”，与同页已实现列表矛盾；仍写 R11“待复核”，但架构文档称修复关闭；顶部测试数 117、测试表 47/22 与当前 119/35 不符。`docs/ARCHITECTURE.md:123` 的 `preview_sanitized_report` 签名缺少实际 IPC 必需的 `format` 参数。CHANGELOG 的 Unreleased 与“启动不自动公网探测、用户主动诊断才发 HTTPS 请求”与代码行为相符。INSTALL 的 WebView2、未签名 SmartScreen、SHA-256、数据位置和 WAL 说明与可查配置/代码相符，但安装流程未实测。 |
| F-05 GitHub Actions 最小权限 | **YAML 静态审查 PASS；真实 CI NOT TESTED** | `.github/workflows/ci.yml:11-12` 显式 `permissions: contents: read`；Windows runner、Node 22、Rust stable、前后端质量命令和手动构建产物上传均可见；安装包上传仅在 `workflow_dispatch`，未发现自动 Release 步骤或硬编码 Secret。Phase 6 尚未推送，当前结论必须为 **NOT TESTED — 等待推送后实际运行**。 |
| F-06 安装包来源与需否重建 | **本地重建 PASS；外部构建证明 NOT TESTED** | `e400d21..a7821fa` 实际变更仅为 remediation report，无产品源码改动；本轮从当前 HEAD 本地执行 Release 构建成功并重新计算哈希，见 §3。当前包的 SHA 与之前 `5624...` 不同是本轮重建后的新产物，不要求匹配旧哈希。没有 GitHub artifact 或签名 provenance 可交叉验证。 |

## 6. 当前用户真实数据库风险调查

**没有打开、读取内容、哈希、复制、迁移或修改** `%APPDATA%\com.aidevcockpit.app` 下的数据库。当前只读取文件系统元数据：

| 文件 | 大小 | 创建时间 | 最后写入时间 |
|---|---:|---|---|
| `history.db` | 4,096 字节 | 2026-10-25 15:57:40 | 2026-10-25 15:57:40 |
| `history.db-wal` | 177,192 字节 | 2026-10-25 15:57:40 | 2026-10-25 18:00:32 |
| `history.db-shm` | 32,768 字节 | 2026-10-25 15:57:40 | 2026-10-25 19:56:23 |

现有记录相互矛盾：V1 验收报告称没有启动应用；修复报告前文又称未启动可能触发清理的应用实例，§5 却称 Release EXE 已启动并执行保留策略。修复报告记录的候选 app EXE mtime 为 19:55:59，当前 SHM mtime 为 19:56:23，与其所称启动时间相邻；这**支持“SQLite 连接可能被打开”的推测**，但不是独立进程/命令日志，也不能证明确实由该 EXE 打开。检索到的会话文本产物未提供启动前文件快照、启动后记录计数或可核对的数据库备份。

`history.db` 与 WAL 的当前元数据仍与 V1 报告所记一致，SHM 的最后写入时间晚于候选 EXE 时间。**没有启动前快照，无法证明启动前后数据库内容完全一致，也无法证明保留策略未删除历史。**目前没有证据证明发生了记录删除或数据库损坏；同样不能据此排除。V1 与修复报告关于是否启动的冲突应保留为未解决数据完整性证据缺口。源码确认应用启动时会打开 `app_data_dir()/history.db` 并立即执行保留策略（`src-tauri/src/lib.rs:36-48`），因此不可为了调查而再次启动当前用户配置中的生产应用。

## 7. Windows 安装、运行和核心功能

| 验收项 | 状态 | 结果/限制 |
|---|---|---|
| 隔离 Windows Sandbox/VM 可用性 | **NOT TESTED / 不可用** | 未找到 `WindowsSandbox.exe`、`vmconnect.exe`、VirtualBox、VMware、QEMU 或 Docker；系统报告 HypervisorPresent=True，但没有可用 VM 管理器/Windows Sandbox 可执行文件。WSL 存在，但不是运行 NSIS 桌面安装流程的隔离 Windows 环境。 |
| 安装程序启动、向导、权限提示、默认路径、开始菜单、启动主界面 | **NOT TESTED** | 未运行安装器；未获得当前用户环境安装授权，也没有隔离 Windows 环境可供先授权后执行。 |
| Release UI 白屏/主题切换/关闭残留进程 | **NOT TESTED** | 未启动 Release EXE，未做桌面 UI 验收。当前检查时没有发现运行中的 `ai-dev-cockpit` 进程。 |
| Release 自动联网/后台流量 | **源代码审查 PASS；运行时监测 NOT TESTED** | 启动装配代码仅初始化本地存储与命令；网络探测从用户调用的诊断命令路径进入。没有启动应用进行网络捕获，因此只确认源码路径，不宣称运行时零联网。 |
| Demo 正式版隔离 | **源代码/构建审查 PASS；UI NOT TESTED** | `DEMO_AVAILABLE = import.meta.env.DEV`，正式 Vite 构建将其关闭，持久化 demo 开关在非 DEV 构建不生效；本轮 `npm run build` 为生产构建。未在 Release UI 检查演示卡片或历史入库行为。 |
| DNS/TCP/HTTPS、3/5 次、超时/取消、HTTP 中性处理 | **自动化 PASS；真实 Release UI NOT TESTED** | 本地 mock E2E 19 项、代理模式 13 项、规则 31 项通过；公网用例 3 项 ignored。本轮未扩大公网目标范围或运行公网诊断。 |
| 三种路由、无效代理拒绝、手动失败不回落 | **自动化 PASS；UI NOT TESTED** | 入口测试和本地代理行为测试通过。命令入口测试使用 Stub；ProductionBackend 没有经过正式桌面 UI 触发。TUN/透明代理边界在代码/说明中保留，不可将 no_app_proxy 解释为绕过 TUN。 |
| 代理快照 WinINet/WinHTTP/环境变量、脱敏、无代理状态 | **代码/测试审查 PASS；安装版 UI NOT TESTED** | 源码为只读快照路径；未改变注册表或系统代理，也没有在安装版查看实际快照展示。 |
| JSON/Markdown 预览、原生另存为、取消、权限不足、覆盖、导出文件打开 | **部分 FAIL / 其余 NOT TESTED** | JSON/Markdown 纯函数独立复验发现内容泄漏及 JSON 预览/导出字节不一致；没有调用原生另存为对话框或写出实际报告文件。覆盖/取消/权限路径均 NOT TESTED。 |
| SQLite 应用安装版自动历史保存和重启 | **NOT TESTED** | 独立临时磁盘库 close/reopen 测试 PASS；仓库 8 项 storage 测试使用内存库。没有在安装版运行诊断/退出/重启。 |
| 迁移、重复写入、30 天/500 次、删除/清空 | **自动化 PASS；真实历史 NOT TESTED** | 8 项 storage 测试通过，覆盖内存库迁移幂等、读写、重复记录/过滤、删除清空及保留设置/策略。独立临时磁盘测试验证了关闭重开和重复写入。没有把这些结果等同于用户数据库或安装版验收。 |
| 两个不同版本之间升级与历史完整性 | **NOT TESTED** | 没有安装旧版本或执行跨版本升级；未用同版本覆盖冒充升级。没有可靠的用户库启动前备份，未触碰真实数据库。 |
| 正常卸载、程序/快捷方式清理、数据保留、重装读取 | **NOT TESTED** | 没有安装或卸载。文档约定用户历史数据库保留，但尚无本轮 Windows UI 实测。 |

## 8. 隐私、安全与发布文档审查

- **Tauri capability：PASS（静态）** — `src-tauri/capabilities/default.json` 仅授权主窗口 `core:default`。
- **CSP：PASS（静态）** — `script-src 'self'`，没有远程脚本源；`connect-src` 仅 self/IPC 本地源。`style-src` 含 `'unsafe-inline'`，是当前策略的例外，不等同于允许远程脚本。
- **Rust IPC 输入校验：PASS（静态+单测）** — 目标 ID 白名单、去重、采样数、超时和模式/代理参数在 Rust 端校验；手动代理限 loopback、有效端口及允许协议。
- **目标/重定向/TLS：自动化 PASS（本地 mock）** — 生产目标由白名单限定；重定向限制为允许主机、HTTPS、443 并限制次数；TLS 正常验证。本轮 configured Rust 测试中的受信测试 CA 路径通过，未受信根证书拒绝测试也通过。未对公网运行 Release EXE。
- **代理失败隐式回落：PASS（本地 mock）** — 手动代理不可用测试 13 项所在套件全通过，死代理路径断言 `proxy_unavailable` 而非直连。
- **硬编码凭证扫描：PASS（限静态模式扫描）** — 对 `src/` 与 `src-tauri/src/` 搜索常见 API token、私钥头和长 Bearer 字面量，没有匹配；测试夹具中的值为明确假凭证。该扫描不是完整秘密扫描器。
- **演示数据进入正式历史：源代码审查 PASS；运行时 NOT TESTED** — release 禁用 Demo 开关，但没有运行最终安装版作历史写入验证。
- **安装/隐私文档：部分 PASS** — `docs/INSTALL.md` 的应用数据目录与 Rust `app_data_dir()` + identifier `com.aidevcockpit.app` 相符；有 SHA-256 校验命令、WebView2 依赖和引导网络说明、SmartScreen 未签名提示、卸载保留数据说明及可靠 WAL 备份步骤。安装器提示、WebView2 引导器、卸载和数据保留均未实测。
- **外部漏洞审计：NOT TESTED** — 锁文件与依赖树解析正常，但本轮未执行联网漏洞数据库扫描；无结果不代表无已知漏洞。

## 9. GitHub CI 状态

`.github/workflows/ci.yml` 静态检查：Windows runner；Node 22；Rust stable 且含 rustfmt/clippy；前端 lint/typecheck/test/build；Rust fmt/clippy/test；显式 `contents: read`；NSIS 构建只允许手动 `workflow_dispatch`，上传 artifact，不含自动 Release 步骤。没有发现硬编码 Secrets。

**NOT TESTED — 等待推送后实际运行。** 当前 `main` ahead 4 且未推送；因此没有 GitHub Actions run，也没有 CI artifact。YAML 静态审查不能替代真实 CI 通过记录。

## 10. 缺陷与剩余阻断项

| 编号 | 严重度 | 独立发现 | 发布影响 |
|---|---|---|---|
| F-02（仍未关闭） | **P1 / 高** | 假凭证可经折叠 Cookie 行、userinfo 多 `@` 情况、第二个 Windows 用户路径、Unicode 查询键前缀及 IPv6 前缀进入 JSON/Markdown 报告或留下部分明文；诊断观察事实同样携带摘要。 | **阻断发布**：报告会把凭证/本机敏感信息泄漏给用户导出的文件。 |
| F-07（新） | **P1 / 高** | JSON 预览与独立导出调用分别重新生成 `exported_at`，完整内容/字节不一致；现有回归只比较 JSON 长度，没有验证字节相等。 | **阻断发布**：用户确认预览的内容不是保存的完整报告。 |
| F-04（部分仍在） | **P2 / 中** | README 状态表、R11 状态、测试计数过时；ARCHITECTURE 的预览 IPC 签名缺 `format`。 | 发布文档与当前实现/验收数据不一致，需修正后复验。 |
| D-01（证据缺口） | **高风险验证缺口** | ZCode 修复报告对是否启动应用有冲突；没有启动前数据库快照。SHM mtime 与声称启动时点接近，但不能证明文件内容/历史完整。 | 不可宣称历史未变；不得再启动当前真实用户实例调查。 |
| R-01（未完成） | **发布验收未完成** | 隔离 Windows 安装、UI、跨版本升级、卸载与重装、原生导出交互均未执行；CI 未触发。 | 本轮无法签署完整 Windows 发布验收。 |
| R-02（风险/未验证） | 中 | 安装包和应用主程序未签名，且未实测 SmartScreen。 | 文档已提示；分发前需评估签名与用户体验。 |

## 11. 最终发布判定

# **FAIL**

依据：F-02 的独立脱敏探针在 JSON 和 Markdown 均复现假敏感信息泄漏，属于阻断缺陷；F-07 确认预览与最终 JSON 导出内容不一致；F-04 文档修复不完整。即使自动化质量基线经过环境修正后通过，也不能覆盖上述失败。Windows 安装/运行/升级/卸载及 GitHub CI 尚未实测，当前数据库是否经历过启动保留策略也无法由现有快照证明。

**本报告完成后停止。没有推送、发布、安装/卸载、启动 Release EXE 或更改系统网络配置。**
