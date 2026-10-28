# AI Dev Cockpit — Gate A 增量复验报告 V8

**验收契约：** `docs/GATE_A_CONTRACT_V6.md`，GA-V6-1.0（本轮未修改）  
**验收对象：** `fix/v6`，HEAD `4f7f33715c5f73bf3dd7c2e2d14737786e0eb987`  
**基线：** `bf1d639`  
**执行日期：** 2026-10-27（Windows 11 家庭中文版 x64，版本 10.0.22631 / Build 22631）  
**范围：** Gate A 增量复验；未安装或启动正式应用，未访问真实用户数据库，未提权、推送或发布。

## 判定

**Gate A：NOT READY。** 本轮独立复验没有复现新的 P1；V7 的 `app_version` 导出泄漏在内存和隔离 SQLite 两条路径上均已关闭。保存原语、快照状态转换和保存 IPC 未被本轮差异触及，按约定复跑的测试通过。但 GA-V6-1.0 第 1 节 F-2 将文件符号链接场景列为阻断项；指定的提权证据 `docs/evidence/v6_symlink_elevated.txt` 不存在，源码测试也在本机运行中被标记为 ignored。按契约第 0 节，任一阻断项为 NOT TESTED 时 Gate A 不能 PASS。

本轮未发现满足 GA-V6-1.0 第 0 节定义的新 P1，因此结论不是 FAIL。其余质量基线通过情况和未测试项目见下文。

## 1. 环境、版本与工作区

- 项目根目录：`E:\ai-dev-cockpit`。
- Git 分支及 HEAD：`fix/v6` / `4f7f33715c5f73bf3dd7c2e2d14737786e0eb987`。
- `package-lock.json` 和 `src-tauri/Cargo.lock` 均存在。
- Node `v24.19.0`、npm `11.17.0`、Rust/Cargo `1.97.1`。
- 本轮产品源文件未修改。工作区开始时已有未跟踪文件 `docs/RELEASE_ACCEPTANCE_REPORT_V7.md`；本报告保留该文件，不覆盖、不暂存。
- 候选 NSIS 安装包由本轮当前 HEAD 的 `npm run tauri build` 生成：
  - 绝对路径：`E:\ai-dev-cockpit\src-tauri\target\release\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe`
  - 大小：2,791,277 bytes
  - SHA-256：`CA792870B5732C938AF47DAD26DD05530EA57386E8F11A6D15B5C10BE6EDE0BE`
  - `Get-FileHash -Algorithm SHA256` 实测值如上；这是本轮重建后的值，不沿用先前哈希。

## 2. 差异边界与 F 组保存复验

`git diff --name-status bf1d639..HEAD` 仅包含：

| 状态 | 文件 | 与保存流程的关系 |
|---|---|---|
| M | `docs/RELEASE_REMEDIATION_REPORT_V6.md` | 文档 |
| M | `src-tauri/src/commands/report_export.rs` | 只改 `SafeAppVersion` 的版本白名单及 `project_report` 中的版本投影 |
| A | `src-tauri/tests/v6_export_field_audit.rs` | 新增报告字段/数据库列审计测试 |

差异未触及 `save_report_atomic`、`SnapshotStore` 状态迁移、`src-tauri/src/commands/report.rs` 中的预览/保存 IPC 流程。因此按本轮约定未重复全套 F 状态机审计，执行保存集成测试和 8 线程 50 轮探针，并沿用 V6/V7 中未变更代码的结论。

### F 组结果

| 项目 | 结论 | 本轮依据 |
|---|---|---|
| F-1 原子 no-replace | PASS（沿用） | 保存实现未进入差异；沿用 V6/V7 的 `persist_noclobber` 源码审查与证据。 |
| F-2 已存在对象保护 | **NOT TESTED（符号链接）** | 文件、目录及 junction 的先前证据沿用；本轮指定提权证据缺失，见下节。 |
| F-3 提交前竞争者创建目标 | PASS（本轮复跑） | 保存集成测试中的 race 用例通过。 |
| F-4 同目标并发保存 | PASS | 保留的 8 线程屏障探针运行 50 轮；每轮 1 次成功、7 次 `TargetExists`，获胜载荷 SHA 匹配。沿用 V6 已完成的 200 轮证据。 |
| F-5 禁止可替换回退 | PASS（沿用） | 保存函数未变化；沿用 V6/V7 对唯一提交路径及无 rename/copy/先删后写回退的审查。 |
| F-6 仅清理事务临时文件 | PASS（本轮复跑） | 保存集成测试通过；已有目标与竞态用例通过。 |
| F-7 独占临时文件 | PASS（沿用） | 保存函数未变化；沿用 V6/V7 的 `create_new` 与同目录随机临时文件证据。 |
| F-8 预览/落盘字节一致 | PASS | 8 线程探针内 JSON 和 Markdown 的预览/文件 SHA-256 分别完全相同。 |
| F-9 失败可重试、成功后消费 | PASS | 探针验证 `TargetExists → Available → 换名重试 → Consumed`，原目标保持不变且重试字节与预览相同。 |
| F-10 保存错误 UI | 后端 PASS；桌面 UI NOT TESTED | 本轮未运行桌面应用；沿用后端错误码结论。 |

