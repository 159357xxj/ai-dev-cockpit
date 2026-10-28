# AI Dev Cockpit — Release Gate V7 独立复验报告

**验收契约：** `docs/GATE_A_CONTRACT_V6.md`，GA-V6-1.0（本轮未变更）  
**复验日期：** 2026-10-27  
**仓库：** `E:\ai-dev-cockpit`  
**分支 / HEAD：** `fix/v6` / `bf1d639640f0054350064e1ce3b0653b9c5eea91` (`V6-R5: remediation report`)  
**产品版本：** `0.1.0`  
**验收范围：** Gate A 增量复验；本轮没有修改产品源码。

## 判定

**Gate A：FAIL。** 在隔离 SQLite 中写入独立构造的 `app_version` 金丝雀后，实际生产存储读取、报告投影、JSON/Markdown 渲染及保存链路会把凭证样式串、内网主机名和私有 IP 写入预览及导出文件。该结果满足 GA-V6-1.0 第 0 节的 P1(a) 定义。另有 F-2 文件符号链接场景因当前进程没有所需权限而未测试，因此即使不考虑 P1，Gate A 也不能判 PASS。

**Gate B：NOT TESTED。Gate C：NOT TESTED。** 本轮未安装、启动或卸载桌面程序；未触发或观察 GitHub Actions。Gate A 的结果不能替代 Windows 桌面验收或真实 CI 结果。

## 环境与安全边界

- Windows 11 Home 中文版 23H2，Build 22631，x64；Node `v24.19.0`、npm `11.17.0`、Rust/Cargo `1.97.1`、Git `2.54.0.windows.1`。
- `OPENSSL_CONF` 未设置。测试环境可解析到外部 `openssl.exe`（Miniconda/Anaconda 安装目录）；TLS mock 测试会启动外部 OpenSSL CLI 生成证书。因此本地 Rust 测试依赖外部工具链，并非完全自包含。
- 测试前工作区干净；复验 HEAD 与报告中记录的 HEAD 相同。报告文件是本轮唯一新增的仓库文件。
- 仅在仓库代码、独立 harness、临时目录和本轮创建的隔离 SQLite 中运行测试。没有读取、复制、迁移或修改真实用户 `history.db`/WAL/SHM；没有安装、启动、升级或卸载正式应用；没有联网探测、管理员提权、系统网络配置变更、push 或 Release 操作。

## F 组：保存与快照

### 差异边界

已审查 `git diff b642334..HEAD`。差异涉及报告构建/DTO（`src-tauri/src/commands/report.rs`、`report_export.rs`）、命令模块、CI、V6 测试和报告文档。`report.rs` 中报告构建函数确有变化，但保存原语 `save_report_atomic`（当前约第 342 行）、`SnapshotStore` 状态转换 `take`/`release`/`finish_success`/`finish_failure`（当前约第 82–256 行）及保存 IPC 流程没有被修改；`f07_snapshot.rs` 的差异是格式调整。故按本轮约定，未重测既有 F 组全套状态机，只重跑保存集成测试与独立 8 线程探针；报告生成改变可能影响的 F-8 则另以当前源码检查了预览/文件字节。

