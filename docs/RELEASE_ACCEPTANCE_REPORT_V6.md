# Release Gate V6 独立复验报告

**契约：** `docs/GATE_A_CONTRACT_V6.md`（GA-V6-1.0）  
**复验日期：** 2026-10-26  
**复验分支 / HEAD：** `fix/v6` / `b6423340450db329ac992609d321a238916e6997`  
**范围：** Gate A 定向复验。未安装、启动、升级或卸载正式应用；未访问真实 AppData/history.db；未联网测试；未修改产品源码、网络配置或 GitHub 状态。

## 判定表

| ID | 结论 | 证据摘要 |
|---|---|---|
| F-1 / F-5 | PASS | `report.rs:582` 唯一提交原语为 `persist_noclobber`；保存实现无 `rename`、`fs::copy`、目标路径删除或可替换回退。 |
| F-2 | NOT TESTED（部分 PASS） | 文件、目录和 junction 的拒绝行为及内容/mtime保护通过；本轮没有创建符号链接的系统权限，符号链接分支未验证。 |
| F-3 | PASS | 提交前钩子竞态通过；独立进程 CREATE_NEW 竞态 500/500 轮无覆盖。 |
| F-4 | PASS | 8 线程屏障 200 轮、8 进程屏障 200 轮，每轮恰好 1 成功、7 个 `TargetExists`，最终字节为胜者载荷。 |
| F-6 / F-7 | PASS | 失败只清理本事务临时文件；探针目录无 `.adc-tmp-*` 遗留；代码使用目标同目录 `.adc-tmp-*` 临时文件。 |
| F-8 / F-9 | PASS | JSON/Markdown 预览与文件 SHA-256 一致；`TargetExists → Available → 换名重试 → Consumed` 成功。 |
| F-10 后端 | PASS（代码审查） | 导出命令将 `TargetExists` 映射为稳定 `target_exists` 与“文件已存在，请选择其他文件名”消息。 |
| F-10 UI | NOT TESTED | 未启动正式桌面应用；对话框与前端实际提示留待 Gate B。 |
| T-1 / T-2 / T-3 / T-4 / T-6 | FAIL | 内存和隔离 SQLite 两条生产报告链路均复现未知目标名、内部地址、假 API key、Markdown/控制字符泄漏。 |
| T-5 | FAIL | 报告样本 DTO 不含 `error.summary` / `error.detail` 字段，但输出仍含同时存在于错误文本与未净化 `target_id` 的假凭证子串；不能据此判定导出不含原始错误文本。 |
| U-1 / U-2 | FAIL | 11 个 run_id 用例中 9 个不符合契约：v1/v7/错误 variant 被原样接受；其他非法值输出 `invalid-run` 而不是 `invalid-run-id`。 |
| U-3 | PASS | `scheduler.rs:335` 以 `uuid::Uuid::new_v4()` 生成生产 run_id；未发现其他生产生成来源。 |
| E-1 | FAIL | `cargo fmt --all -- --check` 与 CI 的 `cargo fmt --check` 均退出 1，两个测试文件存在格式差异。 |
| E-2 | PASS | `cargo clippy --all-targets` 与 CI 严格命令 `cargo clippy --all-targets -- -D warnings` 均退出 0。 |
| E-3 | PASS | `cargo test --all`：157 passed、0 failed、3 ignored；ignored 均为需用户授权联网的公开测试。 |
| E-4 | PASS（有 P2） | lint、typecheck、build 通过；前端测试三次中两次 35/35 通过，一次 33/35 因两个测试超时失败，依契约记为 E-7 P2。 |
| E-5 | PASS | 本轮从 HEAD `b642334` 执行 `npm run tauri build` 成功；NSIS 2.67 MiB，SHA-256 已重新计算。 |
| E-6 静态 | FAIL | CI 在同一步写 `$GITHUB_PATH` 后立即用 `Get-Command` 检查当前进程 PATH；没有按契约拆分步骤或同步设置 `$env:PATH` 并检查绝对路径。 |
| Gate B | NOT TESTED | 没有可用的已确认隔离 Windows Sandbox/VM；未获安装授权，也没有启动正式应用。 |
| Gate C | NOT TESTED | 本地 `fix/v6` 尚无本轮对应的 GitHub Actions 实际运行证据；YAML 审查不等于 CI 通过。 |

## Gate A 总结

