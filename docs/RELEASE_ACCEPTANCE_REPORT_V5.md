# AI Dev Cockpit — Release Acceptance Report V5

独立验收日期：2026-10-26（Asia/Shanghai）

仓库：`E:\ai-dev-cockpit`

范围：安全 DTO/脱敏、文件提交原子性与用户文件保护、报告快照状态机与资源预算、SQLite 遗留键清理、OpenSSL/CI 静态复核、质量基线及 NSIS Release 构建。

执行边界：未改产品源码；未安装、启动、升级或卸载正式应用；未访问真实用户数据库；未 push、创建 Release 或修改系统网络设置。

## 发布判定

### Gate A：安全导出、文件保护和快照可靠性

# **FAIL**

阻断依据：Windows 实测表明 `std::fs::rename` 会覆盖在预检查后由其他进程创建的目标文件。独立竞态探针中，竞争者先创建并写入目标文件，`save_report_atomic` 随后仍返回成功并覆盖它；8 个并发保存同一路径的调用也全部返回成功。另有独立报告探针确认未白名单 `target_id` 可经 Markdown 标题及 JSON 诊断标题泄漏/注入。二者均未被 V5 既有回归测试捕获。

快照状态迁移、TTL、单次消费、8 份/8 MiB 上限和 Arc 内容共享的独立探针未发现新的状态机/预算失败；这些通过项不能抵消文件覆盖与导出泄漏的阻断问题。

### Gate B：Windows 安装及实际桌面功能

# **NOT TESTED**

只构建了 x64 NSIS 候选包，没有运行安装器或产品 EXE。安装界面、默认路径、管理员提示、开始菜单、WebView2、桌面渲染、主题切换、真实安装版 SQLite 持久化、原生另存为交互及卸载均未验证。遵守本轮禁止安装/启动/卸载正式应用的边界。

### Gate C：GitHub CI 与发布资格

# **NOT TESTED**

