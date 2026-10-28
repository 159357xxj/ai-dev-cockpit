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

---

## 11. 对 Codex V7 结论的回应

Codex V7 复验（`docs/RELEASE_ACCEPTANCE_REPORT_V7.md`，基线 `bf1d639`）判定 Gate A 失败，P1 为 **V7-T2-APP-VERSION**：数据库中的 `app_version` 原样进入 JSON/Markdown 导出。R6 修复该项，并把"导出字段可能携带数据库/运行时自由文本"这一类问题一次性用**库结构驱动的金丝雀测试**关掉。

### 11.1 V7-T2-APP-VERSION：修复方式与代码位置

**缺陷**：`SafeAppVersion` 旧实现只做"字符集 + 长度"校验（`normalize_app_version`），通过即**原样返回**。因此 `sk-CANARY0000000000`、`canary-a1.corp.internal`、`10.13.37.42` 这类满足字符集的串可以从 `runs.app_version` 一路进到导出。字符集不是隐私过滤。

**修复**：

| 导出位置 | 旧行为 | 新行为 |
|---|---|---|
| `meta.app_version`（"生成报告的应用版本"） | 取运行记录的版本 | 固定为编译期常量 `env!("CARGO_PKG_VERSION")`，**不读运行记录** |
| `run.app_version`（被记录的那次运行的版本） | 字符集通过即原样输出 | 与编译期版本或 `KNOWN_RELEASED_VERSIONS` 中某项**完全相等**才输出该 `&'static str` 常量，否则固定 `unknown-version` |

| 代码位置 | 内容 |
|---|---|
| `src-tauri/src/commands/report_export.rs:101-145` | `CURRENT_BUILD_VERSION`、`KNOWN_RELEASED_VERSIONS = &["0.1.0"]`、`whitelisted_version`、`SafeAppVersion(&'static str)` |
| `report_export.rs:620` | 投影：`meta.app_version = SafeAppVersion::current_build()` |
| `report_export.rs:632` | 投影：`run.app_version = SafeAppVersion::from_record(&run.app_version)` |
| （已删除） | `normalize_app_version`（"字符集 + 长度通过就原样返回"）整体删除 |

`SafeAppVersion` 现在**只持有 `&'static str`**，`current_build` / `from_record` 均为模块私有，外部无法从任意 `String` 构造；DTO 中该字段的取值集合是 `{编译期常量, "0.1.0", "unknown-version"}`。

**回归测试**：`tests/v6_export_field_audit.rs` 的 `app_version_whitelist_rejects_v7_canaries` 直接用 Codex 的三个最小复现值，断言四份输出（JSON/Markdown 预览与落盘文件）都不含 `canary`/`sk-`/`10.13.37`，且记录版本显示为 `unknown-version`、元数据版本显示为编译期常量。列级审计另覆盖 `runs.app_version => CLEAN`。

### 11.2 导出 DTO 字段审计（任务 2）

渲染器可达的全部结构体与字段（`render_json` 使用全部字段；Markdown 只使用其中一部分，不影响审计口径）。**结论：没有任何字段可以承载数据库/运行时/用户输入的自由文本。**