实际命令：`cargo test --all --features test-hooks --test v6_atomic_save -- --nocapture`，退出码 0：**9 passed / 0 failed / 1 ignored**。被忽略的是 `save_refuses_existing_symlink`。

8 线程探针结果文件显示 50/50 轮通过。JSON 预览/文件 SHA-256 均为 `ae49037cf657ab9d103e2107233cd149a10b8fed57e236e4e27b108405cd6e8a`；Markdown 预览/文件 SHA-256 均为 `e1b64dec48e58a13482c92e0a2dc7f827bae593a60a3f470a813965e59ae750a`。

### F-2 符号链接证据

- `Test-Path docs/evidence/v6_symlink_elevated.txt` 实测为 `False`，`docs/evidence` 下没有指定文件。
- 当前 `src-tauri/tests/v6_atomic_save.rs:330-388` 只有一个 `save_refuses_existing_symlink` 测试函数；函数内包含有效文件符号链接和悬空符号链接两个用例。
- 该函数带 `#[ignore = "requires SeCreateSymbolicLinkPrivilege; run with --ignored in elevated shell"]`。本轮测试输出明确显示 ignored，而不是两个符号链接断言通过。
- 因而无法确认“两个符号链接测试均为 ok 且来自提权环境”的主张。未提权复跑、未启用开发者模式，也未更改系统设置。**F-2 符号链接：NOT TESTED。**

## 3. T/U 组：app_version、全列注入及 DTO 审计

### V7 app_version 金丝雀复跑

将 V7 金丝雀探针重新指向当前仓库 crate；内存构造和隔离 SQLite `Storage::save_run → get_run` 两条路径均执行。三种既有值 `sk-CANARY0000000000`、`canary-a1.corp.internal`、`10.13.37.42` 均在 JSON/Markdown 预览和实际文件中无泄漏；`meta.app_version` 和记录 `app_version` 均由安全常量/精确版本白名单提供。JSON、Markdown 预览和保存文件一致，未知目标仍为 `unknown-target`，6 个白名单目标及统计字段保留。U 组 11 个 run_id 用例通过，生产生成器仍使用 `Uuid::new_v4()`。

探针第一次启动时因输出目录未创建而失败（测试 harness 的目录准备错误，不是产品运行失败）；创建隔离输出目录后重跑退出码 0，所有上述断言通过。首次失败日志保留。

### 独立 schema 驱动注入探针

本轮独立编写临时 Rust harness，未复用开发者 `v6_export_field_audit` 测试。探针在临时 SQLite 数据库上调用生产 `Storage::open/save_run/get_run`、报告构建和保存代码，以 `sqlite_master` 与 `PRAGMA table_info` 动态发现结构，不硬编码列清单：

- 覆盖全部 18 个声明为 `TEXT` 的列（包括 `settings.value_json`）。自由文本值经 JSON/Markdown 预览及文件扫描；枚举/主键约束列出现预期的 `READ_REJECTED`。
- 对发现的 JSON 列解析实际记录，并分别将假凭证写入 **5 个字符串叶子值**；五项均未进入两种格式的输出。
- 另行写入代理 URL `http://v8-probe-user:v8-probe-pass@127.0.0.1:17890/private?token=V8_PROXY_TOKEN`，分别经 runs.notes、results 错误文本及 settings JSON 路径检查；用户名、密码、私有地址和 token 均未出现在预览或保存文件。
- 合计 **24 项断言，0 失败**（18 个 TEXT 列 + 5 个 JSON 字符串叶子 + 1 个代理 URL userinfo 场景）。所有 SQLite 文件均为本轮临时隔离数据；没有访问真实用户数据库。

### 开发者 DTO 字段表的独立核对

逐项对照 `docs/RELEASE_REMEDIATION_REPORT_V6.md` 第 11.2 节和 `report_export.rs` 结构体定义，表中七个渲染 DTO（`ExportReport`、`ExportMeta`、`ExportRun`、`ExportTarget`、`ExportAggregate`、`ExportSample`、`ExportDiagnosis`）共 **45 个字段，缺项 0**。实际来源/处理与表述相符：`meta.app_version` 是编译期版本；run 版本经精确白名单；target 标签白名单；诊断标题由静态模板和安全标签构造；run_id/时间戳规范化；统计字段是数值或由样本重算；原始 `error.summary/detail` 不进入 DTO。

