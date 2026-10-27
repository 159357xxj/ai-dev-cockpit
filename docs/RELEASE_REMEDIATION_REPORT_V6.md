# AI Dev Cockpit — V6 修复报告（Gate A 定向修复）

| 项 | 内容 |
|---|---|
| 验收契约 | `docs/GATE_A_CONTRACT_V6.md`（GA-V6-1.0，2026-10-26 冻结） |
| 基线提交 | `24e9e41`（V5 报告补记最终提交哈希） |
| 工作分支 | `fix/v6` |
| 报告范围 | 契约第 1–4 节的 F / T / U / E 组；第 5 节永久回归测试 |
| 执行 | 开发执行者（本地自测）；独立复验待 Codex |
| 报告日期 | 2026-10-27 |

本报告只陈述**开发者自测**证据。Gate B、Gate C 与真实桌面 UI 表现均未执行，见第 6 节。

---

## 1. 提交清单（`24e9e41..HEAD`）

```
6b6594e V6-R4: 全量检查与 Release 构建（无代码改动）
8c143aa V6-R3: run_id 严格 UUID v4 校验 + invalid-run-id + CI openssl 两步验证 + cargo fmt
7dc9750 V6-R2: 补强金丝雀语料（Codex 16 样本：零宽/RTL/全角/64KiB/NUL/反引号/HTML）+ 公网 IP 投影断言
85dcf3f V6-R2: 目标标识安全投影（ExportReport 安全 DTO + 静态文案表）+ 金丝雀回归
b642334 V6-R1: 原子不覆盖提交（persist_noclobber）+ TargetExists 错误码 + test-hooks 竞态回归
85bc4c4 V6-R0: add frozen Gate A contract
```

| 提交 | 内容 | 说明 |
|---|---|---|
| `85bc4c4` | 冻结契约入库 | R0；契约文件与 `outputs/GATE_A_CONTRACT_V6.md` SHA-256 一致 |
| `b642334` | 原子"不替换"提交 + `TargetExists` + `test-hooks` | R1；**已获 Codex 独立 PASS（见第 5 节）** |
| `85dcf3f` | 安全投影 DTO + 静态文案表 + 渲染器重写 | R2 主体 |
| `7dc9750` | 金丝雀语料补强（16 样本）与公网 IP 断言 | R2 补强；**仅改测试文件** |
| `8c143aa` | run_id 严格 UUID v4 + CI openssl 两步 + `cargo fmt --all` | R3 |
| `6b6594e` | 全量检查与 Release 构建 | R4；**空提交**（`git show --stat` 无文件变更），仅标记检查时的树状态 |

本报告由 `V6-R5: remediation report` 提交引入，该提交同时纳入两份验收历史文档（`docs/RELEASE_ACCEPTANCE_REPORT_V5.md`、`docs/RELEASE_ACCEPTANCE_REPORT_V6.md`），不修改任何产品代码。

---

## 2. 不变量对照表

自测结果一律是本机真实执行结果。`NOT TESTED` 不等于通过（契约 §0 第 3 条）。

### 2.1 F 组 — 文件保存提交