**Gate A：FAIL。** 独立探针在实际报告构建链路中复现契约 P1：未审核目标名可以将假凭证样式文本、内部主机名、私有地址和个人路径写入预览及导出文件。该问题满足契约第 0 节 P1(a)，不依赖理论推演。另有 U-1/U-2、E-1、E-6 等阻断不变量未通过，以及 F-2 符号链接尚未验证。

本结论只判断 Gate A；Gate B 与 Gate C 均为 `NOT TESTED`，不得据此发布或宣称完整 Windows Release Gate 通过。

## 环境、版本与工作区

- 仓库根目录：`E:\ai-dev-cockpit`
- 系统：Windows 11 家庭中文版，23H2，Build 22631，x64（WMI 报告）；`Get-ComputerInfo` 的产品名字段仍显示旧字符串“Windows 10 Home China”，注册表 `DisplayVersion=23H2`、Build 22631 与 WMI 交叉确认。
- 工具链：Node `v24.19.0`、npm `11.17.0`、Rust/Cargo `1.97.1`、Git `2.54.0.windows.1`。
- 产品版本：`0.1.0`。
- HEAD：`b6423340450db329ac992609d321a238916e6997`；提交标题 `V6-R1: 原子不覆盖提交（persist_noclobber）+ TargetExists 错误码 + test-hooks 竞态回归`。
- 当前分支：`fix/v6`。测试前工作区有一个既存未跟踪文件 `docs/RELEASE_ACCEPTANCE_REPORT_V5.md`，所以工作区并非干净；本轮未改产品源码。新增本报告后，它也会作为未跟踪文档出现。既存 V5 报告已保留。
- 用户给出的 `docs/RELEASE_REMEDIATION_REPORT_V6.md` 在工作区不存在；`docs/GATE_A_CONTRACT_V6.md` 存在并已按 GA-V6-1.0 复验。缺少开发方 V6 修复报告不作为测试证据。

## 安装包与来源

- 本轮实际构建命令：`npm run tauri build`，退出码 `0`。
- 安装包：`E:\ai-dev-cockpit\src-tauri\target\release\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe`
- 大小：2,797,573 bytes（2.67 MiB）
- SHA-256（本轮 `Get-FileHash -Algorithm SHA256`）：`107398EFD43A327C20A956FE1D12C4B90530C9D56AEF6A68D954420C99AD5A3F`
- 版本信息：安装器 FileVersion / ProductVersion 均为 `0.1.0`；内含应用 `ai-dev-cockpit.exe` 为 x64 AMD64、版本 `0.1.0`。NSIS 启动器自身 PE machine 为 I386；Tauri 构建输出目标为 x64，应用主 EXE 检测为 x64。
- Authenticode：安装器和应用 EXE 均为 `NotSigned`。未运行安装器，因此未观察 SmartScreen 或安装界面。
- 来源：命令从当前本地 HEAD `b642334` 的工作树执行；构建前没有已跟踪源码差异。构建后检查仍未发现产品源码差异。

## 自动化质量基线

| 命令 | 退出码 | 实际结果 |
|---|---:|---|
| `npm run lint` | 0 | ESLint 完成，无错误输出。 |
| `npm run typecheck` | 0 | `tsc --noEmit` 通过。 |
| `npm run test` 第 1 次 | 1 | 9 个测试文件中 7 passed、2 failed；共 33 passed / 2 failed / 0 ignored。`navigation.test.tsx` 的“可通过导航在五个页面间切换”和 `demo-mode.test.tsx` 的设置页切换均超过 5 秒超时；本轮首次执行时还在并行编译 Rust 探针。 |
| `npm run test` 第 2 次 | 0 | 9/9 文件、35/35 passed，0 failed、0 ignored，4.93 秒。 |
| `npm run test` 第 3 次 | 0 | 9/9 文件、35/35 passed，0 failed、0 ignored，4.09 秒。 |
| `npm run build` | 0 | `tsc && vite build` 成功，Vite 8.3.5，2,630 modules transformed。 |
| `cargo fmt --all -- --check` | 1 | `src-tauri/tests/f07_snapshot.rs` 第 53、252、275 行附近及 `src-tauri/tests/v5_file_and_limits.rs` 第 109、206 行附近未格式化。没有运行 fmt 自动改写。 |
| CI 同命令 `cargo fmt --check` | 1 | 同样的两份测试文件差异。 |
| `cargo clippy --all-targets` | 0 | 通过。 |
| CI 严格命令 `cargo clippy --all-targets -- -D warnings` | 0 | 通过。 |
| `cargo test --all` | 0 | 157 passed、0 failed、3 ignored。分组 passed 数：单元 37、export regression 16、F07 snapshot 10、probe E2E 19、proxy modes 13、rules 31、storage 8、V5 file/limits 14、V6 atomic-save 9；另 3 个公开网络测试 ignored。 |
| F07 定向稳定性复跑 | 0 | `f07_snapshot_survives_history_change` 连续 10 次，每次 1 passed、0 failed、9 filtered。 |
| `npm run tauri build` | 0 | Vite 与 Rust Release 构建成功，NSIS 安装包生成。 |

