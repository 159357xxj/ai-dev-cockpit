# Release Remediation Report V5（第四轮 FAIL 根因修复）

修复日期：2026-10-26（Asia/Shanghai）
依据：`docs/RELEASE_ACCEPTANCE_REPORT_V4.md`（F-07-DATA 用户文件误删、F-02 恶意字段注入、F-07-LIMIT 资源上限、F-07-TEST 失败测试、SQLITE-CLEANUP 静默、F-08 CI openssl 检查）
产品修复提交：`94bd645`（本地 main，领先 origin/main 9 个提交，未推送）。
原则：保留 V4 已通过的快照状态机语义（未推翻重做）；文件保护与字段契约从根因层修复。

## 1. 缺陷处理状态

| ID | V4 结论 | V5 状态 |
|---|---|---|
| F-07-DATA（P1：finish_failure 删除预存用户文件） | FAIL | **修复完成**：`save_report_atomic` 原子事务 + finish_failure 不再删除任何路径；14 项隔离文件测试全绿 |
| F-02（P1：run_id/时间戳/版本/文件名/Markdown 注入 + sanitize panic） | FAIL | **修复完成**：统一安全 DTO（safe_run_id/safe_timestamp/safe_app_version/safe_filename/markdown_escape）+ `C:\Users` 越界根因修复；恶意字段测试全绿 |
| F-07-LIMIT（P2：release 后 9 份 / 单份 8MiB+1 / InFlight 不计费） | FAIL | **修复完成**：Available+InFlight 统一预算 + 单份超限拒绝 + Arc<str> 零拷贝；预算测试全绿 |
| F-07-TEST（P2：survives_history_change 失败） | FAIL | **修复完成**：改用真正导出的结构化字段（repeats）+ 确定性断言（不依赖毫秒差）；未删测试未弱化断言 |
| SQLITE-CLEANUP（P2：let _ = 静默） | FAIL | **修复完成**：非关键维护失败 → 非敏感 stderr 警告（可观测），数据保留启动继续；语句级原子性维持 |
| F-08（CI openssl 无存在性检查） | FAIL | **修复完成**：CI 步骤显式 `Get-Command openssl` 检查 + 版本打印 + 可诊断错误退出 |

## 2. 任务一：文件写入事务（F-07-DATA 根因修复）

### 设计（`src/commands/report.rs::save_report_atomic`）

```
save_report_atomic(target, content):
  1. target.parent 必须是目录（否则明确错误）
  2. target 已存在 → fail-closed 拒绝（"为保护现有内容已拒绝覆盖，请选择新文件名"）
     ——V5 覆盖策略：暂不实现原子替换，保拒绝（任务一第 8 条选项 A）
  3. 同目录 .adc-tmp-{uuid}.tmp，OpenOptions.create_new(true) 独占创建
     ——不覆盖任何已有文件；两个并发事务各得各的随机临时文件
  4. write_all → flush → sync_all → drop → rename(temp, target)
     ——Windows 同卷 rename 对不存在目标原子；目标被并发创建 → rename 失败 → 回退拒绝
  5. 任一步失败 → 仅 remove 本事务 create_new 创建的临时文件（所有权可证明）
     ——绝不触碰 target 或任何其它路径
```

`finish_failure` **不再接受删除路径**（参数保留但忽略，文档注明废除）；临时文件清理职责完全收敛于 `save_report_atomic` 内部事务。导出命令改为调用 `save_report_atomic`，失败经 `finish_failure(_, None)` 恢复快照可重试。

### 用户文件保护证据（tests/v5_file_and_limits.rs，隔离临时目录）

| 场景 | 测试 | 结果 |
|---|---|---|
| 新文件保存成功+字节一致+无临时残留 | v5_save_new_file_success_bytes_match | ✅ |
| 写入失败后预存文件逐字节不变 | v5_write_failure_preserves_existing_user_file | ✅ |
| **finish_failure 不删除预存用户文件（V4 复现）** | v5_finish_failure_never_deletes_user_file | ✅ |
| 父路径不可写失败：零残留+相邻文件不变 | v5_partial_write_failure_atomic | ✅ |
| 覆盖已存在文件 → fail-closed 拒绝+原文件不变 | v5_overwrite_existing_fail_closed | ✅ |
| 并发争用同一目标：后提交者被拒、先提交者完整、无残留 | v5_concurrent_same_target_no_corruption | ✅ |
| 预览字节=最终保存文件字节（走 save_report_atomic） | v5_saved_bytes_equal_preview_via_atomic_save | ✅ |

（权限不足以"父路径为文件"确定性模拟——Windows 目录 readonly 位不阻止创建，已验证；真实 ACL 拒绝场景维持 NOT TESTED 如实标注。）

## 3. 任务二：统一安全 DTO（F-02）

### 契约（`src/commands/report.rs` 安全投影函数）