| ID | 实现位置（文件:行号） | 对应测试名 | 自测结果 |
|---|---|---|---|
| F-1 | `src-tauri/src/commands/report.rs:373`（唯一提交原语 `temp.persist_noclobber(target)`） | `save_8_concurrent_exactly_one_wins`（`tests/v6_atomic_save.rs:488`）、`save_refuses_existing_file`（:286） | 通过 |
| F-2 | `report.rs:264`（`SaveError::TargetExists`）、`report.rs:342`（`save_report_atomic`）、`report.rs:471`（IPC 映射） | `save_refuses_existing_file`(:286)、`save_refuses_existing_dir`(:316)、`save_refuses_existing_junction`(:400)、`save_refuses_existing_symlink`(:336，`#[ignore]`) | 文件 / 目录 / junction **通过**；**符号链接 NOT TESTED**（见 2.1 末注） |
| F-3 | `report.rs:296`（`test_hooks` 模块）、`report.rs:370`（`before_commit` 调用点）、`report.rs:373` | `save_race_target_created_before_commit`（:448，需 `--features test-hooks`） | 通过（test-hooks 构建） |
| F-4 | `report.rs:373` | `save_8_concurrent_exactly_one_wins`（:488，200 轮 × 8 线程） | 通过 |
| F-5 | 保存模块无 `fs::rename` / `fs::copy` / `fs::remove_file`（全仓库 `src` 检索为空） | 同上 + 代码审查 | 通过 |
| F-6 | `report.rs:373`（失败分支仅分类错误，`PersistError.file` 的 Drop 删除自有临时文件） | `save_failure_cleans_only_own_temp`（:552） | 通过 |
| F-7 | `report.rs:363-365`（`.prefix(".adc-tmp-")` + `tempfile_in(parent)`，独占创建） | `save_failure_cleans_only_own_temp`（:552）、`assert_no_temp_leftovers`（各用例） | 通过 |
| F-8 | `report.rs:61`（`build_report` → 投影 → 渲染）；`report_export.rs:630/634` | `saved_bytes_equal_preview_bytes`（`tests/v6_atomic_save.rs:591`）、`f07_saved_file_bytes_equal_preview_bytes`（`tests/f07_snapshot.rs`） | 通过 |
| F-9 | `report.rs:216`（`finish_success`）、`report.rs:233`（`finish_failure`）、`report.rs:429`（导出命令） | `snapshot_retry_after_target_exists`（:618）、`f07_cancel_allows_retry_same_bytes` | 通过 |
| F-10（后端） | `src-tauri/src/diagnostic/types.rs:93`（`IpcErrorCode::TargetExists`）、`report.rs:471`（→ `target_exists`）；前端 `src/lib/types.ts:41`、`src/pages/HistoryPage.tsx:151-154` | 无自动化 UI 用例；后端行为经 `tests/v6_atomic_save.rs` 的保存层与代码审查确认 | 后端：通过（代码审查）；**UI 表现：NOT TESTED（Gate B）** |

**F-2 符号链接注**：用例 `save_refuses_existing_symlink` 已在 R3（`8c143aa`）改为 `#[ignore = "requires SeCreateSymbolicLinkPrivilege; run with --ignored in elevated shell"]`，创建链接失败即 panic（不再"无权限提前返回"而显示为通过）。本机无 `SeCreateSymbolicLinkPrivilege`（os error 1314），**提权环境证据文件 `docs/evidence/v6_symlink_elevated.txt` 不存在**，故符号链接分支记为 **NOT TESTED**。作为可执行替代，`save_refuses_existing_junction`（:400）在本机真实执行并通过（junction 创建不需要该特权），覆盖 F-2 的 reparse point 分支。该未执行用例内部同时包含悬空符号链接子场景（要求 `TargetExists`、链接不被替换、不在指向位置建文件），同样随 `#[ignore]` 未执行。

### 2.2 T 组 — 目标标识导出

