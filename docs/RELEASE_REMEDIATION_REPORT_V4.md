# Release Remediation Report V4（第三轮 FAIL 根因修复）

修复日期：2026-10-26（Asia/Shanghai）
依据：`docs/RELEASE_ACCEPTANCE_REPORT_V3.md`（F-02 panic+泄漏、F-07 快照立即过期不可达、F-04 文档残留、F-08 OpenSSL 依赖声明）
产品修复提交：`5a3fdb1`（本地 main，领先 origin/main 7 个提交，未推送）
核心原则执行情况：**已停止堆叠字符串替换规则**——安全边界转移到数据契约（自由文本默认不导出 + target_id 白名单 fail-closed）；sanitize_text 降级为纵深防御。

## 1. 处理状态总览

| ID | V3 状态 | V4 状态 | 分类 |
|---|---|---|---|
| F-02 脱敏（panic + 六类泄漏 + 合成 target_id） | FAIL | **修复完成**：契约收敛 + 索引根因修复；QA 全部场景由失败转通过 + 性质测试 | 修复完成 / **独立 QA 尚未确认** |
| F-07 快照（立即过期/不可达/失败不可重试/settings 遗留） | FAIL | **修复完成**：状态机 + 单调 TTL + 全生命周期测试（含真实写盘字节比对） | 修复完成 / **独立 QA 尚未确认** |
| F-04 文档（重复签名/过期表/断言） | 残留 | **修复完成**：ARCHITECTURE 去重、TEST_PLAN 两级防线、README 134+35 与状态机如实 | 修复完成 |
| F-08 OpenSSL | FAIL（依赖声明过强） | **修复完成**：CI 显式定位 Git 自带 openssl + README 如实声明"测试依赖 openssl.exe"；未宣称零外部依赖 | 修复完成 / **真实 CI NOT TESTED** |

## 2. F-02 修复详情（契约优先）

### 2.1 数据契约收敛（`src/commands/report.rs`）

**导出白名单（仅此允许）**：白名单 target_id、run_id、时间戳、route_mode/probe_type/status 枚举、http_status、error_kind 枚举、耗时与统计数量、规则引擎固定文案（经 sanitize_and_cap）。

**默认不导出（fail-closed）**：
- `error.summary` / `error.detail` —— 字段整体移除（V3 QA 探针的 canary 通道从源头消失）；
- 任意未验证主机名 —— `safe_target_id()`：非 targets.rs 白名单的 target_id（含 V3 合成的 `db.internal.corp`）→ `[未知目标]`，应用于 JSON 样本/Markdown 标题/诊断 related_target_ids 三处；
- 原始 URL —— 本就不导出，维持。

**严禁直接序列化 DiagnosticRun**：`secure_run_json`/`secure_diagnoses_json` 逐字段挑选（既有结构维持，本轮删除其中仅存的自由文本字段）。

### 2.2 redaction.rs 索引 panic 根因（QA 三轮 panic 的统一根因）

| panic/泄漏 | 根因 | 修复 |
|---|---|---|
| 五种头名输入 panic（V3） | 头名后无冒号仍执行 `text[colon+1..]` | 强制"头名+冒号"形态验证，无冒号跳过 |
| `İ?token=` 值首字符泄漏（V2）/ 对抗测试 panic | `mask_key_pattern` 相对偏移漏加 `value_start`（绝对坐标） | `.map(|off| value_start + off)` |
| 性质测试反向切片 panic（V2 轮） | `mask_url_userinfo` 同类漏加 `authority_start` | 同上修复 |
| 带端口私有 IPv4（V3） | 端口后置边界把 `:` 当 IP 字符 | 端口数字段并入遮蔽 |
| IPv6 scope `eth0`（V3） | scope 不在扫描字符集 | `%scope` 吸收进 token |
| 小写盘符/非 C 盘路径（V3） | 仅匹配 `C:\Users\` 字面量 | 任意 ASCII 盘符 + 大小写不敏感 |
| JSON 转义键值（V3） | `\"token\":\"` 形态无规则 | `mask_escaped_json_key` |
| `db.internal.github.com`（V3） | 白名单后缀优先于内部标签 | **fail-closed 反转**：内部标签遮蔽；白名单仅保护 token 本身或其直接父域命中者；IP 字面量跳过（公网 IP 保留） |

