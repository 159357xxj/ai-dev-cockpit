# AI Dev Cockpit — Gate A 冻结验收契约（V6）

| 项 | 内容 |
|---|---|
| 契约版本 | GA-V6-1.0 |
| 冻结日期 | 2026-10-26 |
| 适用基线 | 本地 main `24e9e41` 之后的 V6 修复提交 |
| 前序结论 | V5：Gate A FAIL（V5-F07-TOCTOU、V5-F02-TARGET） |
| 开发执行 | ZCode |
| 独立复验 | Codex |
| 契约变更 | 仅经用户批准，且必须升级版本号；复验期间不得修改 |

## 0. 判定规则

1. Gate A = PASS，当且仅当本契约所有标记为 **[阻断]** 的不变量均有可复现的通过证据。
2. 任一 [阻断] 不变量被独立复现违反 → Gate A = FAIL。
3. 未执行的检查只能记为 NOT TESTED，不得记为 PASS；任一 [阻断] 项为 NOT TESTED → Gate A 不能 PASS。
4. 复验范围以本契约为准。契约外的新发现记入"观察项"，不影响本轮判定，**除非**它满足下方 P1 定义。
5. **P1 定义（唯一可新增阻断的条件）**：在生产调用链上可复现，且导致以下任一后果：(a) 敏感数据（凭证、个人路径、内网地址、原始错误文本、未审核目标名）进入导出或预览内容；(b) 用户已有文件被覆盖、截断或删除；(c) 用户 history.db 损坏或被非预期修改。仅在测试桩、开发入口或理论推演中成立的问题记为 P2。

## 1. 文件保存提交（关闭 V5-F07-TOCTOU）

已冻结的产品决策：**目标路径上已存在任何对象时，一律拒绝保存并提示用户换名。** 不提供覆盖选项。

| ID | 不变量 | 级别 |
|---|---|---|
| F-1 | 最终提交必须是原子的"不替换"操作（如 `tempfile::NamedTempFile::persist_noclobber`，或 `MoveFileExW` 不带 `MOVEFILE_REPLACE_EXISTING`，或 `hard_link` + 删除临时文件）。安全性不得依赖 `exists()` / `metadata()` 预检查；预检查可保留，但只能用于提前给出友好提示。 | [阻断] |
| F-2 | 目标路径已存在文件、目录、符号链接或 junction 时，提交失败并返回可区分的错误 `TargetExists`；目标的字节、长度和修改时间保持不变。 | [阻断] |
| F-3 | 竞态：在临时文件写完、提交之前，由其他线程或进程创建目标 → 本次提交返回 `TargetExists`，竞争者写入的内容保持完整。 | [阻断] |
| F-4 | 8 个线程用屏障同步，同时保存到同一目标：恰好 1 个成功，其余 7 个返回 `TargetExists`；最终文件内容等于获胜者的载荷（SHA-256 一致）。至少重复 200 轮，每轮都满足。 | [阻断] |
| F-5 | 提交返回成功以外的任何结果时，**不得**回退到 `std::fs::rename`、`fs::copy`、先删后写或任何其他可替换目标的路径。保存模块中不存在此类回退代码（由代码审查确认）。 | [阻断] |
| F-6 | 失败（包括 `TargetExists`、IO 错误、用户取消）时只清理本事务创建的临时文件，不删除、不截断任何其他路径。所有测试结束后不残留临时文件。 | [阻断] |
| F-7 | 临时文件位于目标同目录，使用独占创建（`CREATE_NEW` 语义）和随机名，带可识别前缀（如 `.adc-tmp-`）。 | [阻断] |
| F-8 | 成功保存的文件字节与该快照的预览字节完全一致（SHA-256 比对）。 | [阻断] |
| F-9 | 快照状态：成功 → `Consumed`；`TargetExists`、IO 失败或取消 → 回到 `Available`，用户可以换名重试，且重试内容与原预览字节一致。 | [阻断] |
| F-10 | 前端收到 `TargetExists` 时显示明确的"文件已存在，请选择其他文件名"，不得显示为成功或泛化错误。如果原生对话框无法关闭覆盖确认提示，可以接受"用户确认覆盖后被后端拒绝并提示"，此项的真实桌面表现留到 Gate B 验证。 | 后端：[阻断]；UI 表现：Gate B |

## 2. 目标标识导出（关闭 V5-F02-TARGET）

| ID | 不变量 | 级别 |
|---|---|---|
| T-1 | 只能通过固定白名单映射（`target_id → &'static str`）构造安全目标名。不在白名单中的值一律映射为固定占位符 `unknown-target`。 | [阻断] |
| T-2 | JSON 渲染器、Markdown 渲染器和预览只接收已净化的报告 DTO。DTO 中不得包含直接来源于探测数据、错误文本、数据库记录或用户输入的裸 `String`。允许的字段类型限于：安全目标名、`rule_id` 等枚举、数值、时间戳、规范化 run_id、静态文案。 | [阻断] |
| T-3 | 诊断标题由 `rule_id`、静态模板和安全目标名重新构造，不透传原始诊断标题或任何动态文本。 | [阻断] |
| T-4 | 对同一目标，JSON 与 Markdown 中的目标名完全一致（来源于同一份投影）。 | [阻断] |
| T-5 | 默认导出与预览不包含 `error.summary`、`error.detail` 或任何原始错误文本。 | [阻断] |
| T-6 | 导出字符串字段中不出现 CR、LF 或其他控制字符；Markdown 输出中不出现来源于数据的标题、链接、HTML 或代码块语法。 | [阻断] |