| 项目 | 结论 | 本轮证据 |
|---|---|---|
| F-1 原子 no-replace 提交 | PASS（代码审查 + V6 既有证据） | 当前 `save_report_atomic` 仍以 `persist_noclobber` 作为唯一提交原语；没有 `rename`/覆盖回退。保存函数未出现在本轮产品差异中。 |
| F-2 已存在对象保护 | PASS（文件、目录、junction）；**NOT TESTED（文件符号链接）** | 当前 `v6_atomic_save` 对文件、目录、junction 用例通过。符号链接测试被明确 `ignored`，要求 `SeCreateSymbolicLinkPrivilege`；`docs/evidence/v6_symlink_elvated.txt` 不存在，当前进程未获该权限，本轮没有提权或改变系统设置。 |
| F-3 提交前竞态 | PASS | 当前 `cargo test ... --features test-hooks --test v6_atomic_save` 中 `save_race_target_created_before_commit` 通过；竞争者目标未被覆盖。 |
| F-4 并发同目标 | PASS（本轮 50 轮 + V6 200 轮既有证据） | 本轮保留探针的 8 线程、50 轮测试每轮均为 1 次成功、7 次 `TargetExists`，获胜文件 SHA 与获胜载荷一致；V6 已有的 200 轮线程/进程结果因对应代码未改变而沿用。 |
| F-5 禁止可替换回退 | PASS（代码审查 + V6 既有证据） | 当前保存函数仍只有 `persist_noclobber` 提交路径；未发现删除目标、复制或普通 rename 回退。 |
| F-6 失败只清理事务临时文件 | PASS | 当前 `save_failure_cleans_only_own_temp` 通过；既有目标失败和竞态用例没有发现临时文件遗留。 |
| F-7 临时文件独占创建 | PASS（代码审查 + 回归测试） | 同目录 `.adc-tmp-` 临时文件仍由 `tempfile` 独占创建；保存实现无差异。 |
| F-8 预览与保存字节一致 | PASS | 当前 JSON/Markdown 输出均进行了预览字节与实际保存字节比较；V7 金丝雀 harness 中 `exact=true`，两种格式 SHA-256 相同。 |
| F-9 取消/失败重试与消费 | PASS | 当前 `TargetExists → Available → 换名重试 → Consumed` 测试通过；预览字节重试保持一致。 |
| F-10 错误反馈 | PASS（后端）；NOT TESTED（桌面 UI） | 后端 `TargetExists` 映射仍返回可区分错误；未运行桌面 UI 核对原生保存对话框和最终提示。 |

本轮复跑命令：`cargo test --all --features test-hooks --test v6_atomic_save -- --nocapture`（工作目录 `src-tauri`），退出码 **0**；**9 passed / 0 failed / 1 ignored**。ignored 为 `save_refuses_existing_symlink`，原因是缺少 `SeCreateSymbolicLinkPrivilege`。编译时出现 linker stdout 的 `.dll.lib`/`.dll.exp` 生成信息 warning；没有因该信息导致失败。

## T/U 组：独立报告金丝雀与 run_id

新 harness 使用当前仓库路径作为真实 `ai-dev-cockpit` crate 依赖，分别通过内存运行记录和独立 SQLite `Storage::save_run` → `get_run` 路径进入实际报告构建和保存代码。JSON 与 Markdown 均检查预览及落盘字节；没有触碰用户数据库。

| 项目 | 结论 | 证据 |
|---|---|---|
| T-1 白名单目标映射 | PASS | 内存和 SQLite 报告中未知 target 均输出 `unknown-target`；已知白名单标签和 6 个目标的统计数据仍存在。 |
| T-2 DTO 不透传数据库/自由字符串 | **FAIL — P1** | `app_version` 金丝雀通过内存及 SQLite 路径进入 JSON/Markdown 预览和保存文件。详见下方缺陷记录。 |
| T-3 诊断文本重建 | PASS（回归测试） | 当前 `v6_target_projection` 回归测试组通过；报告 DTO 的标题、原因、建议和局限由静态 rule profile 重建。 |
| T-4 JSON/Markdown 目标名一致 | PASS | 独立探针中两种格式的目标标签一致，统计字段保留。 |
| T-5 原始错误摘要/详情默认不导出 | PASS | 内存和隔离 SQLite 输出分别扫描，`error.summary`/`error.detail` 不在预览或文件中。 |
| T-6 控制字符/Markdown 注入 | PASS（回归语料 + 本轮输出扫描） | 当前 `v6_target_projection` 测试与独立扫描未发现来源于目标/诊断自由文本的控制字符或 Markdown 注入。T-2 中的 metadata 泄漏不含这些语法字符，但仍构成 P1 敏感信息泄漏。 |
| U-1/U-2 UUID v4 与非法 run_id | PASS | 独立 11-case 语料全部符合预期；非法值不回显，旧格式被拒绝。 |
| U-3 生产 run_id 来源 | PASS | 当前生成器仍由 `scheduler.rs` 的 `Uuid::new_v4()` 创建。 |

### 新发现 V7-T2-APP-VERSION — P1