| 结构体.字段 | 类型 | 数据来源 | 可能来自数据库/运行时/输入 | 处理方式 |
|---|---|---|---|---|
| `ExportReport.meta` | `ExportMeta` | 投影构造 | 否 | 嵌套结构体 |
| `ExportReport.run` | `ExportRun` | 投影构造 | 否 | 嵌套结构体 |
| `ExportReport.diagnoses` | `Vec<ExportDiagnosis>` | 引擎诊断投影 | 否 | 集合 |
| `ExportMeta.app_version` | `SafeAppVersion(&'static str)` | 编译期常量 `env!("CARGO_PKG_VERSION")` | 否（**不读记录**） | 编译期常量 |
| `ExportMeta.exported_at` | `SafeTimestamp(String)` | 运行时系统时钟 | 是（运行时） | chrono 规范化 RFC3339 UTC；不可解析 → `invalid-timestamp` |
| `ExportMeta.note` | `&'static str` | 常量 `REPORT_NOTE` | 否 | 静态文案 |
| `ExportRun.run_id` | `SafeRunId(String)` | DB `runs.id` | 是 | 严格 UUID v4 规范化；否则 `invalid-run-id`（T-2 明确允许"规范化 run_id"） |
| `ExportRun.started_at` | `SafeTimestamp(String)` | DB `runs.started_at` | 是 | RFC3339 规范化；否则 `invalid-timestamp` |
| `ExportRun.finished_at` | `Option<SafeTimestamp>` | DB `runs.finished_at` | 是 | 同上 |
| `ExportRun.status` | `RunStatus` | DB `runs.status` | 是（枚举列） | 封闭枚举；解析失败 → `get_run` 拒绝（READ_REJECTED） |
| `ExportRun.route_mode` | `RouteMode` | DB `runs.route_mode` | 是（枚举列） | 封闭枚举；同上 |
| `ExportRun.repeats` | `u8` | DB `runs.repeats`（INTEGER） | 是 | 数值 |
| `ExportRun.app_version` | `SafeAppVersion(&'static str)` | DB `runs.app_version` | 是 | **白名单版本常量**；未命中 → `unknown-version` |
| `ExportRun.targets` | `Vec<ExportTarget>` | DB `results` 分组 | 否 | 集合 |
| `ExportTarget.target` | `SafeTargetLabel(&'static str)` | DB `results.target_id` | 是 | 白名单 `id → label`；未命中 → `unknown-target` |
| `ExportTarget.aggregates` | `Vec<ExportAggregate>` | 由样本重算 | 否 | 集合 |
| `ExportTarget.samples` | `Vec<ExportSample>` | DB `results` | 否 | 集合 |
| `ExportAggregate.probe_type` | `ProbeType` | DB `results.probe_type` | 是（枚举列） | 封闭枚举 |
| `ExportAggregate.attempts_total` | `u32` | 计数 | 是 | 数值 |
| `ExportAggregate.cancelled_count` | `u32` | 计数 | 是 | 数值 |
| `ExportAggregate.success_count` | `u32` | 计数 | 是 | 数值 |
| `ExportAggregate.transport_failure_count` | `u32` | 计数 | 是 | 数值 |
| `ExportAggregate.http_response_count` | `u32` | 计数 | 是 | 数值 |
| `ExportAggregate.request_failure_rate` | `f64` | 计算值 | 是 | 数值 |
| `ExportAggregate.duration_min_ms` | `Option<u64>` | 计算值 | 是 | 数值 / null |
| `ExportAggregate.duration_median_ms` | `Option<u64>` | 计算值 | 是 | 数值 / null |
| `ExportAggregate.duration_max_ms` | `Option<u64>` | 计算值 | 是 | 数值 / null |
| `ExportSample.probe_type` | `ProbeType` | DB `results.probe_type` | 是（枚举列） | 封闭枚举 |
| `ExportSample.route_mode` | `RouteMode` | DB `results.route_mode` | 是（枚举列） | 封闭枚举 |
| `ExportSample.attempt_no` | `u32` | DB INTEGER | 是 | 数值 |
| `ExportSample.success` | `bool` | DB INTEGER(0/1) | 是 | 布尔数值 |
| `ExportSample.duration_ms` | `Option<u64>` | DB INTEGER | 是 | 数值 / null |
| `ExportSample.http_status` | `Option<u16>` | DB INTEGER | 是 | 数值 / null |
| `ExportSample.http_outcome` | `Option<HttpOutcome>` | DB `results.http_outcome` | 是（枚举列） | 封闭枚举；解析失败 → READ_REJECTED |
| `ExportSample.error_kind` | `Option<ErrorKind>` | DB `results.error_kind` | 是（枚举列） | 封闭枚举；解析失败 → READ_REJECTED |
| `ExportSample.tested_at` | `SafeTimestamp(String)` | DB `results.tested_at` | 是 | RFC3339 规范化；否则 `invalid-timestamp` |
| `ExportDiagnosis.rule_id` | `RuleId` | 引擎 `rule_id` | 否 | 封闭枚举（R01..R12 + `unknown-rule`） |
| `ExportDiagnosis.title` | `SafeDiagnosisTitle(String)` | `diagnosis_title(rule, 安全标签)` | 否 | 静态模板 + `SafeTargetLabel` 确定性生成 |
| `ExportDiagnosis.severity` | `Severity` | 引擎 | 否 | 封闭枚举 |
| `ExportDiagnosis.confidence` | `Confidence` | 引擎 | 否 | 封闭枚举 |
| `ExportDiagnosis.possible_causes` | `&'static [&'static str]` | 模块内静态表 | 否 | 静态文案（源码扫描守护与引擎一致） |
| `ExportDiagnosis.verification_steps` | `&'static [&'static str]` | 模块内静态表 | 否 | 静态文案 |
| `ExportDiagnosis.limitations` | `&'static [&'static str]` | 模块内静态表 | 否 | 静态文案 |
| `ExportDiagnosis.related_targets` | `Vec<SafeTargetLabel>` | 引擎 `related_target_ids` | 是 | 白名单标签；未命中 → `unknown-target` |
| `ExportDiagnosis.related_probe_types` | `Vec<ProbeType>` | 引擎 | 否 | 封闭枚举 |