`cargo test --all` 中 ignored 的具体测试为 `public_compare_three_modes_on_github`、`public_target_github`、`public_target_zcode`；测试源码标明需要用户明确授权联网，默认跳过。未运行这些外网测试。Rust 测试/Release 链接阶段观察到 `linker stdout` 警告（生成 DLL import library / `.exp` 文件）；Tauri 另警告 bundle identifier 以 `.app` 结尾不推荐（与 macOS bundle 扩展冲突）。Clippy 未报告 lint 警告。

本机未设置全局 `OPENSSL_CONF`；Rust TLS mock helper 为其 OpenSSL 子进程写临时配置并显式设置子进程级 `OPENSSL_CONF`。测试进程通过 PATH 找到外部 `E:\Miniconda3\Library\bin\openssl.exe`，所以测试依赖外部 OpenSSL CLI，不能称为无外部 OpenSSL 依赖。`C:\Program Files\Git\usr\bin\openssl.exe` 文件存在。CI 的 OpenSSL 步骤静态缺陷见 E-6；没有实际 GitHub runner 证据。

## 文件保存与快照独立探针

独立 Rust 探针位于仓库外的临时 Cargo 项目，链接当前生产 crate；开启 `test-hooks` 仅用于提交前竞态。Release/default feature tree 的 `cargo tree -e features -p ai-dev-cockpit` 未包含 `test-hooks`，`npm run tauri build` 未传入该 feature。

- **F-1 / F-5 静态审查：** `report.rs:540-594`。没有 `std::fs::rename`、`fs::copy`、`remove_file`、`truncate` 或失败后的可替换回退；最终提交只调用 `temp.persist_noclobber(target)`。目标存在预检查只用于提前提示，安全依赖提交原语。
- **F-2 对象测试：** 已有文件原始字节、长度、mtime 不变并返回 `TargetExists`；已有目录与 junction 也返回 `TargetExists` 且仍存在。junction 指向目录的内容未变。独立符号链接创建收到 Windows `os error 1314`（缺少 `SeCreateSymbolicLinkPrivilege`），所以符号链接与悬空符号链接在本轮为 `NOT TESTED`。仓库 `v6_atomic_save` 中对应测试在权限不足时打印 `SKIPPED` 后正常返回，因此全量测试的“passed”计数不代表符号链接实际覆盖。
- **F-3 提交前注入：** test hook 在临时文件写完并 `sync_all` 后、`persist_noclobber` 前创建竞争目标；保存返回 `TargetExists`，竞争者 SHA-256 为 `9d4b516d…a13fddf5`，钩子观察到完整临时内容。
- **仓库 test-hooks 回归：** `cargo test --all --features test-hooks --test v6_atomic_save -- --nocapture` 退出码 0，10 passed / 0 failed / 0 ignored；其中 `save_race_target_created_before_commit` 与 8 并发通过。输出明确显示符号链接分支 `SKIPPED: no symlink privilege (os error 1314)`，该测试用例虽计为 passed，符号链接行为仍是 `NOT TESTED`。
- **F-3 独立进程循环：** 500 轮主保存与另一进程 `CREATE_NEW` 竞争，竞争者先创建 489 次、保存先创建 11 次；500 轮均只有一个胜者，竞争者文件载荷完整，无 `.adc-tmp-*` 遗留。
- **F-4 并发：** 8 线程屏障同步 200 轮、8 子进程屏障同步 200 轮；无异常轮次，均为一个成功与七个 `TargetExists`，落盘 SHA-256 对应赢家载荷。
- **F-6 / F-7：** 失败用例检查自有临时文件清理；相关目标目录检查无 `.adc-tmp-*` 残留。实现使用 `tempfile::Builder::prefix(".adc-tmp-").tempfile_in(parent)`，再写入和 `sync_all`。
- **F-8：** JSON 预览/文件 SHA-256 均为 `20b650cc…e73913`；Markdown 预览/文件均为 `9d4a7783…65815f9`。
- **F-9：** 已占用目标返回 `TargetExists` 且旧内容 `USER-ORIGINAL` 未改；快照经 `finish_failure` 回到 Available，改名后保存字节等于原预览，`finish_success` 后状态消费且不能重复使用。此项与现有 snapshot 资源预算/TTL 测试一并通过。
- **F-10 后端：** `report.rs:666-683` 将冲突映射到 `IpcErrorCode::TargetExists` 与明确的换名提示；前端实际显示与原生保存对话框行为未启动验证。