- **最小复现值：** `sk-CANARY0000000000`、`canary-a1.corp.internal`、`10.13.37.42`。这些值均符合当前 32 字节以内、只允许 ASCII 字母/数字/`.`/`+`/`-` 的校验规则。
- **复现路径：** 将上述任一值放入隔离 `DiagnosticRun.app_version`；分别走内存报告和隔离 SQLite `runs.app_version` 保存/读取；调用实际 `build_report` 并生成 JSON、Markdown 预览和文件。每个值均可在两种格式的预览与落盘字节中找到，且预览与文件字节一致。
- **源码位置：** `src-tauri/src/commands/report_export.rs:104-114` 定义 `SafeAppVersion`；`:586-600` 将 `run.app_version` 同时投影到元数据及运行字段；`:736-747` 的 `normalize_app_version` 对字符集/长度通过的值原样返回。历史记录读取位于 `src-tauri/src/storage/mod.rs:285-296`；生产预览从 `src-tauri/src/commands/report.rs:393-399` 读取历史记录并构建报告。
- **影响判定：** 该结果不是普通 UI 输入路径的证明；正常新记录版本来自产品构建版本。但它确实经生产 Storage 读取、报告投影和导出路径复现了可污染/旧记录中的敏感值进入输出，满足冻结契约 P1(a)。类型包装本身没有对该数据建立隐私白名单。
- **修复方向：** 导出版本字段使用固定构建版本/静态安全值，或采用严格的精确版本白名单；不能仅依靠可接受字符集作为隐私过滤。修复后需重跑内存与 SQLite 两路 JSON/Markdown 预览和文件探针。

本轮 T/U 结果文件为 9 条：目标映射、原始错误字段、run_id 与生产 UUID 来源检查通过；3 种 `app_version` 金丝雀均失败。具体预览/文件 SHA 和用例路径保存在下方 V7 探针目录中。

## E 组：质量基线、构建和 CI

### 实际命令结果

| 命令 | 退出码 | 实际结果 |
|---|---:|---|
| `npm run lint` | 0 | ESLint 完成，无 lint 失败。 |
| `npm run typecheck` | 0 | `tsc --noEmit` 完成。 |
| `npm run test`（连续 3 次） | 0 / 0 / 0 | 每次 9 个测试文件、**35 passed / 0 failed**。 |
| `npm run build` | 0 | Vite 生产构建完成，2630 modules。 |
| `cargo fmt --all -- --check` | 0 | 无格式差异。 |
| `cargo clippy --all-targets -- -D warnings` | 0 | 严格 warning-as-error 检查通过。 |
| `cargo test --all`（首次） | 101 | **94 passed / 1 failed / 3 ignored** 后 fail-fast。`manual_https_proxy_full_tls_success_with_trusted_test_cert` 在 TLS mock 启动时报告 OpenSSL 无法打开 `leaf.pem`；本轮没有证据确认根因。 |
| `cargo test --all --no-fail-fast` | 0 | 全量 **167 passed / 0 failed / 4 ignored**。 |
| `cargo test --all`（复跑） | 0 | 再次全量 **167 passed / 0 failed / 4 ignored**。 |
| `npm run tauri build` | 0 | Tauri Release x64 与 NSIS 构建完成。 |

Rust 完整测试的 4 个 ignored 项为 3 个需要显式联网的 `probes_public` 用例，以及需符号链接权限的 `save_refuses_existing_symlink`。首次 TLS mock 失败之后，完整 no-fail-fast 与随后的全量重跑均通过；因此记录为真实的一次失败和两次完整通过，不推断失败原因已消失或环境绝对稳定。构建日志还包含 linker stdout 的 `.dll.lib`/`.dll.exp` 生成 warning；Clippy 严格命令通过。

### Release 安装包

- 绝对路径：`E:\ai-dev-cockpit\src-tauri\target\release\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe`
- 大小：**2,792,052 bytes**（约 **2.66 MiB**）
- 独立 `Get-FileHash -Algorithm SHA256`：`6FBDB1164A2A76CFA0464C8FC58A2E8B844A5B7E586E2DB2AE15C4A7BE2A196E`
- 文件/产品版本：`0.1.0`；签名：未签名。
- 构建来源：本地 HEAD `bf1d639640f0054350064e1ce3b0653b9c5eea91`；Release build 成功。没有运行安装包或宣称安装/UI 验收通过。

### CI 静态检查与运行状态