**金丝雀语料**（必须同时通过 ① 内存中构造的探测结果，以及 ② 写入隔离 SQLite 后再读出的历史记录，进入导出链路）：

| 类别 | 示例值 |
|---|---|
| 内部域名 | `canary-a1.corp.internal` |
| 私有 IPv4 | `10.13.37.42`、`192.168.77.7` |
| 私有 IPv6 | `fd12:3456:789a::42` |
| CRLF 注入 | `api.openai.com\r\n# CANARY_CRLF` |
| Markdown 注入 | `# CANARY_H1 [x](http://canary.invalid)` |
| 凭证样式 | `sk-CANARY0000000000`、`Cookie: CANARY=1` |
| 个人路径 | `C:\Users\CANARY_USER\` |

**断言**：对 JSON 预览、Markdown 预览以及实际保存的两种文件，逐一检查输出字节中不包含任何金丝雀子串（包括 `CANARY`、各 IP 和域名片段），且未知目标显示为 `unknown-target`。

## 3. run_id 校验

| ID | 不变量 | 级别 |
|---|---|---|
| U-1 | 使用 `uuid::Uuid::parse_str` 解析，并要求 `get_version() == Some(Version::Random)` 且 `get_variant() == Variant::RFC4122`。导出统一使用小写、带连字符的规范形式。 | [阻断] |
| U-2 | 不合法的 run_id（v1/v7、错误 variant、带花括号、`urn:` 前缀、长度错误、非十六进制字符）导出为固定占位符 `invalid-run-id`，不使整个导出失败，也不回显原值。 | [阻断] |
| U-3 | 兼容性：给出代码证据，证明生产环境中 run_id 由 `Uuid::new_v4()` 生成；若存在其他历史来源，需在 V6 报告中说明处理方式。 | [阻断] |

## 4. 工程检查

| ID | 检查 | 级别 |
|---|---|---|
| E-1 | `cargo fmt --all -- --check` 通过。 | [阻断] |
| E-2 | `cargo clippy --all-targets` 保持当前配置下通过。 | [阻断] |
| E-3 | 全部 Rust 测试通过；ignored 测试需逐一列出名称和原因。 | [阻断] |
| E-4 | 前端 lint、类型检查、测试、生产构建全部通过。 | [阻断] |
| E-5 | Tauri Release 与 NSIS 构建成功，记录安装包大小和 SHA-256。 | [阻断] |
| E-6 | CI 中 openssl 路径的设置与验证分到不同 step，或在同一 step 内同步更新 `$env:PATH` 并用 `Test-Path` 检查 `openssl.exe` 的绝对路径。Gate A 只做静态审查，真实运行结果属于 Gate C。 | 静态：[阻断]；运行：Gate C |
| E-7 | 前端偶发超时：连续运行 3 次，记录每次结果。不阻断 Gate A，作为 P2 跟踪。 | P2 |

## 5. 必须进入仓库的永久回归测试

所有测试使用隔离临时目录和隔离数据库，不读写真实 AppData，不访问外网。

| 测试（建议名称） | 覆盖 |
|---|---|
| `save_refuses_existing_file` / `_dir` / `_symlink` | F-2 |
| `save_race_target_created_before_commit`（通过仅测试可用的钩子，在提交前注入竞争者） | F-3 |
| `save_8_concurrent_exactly_one_wins`（≥200 轮） | F-4 |
| `save_failure_cleans_only_own_temp` | F-6、F-7 |
| `saved_bytes_equal_preview_bytes` | F-8 |
| `snapshot_retry_after_target_exists` | F-9 |
| `export_canary_corpus_json_md_preview_and_file` | T-1 至 T-6 |
| `export_canary_from_history_db` | T-1 至 T-6（经数据库路径） |
| `run_id_uuid_v4_strict` | U-1、U-2 |

Codex 在 V5 中使用的临时探针（覆盖竞态、8 并发、目标泄漏）应等价迁入上述测试，或原样保留在 `tests/` 下。

## 6. V6 修复报告必须包含的证据

`docs/RELEASE_REMEDIATION_REPORT_V6.md` 需包含：修复提交哈希；每个不变量 ID 对应的测试名称与代码位置；E-1 至 E-5 的完整命令及结果摘要（通过、失败、ignored 数量）；安装包大小与 SHA-256；明确列出的 NOT TESTED 项（至少包括 Gate B 全部内容、Gate C 全部内容、F-10 的 UI 表现）。开发者在报告中只能写"自测通过"，不得写"Gate A PASS"。

## 7. 不在本契约范围内

新功能、重构已有独立通过证据的模块（网络探测、代理入口、诊断规则 R01–R12、SQLite 保留策略、快照预算）、UI 美化、性能优化、Gate B 安装验收、Gate C 的真实 CI 运行与发布流程。

## 8. 操作边界（复验期间同样适用）

不推送 GitHub、不创建 Release、不改变仓库可见性；不安装、升级、卸载或启动正式实例；不读写真实 `history.db` 及其 WAL/SHM；不修改系统代理、DNS、TUN、防火墙；不删除任何未经用户核对的文件或目录（包括 `%TEMP%\adc-independent-v5-*`）。