| ID | 实现位置（文件:行号） | 对应测试名 | 自测结果 |
|---|---|---|---|
| T-1 | `src-tauri/src/commands/report_export.rs:48`（`SafeTargetLabel`）、`:53`（`from_target_id`，白名单完全相等比对，未命中 → `unknown-target`） | `target_label_mapping_is_exact_match_only`（`tests/v6_target_projection.rs:369`）、`public_ip_target_maps_to_unknown_target`（:398）、`export_canary_corpus_json_md_preview_and_file`（:220） | 通过 |
| T-2 | `report_export.rs:441-515`（`ExportReport`/`ExportMeta`/`ExportRun`/`ExportTarget`/`ExportAggregate`/`ExportSample`/`ExportDiagnosis`，无裸 `String` 字段）；`:630`/`:634`（渲染器只收 `&ExportReport`）；旧 `secure_run_json`/`secure_diagnoses_json` 已删除 | `engine_free_text_is_not_projected`（:437）、`export_canary_corpus_json_md_preview_and_file`（:220）、`export_canary_from_history_db`（:305） | 通过 |
| T-3 | `report_export.rs:120`（`SafeDiagnosisTitle`）、`:123`（唯一构造入口 `from_rule`）、`:237`（`diagnosis_title` 静态模板） | `engine_free_text_is_not_projected`（:437，伪造 `title = "CANARY_TITLE_FROM_ENGINE"` 后导出无 `CANARY`） | 通过 |
| T-4 | `report_export.rs:468`（`ExportTarget.target`，JSON 与 Markdown 共用同一字段） | `assert_target_name_consistent`（:200，被 :220 / :305 调用；断言 JSON `"target": "…"` 与 Markdown `### …` 次数与名称一致） | 通过 |
| T-5 | `report_export.rs:491/500/550`（样本只保留 `error_kind`；DTO 无 `summary`/`detail` 字段） | `error_free_text_never_reaches_export`（:504） | 通过 |
| T-6 | `report_export.rs:630/634`（渲染器插入内容全部为 `&'static str` / 受限 newtype；`markdown_escape` 已随动态文本删除） | `export_canary_corpus_json_md_preview_and_file`（:220，16 样本 + 码点断言 U+200B/U+202E/U+0000/全角/`<img`）、`export_canary_from_history_db`（:305） | 通过 |

金丝雀语料（契约表 9 条 + Codex 追加 7 条 = **16 个 target_id**）同时经两条生产链路：内存对象与隔离 SQLite（`tempdir` 内 `history.db`）。每条链路都断言 **JSON 预览 / Markdown 预览 / 实际保存后的 JSON 文件 / Markdown 文件**四份输出，并额外断言"预览字节 == 文件字节"。

### 2.3 U 组 — run_id 校验

| ID | 实现位置（文件:行号） | 对应测试名 | 自测结果 |
|---|---|---|---|
| U-1 | `report_export.rs:715`（`normalize_run_id`：`Uuid::parse_str` + `get_version() == Some(Version::Random)` + `get_variant() == Variant::RFC4122` + `raw.len() == 36` → `hyphenated().to_string()`） | `run_id_uuid_v4_strict`（`tests/v6_run_id.rs:53`） | 通过 |
| U-2 | 同上（非法 → 固定占位符 `invalid-run-id`） | `run_id_uuid_v4_strict`（:53）、`exported_report_uses_normalized_run_id`（:107） | 通过 |
| U-3 | `src-tauri/src/diagnostic/scheduler.rs:335`（`run_id: uuid::Uuid::new_v4().to_string()`，未改动） | 代码证据（无独立测试） | 通过（代码审查） |

`run_id_uuid_v4_strict` 覆盖：合法 v4 小写（原样）、合法 v4 大写（归一为小写）、v1、v7、错误 variant（第 20 个字符为 `c`）、`{…}`、`urn:uuid:…`、35 位、37 位、含 `g`、无连字符 32 位、空串；全部非法输入输出 `invalid-run-id` 且断言不回显原值。

### 2.4 E 组 — 工程检查

| ID | 实现/命令 | 结果 | 数字 |
|---|---|---|---|
| E-1 | `cargo fmt --all -- --check`（`src-tauri`） | 通过 | exit 0；CI 同形 `cargo fmt --check` 亦 exit 0 |
| E-2 | `cargo clippy --all-targets`（`src-tauri`） | 通过 | 0 warning / 0 error |
| E-3 | `cargo test --all`（`src-tauri`） | 通过 | **167 passed / 0 failed / 4 ignored**；`--features test-hooks`：**168 / 0 / 4** |
| E-4 | `npm run lint` / `typecheck` / `test`（3 次）/ `build`（根目录） | 通过（含 P2，见第 7 节） | lint 0；typecheck 0；test 33/35、35/35、35/35；build 成功 |
| E-5 | `npm run tauri build`（根目录） | 通过 | exit 0；NSIS 2,792,130 bytes |
| E-6（静态） | `.github/workflows/ci.yml:41-56`（两步：`Put Git OpenSSL on PATH` + `Verify OpenSSL`） | 静态通过 | 运行结果属 Gate C，**NOT TESTED** |
| E-7 | 前端测试连续 3 次 | 见第 7 节 | 1 次 2 例 5s 超时（P2 跟踪） |

