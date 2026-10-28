# Release Remediation Report V3（第二轮 FAIL 专项修复）

修复日期：2026-10-25（Asia/Shanghai）
依据：`docs/RELEASE_ACCEPTANCE_REPORT_V2.md`（F-02 未关闭、F-07 新增、F-04 部分仍在、OpenSSL 环境可移植性两轮记录）
修复提交：`68a5e59`（本地 main，领先 origin/main 5 个提交，未推送）
约束遵守：未启动接触用户真实 `history.db` 的程序实例、未安装/卸载、未修改系统网络配置；全部新测试使用假凭证与内存/隔离资源。

## 1. F-02 / F-07 / F-04 处理状态

| ID | V2 状态 | V3 状态 |
|---|---|---|
| F-02 导出脱敏 | FAIL（六类泄漏/部分泄漏） | **已修复**：sanitize_text V2 重写 + 结构化白名单导出 + 保守截断；QA 六场景全部由失败转通过；性质测试 3 项（600+ 随机输入）无 panic/无死循环/长度有界/已知秘密不漏 |
| F-07 预览≠导出 | FAIL（exported_at 重新生成） | **已修复**：服务端不可变 ReportSnapshot；预览与导出复用同一份脱敏字节；字节一致回归通过 |
| F-04 文档一致性 | 部分 FAIL | **已修复**：README 状态表重写、测试数 119+35、R11 冲突消除；ARCHITECTURE 补 `format` 与 `snapshot_id` 签名 |
| OpenSSL 可移植性 | 两轮记录 | **已解决 OPENSSL_CONF 配置路径问题**：TLS mock 生成最小 `openssl.cnf` 并以进程级 `OPENSSL_CONF` 传给子进程；`env -u OPENSSL_CONF` 下 13 项测试全绿。~~无 Conda/Git Bash 依赖~~ **（V3 QA 更正：openssl.exe 本身仍是 PATH 上的外部依赖，本机来自 Miniconda；已在 V4 修复中于 CI 显式定位 Git for Windows 自带的 openssl 并在 README 如实声明）** |

## 2. 每项修复的文件与核心改动

### F-02（`src-tauri/src/security/redaction.rs` 全面重写 + `rules.rs` + `report.rs`）

1. **根因消除——索引安全**：旧实现用 `to_lowercase()` 的字节索引切割原文（İ U+0130 小写展开 2 字节 → 错位）。V2 所有匹配改为 `find_ascii_ci`：在**原文字节**上做 ASCII 大小写不敏感窗口匹配（needle 全 ASCII → 命中边界必为 UTF-8 字符边界）；`from` 超界直接返回 `None`（禁止回退钳制——修复中发现回退钳制会引发 `cursor > start` 反向切片 panic，对抗测试捕获）。
2. **折叠 Cookie 行**：`mask_headers_folded` 值边界吸收以空白开头的续行（RFC 5322 折叠）。
3. **多 `@` userinfo**：`mask_url_userinfo` 按 authority 段内**最后一个** `@` 遮蔽（`://` 到 `/ ? # 空白` 之间），覆盖 `u:pass@inner@host` 任意层数。
4. **第二个及后续 Windows 路径**：`mask_all_user_paths` 对 `C:\Users\` / `/home/` / `/Users/` 循环遮蔽至无匹配。
5. **Unicode 前缀查询键**：ASCII CI 匹配天然命中 `İ?token=`（İ 是多字节，needle `?token=` 的命中位置不受影响）。
6. **IPv6 整段遮蔽**：`scan_ipv6_token` + `Ipv6Addr` 解析器验证（时间串 `12:34:56` 天然拒绝）——全部合法 IPv6 字面量整段 `[内网地址]`，无前缀残留。
7. **R04 污染通道关闭**：`rule_r04` 的 observed_facts 改为**结构化计数**，不再嵌入原始错误摘要（rules.rs）。
8. **保守截断**：`sanitize_and_cap`——超 300 字符的自由文本截断并标注 `[已截断]`（指令 #9）。
9. 头识别边界从 ASCII 白名单改为**无边界保守遮蔽**（QA：随机前缀 `9Cookie:` 曾漏判；误伤面仅为导出文本中的自定义 `xxxCookie:`，可接受）。

### F-07（`src-tauri/src/commands/report.rs` + `lib.rs`）

- `SnapshotStore`：`VecDeque<ReportSnapshot>`（snapshot_id/run_id/format/content/filename/created_at/exported）；插入时淘汰过期（TTL 600s）/超量（8 个）/超字节（8 MiB 总量）；**只存脱敏内容**。
- `preview_sanitized_report`：生成一次完整脱敏内容 → 存入快照 → 返回预览 + 不透明 `snapshot_id`（UUID v4）。
- `export_sanitized_report`：新增 `snapshot_id` 参数；`take` 按 id/run/format/未导出/TTL 校验，**取出即失效**（单次使用）；写入字节 = 预览字节。用户取消（另存为取消）→ 快照放回队列可重试；历史数据变化不影响已确认预览。无效/过期/重复使用 → `invalid_input` 明确错误。
- settings 表记录一次性快照 id，导出后清理（`delete_setting` 新增）。

### F-04（README / ARCHITECTURE / TEST_PLAN）

- README 状态表重写：P3/P4/P5 全部移入"已实现"列；测试数 119+35；R11 标注"已修复关闭"（消除与 ARCHITECTURE 的冲突）。
- ARCHITECTURE：`preview_sanitized_report(run_id, format) -> ReportPreview`（含 snapshot_id）与 `export_sanitized_report(run_id, format, snapshot_id)` 签名补全。
- TEST_PLAN：绝对测试计数改为动态引用（避免再过期）。

### OpenSSL 可移植性（`tests/common/tls_mock.rs`）

`generate_certs` 在临时目录生成最小 `openssl.cnf`，并以**进程级** `OPENSSL_CONF` 环境变量传给 openssl 子进程（`Command::env`，不改系统环境）。验证：`env -u OPENSSL_CONF cargo test --test proxy_modes tls` → 3/3 通过——干净 Windows 环境不再依赖 Conda/Git Bash 配置路径。

## 3. 失败测试复现情况

| V2 QA 场景 | 修复前 | 修复后 |
|---|---|---|
| 折叠 Cookie 行 `QA_FOLDED_COOKIE_11C` | 泄漏 | ✅ 遮蔽（含续行 `QA_FOLDED_MORE`） |
| 多 `@` userinfo `QA_URL_PASS_12D` / `QA_MULTI_PASS_15G` | 泄漏 | ✅ 遮蔽（3 种形态全过） |
| 第二 Windows 路径 `qa-two`（及 `qa-three`） | 泄漏 | ✅ 全部遮蔽 |
| Unicode `İ?token=QA_UNICODE_16H` 部分泄漏 | `Q` 残留 | ✅ 全遮蔽 |
| IPv6 `fd12:3456:789a::1` 前缀残留 | `fd12:3456:789a` 残留 | ✅ 整段 `[内网地址]`（`fe90::1` 同） |
| R04 事实携带原始摘要 `QA_R04_COOKIE_99Z` | 进 JSON+MD | ✅ 结构化计数化，零泄漏 |
| 预览/导出字节不一致（exported_at） | 不一致 | ✅ 快照复用，字节一致 |

**测试驱动过程**：先写 6 个 QA 场景测试（4 项失败复现）→ 重写 sanitize_text → 全部转绿；对抗测试又捕获 `find_ascii_ci` 回退钳制 panic → 修复 → 性质测试通过。

## 4. 自动化测试统计（2026-10-25 实际执行）

| 命令 | 退出码 | 结果 |
|---|---:|---|
| `npm run lint` / `typecheck` | 0 / 0 | 通过 |
| `npm run test` | 0 | **35/35** |
| `npm run build` | 0 | 通过 |
| `cargo fmt --check` | 0 | 通过 |
| `cargo clippy --all-targets -- -D warnings` | 0 | 0 错误 |
| `cargo test`（无 OPENSSL_CONF） | 0 | **127 项全绿**：40 单元 + 19 E2E + 13 代理 + 31 规则 + 8 存储 + **16 导出回归（新增 9：6 QA 场景 + 3 性质）**；3 公网 ignored |
| `npm run tauri build` | 0 | NSIS 2.66 MiB（见 §5） |

## 5. 安全导出架构（摘要）

```
DiagnosticRun（内部对象，绝不直接序列化）
        │ diagnose() 在原始数据上求值
        ▼