## T-1 至 T-6：独立目标名探针与 P1

探针新增 16 个 `target_id`：契约金丝雀及零宽字符、RTL 覆盖、全角同形文本、64 KiB 字符串、NUL、反引号代码块、HTML 标签。使用生产 `Storage::save_run → get_run → build_report` 代码链，在内存对象和独立 SQLite 数据库各生成 JSON/Markdown 预览并实际保存文件。四份输出的预览/落盘字节逐一相同；SQLite 数据库只在临时目录创建，没有触碰真实 history.db。

**失败证据：**

1. `report.rs:236` 将原始 `target.target_id` 插入 Markdown `###` 标题。独立 Markdown 文件实际包含 `canary-a1.corp.internal`、`10.13.37.42`、`192.168.77.7`、`fd12:3456:789a::42`、CRLF 后的 `# CANARY_CRLF`、Markdown 链接文本、`sk-CANARY0000000000`、`Cookie: CANARY=1` 与 `C:\Users\CANARY_USER\`。
2. `report.rs:204` 在原始 run 上调用 `diagnose`；JSON 路径 `report.rs:186` 虽然调用 `sanitize_and_cap`，但仍将 `diagnosis.title`（由动态 target id 构造）序列化。独立 JSON 中实际发现 `# CANARY_H1 ...`、假 key、HTML/反引号语法、CRLF/NUL 目标文本；JSON 语法本身可解析，但解析后的字符串仍有控制字符。
3. 同样的目标值经隔离 SQLite 保存、读回后再导出时，泄漏复现。故这是生产报告 builder 与历史读取链路可复现的问题，而非只存在于测试桩的推演。
4. DTO 的 `run.targets[].target_id` 使用 `safe_target_id`，但未知值映射成 `[未知目标]`，不是契约要求的 `unknown-target`；Markdown 的目标名仍为原文，JSON 与 Markdown 不一致。`Diagnosis.title` 为动态 `String`，没有按规则 ID、静态模板和安全标签重新构造；不存在契约要求的 `SafeTargetLabel` 单一类型。
5. 样本 JSON 属性包含 `error_kind`，不包含 `error.summary` / `error.detail` 字段；但探针将同一假凭证同时放入错误文本与 `target_id`，报告经不安全目标标题输出该子串。因此字段本身虽未序列化，T-5 的输出级断言仍失败；泄漏来源可定位为 `target_id` / 派生标题，而不是嵌套错误字段直接序列化。
6. 合法统计未被移除：独立 SQLite / 内存输出均保留 16 个样本目标，各自 `attempts_total=1`；既有 `regression_f02_keeps_public_ips_and_whitelist_domains` 通过。

**生产复现步骤（仅隔离数据）：** 在临时 `DiagnosticRun` 中将 `target_id` 设为 `canary-a1.corp.internal` 或 `sk-CANARY0000000000`；对同一 run 调用 `Storage::save_run`、`get_run`，再按 `preview_sanitized_report` 的实际调用顺序进入 `build_report`，分别以 JSON/Markdown 生成预览并调用 `save_report_atomic` 写入临时目录。结果在内存与 SQLite 两条路径均包含金丝雀。生产 IPC 预览函数本身在 `report.rs:600-621` 从历史存储取 run 后调用同一个 `build_report`。

## U-1 至 U-3：run_id

独立输入 11 个值：合法 v4 大写/小写、v1、v7、错误 variant、花括号、`urn:uuid:`、35/37 字符、非十六进制和路径样式输入。合法 v4 大小写规范化通过；其余 9 项未达到契约：v1、v7、错误 variant 因 `safe_run_id` 只检查格式而被原样接受；其余非法输入被改写为 `invalid-run`，而不是 `invalid-run-id`。错误输入未导致报告构造失败，也没有在无效格式时回显原值。当前 `safe_run_id` 见 `report.rs:65-80`；唯一生产生成位置见 `scheduler.rs:335`。