---

## 3. R4 检查结果与安装包

### 3.1 结果汇总

| 检查 | 结果 | passed / failed / ignored |
|---|---|---|
| 1. `cargo fmt --all -- --check` | 通过 | —（无差异） |
| 2. `cargo clippy --all-targets` | 通过 | 0 / 0 / 0 |
| 3a. `cargo test --all` | 通过 | **167 / 0 / 4** |
| 3b. `cargo test --all --features test-hooks` | 通过 | **168 / 0 / 4** |
| 4a. `npm run lint` | 通过 | exit 0 |
| 4b. `npm run typecheck` | 通过 | exit 0 |
| 4c. `npm run build` | 通过 | 2630 modules，dist 生成成功 |
| 5. 前端测试 ×3 | 第 1 次失败（2 例超时，E-7 P2）；第 2、3 次通过 | 33/35、35/35、35/35 |
| 6. `npm run tauri build` | 通过 | release 2m53s + NSIS 打包 |
| 7. 安装包 SHA-256 | 完成 | 见 3.4 |

### 3.2 E-1 至 E-5 的完整命令及结果摘要

| ID | 命令（工作目录） | 结果摘要 |
|---|---|---|
| E-1 | `cargo fmt --all -- --check`（`src-tauri`） | 退出 0，无输出（无差异）；另跑 CI 形式 `cargo fmt --check` 亦退出 0 |
| E-2 | `cargo clippy --all-targets`（`src-tauri`） | 退出 0，无 warning/error 行；`Finished dev profile … in 1.55s` |
| E-3 | `cargo test --all`（`src-tauri`） | 167 passed / 0 failed / 4 ignored；`cargo test --all --features test-hooks` → 168 / 0 / 4 |
| E-4 | `npm run lint`、`npm run typecheck`、`npm run test`（×3）、`npm run build`（根目录） | lint/typecheck/build 均退出 0；前端测试 3 次为 33/35、35/35、35/35 |
| E-5 | `npm run tauri build`（根目录） | 退出 0；`Finished 1 bundle at: …\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe (2.66 MiB)` |

### 3.3 ignored 测试清单（4 个）

| # | 测试名 | 文件 | ignore 原因 |
|---|---|---|---|
| 1 | `public_compare_three_modes_on_github` | `tests/probes_public.rs` | 需要用户明确授权联网；`cargo test` 默认跳过 |
| 2 | `public_target_github` | `tests/probes_public.rs` | 需要用户明确授权联网；`cargo test` 默认跳过 |
| 3 | `public_target_zcode` | `tests/probes_public.rs` | 需要用户明确授权联网；`cargo test` 默认跳过 |
| 4 | `save_refuses_existing_symlink` | `tests/v6_atomic_save.rs:336` | requires SeCreateSymbolicLinkPrivilege; run with --ignored in elevated shell |

悬空符号链接断言位于用例 4 内部（同一函数的第二个子场景），因此不计为独立 ignored 条目。

### 3.4 安装包

| 项 | 值 |
|---|---|
| 文件名 | `AI Dev Cockpit_0.1.0_x64-setup.exe` |
| 路径 | `src-tauri/target/release/bundle/nsis/AI Dev Cockpit_0.1.0_x64-setup.exe` |
| 大小 | **2,792,130 bytes（2.66 MiB）** |
| SHA-256 | `F529FF8BAAAF108F291C2EBF64A9D20C13DFF23CFFBAEA71B30A47AC1D9EC2EB` |
| 构建时间 | 2026-10-27 15:48:57 |

未运行安装包，未运行 release exe。

---

## 4. 对 Codex V6 复验结论的逐项回应

Codex 的复验基线为 `b642334`（R1）。下表逐项回应其判定表中的每个 **FAIL** 与 **NOT TESTED**。