- `.github/workflows/ci.yml` 使用 `windows-latest`；前端 Node 22，Rust stable 并安装 rustfmt/clippy；前端 lint/typecheck/test/build 与 Rust fmt/clippy/test 均配置。
- OpenSSL PATH 写入与检查分成不同 step；验证 step 检查 `C:\Program Files\Git\usr\bin\openssl.exe` 存在，确认 `Get-Command openssl` 解析到该绝对路径并运行 `openssl version`。**静态审查 PASS。**
- 顶层权限为 `contents: read`。NSIS workflow 仅在 `workflow_dispatch` 构建并上传 artifact，不自动发布 Release。未发现 workflow 中硬编码 Secrets。
- 本地分支尚未推送，本轮没有 GitHub Actions 运行记录：**真实 CI = NOT TESTED**。YAML 静态 PASS 不等同 GitHub CI PASS。

## 剩余未完成项与风险

1. **阻断：** 修复 `app_version` 数据来源并通过独立恶意值探针；当前 Gate A 为 FAIL。
2. **阻断验收完成：** F-2 文件符号链接及悬空链接行为未实测。测试代码在 `src-tauri/tests/v6_atomic_save.rs:336` 起，明确要求符号链接权限并被 ignored；指定证据文件 `docs/evidence/v6_symlink_elvated.txt` 缺失。
3. **NOT TESTED：** Gate B 安装/升级/卸载、原生对话框、实际桌面 UI、应用退出与数据库运行时行为。
4. **NOT TESTED：** Gate C GitHub Actions 实际运行，以及发布资格/Release 流程。
5. **已记录的质量波动：** 首次全量 `cargo test --all` 的 TLS mock/OpenSSL `leaf.pem` 错误；后续两次全量执行通过，根因未知。

## 保留的本轮临时证据路径

没有清理临时目录。每个根目录中的 `ALL_RETAINED_PATHS.txt` 列出其完整文件/子目录清单。

- F 组保存探针：`C:\Users\ASUS\AppData\Local\Temp\codex-v7-f-bf1d639-1242fdeb\`；清单：`C:\Users\ASUS\AppData\Local\Temp\codex-v7-f-bf1d639-1242fdeb\ALL_RETAINED_PATHS.txt`；结果：`C:\Users\ASUS\AppData\Local\Temp\codex-v7-f-bf1d639-1242fdeb\PROBE_RESULTS.tsv`、`C:\Users\ASUS\AppData\Local\Temp\codex-v7-f-bf1d639-1242fdeb\F4_SHA256_VERIFICATION.csv`。
- T/U 独立金丝雀探针：`C:\Users\ASUS\AppData\Local\Temp\codex-gate-a-v7-tu-bf1d639-20261027-164543\`；清单：`C:\Users\ASUS\AppData\Local\Temp\codex-gate-a-v7-tu-bf1d639-20261027-164543\ALL_RETAINED_PATHS.txt`；结果：`C:\Users\ASUS\AppData\Local\Temp\codex-gate-a-v7-tu-bf1d639-20261027-164543\final-run\PROBE_RESULTS.tsv`、`C:\Users\ASUS\AppData\Local\Temp\codex-gate-a-v7-tu-bf1d639-20261027-164543\final-run\console.log`。
- E 组命令日志与构建输出：`C:\Users\ASUS\AppData\Local\Temp\codex-v7-e-bf1d639-20261027-audit\`；清单：`C:\Users\ASUS\AppData\Local\Temp\codex-v7-e-bf1d639-20261027-audit\ALL_RETAINED_PATHS.txt`；全量测试日志包括 `cargo-test-all.log`、`cargo-test-all-no-fail-fast.log`、`cargo-test-all-final.log`、`v6-atomic-save-test-hooks.log`，前端测试为 `npm-test-1.log`、`npm-test-2.log`、`npm-test-3.log`，构建日志为 `tauri-build.log`。
- V6 既有探针目录 `C:\Users\ASUS\AppData\Local\Temp\codex-gate-a-v6-b642334-20261026\` 保持原样；其中记录的是旧基线 b642334，未把旧结果冒充本轮结果。

## 最终结论

**Gate A：FAIL。** P1 `app_version` 泄漏在隔离 SQLite 的生产读取/导出链路中独立复现；F-2 符号链接另为 NOT TESTED。其余通过项不能抵消阻断缺陷。

**Gate B：NOT TESTED。Gate C：NOT TESTED。** 本报告不构成安装验收、真实桌面验收、CI 通过或发布批准。