secure_run_json / secure_diagnoses_json（逐字段白名单挑选）
        │ 自由文本 → sanitize_and_cap（sanitize_text + 300 字符保守截断）
        ▼
serde_json::json! 白名单结构 / Markdown 模板（同样经 sanitize_and_cap）
        ▼
ReportSnapshot（服务端，只存脱敏字节；UUID/TTL/限额/单次使用）
        ▼ preview 返回 (content, snapshot_id)
export(snapshot_id) → 原生另存为（Rust 侧）→ 写入 = 预览字节
```

## 6. 预览与导出字节一致性证据

`regression_f07_snapshot_bytes_equal_preview_and_survive_history_change`：
1. 生成预览字节 `content1` 并存入快照；
2. sleep 15ms 后**修改原始历史数据**重新 build（确认重生成内容确实不同）；
3. 从快照取出字节 `snapshot.content`；
4. 断言 `snapshot.content == content1`（**完全一致，不受历史变化影响**）✅；
5. 取出后快照失效（防重复导出）✅。
JSON 与 Markdown 均走同一 `SnapshotStore` 路径。

## 7. 最新安装包（替换 V2 报告哈希）

| 项 | 值 |
|---|---|
| Git Commit | `68a5e59`（本地 main，ahead 5，未推送） |
| 版本 | 0.1.0 |
| 绝对路径 | `E:\ai-dev-cockpit\src-tauri\target\release\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe` |
| 大小 | 2,788,103 字节（2.66 MiB） |
| **SHA-256** | `03F9112D287621BA71251F99A6A603864FCC8DC9EC859C84C7BD4EE219D20E01` |
| 构建 | `npm run tauri build` 退出码 0；由含 F-02/F-07 修复的 `68a5e59` 构建 |

## 8. 未执行的人工验收（保持 NOT TESTED）

安装向导/升级/卸载全流程、Release UI 三模式真实运行、另存为对话框真实交互、当前用户真实历史重启、断网首启、推送后 CI 首跑、公网诊断——均等待隔离环境（Sandbox/VM）与授权后执行。

## 9. 剩余安全风险

1. 未签名（SmartScreen）；2. 未做漏洞数据库审计；3. 脱敏规则集为保守已知形态，**不能对任意输入保证零泄漏**——结构性缓解为逐字段白名单 + 保守截断 + 127 项自动化守护，性质测试提供统计置信但非完备证明；4. `rfd` 另存为可写任意用户可写路径（功能本意，内容已脱敏）。

## 10. 结论

F-02/F-07/F-04 与 OpenSSL 可移植性已修复并有测试证据；**具备重启 Release Acceptance Test（第三轮）的条件**。等待独立 QA 复验；v1.0 未发布。