| 复验项 | Codex 结论 | 修复提交 | 修改位置 | 新增/相关测试 | 当前自测结果 |
|---|---|---|---|---|---|
| T-1 | FAIL | `85dcf3f`（语料补强 `7dc9750`） | `report_export.rs:48/53`；`report.rs:61`（`build_report` 改为"投影 → 渲染"） | `v6_target_projection.rs:220`、`:305`、`:348`、`:369`、`:398` | 通过（9 用例全绿） |
| T-2 | FAIL | `85dcf3f` | `report_export.rs:441-515`（DTO，无裸 `String`）；`:630/634`（渲染器只收 `&ExportReport`）；`secure_run_json`/`secure_diagnoses_json` 删除（`report.rs` −224 行） | `v6_target_projection.rs:437`、`:504` | 通过 |
| T-3 | FAIL | `85dcf3f` | `report_export.rs:120/123/237`（`SafeDiagnosisTitle` + `diagnosis_title`） | `v6_target_projection.rs:437` | 通过（引擎标题被丢弃，标题由 rule_id + 模板 + 安全标签重建） |
| T-4 | FAIL | `85dcf3f` | `report_export.rs:468`（JSON/Markdown 共用 `ExportTarget.target`） | `v6_target_projection.rs:200`（经 :220/:305 调用） | 通过（同一目标两处名称一致，16 次） |
| T-5 | FAIL | `85dcf3f` | `report_export.rs:491/500/550`（仅 `error_kind`，无 `summary`/`detail` 字段） | `v6_target_projection.rs:504` | 通过（四份输出均无 `CANARY_ERR_SUMMARY`/`CANARY_ERR_DETAIL`，`error_kind` 仍导出） |
| T-6 | FAIL | `85dcf3f` + `7dc9750` | 渲染器只插入 `&'static str`/受限 newtype；`markdown_escape` 删除 | `v6_target_projection.rs:220`、`:305`（含 U+200B/U+202E/U+0000/全角/`<img` 断言） | 通过 |
| U-1 / U-2 | FAIL | `8c143aa` | `report_export.rs:715`（`normalize_run_id` 严格 UUID v4） | `v6_run_id.rs:53`、`:107` | 通过（12 类输入，非法一律 `invalid-run-id` 且不回显） |
| E-1 | FAIL | `8c143aa` | `cargo fmt --all`（`tests/f07_snapshot.rs`、`tests/v5_file_and_limits.rs`） | R4 命令 1（两种 fmt 形式） | 通过（exit 0） |
| E-6（静态） | FAIL | `8c143aa` | `.github/workflows/ci.yml:41-56`（设置与验证拆成两个 step；`Test-Path` 校验绝对路径；`Get-Command … .Source` 与绝对路径比对） | 静态审查 | 静态通过；**运行 NOT TESTED（Gate C）** |
| F-2 | NOT TESTED（部分 PASS） | `8c143aa`（把符号链接用例改为 `#[ignore]`，不再因无权限而显示为通过） | `tests/v6_atomic_save.rs:336` | 文件/目录/junction 用例通过；符号链接用例需提权环境 | **符号链接仍为 NOT TESTED**（`docs/evidence/v6_symlink_elevated.txt` 不存在）；junction 分支通过 |
| F-10 UI | NOT TESTED | `85dcf3f`（后端 + 前端代码） | `types.ts:41`、`HistoryPage.tsx:151-154`、`report.rs:471` | 无自动化 UI 用例 | 后端通过（代码审查）；**UI 待 Gate B** |
| Gate B | NOT TESTED | — | — | — | 不在 V6 代码修复范围 |
| Gate C | NOT TESTED | — | — | — | 不在 V6 代码修复范围（未 push、未触发远端） |

Codex 判为 PASS 的项（F-1/F-3/F-4/F-5/F-6/F-7/F-8/F-9/F-10 后端/U-3/E-2/E-4/E-5）本轮未改变判定口径；其中 **E-3 的计数随新增测试由 157 变为 167**（`--features test-hooks` 为 168），**E-5 的安装包哈希因代码变化而重算**（见 3.4）。

---

## 5. F 组：R1 独立 PASS 与后续未改动证据