范围说明：该表准确覆盖其声明的“渲染器可达 DTO”，但没有把 `report.rs` 中的 IPC/状态结构 `ReportPreview`、`ExportResult`、`ReportSnapshot` 逐项列入 45 字段。静态追踪到的字符串来源分别是已渲染的安全报告字节、规范化 run_id、安全文件名、固定说明文案、快照 UUID，以及用户在保存对话框选择的路径；未发现原始历史文本被追加到预览/报告。该范围差异作为观察项记录，不满足契约 P1 定义。

**T/U 结论：** 本轮独立探针未复现 V7 的 P1；T-1 至 T-6、U-1 至 U-3 的本轮相关回归结果均通过。全量 Rust 测试和列探针证据见下节及证据路径。

## 4. E 组质量基线与 Release 构建

| 命令 | 退出码 | 实际结果 |
|---|---:|---|
| `npm run lint` | 0 | ESLint 完成，无 lint 错误。 |
| `npm run typecheck` | 0 | `tsc --noEmit` 完成。 |
| `npm run test`（串行第 1 次） | 0 | 9 个测试文件通过，35 passed。 |
| `npm run test`（串行第 2 次） | 0 | 9 个测试文件通过，35 passed。 |
| `npm run test`（串行第 3 次） | 0 | 9 个测试文件通过，35 passed。 |
| `npm run build` | 0 | TypeScript 与 Vite 生产构建完成，2,630 modules transformed。 |
| `cargo fmt --all -- --check` | 0 | 通过。 |
| `cargo clippy --all-targets -- -D warnings` | 0 | 通过，无 Clippy 警告。 |
| `cargo test --all`（两次独立全量运行） | 0 / 0 | 每次 **170 passed / 0 failed / 4 ignored**。 |
| `npm run tauri build` | 0 | Release EXE 与 NSIS 包构建完成。 |

Rust 全量测试每次 ignored 的 4 项：3 个需用户授权联网的 `probes_public` 测试（`public_compare_three_modes_on_github`、`public_target_github`、`public_target_zcode`），以及需要符号链接特权的 `save_refuses_existing_symlink`。本轮没有执行公网测试或提权测试。

另有一次在首次三次前端测试调度过程中发生的重叠运行：其中一个运行超时失败 2 项（`navigation.test.tsx`、`demo-mode.test.tsx` 各一项，均为 5 秒超时，33 passed / 2 failed）；另两个并发运行各 35 项通过。随后严格串行复跑三次，结果均为 35/35、退出码 0。此项记录为 E-7 前端间歇超时观察，不掩盖首次失败；串行验收基线通过。

两个 `cargo test --all` 全量运行中的 `manual_https_proxy_full_tls_success_with_trusted_test_cert` 均通过，本轮 TLS mock 失败 **0 次**。开发者 V6 报告关于时间戳目录冲突的调查与当前 `tls_mock.rs` 中 `SystemTime` 纳秒目录名、`create_dir_all` 的实现相符，但根因仍是解释性假设；本轮未复现失败，不能以此证明或否定该竞态。

构建输出有 Tauri 配置标识符以 `.app` 结尾的提示；Release Rust 链接器还打印创建 `.dll.lib/.dll.exp` 的 stdout warning（`ai-dev-cockpit` generated 1 warning）。构建成功，Clippy `-D warnings` 通过。

### CI

`.github/workflows/ci.yml` 未出现在 `bf1d639..HEAD` 差异中，沿用 V7 的本地静态审查结果；本轮未重新审阅无变化的 workflow。没有 GitHub Actions 实际运行记录，状态仍为 **NOT TESTED — 等待推送后实际运行**。本地构建或 YAML 静态审查不等于 CI 通过。

## 5. 未测试项与观察项

### 阻断 NOT TESTED

1. **F-2 文件符号链接和悬空符号链接：NOT TESTED**，缺少指定的提权运行证据文件；这是 Gate A 不能 PASS 的直接原因。

### 观察项（不构成契约 P1）

1. 开发者 DTO 表的 45 字段范围限于渲染 DTO；IPC 预览/保存响应和快照结构包含单独的 `String`/`Arc<str>` 字段，虽然本轮来源追踪及探针未发现原始敏感文本进入预览或报告，建议文档将此范围边界明确写出。
2. 前端并发调度的一次测试运行有 2 个 5 秒超时；随后三次串行运行全部通过。按 GA-V6-1.0 E-7 作为非阻断间歇超时跟踪。
3. 本轮 TLS mock 全量测试未失败（两次均通过）；开发者对时钟命名碰撞的解释仍未被本轮实验确认。

### 其他 NOT TESTED