**索引安全不变式**：全部匹配基于原文字节的 ASCII CI 窗口（needle 全 ASCII → 命中必在 UTF-8 字符边界）；`find_ascii_ci` 起点超界返回 None 不回退；所有 `.find()` 相对偏移显式加回绝对坐标。

### 2.3 QA 场景复现→通过（先失败后修复）

| V3 QA 探针 | 修复前 | 修复后 |
|---|---|---|
| Cookie/Cookieé/Authorization🎉/ordinary…cookie/x-api-key | **panic** | ✅ 无 panic |
| 192.168.77.4:8443、10.9.8.7:443 | 泄漏 | ✅ 含端口遮蔽 |
| fe80::1%eth0 | `fe90` 前缀+eth0 残留 | ✅ 整段遮蔽 |
| c:\users…、D:\Users… | 泄漏 | ✅ 全部遮蔽 |
| `{"token":"…"}` 转义值 | 原样留存 | ✅ 遮蔽 |
| db.internal.github.com | 白名单放行 | ✅ [内部主机] |
| 合成 target_id db.internal.corp | 两格式明文 | ✅ [未知目标] |
| JSON summary/detail canary | 进入 JSON | ✅ 字段不存在 |

性质测试维持：300 轮混合随机（含 İ/CJK/emoji/控制字符）无 panic/无死循环/长度 ≤16×+4096/已知秘密零泄漏；随机字节 lossy 安全。

## 3. F-07 快照生命周期重构（`src/commands/report.rs`）

**状态机**：`Available →（take 预占）→ InFlight →（finish_success）→ Consumed /（release 取消 | finish_failure 写盘失败）→ Available（可重试）`。

- **TTL 根因**：旧 `now_secs()`（Unix epoch 秒）与 `Instant.elapsed()`（年龄秒）混算——新快照年龄 0，`epoch秒 - 0` 仍 ≈ 1.7e9 ≫ 600 → 立即过期、互相误淘汰。**修复：全程 `created_at.elapsed() < SNAPSHOT_TTL` 单调基准**。
- **并发防重**：take 将快照移入 `in_flight`（不匹配 Available 查询），第二次 take 同 id 拒绝。
- **取消重试**：`release()` InFlight→Available，同一份字节可再次 take。
- **写盘失败**：`finish_failure()` InFlight→Available + 尽力清理部分文件（`fs::remove_file`）。
- **成功**：`finish_success()` 消费失效，`consumed_count` 计数。
- **settings 遗留键**：preview 不再写 settings；`Storage::initialize` 一次性幂等 `DELETE FROM settings WHERE key LIKE 'report.snapshot.%'`（隔离内存库验证，不触碰真实数据库）。
- **限额维持**：TTL 600s / ≤8 份 / ≤8MiB 总量（InFlight 不参与淘汰）。
- **预览=导出字节**：预览生成一次存快照，导出 `take` 后写盘即同一字符串（见 §4 证据）。

## 4. 链路级回归测试（`tests/f07_snapshot.rs`，10 项全走生产代码路径）

| 测试 | 断言 |
|---|---|
| f07_new_snapshot_immediately_consumable_bytes_equal_preview | 新快照**立即** take 成功（V3 失败点）+ 字节=预览 + InFlight 态 |
| f07_snapshot_survives_history_change | 历史修改后重生成内容确实不同，快照字节不变 |
| f07_invalid_wrong_run_wrong_format_rejected | 错 id/错 run/错 format 三拒 + 正确关联可消费 |
| f07_double_consumption_rejected | 成功后同 id 再 take 拒绝 |
| f07_cancel_allows_retry_same_bytes | take→release→再 take 同字节 |
| f07_write_failure_allows_retry_and_cleans_partial | 不可写路径失败→回退+清理→重试写盘成功且字节=预览 |
| f07_three_snapshots_coexist_independently | 3 快照共存、独立消费不混淆（V3 失败点） |
| f07_ttl_expiry_rejects_expired_snapshot | created_at 回拨 660s → take 拒绝 |
| f07_count_limit_enforced | 10 份→剩 8 份、最旧淘汰最新保留 |
| f07_saved_file_bytes_equal_preview_bytes | **真实写盘后读回文件字节 == 预览字节** |