**R1 已于 `b642334` 获 Codex 独立 PASS；之后的提交未修改保存相关函数。**

证据（`b642334..HEAD`，路径为实际的 `src-tauri/src/commands/report.rs`）：

```
$ git diff --stat b642334 -- src-tauri/src/commands/report.rs
 src-tauri/src/commands/report.rs | 239 +++------------------------------------
 1 file changed, 15 insertions(+), 224 deletions(-)

$ git diff -U0 b642334 -- src-tauri/src/commands/report.rs | grep '^@@'
@@ -9 +9,2 @@      use crate::AppState;
@@ -11 +11,0 @@     use crate::diagnostic::types::{…};
@@ -46,61 +46,3 @@   pub const REPORT_NOTE: …;
@@ -111,88 +53 @@   fn safe_filename(run_id: &str, format: ReportFormat) -> String {
@@ -201,0 +57,4 @@  fn secure_diagnoses_json(…);
@@ -203,3 +62,3 @@   pub fn build_report(…);
@@ -207,70 +66,2 @@  pub fn build_report(…);
```

- 改动覆盖的**旧文件行号最大值 = 276**（最后一段 hunk 为 `@@ -207,70`，即旧 207–276）。
- 保存相关内容在 `b642334` 中的起始行：`fn storage` **281**、`SnapshotState` **291**、`SnapshotStore` **318**、`test_hooks` **505**、`save_report_atomic` **551**、`preview_sanitized_report` **602**、`export_sanitized_report` **638**、`make_persister` **704**。
- 276 < 281 ⇒ **所有改动行都在 `fn storage` 之前**，F 组的 `SnapshotStore` / `save_report_atomic` / `test_hooks` / 两个 IPC 命令 / `make_persister` 均未被触碰。
- 交叉验证（无输出即无改动）：

```
$ git diff b642334 -- src-tauri/src/commands/report.rs \
    | grep -E '^[-+].*(save_report_atomic|persist_noclobber|SnapshotStore|finish_failure|finish_success|export_sanitized_report|test_hooks|make_persister)'
（无输出）
```

- 行号变化的唯一原因：被删除的 224 行位于这些函数**之上**，因此它们在当前 HEAD 中整体前移（`save_report_atomic` 551 → `report.rs:342`）；函数体内容逐字节未变。
- F 组的行为变化全部发生在 `b642334`（R1）**自身**内。R2 的改动只影响报告内容的生成（即 F-8 上端的投影），预览与保存仍共用同一份快照字节，F-8 不变式未被削弱；R2/R3 对提交、临时文件清理、快照状态机三者**没有任何改动**，因此 `b642334` 的 Codex 独立 PASS 结论对当前 HEAD 继续成立。

---

## 6. NOT TESTED 清单

以下项目**未执行**，不得记为通过：

1. **Gate B 全部内容**：安装、升级、卸载、首次启动、真实桌面 UI 与"另存为"对话框交互、正式实例行为。
2. **Gate C 全部内容**：GitHub Actions 真实运行结果（E-6 的运行侧）、发布流程与产物签名分发。
3. **F-10 的真实桌面 UI 表现**：`target_exists` 提示的实际渲染、rfd 覆盖确认对话框与后端拒绝的交互序列（`rfd 0.15.4` 无关闭覆盖确认的 API，已在 R1 说明）。
4. **exFAT / 网络共享路径**：`persist_noclobber`（`MoveFileExW`）在非 NTFS 卷、UNC 路径上的原子性与错误码未验证（契约未要求，但属已知未覆盖面）。
5. **F-2 符号链接分支**：本机无 `SeCreateSymbolicLinkPrivilege`；提权环境证据文件 `docs/evidence/v6_symlink_elevated.txt` 不存在。
6. **真实 `%APPDATA%` / `history.db`**：所有测试均使用 `tempfile::tempdir()` 隔离目录与隔离 SQLite，未读写真实用户数据。

---

## 7. 已知 P2