- 未在桌面 UI 验证预览、原生保存对话框、主题切换或提示文本。
- 未安装、升级、卸载或启动候选安装包；未触碰真实用户数据库或用户文件。
- 未运行 GitHub Actions；未执行外网探测。

## 6. 可复核证据位置

本轮主要输出保留在 `C:\Users\ASUS\AppData\Local\Temp\codex-gate-a-v8-4f7f337-20261027\`：

- F 保存集成测试：`f-v6-atomic-test.log`
- F 8 线程结果：`retained-8-thread-probe\run-final\PROBE_RESULTS.tsv`；运行输出：`retained-8-thread-probe-final.log`
- V7 app_version/U 金丝雀结果：`v7-app-version-probe\run-current-head\PROBE_RESULTS.tsv`；首次 harness 输出目录失败日志：`v7-app-version-probe-initial-missing-output-dir.log`
- 独立 schema 注入源代码：`independent-db-column-probe\src\main.rs`
- 独立 schema 注入结果：`independent-db-column-probe\run-final\RESULTS.tsv`、`DISCOVERED_TEXT_COLUMNS.tsv`；进程日志：`independent-db-column-probe-final.log`
- Rust 全量测试：`cargo-test-all.log`、`cargo-test-all-rerun.log`、`cargo-test-all-rerun.exitcode.txt`
- 前端测试：`npm-test-v8-serial-1.log`、`npm-test-v8-serial-2.log`、`npm-test-v8-serial-3.log`；早期重叠运行日志为 `npm-test-1.log` 至 `npm-test-3.log`
- 质量命令/构建日志：`npm-lint.log`、`npm-typecheck.log`、`npm-build.log`、`cargo-fmt.log`、`cargo-clippy.log`、`tauri-build.log`、`tauri-build.exitcode.txt`

## 最终结论

**Gate A：NOT READY。** V7 `app_version` P1 修复经独立内存、隔离 SQLite、全 TEXT 列、JSON 字符串叶子和代理 userinfo 探针确认未再泄漏；本轮没有复现新 P1。保存和快照路径未变更，定向回归通过。由于 F-2 符号链接这一契约阻断项缺少提权执行证据，不能按 GA-V6-1.0 判为 PASS。补齐有效提权证据后，仍需按该契约复核 F-2 两种符号链接场景。

**本报告不代表桌面安装验收、GitHub CI 通过或发布批准。**

## F-2 收尾

本节是 V8 的增量补遗；前文 `NOT READY` 是缺少提权证据时的阶段性结论，现由本节基于新增证据作出的最终 Gate A 判定取代。

**收尾复验日期：** 2026-10-27  
**HEAD 核对：** `git rev-parse HEAD` 为 `4f7f33715c5f73bf3dd7c2e2d14737786e0eb987`，与 V8 验收对象完全相同；故本次只复核 F-2，不重跑其余 V8 项。

### 提权证据与源码核对

- `docs/evidence/v6_symlink_elevated.txt` 存在（10,472 bytes）。文件开头显示 `S-1-16-12288`，并明确标注 `High Mandatory Level`。
- 同一证据中的命令为 `cargo test --test v6_atomic_save -- --ignored --nocapture`；随后显示 `running 1 test`、`test save_refuses_existing_symlink ... ok` 以及 `test result: ok. 1 passed; 0 failed; 0 ignored; ...`。因此确认该符号链接测试在 High Mandatory Level 环境中执行并通过。
- 输出开头的 PowerShell `NativeCommandError` 位于 `2>&1` 重定向后的 cargo 编译 stderr 显示中；依照本轮约定将其视为 PowerShell 对原生命令 stderr 的显示问题。后续测试输出明确报告该测试 `ok`、1 passed、0 failed，不把该 stderr 显示误判为测试失败。
- 对照 `src-tauri/tests/v6_atomic_save.rs:349-368`：测试要求保存返回 `SaveError::TargetExists`；随后用 `symlink_metadata` 断言目标仍为符号链接；读回被指向文件并逐字节断言等于 `VICTIM`，同时检查长度和修改时间不变。代码亦检查没有遗留临时文件。
- 同一个测试函数还执行悬空符号链接分支（`:371-392`）。整体测试为 `ok`，所以该分支也在这次运行中通过；此项只是额外记录，不作为契约 F-2 的单列判定条件。

### F-2 与 Gate A 最终判定

**F-2：PASS。** 本次高完整性级别证据与源码断言相互吻合；既有普通文件、目录和 junction 检查沿用 V8/V7 结论。

**Gate A：PASS（GA-V6-1.0）。** V8 判定时唯一未完成的阻断项 F-2 现已取得提权运行通过证据；HEAD 与 V8 相同，其他契约项沿用 V8 的复验结果。本结论只覆盖 Gate A，不代表 Windows 桌面安装验收或 GitHub Actions 已通过，也不构成发布批准。