HEAD 尚未推送，未找到本轮提交对应的 GitHub Actions 运行记录。CI 结果为 **NOT TESTED — 等待推送后实际运行**。静态审查发现 OpenSSL 路径步骤只写入 `GITHUB_PATH`，却在同一步立即执行 `Get-Command openssl`；GitHub 文档说明该路径变更供后续步骤使用，创建该变更的当前步骤不能使用新值。因此当前步骤的探测不能证明刚添加的目录已进入当前进程 PATH；应显式检查目标 exe 或同步更新当前 `$env:PATH`。这是静态配置发现，不是 GitHub CI 失败实测。[GitHub Actions workflow commands 文档](https://docs.github.com/en/actions/writing-workflows/choosing-what-your-workflow-does/workflow-commands-for-github-actions#adding-a-system-path)

## 1. 验收环境、Git 与候选包

| 项目 | 本轮现场实测 |
|---|---|
| OS | Windows 11 家庭中文版，10.0.22631，Build 22631，x64 |
| Node.js / npm | v24.19.0 / 11.17.0 |
| rustc / cargo | 1.97.1 / 1.97.1，MSVC 主机 |
| 分支 / HEAD | `main` / `24e9e41b0ee61f51c19605974d8efd98811f0e8c` |
| 产品修复提交 | `94bd645e6da5ca3cc70be560d76d91c25e477a6e` |
| 分支状态 | `main...origin/main [ahead 10]`；未推送 |
| HEAD 相对产品修复提交的文件变化 | 仅 `docs/RELEASE_REMEDIATION_REPORT_V5.md`；产品源码差异为空 |
| 锁文件 | `package-lock.json` 与 `src-tauri/Cargo.lock` 均存在 |
| `git diff --check` | 退出码 0，无 whitespace 错误 |

`npm run tauri build` 实际构建成功，Tauri 输出 `Target: x64`。因 HEAD 相对产品修复提交只改了文档，以下构建源码与产品修复提交 `94bd645` 一致。

| 候选文件 | 实测值 |
|---|---|
| NSIS 绝对路径 | `E:\ai-dev-cockpit\src-tauri\target\release\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe` |
| 大小 | 2,794,078 字节（约 2.66 MiB） |
| SHA-256（本轮 `Get-FileHash -Algorithm SHA256`） | `08017273D4769B55AABB3C0F477289045AB6BE4F6F2688F8D192C5B0FE37D533` |
| FileVersion / ProductVersion | 0.1.0 / 0.1.0 |
| Authenticode | `NotSigned`（`Get-AuthenticodeSignature`） |
| Release 主程序 | `E:\ai-dev-cockpit\src-tauri\target\release\ai-dev-cockpit.exe`，7,445,504 字节，SHA-256 `22C0C0D85467A1DD4C1AADFEF3824E4BC425A1089253D7380AE44EF26F4A8971` |

V5 报告中的旧安装包哈希 `8BB0806797A784DF6ED2B45708822ECC0584BE7BA950A4AEC4D1626FFD8A9BF4` 与本轮重建结果不同。本报告采用本轮真实构建后的哈希。没有运行安装器，也没有验证 SmartScreen/安装签名体验。

## 2. 独立 F-07 文件事务复验

审查位置：[report.rs](../src-tauri/src/commands/report.rs#L478)

### 通过项

- 临时文件确实使用 `OpenOptions::create_new(true)` 独占创建（`report.rs:492-500`）。
- 临时文件写入、flush、sync 或提交报错后，清理路径只调用 `remove_file(&temp_path)`；失败清理不删除目标文件（`report.rs:518-522`）。
- `finish_failure` 只恢复快照，不再接受删除用户路径的行为；传入的旧参数被忽略（`report.rs:442-459`）。
- 目标在函数进入时已存在的常规情形会被拒绝；隔离临时目录中已有文件的字节保持不变。新文件内容与预览逐字节相等。
- 写入使用快照中的同一 `Arc<str>` 内容；独立探针对照预览字节和成功保存文件字节完全相同。

### F-07-DATA：竞态覆盖用户文件 — **FAIL / P1 发布阻断**

代码先在 `report.rs:488` 调用 `target.exists()`，然后写入临时文件，最后在 `report.rs:515` 调用普通 `std::fs::rename`。Windows 运行实测证明：该 `rename` 在目标于预检查后出现时会替换目标。普通预检查与后续提交之间存在 TOCTOU 窗口，当前逻辑不是原子 no-replace。

**最小复现及实际结果：**临时集成探针 `qa5_atomic_save_rejects_competitor_created_after_precheck` 在隔离目录启动 `save_report_atomic` 写 8 MiB 内容；检测到 `.adc-tmp-*` 后（已越过 `target.exists()` 检查）另一线程用 `create_new(true)` 在目标路径写入 `KEEP-COMPETITOR-ORIGINAL`。初始回归断言失败；随后确认探针 `read_back_confirms_competitor_file_is_replaced` 直接读回文件，观察到 `save_result=Ok(())`、`final_len=8388608`、`final_is_report=true`、`final_is_original=false`。并发探针 `qa5_same_path_concurrent_exporters_at_most_one_commit` 用 barrier 同时启动 8 个 2 MiB 写入，8/8 返回成功；独立读回探针确认最终文件来自成功 writer 4。违反“最多一个提交成功”。所有输入/文件都位于独立临时目录。

普通“目标不存在检查 + rename”不满足覆盖保护。需要平台提供的原子 no-replace 提交语义；在修复并重跑上述竞态前，不应接受目标路径安全保证。写入途中真实磁盘满/ACL 拒绝未模拟；现有父路径错误用例是在创建临时文件前失败，不证明写入中断场景。

### 临时文件及预览字节

同目录临时文件的 create_new 与清理归属逻辑源码清晰；目标不存在的成功保存及预览字节相等通过独立临时目录探针。目标在竞态提交时被覆盖，因此“竞争者文件错误/取消/权限失败后逐字节不变”整体不通过。权限不足和取消的原生 Save As 端到端场景没有运行。

## 3. 独立 F-02 安全 DTO 与自由文本复验

审查位置：[report.rs](../src-tauri/src/commands/report.rs#L65)、[redaction.rs](../src-tauri/src/security/redaction.rs#L83)。

### 通过项

- 原始 `error.summary` / `error.detail` 不出现在 JSON/Markdown 默认报告。独立假凭证 `Authorization: QA5_SECRET`、`Cookie: QA5_COOKIE`、`api_key=...` 均未出现在两个格式中。
- 非法 run_id `../../run\r\n# ...` 投影为 `invalid-run`，生成固定安全文件名；非法 started/finished/tested 时间变为 `invalid-timestamp`；含换行/标题注入的 app_version 变为 `unknown`。JSON 可正常解析。
- 统计字段保留：独立恶意报告中的 `attempts_total=1`、`transport_failure_count=1` 及失败率仍在 JSON 中。
- `C:\Users`、短路径、Unicode 边界等 `sanitize_text` 输入的 panic 检查通过。
- 从历史源码 `c5fa054` 可确认生产 scheduler 使用 UUID v4；合法历史 v4 形态仍能导出。没有发现生产生成器曾使用非 UUID 格式；测试夹具中的 `run-1` 不能当作真实历史数据证据。

### F-02：不可信 target_id 越过导出投影 — **FAIL / P1**

`secure_run_json` 的 target 字段及 `related_target_ids` 用 `safe_target_id`，但诊断标题仍由 `sanitize_and_cap(&diagnosis.title)` 导出（`report.rs:186`），而 Markdown 目标统计标题直接插值原始 `target.target_id`（`report.rs:235-236`）。`sanitize_text` 是部分脱敏器，不会移除全部控制字符/标记语法；`safe_target_id` 没有用于 Markdown 标题或诊断 title 的来源 ID。

**独立最小复现：**临时探针 `independent_non_whitelist_internal_target_probe` 构造 `target_id = "vault.corp.example\r\n# QA5_INTERNAL_CANARY [x](javascript:qa5())"`，并以 HTTPS Timeout 触发 R04/R12。实际 JSON 中 `related_target_ids` 是 `[未知目标]`，但 `diagnoses[].title` 仍包含换行及 `QA5_INTERNAL_CANARY`；Markdown 既产生注入的新标题行，也把未白名单 host 原样写入目标标题。探针断言泄漏存在后通过（1 passed），这是“确认缺陷”的测试，不代表产品安全通过。

另一个独立报告探针输入 `192.168.44.9` 与 `fd12:3456:789a::1` 作为目标 ID；Markdown 文件实际包含 `### 192.168.44.9` 和 `### fd12:3456:789a::1`，而 JSON 的安全目标字段不保留原始目标 ID。相关探针 `private_target_addresses_are_still_emitted_in_markdown_heading` 以检测到泄漏为预期后通过（1 passed）。

诊断标题的 Markdown 渲染会转义部分元字符，但这不能修复 JSON 诊断标题的控制字符/原始 ID 泄漏，也不能修复上面的目标统计标题。建议来源标识在任何报告格式进入渲染前都投影为安全目标名；未白名单自由文本应省略或用固定占位符，不应依赖 sanitizer 的替换列表。

### F-02：UUID v4 位约束不完整 — **FAIL / P2**

`safe_run_id`（`report.rs:65-74`）只检查长度、连字符位置和十六进制字符，没有校验 UUID version nibble 为 `4` 或 RFC variant 位。独立探针 `qa5_uuid_version_and_historical_id_behavior` 输入 `018f4e5d-238b-1bce-1ce2-dbf71cf71001`（version/variant 位均不符合 UUID v4）后，`build_report` 仍原样输出该 ID 且将其用于文件名；断言“应拒绝”失败。它不含路径穿越字符，未单独证明文件覆盖或路径穿越，但与 V5 所称严格 UUID v4 校验不一致。真实生产历史 UUID v4 兼容性从 scheduler 源码确认；实际生产 DB 未读取。

## 4. 独立 F-07 快照生命周期与资源预算

审查位置：[report.rs](../src-tauri/src/commands/report.rs#L312)。以真实 `SnapshotStore` 和 `save_report_atomic` 在临时测试数据上执行；临时 probe 源文件均已从仓库移除，不计入既有测试数。

| 检查项 | 独立结果 | 证据 |
|---|---|---|
| 新快照立即可消费 / 多快照并存 | PASS | `qa5_snapshot_lifecycle_budget_and_arc_sharing` 插入并取出两份不同快照 |
| 并发 take 仅成功一次 | PASS | 8 个线程同一 barrier 竞争，恰 1 个成功 |
| 无效 ID、重复消费、错误格式 | PASS | 不匹配 ID/状态被拒；既有 `f07_invalid_wrong_run_wrong_format_rejected` 也通过 |
| 取消/失败后重试 | PASS | release 与 finish_failure 后同内容可再次 take |
| 成功后不可重用 | PASS | finish_success 后相同 snapshot_id 被拒 |
| TTL 到期 | PASS | 将 Available 快照时间回拨 601 秒后 take 拒绝并释放 Available 项 |
| 单份大于 8 MiB | PASS | 8 MiB+1 内容插入被拒 |
| Available + InFlight 统一预算 | PASS | 8 份 × 1 MiB 取为 InFlight 后，再插入一份被拒；InFlight 不被淘汰 |
| Arc clone 内容共享 | PASS | 生产 `take` 返回的 Arc 与输入分配地址相同；持有 1000 个额外 Arc clone 后仍指向同一 payload 分配（测试 `production_store_take_clones_arc_without_copying_payload`） |
| 取消/保存后的字节一致 | PASS | 独立快照与生产保存核心写入文件逐字节比较 |
| 原生另存为真实 UI | NOT TESTED | 未启动桌面应用或原生 FileDialog |

本轮没有证明用户保持 FileDialog 打开超过 TTL 时 InFlight 会被自动回收；实现将 InFlight 留在预算中直至完成、失败或取消，此行为没有作为失败认定。Gate A 仍因文件提交 TOCTOU 与 F-02 泄漏判 FAIL。

## 5. SQLite 初始化清理

审查位置：[storage/mod.rs](../src-tauri/src/storage/mod.rs#L54)。只使用单独临时数据库，没有打开、复制、哈希、迁移或启动会访问真实用户 `history.db` 的实例。

独立测试 `cleanup_warning_preserves_history_and_unrelated_settings` 启动时安装 SQLite DELETE 触发器，让 `report.snapshot.%` 清理故意触发 `ABORT`，随后重新 `Storage::open`：

- `--nocapture` 输出真实 stderr：`[storage] 非关键维护失败：清理遗留快照键未执行（数据保留、启动继续）：qa5 injected maintenance failure`。
- `report.snapshot.legacy` 与 `retention.max_runs=500` 都保留。
- 已保存的一条 history run 和一个 sample 均能重读，`run_count=1`。
- 测试退出码 0，1 passed、0 failed、0 ignored。

遗留键清理失败可观测、无关设置和历史记录保持：**PASS**。重复初始化幂等与正常清理另由既有 storage 全量测试覆盖（8/8 通过）。

## 6. OpenSSL 与 GitHub CI

- 本轮环境 `OPENSSL_CONF` 未设置；本机实际 `openssl.exe` 是 `E:\Miniconda3\Library\bin\openssl.exe`，版本 3.5.9.0。Rust 全量测试在此条件下通过，但依赖外部可执行文件，不能宣称测试与外部 OpenSSL 无关。
- workflow 的 OpenSSL 步骤位于 `.github/workflows/ci.yml:43-51`，含 `Get-Command openssl` 和缺失时 `exit 1`，失败路径本身会使步骤失败；但 `.github/workflows/ci.yml:46` 的 `GITHUB_PATH` 写入仅影响后续步骤，不影响同一步的立即检查。静态配置不够可靠，尚无 Windows Actions 实测。
- 静态可见：Windows runner、Node 22、Rust stable + rustfmt/clippy、前端 lint/typecheck/test/build、Rust fmt/clippy/test、`permissions: contents: read`；构建 job 仅 workflow_dispatch，上传 artifact，不自动创建 Release。
- GitHub Actions 实际运行：**NOT TESTED — 本地 HEAD 未推送，无本轮 Actions run**。

## 7. 质量基线实际记录

以下为本轮在仓库中执行。前端测试第一次因两个 UI 测试超过 5 秒超时失败；未更改源码/超时，低负载重跑通过。所有数量均为实际 runner 输出。

| 命令 | 退出码 | 实际结果 |
|---|---:|---|
| `npm run lint` | 0 | ESLint 完成，无输出错误 |
| `npm run typecheck` | 0 | `tsc --noEmit` 通过 |
| `npm run test`（第 1 次） | 1 | 9 files：7 passed、2 failed；35 tests：33 passed、2 failed。`navigation.test.tsx` 与 `demo-mode.test.tsx` 各自超过 5000 ms |
| `npm run test`（重跑） | 0 | 9/9 files、35/35 tests passed，4.76 秒；无 ignored |
| `npm run build` | 0 | TypeScript + Vite 生产构建成功，2,630 modules，无 Vite warning |
| `cargo fmt --check` | 1 | 格式 diff 出现在 `tests/f07_snapshot.rs` 和 `tests/v5_file_and_limits.rs`；未运行格式化或改源码 |
| `cargo clippy --all-targets -- -D warnings` | 0 | 通过，无 warning/error |
| `cargo test --no-fail-fast` | 0 | **148 passed、0 failed、3 ignored**；ignored 为需明确授权联网的 3 个公网测试；main/doc test 为 0 |
| `npm run tauri build` | 0 | x64 NSIS 构建成功，安装包 2,794,078 字节；见 §1 |

Rust 148 passed 分组为 37 unit + 16 export regression + 10 snapshot + 19 E2E + 13 proxy + 31 rules + 8 storage + 14 V5 regressions。Cargo 测试未打印编译警告。Release 构建提示 bundle identifier `com.aidevcockpit.app` 以 `.app` 结尾（macOS 命名建议），并有一条 MSVC linker stdout warning（生成 `.lib/.exp`）；构建成功。

### 独立探针计数（不计入 148 项既有质量基线）

| 临时探针 / 命令 | 实际输出 | 退出码/解释 |
|---|---|---|
| `qa_v5_independent` / `cargo test --test qa_v5_independent -- --nocapture` | 9 tests：5 passed、4 failed、0 ignored。失败断言分别复现 Markdown target 注入、非 v4 UUID 接受、预检查后竞争者文件被覆盖、8 并发保存均成功 | 1；失败用于暴露缺陷 |
| `qa_v5_target_leak` / `cargo test --test qa_v5_target_leak -- --nocapture` | 1 passed；打印 JSON/Markdown，确证 R04/R12 JSON title 含 CRLF 与目标 canary、Markdown 目标统计含攻击标题 | 0；探针以发现泄漏为预期 |
| `qa_v5_internal_target` / `cargo test --test qa_v5_internal_target -- --nocapture` | 1 passed；断言非白名单内部 host/canary 从不安全字段泄出 | 0；探针以发现泄漏为预期 |
| `qa_v5_ip_export` / `cargo test --test qa_v5_ip_export -- --nocapture` | 1 passed；确认私有 IPv4/IPv6 被 Markdown 标题原样输出 | 0；探针以发现泄漏为预期 |
| `qa_v5_overwrite_confirmation` / `cargo test --test qa_v5_overwrite_confirmation -- --nocapture` | 2 passed；读回 `final_len=8388608`、`final_is_report=true`、`final_is_original=false`；8 个并发调用成功，最终文件对应 writer 4 | 0；探针以复现覆盖为预期 |
| `qa_v5_sqlite` / `cargo test --test qa_v5_sqlite -- --nocapture` | 1 passed、0 failed；stderr 警告可见，history sample 和无关 settings 保留 | 0 |
| `qa_v5_arc` / `cargo test --test qa_v5_arc -- --nocapture`（最终无警告运行） | 1 passed、0 failed；同一 Arc payload 的 1000 次 clone 共享同一地址 | 0 |

临时 probe 源文件已删除。它们的临时目录不计入既有质量基线；target/internal/IP/overwrite 探针的 passed 表示成功复现缺陷，不是产品 Gate 通过。

## 8. 问题清单与严重度

| ID | 严重度 | 状态 | 证据/位置 |
|---|---|---|---|
| V5-F07-TOCTOU | **P1 发布阻断** | 未关闭 | `report.rs:488-515`；独立竞争创建目标后普通 rename 覆盖已有文件；8 路同目标全部成功 |
| V5-F02-TARGET | **P1 发布阻断** | 未关闭 | `report.rs:186,235-236`；诊断 title 的不可信 target_id 可残留；Markdown heading 直接输出原始目标 ID，已用内部 host、IPv4、IPv6 和注入换行复现 |
| V5-F02-UUID | P2 | 未关闭 | `report.rs:65-74`；没有校验 UUID v4/variant 位，UUID 形状非法值仍被接受。路径穿越形态则已投影为 `invalid-run` |
| V5-FMT | P2 | 未通过 | `cargo fmt --check` 退出 1，两个已提交 Rust 测试文件格式不符合 rustfmt |
| V5-UI-TEST-FLAKE | P2 / 非确定性待观察 | 有一次失败、重跑通过 | 首次 Vitest 2 项 5 秒超时，重跑 35/35。不能据一次通过排除时序波动 |
| V5-CI-OPENSSL-PATH | P2 静态风险 | 未经 Actions 验证 | `.github/workflows/ci.yml:46-51` 在同一步写入 GITHUB_PATH 后立即查当前 PATH；没有直接检查 Git `usr\bin\openssl.exe` 文件 |
| SQLITE-CLEANUP | — | PASS | `storage/mod.rs:58-64`；故障可观测，数据未丢，独立临时 DB 测试通过 |
| 真实用户数据库 | — | NOT TOUCHED | 没有访问/启动可能打开该数据库的程序；当前用户数据没有本轮前后快照，本轮不对其作完整性声明 |

## 9. 未完成验证与副作用记录

- 安装/升级/卸载、真实 Release UI、主题、主界面、真实安装版本地数据库读写、原生对话框取消/覆盖/权限失败：**NOT TESTED**。
- GitHub Actions 对当前 HEAD：**NOT TESTED**；公网诊断未执行；系统网络配置未更改。
- 没有产品源码修改、安装、启动、升级、卸载、push 或创建 Release。
- 独立竞态探针失败前在 `%TEMP%` 留下 4 个仅由本轮创建的 QA 临时目录（前缀 `adc-independent-v5-`，合计约 20 MiB，内容为重复字符/测试输出，无真实凭证或用户数据）。通过 shell 清理这些已核对的临时目录时，运行环境策略拒绝了删除命令；因此它们仍待本机清理。探针源码已从仓库移除；除本报告外没有未跟踪或修改的产品文件。

## 最终结论

**当前版本不可发布。** Gate A 为 **FAIL**，因为同目录 `rename` 存在已实测的目标覆盖竞态，且 target_id 从诊断 title/Markdown heading 泄漏。Gate B 和 Gate C 均为 **NOT TESTED**；本地 Release 构建、自动化测试通过不等同于安装/桌面验收或 GitHub CI 通过。需要先修复并独立重跑 Gate A，再由获授权的隔离 Windows 环境完成 Gate B，并在推送后等待真实 Actions 结果，才能重新作发布判定。