## CI 静态审查与实际运行状态

`.github/workflows/ci.yml` 声明 `permissions: contents: read`，前端与 Rust job 使用 `windows-latest`；工作流中没有自动创建 Release 的步骤，NSIS 候选构建仅由 `workflow_dispatch` 触发并上传 artifact。

E-6 静态不符合契约：同一 PowerShell step 先执行 `Add-Content $env:GITHUB_PATH "C:\Program Files\Git\usr\bin"`，紧接着执行 `Get-Command openssl`。写入 `GITHUB_PATH` 供后续 step 使用，不会同步修改当前 step 的 `$env:PATH`；同时没有 `Test-Path` 检查该绝对路径。应将此处记录为 CI 配置风险，不能据此声称 GitHub Actions 已通过。本轮没有本地 HEAD `b642334` 的 GitHub Actions 运行结果，Gate C 为 `NOT TESTED`。

## 新问题、P2 与观察项

- **P1 — 未审核目标名进入导出。** 见 T-1 至 T-6 及上述生产链路复现。直接违反唯一 P1 定义中的“未审核目标名/敏感数据进入导出或预览”；Gate A 阻断。
- **阻断契约失败（非另立 P1）：** U-1/U-2 的 UUID v4 约束；E-1 格式检查；E-6 OpenSSL 检查步骤；F-2 符号链接分支未测试；T-1 至 T-6。
- **P2 — 前端测试偶发超时。** 连续三次结果 33/35、35/35、35/35；失败发生在两个 5 秒测试超时，不修改其超时配置。依契约 E-7 记录，不单独作为 Gate A 阻断。
- **观察 — 工作区不完全干净。** 既存的 V5 验收报告未跟踪；未删改。V6 报告新增后同样是新文件。
- **观察 — 符号链接权限。** 独立探针报 `os error 1314`；本报告不把 Rust 测试中提前返回的“测试 passed”当作符号链接验证通过。
- **观察 — 发布签名。** 本地候选安装器和应用 EXE 未签名；未安装，不推断 SmartScreen 的实际提示。

## NOT TESTED 清单

- Gate B 全部桌面安装/向导/权限/默认路径/快捷方式/启动/渲染/主题/关闭/本地数据库读写验收；未安装、启动或卸载正式应用。
- Gate C GitHub Actions 当前 HEAD 的真实运行、artifact 上传验证。
- F-10 前端实际错误提示和 Windows 原生另存为对话框行为。
- F-2 文件符号链接与悬空符号链接（缺少创建权限）；exFAT 与 SMB 是契约明确标为可选观察项，本轮未测。
- 未经用户授权的公网测试、真实用户 history.db 读写/迁移/备份检查及卸载后数据保留检查。
- Windows Sandbox 可执行文件及常见 VM 命令未检测到。DISM 可选功能查询返回权限错误 740；未请求管理员权限，因此没有声称 Sandbox 功能状态已确认。

## 本轮创建并保留的临时路径

按照清单不删除任何本轮探针目录或文件。以下独立临时根目录保留自建 Rust 探针、Cargo 构建输出、金丝雀 JSON/Markdown 文件、隔离 SQLite 数据库、测试记录及路径清单：

`C:\Users\ASUS\AppData\Local\Temp\codex-gate-a-v6-b642334-20261026`

测试数据目录（均位于该根目录下）：

- `f2-existing-4QWqiB`
- `f3-hook-IdBoh5`
- `f3-cross-process-500-hpfnWd`
- `f4-threads-200-HqPxli`
- `f4-processes-200-OZsG8o`
- `f8-f9-report-files-07KBHh`
- `f9-snapshot-retry-tqqWjj`
- `t-memory-hl5PW6`
- `t-sqlite-7Yt0V6`
- `u-run-id-sTDVIb`

根目录还保留 `Cargo.toml`、`Cargo.lock`、`src\main.rs`、`PROBE_RESULTS.tsv`、`f07-stability-repeat.txt`、`TEMP_PATHS.txt`、探针 `target\` 构建文件及完整路径清单 `ALL_RETAINED_PATHS.txt`。完整清单列出 12,294 个路径，文件位于：

`C:\Users\ASUS\AppData\Local\Temp\codex-gate-a-v6-b642334-20261026\ALL_RETAINED_PATHS.txt`

以上临时目录与隔离数据库均未清理；真实用户 AppData 数据目录未读取。