| 字段 | 投影 | 非法输入结果 |
|---|---|---|
| target_id | `safe_target_id`（targets.rs 白名单） | `[未知目标]`（JSON 样本/MD 标题/诊断 related 三处） |
| run_id | `safe_run_id`：严格 UUID v4 格式（36 位、连字符位置、hex） | `invalid-run`（阻断路径分隔符/CRLF/穿越/Markdown） |
| started_at/finished_at/tested_at | `safe_timestamp`：chrono RFC3339 解析+规范化输出 | `invalid-timestamp`（原始噪声不出现在输出） |
| app_version | `safe_app_version`：≤32 字符、仅 [0-9A-Za-z.+-] | `unknown`（阻断 `[link](javascript:…)` 注入） |
| 导出文件名 | `safe_filename`：固定前缀 `adc-report-` + 已验证 UUID + 固定扩展 | 不可能包含原始 run_id 片段 |
| 诊断 title（MD） | `sanitize_and_cap` → `markdown_escape`（剥离控制字符+转义 `#[]\`<>*_()!`） | 注入元字符全部转义 |
| error.summary/detail | **维持默认不导出**（V4 契约不变） | 字段不存在 |

### sanitize_text `C:\Users` panic 根因

`mask_all_user_paths` 循环条件 `index + 8 <= len` 在 `index+8 == len` 时访问 `bytes[index+8]` 越界（`X:\users\` 需 9 字节）→ 条件改为 `index + 9 <= len`。回归：`v5_sanitize_users_prefix_no_panic`（含更短前缀变体）。

### 恶意字段测试（v5_malicious_run_id_timestamps_version_neutralized）

合成 V4 QA 全部注入向量（路径分隔+CRLF+标题+控制字符的 run_id、非法日期 canary、链接注入版本、控制字符 target_id）+ 断言 JSON/MD 零出现、文件名无穿越、占位符正确。**全绿**。

## 4. 任务三：快照资源预算（F-07-LIMIT）

- `total_managed_count/bytes` = **Available + InFlight 统一计费**；
- `insert`：单份 > 8 MiB → 拒绝（明确错误）；先淘汰过期 Available，再按需淘汰最旧 Available；InFlight 不可被普通淘汰——预算仍满 → 拒绝插入（不 panic 不死锁）；
- `take` = 桶间转移（Arc 克隆，零拷贝），预算不变 → **release 必然不越限**（V4 复现根因：旧实现 InFlight 不计费，insert 期间 7 份 → release 回 9）；
- content 改 `Arc<str>`：take/桶转移共享字节，实际内存=预算口径。

预算测试：单份 8MiB+1 拒绝 ✅；release 后 ≤8 ✅；InFlight 4MiB + 新 5MiB 拒绝 ✅；TTL 到期释放（f07 既有）✅；交错/并发（f07 既有 10 项）✅。

## 5. 任务四/五/六

- **F-07-TEST**：`f07_snapshot_survives_history_change` 改用 `repeats`（真正导出的结构化字段，attempts_total 确定性变化）；快照字节不变断言保留；核心语义未削弱。
- **SQLITE-CLEANUP**：失败 → `eprintln!("[storage] 非关键维护失败：…（数据保留、启动继续）")` 可观测警告；不阻断启动、不假成功；语句级原子性（V4 QA 触发器探针已证）维持；隔离库测试（storage 8 项）全绿。
- **CI openssl**：步骤内 `Get-Command openssl` 检查，缺失即 `Write-Error` + `exit 1`（可诊断），存在则打印版本。

## 6. 质量基线（2026-10-26 真实执行）

| 命令 | 退出码 | 结果 |
|---|---:|---|
| `npm run lint` / `typecheck` | 0 | 通过 |
| `npm run test` | 0 | **35/35** |
| `npm run build` | 0 | 通过 |
| `cargo fmt --check` | 0 | 通过 |
| `cargo clippy --all-targets -- -D warnings` | 0 | 0 错误 |
| `cargo test --no-fail-fast` | 0 | **148 全绿**：37 单元 + 16 导出回归 + 10 F-07（修复后）+ **14 V5 文件/预算/恶意字段（新增）** + 19 E2E + 13 代理 + 31 规则 + 8 存储；3 公网 ignored |
| `npm run tauri build` | 0 | NSIS 2.66 MiB（见 §7） |

## 7. 安装包与 Git

| 项 | 值 |
|---|---|
| Git Commit | `94bd645`（本地 main，领先 origin/main 9 个提交，未推送） |
| 版本 | 0.1.0 |
| 绝对路径 | `E:\ai-dev-cockpit\src-tauri\target\release\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe` |
| 大小 | 2,794,359 字节（2.66 MiB） |
| **SHA-256** | `8BB0806797A784DF6ED2B45708822ECC0584BE7BA950A4AEC4D1626FFD8A9BF4` |

## 8. 未执行（如实）

- **Windows 安装版 NOT TESTED**：安装/升级/卸载、Release UI、原生另存为真实交互（取消/覆盖提示/ACL 失败端到端）、SmartScreen。
- **GitHub CI NOT TESTED**：YAML 含 openssl 检查，待推送后真实运行。
- 真实 ACL/磁盘满场景以等价确定性失败模拟（父路径为文件），如实标注。

## 9. 剩余风险与待确认

1. F-02/F-07-DATA/F-07-LIMIT/F-07-TEST/SQLITE-CLEANUP/F-08 均为**修复完成待独立 QA 确认**。
2. 覆盖策略为 fail-closed 拒绝（选项 A）——用户反馈"每次都要改名"可在后续版本实现 `ReplaceFileW` 原子替换（选项 B）。
3. sanitize_text 仍为纵深防御（非主边界）；性质测试统计置信、非完备证明（维持声明）。

**结论：具备第五轮独立 Release Acceptance Test 条件。等待 QA。**