重点排查结论：

- **代理地址 / 代理 URL / `user:pass@`**：导出 DTO 中**不存在**任何代理字段。`route_mode` 只是三值枚举；应用内 `ProxySnapshot`（含系统代理、WinHTTP、环境变量来源）只走 UI IPC `get_proxy_snapshot`，**不进入** `build_report` 的输入（`build_report` 的唯一入参是 `&DiagnosticRun`，该结构体没有代理字段）。`report_export.rs` 中与 proxy/host/url 相关的字符串仅为规则静态文案（如 `"用 no_app_proxy 模式对比…"`），不是数据字段。
- **系统/主机信息、用户名、环境变量**：DTO 无对应字段；导出模块不读环境变量、不读主机信息。
- **名字带 message / note / label / name / version / path / url / host 的字段**：`note` 是 `&'static str` 常量；`target` 是白名单标签；`app_version` 已白名单化（本次修复）；无 message/path/url/host 字段。
- **仍由数据库驱动但已被结构约束的字段**：`run_id`（UUID v4 规范化）、`started_at` / `finished_at` / `tested_at`（RFC3339 规范化）。二者属 T-2 明文允许的"规范化 run_id / 时间戳"。

### 11.3 列级金丝雀汇总（任务 3）

测试：`tests/v6_export_field_audit.rs::export_dto_column_canary_audit`。流程：隔离 `tempdir` 中用正式 `Storage::save_run` 写入一条字段尽量填满的基线记录（2 个目标、DNS/TCP/HTTPS 样本、200 与 401 响应、超时与 DNS 失败的 `summary`/`detail`、`manual_local_proxy` 与 `no_app_proxy`、一条 `settings` JSON 记录）→ 用 `SELECT name FROM sqlite_master WHERE type='table'` + `PRAGMA table_info` 枚举**全部 TEXT 列**（不写死列名）→ 每列复制一份基线库、`UPDATE` 该列为 `CANARY_<表>_<列>` → `get_run → build_report → save_report_atomic` → 读回两份文件 → 四份输出统一检查。

```
---- V6-R6 导出字段列级金丝雀汇总（共 19 轮）----
results.run_id => CLEAN
results.target_id => CLEAN
results.probe_type => READ_REJECTED
results.route_mode => READ_REJECTED
results.http_outcome => READ_REJECTED
results.error_kind => READ_REJECTED
results.error_summary => CLEAN
results.error_detail => CLEAN
results.tested_at => CLEAN
runs.id => READ_REJECTED
runs.started_at => CLEAN
runs.finished_at => CLEAN
runs.status => READ_REJECTED
runs.route_mode => READ_REJECTED
runs.app_version => CLEAN
runs.notes => CLEAN
settings.key => CLEAN
settings.value_json => CLEAN
settings.value_json (JSON) => CLEAN
---- 合计：CLEAN 12 / READ_REJECTED 7 / BUILD_REJECTED 0 ----
---- 说明：build_report 为不可失败签名，导出采用白名单替换而非报错，故 BUILD_REJECTED 恒为 0 ----
```