另有 `storage.rs` 验证遗留键清理不影响其它 settings（`settings_whitelist_behavior` 维持通过）。

## 5. F-04 文档与 F-08 OpenSSL

- **ARCHITECTURE**：删除无 snapshot_id 的重复 export 签名（现仅一行三参数签名）。
- **TEST_PLAN**：Phase 6 脱敏审计表改写为两级防线（契约 + sanitize_text 纵深）如实描述。
- **README**：测试数 134+35（精确分组）；F-07 描述改为状态机真实行为；OpenSSL 加入环境表并标注"测试依赖、随 Git for Windows 分发、生产运行时不依赖"。
- **V3 报告更正注记**：原文"无 Conda/Git Bash 依赖"标注 QA 更正。
- **CI**（`.github/workflows/ci.yml`）：Rust job 增加 `Locate openssl` 步骤，显式将 `C:\Program Files\Git\usr\bin` 加入 PATH——**不再宣称零外部依赖**；真实 CI 运行 NOT TESTED。

## 6. 质量基线（2026-10-26 真实执行）

| 命令 | 退出码 | 结果 |
|---|---:|---|
| `npm run lint` | 0 | 通过 |
| `npm run typecheck` | 0 | 通过 |
| `npm run test` | 0 | **35/35**（首轮 2 项为已记录的资源抖动超时，独立复跑通过） |
| `npm run build` | 0 | 通过 |
| `cargo fmt --check` | 0 | 通过 |
| `cargo clippy --all-targets -- -D warnings` | 0 | 0 错误 |
| `cargo test`（**无 OPENSSL_CONF**） | 0 | **134 全绿**：37 单元 + 16 导出回归（+3 V3 场景与性质）+ 10 F-07 快照 + 19 E2E + 13 代理 + 31 规则 + 8 存储；3 公网 ignored |
| `npm run tauri build` | 0 | NSIS 2.66 MiB（见 §7） |

失败项记录：前端首轮 2 项超时（并行资源抖动，历史已记录，复跑全绿）；无其它失败。

## 7. 安装包与 Git

| 项 | 值 |
|---|---|
| Git Commit | `5a3fdb1`（本地 main，ahead 7，未推送） |
| 版本 | 0.1.0 |
| 绝对路径 | `E:\ai-dev-cockpit\src-tauri\target\release\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe` |
| 大小 | 2,789,145 字节（2.66 MiB） |
| **SHA-256** | `F2573BB2F0A70C3680BFE119B63C040206758695B21AABF948B3943153D561EF` |
| 构建 | `npm run tauri build` 退出码 0，由 `5a3fdb1` 构建（V3 报告的 `B5AF...` 作废） |

## 8. 状态分类（如实）

- **修复完成（独立 QA 尚未确认）**：F-02、F-07、F-04、F-08 声明部分。
- **Windows 安装版 NOT TESTED**：安装/升级/卸载、Release UI（三模式/另存为对话框真实交互/断网首启）、用户真实库重启、SmartScreen。
- **GitHub CI NOT TESTED**：YAML 已含 openssl 定位步骤，待推送后首跑验证。
- 诚实边界：契约白名单是结构性收敛，sanitize_text 性质测试提供统计置信；**不宣称对任意输入的完备零泄漏证明**。

## 9. 结论

V3 的两个 P1（F-02 panic+泄漏、F-07 快照不可达）已在根因层面修复并全部链路级测试覆盖；134+35 全绿、八命令 PASS。**具备第四轮独立 Release Acceptance Test 的条件；等待 Codex QA 复验。** v1.0 未发布。