| 项 | 现象 | 证据 | 处理 |
|---|---|---|---|
| E-7 前端偶发超时 | 连续 3 次运行：第 1 次 2 例 `Test timed out in 5000ms`（`navigation.test.tsx`「五个页面导航 > 可通过导航在五个页面间切换」文件耗时 11097ms；`demo-mode.test.tsx`「演示模式标识与开关 > 设置页可切换演示模式并持久化到 localStorage」文件耗时 11736ms）；第 2、3 次 35/35 | R4 步骤 5 原始输出 | 契约 E-7 明确「不阻断 Gate A，作为 P2 跟踪」。观察：首次运行整体耗时 39.05s，为后续（≈4.2s）的 9 倍，属环境级整体变慢而非用例逻辑失败 |
| 诊断正文条目未导出 | 为满足 T-2「无裸 String」，导出 DTO 不包含引擎的 `observed_facts`（其内容为"静态模板 + 计数"的动态字符串）；数值证据仍在 `ExportAggregate` / `ExportSample` 中完整保留 | `report_export.rs` 投影实现 | 产品口径取舍，已征询用户；如需恢复需授权改 `rules.rs` 或引入类型化计数事实 |
| 报告文案静态表维护成本 | `RuleProfile`（`report_export.rs:223`）逐字复制引擎文案，由 `export_static_copy_mirrors_engine_copy`（`v6_target_projection.rs:467`）以源码扫描守护 | 该测试通过 | 修改引擎文案时须同步改表，测试会立即报错 |

---

## 8. 观察项（不影响本轮判定）

1. **bundle identifier 以 `.app` 结尾**：`tauri.conf.json` 中 `com.aidevcockpit.app` 触发 Tauri 的 macOS 相关警告。**未修改**——修改 identifier 会改变用户数据目录（`history.db` 所在位置），属于会影响既有用户的变更。建议作为独立议题评估，本轮仅记录。
2. **msvc linker 提示**：`warning: linker stdout: 正在创建库 …ai_dev_cockpit_lib.dll.lib 和对象 ….exp`，为既有构建提示（`#[warn(linker_messages)]`），非代码告警，R1 之前即存在。
3. **`ExportAggregate` 命名**：与提示词示例中的 `ExportTargetStat` 语义相同、名称不同；未改名以避免与内部 `ProbeAggregate` 混淆。
4. **R4 空提交**：`6b6594e` 无文件变更，仅用于在 `24e9e41..HEAD` 留下"该树状态已执行全量检查"的记录。

---

## 9. 对既有测试的影响

| 类别 | 数量 | 明细 |
|---|---|---|
| 删除的测试 | **0** | — |
| 变更期望的既有测试 | **1** | `v5_malicious_run_id_timestamps_version_neutralized`（`tests/v5_file_and_limits.rs:391`）：`contains("invalid-run")` → `contains("invalid-run-id")`，对应契约 **U-2**；属收紧（旧串只是新占位符的前缀） |
| 纯格式改动（无逻辑变化） | 2 个文件 | `tests/f07_snapshot.rs`、`tests/v5_file_and_limits.rs`，由 `cargo fmt --all` 产生 |
| 新增测试文件 | 3 | `tests/v6_atomic_save.rs`（10 个测试函数：默认构建 9 个参与，其中 1 个 ignored；`save_race_target_created_before_commit` 仅在 `--features test-hooks` 下参与）、`tests/v6_target_projection.rs`（9 个用例）、`tests/v6_run_id.rs`（2 个用例） |
| 新增 ignored 用例 | 1 | `save_refuses_existing_symlink`（原为"无权限即提前返回并显示 passed"，现为 ignored） |

未修改 `tauri.conf.json`；未修改 `diagnostic/rules.rs` 的 R01–R12 判断逻辑；未修改 `scheduler.rs` 的 run_id 生成。

---

## 10. 结论

**开发者自测通过，待 Codex 按 Gate A 契约独立复验。**

本结论仅覆盖开发者本地自测范围。Gate B 与 Gate C 全部为 `NOT TESTED`，F-2 符号链接分支与 F-10 的真实桌面 UI 表现亦为 `NOT TESTED`，不得据此发布或宣称完整 Windows Release Gate 通过。