READ_REJECTED 的 7 列都是**结构/枚举约束列**：`runs.id`（按原 id 查不到记录）、`runs.status`、`runs.route_mode`、`results.probe_type`、`results.route_mode`、`results.http_outcome`、`results.error_kind`（枚举字面量非法 → `get_run` 返回错误）。这些列的任何非法值都在读取阶段被拒绝，不可能进入导出。

CLEAN 的 12 列覆盖了所有自由文本列：`runs.app_version`（白名单）、`runs.notes`（不导出）、`results.error_summary` / `error_detail`（不导出）、`results.target_id`（白名单标签）、`results.tested_at` / `runs.started_at` / `finished_at`（时间戳规范化）、`results.run_id`（无匹配样本 → 空目标集）、`settings.key` / `value_json`（不参与导出）。

补充用例：

- `run_id_canary_in_database_is_not_exported`：`runs.id` 在列级审计中因"按原 id 查找失败"而 READ_REJECTED，故另按金丝雀 id 读取一次，证明 run_id 即使来自数据库也只会输出 `invalid-run-id`。
- `app_version_whitelist_rejects_v7_canaries`：直接复现 V7 的三个最小值。

### 11.4 TLS mock 偶发失败调查（任务 4，只调查不修改）

现象：Codex V7 首次 `cargo test --all` 在 `manual_https_proxy_full_tls_success_with_trusted_test_cert` 处报 OpenSSL 无法打开 `leaf.pem`，之后两次全量运行均通过。

**问题 1：证书文件写在哪里？**
`tests/common/tls_mock.rs:32-39`：目录为
`std::env::temp_dir().join(format!("adc-tls-mock-{}", SystemTime::now().duration_since(UNIX_EPOCH).as_nanos()))`，
即以"纳秒时间戳"命名的**系统临时目录子目录**；`openssl` 子进程以 `current_dir(work_dir)` 运行，`ca.key` / `ca.pem` / `leaf.key` / `leaf.csr` / `leaf.pem` / `*.der` 全部写在该目录内。生成结束后 `remove_dir_all(&work_dir)`（`tls_mock.rs:150`）删除整个目录。

**问题 2：多个测试是否共享同一路径或同一目录？**
设计上**不共享**——每次 `MockTlsServer::spawn()` 都重新生成一个带时间戳的目录。但目录名的时间戳并非真正的高精度唯一值：本机实测 2000 次连续取时（`DateTime::UtcNow`，与 `SystemTime` 同源）在 13.4 ms 内只产生 **3 个不同取值**，即系统时钟的实际粒度约 7 ms 量级。**只要两个 `spawn()` 落在同一个时钟刻度窗口内，就会得到完全相同的目录名**，于是两个测试共享同一目录；`create_dir_all` 对已存在目录不报错，冲突是静默的。

**问题 3：并行运行是否可能互相覆盖或提前删除？**
可能，且能精确解释观测到的症状。`proxy_modes.rs` 中有 3 处 `MockTlsServer::spawn()`（第 439、477、496 行），同一测试二进制内并行执行。一旦目录名相同：

1. 两个测试交替写 `ca.pem` / `leaf.csr` / `leaf.pem`（互相覆盖）；
2. 先完成的一方执行 `remove_dir_all(&work_dir)`，把另一方仍在使用的证书文件整体删除；
3. 另一方随后的 `openssl x509 -in leaf.pem -outform der …` 就会报"无法打开 `leaf.pem`"——与 Codex 观察到的报错一致。

因此根因是**目录名唯一性依赖粗粒度系统时钟 + 无并发隔离 + 结束后无条件删除**，属测试基础设施竞态；不是 TLS 或产品代码缺陷。触发概率低（需要两个 spawn 落在同一时钟窗口），与"三轮中仅首轮失败一次"的观察相符。

**建议的修复方向（本轮按指令不修改）**：目录名加入进程/线程唯一量（如 `std::process::id()` + 原子计数器，或用 `tempfile::TempDir` 保证唯一且延迟清理），并在生成失败时给出明确错误而非让 openssl 直接报缺文件。修复后应连续多轮全量运行验证。

### 11.5 R6 追加结论

**开发者自测通过，待 Codex 复验。**
