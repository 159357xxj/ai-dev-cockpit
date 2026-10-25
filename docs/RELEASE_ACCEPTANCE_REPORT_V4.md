# AI Dev Cockpit — Release Acceptance Report V4

独立复验日期：2026-10-26（Asia/Shanghai）  
仓库：E:\ai-dev-cockpit  
范围：F-02 安全导出、F-07 快照状态机与用户文件保护、SQLite 旧键清理、OpenSSL/CI 静态检查、完整自动化质量基线与 Release 构建。  
边界：未改产品源码，未安装/启动/升级/卸载应用，未访问真实用户数据库，未 push 或创建 Release。

## 验收结论

### Gate A：安全导出与报告快照

# **FAIL**

错误摘要/详情确实已从 JSON 与 Markdown 报告默认省略，快照核心也修正了此前立即过期问题；但恶意标识符和元数据仍可注入报告，失败清理会删除预先存在的用户文件，且 8 MiB 内存上限没有覆盖单个超大快照、释放后的快照数及 in-flight 内容。以上均为本轮实际探针发现。

### 完整 Windows Release Gate

# **FAIL**

Gate A 有已复现的隐私泄漏和用户文件删除风险，故当前版本不可发布。安装器/Release UI、原生另存为交互、升级/卸载以及 GitHub Actions 没有真实执行；这些事项仍为 NOT TESTED，不能视为通过。

## 1. 环境、Git 与产物

| 项目 | 实测 |
|---|---|
| 仓库根目录 | E:\ai-dev-cockpit |
| OS | Windows 11 家庭中文版，10.0.22631，Build 22631，x64 |
| Node.js / npm | v24.19.0 / 11.17.0 |
| Rust / Cargo | rustc 1.97.1 / cargo 1.97.1，MSVC |
| 分支 / HEAD | main / 2526f58aec4cbc40cb0ecedb2de1391ac5b0e936 |
| 产品修复提交 | 5a3fdb1 |
| 远端状态 | main ahead 8；未推送 |
| 锁文件 | package-lock.json、src-tauri/Cargo.lock 均存在 |
| 版本 | 0.1.0 |
| 5a3fdb1..HEAD 差异 | 仅新增 docs/RELEASE_REMEDIATION_REPORT_V4.md；无产品源码差异 |

本轮真实执行 npm run tauri build，退出码 **0**，Tauri 输出 Target: x64 并生成 NSIS 包。

| 候选产物 | 实测 |
|---|---|
| NSIS 绝对路径 | E:\ai-dev-cockpit\src-tauri\target\release\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe |
| 大小 | 2,788,654 字节（2.66 MiB） |
| SHA-256（Get-FileHash -Algorithm SHA256） | 0BE3401114D74972C6A380DBE786A5CF8EF13FC1F5A0799786F6F7A9A87226E3 |
| FileVersion / ProductVersion | 0.1.0 / 0.1.0 |
| Authenticode | NotSigned |
| Release 主程序 | E:\ai-dev-cockpit\src-tauri\target\release\ai-dev-cockpit.exe，7,430,656 字节，SHA-256 4ADD56ADC75C1CB86CDEFDE53033DE3CAD34352E3C0354FDD0812DF0DE899D08，NotSigned |

构建前再次实测旧候选包为 2,789,145 字节、SHA-256 F2573BB2F0A70C3680BFE119B63C040206758695B21AABF948B3943153D561EF。重建后哈希已变更，以上表中的新哈希代表当前产物。构建日志有 Tauri identifier 以 .app 结尾提示及一条 linker stdout warning（创建 .lib/.exp）；构建成功。Vite 生产构建 2,630 个模块，无 Vite warning。未验证安装器签名、SmartScreen 或安装界面。

## 2. 自动化质量基线

| 命令 | 退出码 | 实际结果 |
|---|---:|---|
| npm run lint | 0 | 通过 |
| npm run typecheck | 0 | 通过 |
| npm run test | 0 | 9 个文件，35 passed、0 failed |
| npm run build | 0 | TypeScript + Vite 生产构建通过，2,630 modules |
| cargo fmt --check（src-tauri） | 0 | 通过 |
| cargo clippy --all-targets -- -D warnings（src-tauri） | 0 | 通过 |
| cargo test --quiet（src-tauri，OPENSSL_CONF 未设置） | 1 | fail-fast 在 F-07 测试组停止；已出现 1 个失败 |
| cargo test --no-fail-fast --quiet（src-tauri，OPENSSL_CONF 未设置） | 1 | **133 passed、1 failed、3 ignored** |
| npm run tauri build | 0 | NSIS x64 Release 构建成功，见 §1 |

Rust 全量 no-fail-fast 分组：37 + 16 +（9 passed、1 failed）+ 19 + 13 + 31 + 8 = **133 passed、1 failed**；另有 3 个公网用例 ignored。失败项为 tests/f07_snapshot.rs 的 f07_snapshot_survives_history_change（exit code 1）：测试只修改了现已默认省略的 error.summary，却断言重生成报告必与旧内容不同；两次 exported_at 落在同一毫秒时报告相同。该断言与新导出契约不一致，并存在时间边界竞态。V4 remediation 报告中的 134 全绿未在本轮复现。

### F-02 独立恶意字段探针

临时构造测试使用全新假值，并经实际 build_report 生成 JSON/Markdown。测试只存在于工作区临时文件，已在验收后移除，不计入产品测试总数。独立报告字段探针 1 项失败；脱敏边界探针 1 项失败。

| 字段/检查 | 结果 |
|---|---|
| error.summary / error.detail 默认导出 | **PASS**：两种格式均无原文 canary；JSON 中也不存在 error_summary/error_detail 字段 |
| 错误类别、样本数/统计 | **PASS**：枚举 error_kind 保留，JSON 可解析，attempts_total=3，Markdown 样本统计保留 |
| 非白名单内部 target_id | **FAIL**：主 JSON target_id 被替换为“未知目标”，但动态 target_id 被诊断标题再次使用；控制字符后的 QA4_TARGET_CONTROL 仍进入 JSON/Markdown。Markdown 目标标题直接输出原 target_id |
| run_id | **FAIL**：含路径分隔符、CRLF 和 Markdown 标题的非法 run_id 被原样写入 JSON/Markdown；默认文件名包含 ../ 和控制字符。未被拒绝或安全替换 |
| started_at / finished_at / sample.tested_at | **FAIL**：非法 RFC3339 字符串和控制字符中的 canary 出现在 JSON/Markdown；没有日期解析或格式校验 |
| JSON meta.app_version | **FAIL**：构造的版本字符串及 Markdown 链接注入原样进入 JSON 元数据和 Markdown |
| Markdown 注入 | **FAIL**：不可信 target_id / run_id / app_version 的换行、标题和 javascript 链接未经 Markdown 转义 |
| panic 检查 | **PASS（报告生成探针）**：上述恶意报告对象调用 build_report 未 panic；**FAIL（直接 sanitizer 边界）**：sanitize_text(\"C:\\Users\") 在 redaction.rs:404 发生 index out of bounds |
| Unicode、头名、JSON 键 | 敏感头无冒号、Unicode 头名及普通文本末尾 cookie 在直接 sanitizer 探针中不再 panic；İ?ToKeN 查询值被遮蔽。普通嵌入 JSON {"token":"..."} 仍输出含原值的畸形文本；反斜线转义 JSON 形态本探针已遮蔽 |
| IPv4/IPv6、路径、公开值 | 合成输入中的带端口私网 IPv4、IPv6 scope、Windows 用户名、非白名单内部域名均被遮蔽；8.8.8.8 与 github.com 保留 |

补充：safe_target_id 只覆盖 JSON run target 和诊断 related_target_ids。它没有覆盖 Markdown 目标标题，也没有覆盖从原始 target_id 构造后再经 sanitize_text 的 diagnosis.title。run_id、时间戳和 app_version 则在 report.rs 中直接进入 JSON/Markdown 和文件名。serde JSON escaping 保证了 JSON 文法可解析，但不能把这些字段中的敏感内容变成安全内容。测试未发现统计字段被误删。

## 3. F-07 生产快照生命周期与文件保护

使用实际 SnapshotStore、实际状态迁移函数和临时目录文件执行了独立探针；没有调用 Tauri 原生 FileDialog，也没有触碰用户文件。

| 验收点 | 结果 | 证据 |
|---|---|---|
| 新快照立即消费 | PASS | 新快照 take 成功 |
| 多快照并存 | PASS | 三份可用快照共存并独立消费 |
| 并发重复导出 | PASS | 两个并发 take 争用同一 ID，恰有一个成功 |
| TTL 到期 | PASS | Instant 回拨 601 秒的快照被拒绝 |
| 取消后重试 | PASS | take → release → 再 take，内容相同 |
| 写入成功字节 | PASS（保存核心） | 临时新文件读回字节与快照预览逐字节相等 |
| 成功后重复使用 | PASS | finish_success 后同一快照再次 take 被拒绝 |
| 错 snapshot_id / run_id / format | PASS | 均被拒绝 |
| 数量上限插入路径 | PASS | 10 份插入后最旧项被淘汰、保留 8 份 |
| release 后数量上限 | **FAIL** | 已有 8 份时 take 一份、再 insert 一份、release 原快照，可用数变为 9 |
| 单个快照内存上限 | **FAIL** | 插入 8 MiB + 1 字节快照后 available_count=1，超限内容仍被缓存 |
| in-flight 内存上限 | **FAIL** | 两份 8 MiB 快照可同时 take；返回内容合计 16 MiB。take 还复制内容到 in_flight，且其总量不计入 8 MiB available 上限 |
| 用户文件覆盖失败清理 | **FAIL，数据丢失风险** | 在隔离临时目录预先写入 KEEP ORIGINAL USER CONTENT，模拟进入生产 finish_failure 清理路径；finish_failure 无条件 remove_file(path)，预置文件被删除，内容不可恢复 |
| 失败后重试 | 状态机 PASS；用户文件保护 FAIL | finish_failure 会恢复快照且后续重试写入字节成功；但它同时删除传入路径，不区分本次创建的 partial file 和先前存在的用户文件 |

对文件失败场景，本轮直接执行了生产 finish_failure 处理函数，并以已有内容文件作为 partial_path；没有制造真实磁盘满或 ACL 拒绝，也没有运行原生 Save As。结果已证明当前清理实现不具备文件所有权判断：代码仅在 std::fs::write 返回错误后调用它，但会删除被传入的既有路径。任何写入错误都不能安全地把用户选中的路径视为本次新建文件。属于发布阻断的数据丢失风险。

预览/写入字节比对通过的是独立临时文件上的生产 SnapshotStore take + std::fs::write 核心路径；原生 Windows 对话框、实际覆盖确认、权限失败时的端到端行为均 **NOT TESTED**。

## 4. SQLite 初始化清理复验

在独立临时磁盘数据库上使用生产 Storage::open 初始化和真实 runs/results：

- 两个 report.snapshot.* 遗留键在重开时被清理；report.snapshot_extra.keep 和 retention.max_runs 均保留。
- run_count 仍为 1，重新读取同一 run 仍有原 1 个 sample；重复初始化后结果不变。
- 安装一个仅针对 report.snapshot.* DELETE 的 SQLite 测试触发器并触发 ABORT，两个匹配键都保留，retention 设置和历史记录/样本不变；去除触发器后下一次初始化清理成功。

**隔离数据保护结果：PASS。** SQLite 的 DELETE 语句失败时没有部分删除；无关 settings 和历史数据未被改变。本测试没有读取、复制、哈希、迁移或启动会访问当前用户真实数据库的程序。

发现：storage.rs 将清理语句错误丢弃（let _ = execute）。在测试触发器场景下 Storage::open 仍成功，遗留键保持不动且没有向调用方报告；因此幂等和数据安全成立，但清理失败会被静默忽略。该语句不是显式包在应用事务中，SQLite 对单条 DELETE 保证语句级原子性；本轮触发器验证的是这一回滚边界，并非迁移事务失败。

## 5. OpenSSL、CI 与完整 Windows 验收边界

- 本轮 Rust 全量测试期间 OPENSSL_CONF 未设置，测试中 TLS mock 成功；但本机实际 openssl.exe 来自 E:\Miniconda3\Library\bin\openssl.exe。tls_mock.rs 调用 Command::new(\"openssl\")，所以本地测试仍依赖 PATH 上的外部 openssl.exe，结论不是“无需外部 OpenSSL”。
- CI workflow 的 Locate openssl 步骤只把 C:\Program Files\Git\usr\bin 添加到 GITHUB_PATH，没有 Test-Path、Get-Command、where.exe 检查，也没有安装步骤。本轮没有真实 GitHub Actions 环境，不能确认该路径确有 openssl.exe。
- CI 状态：**NOT TESTED — 本地 main ahead 8，未推送；没有 Actions run。** YAML 仅静态可见 permissions: contents: read；构建 job 需手动 workflow_dispatch，不自动发布 Release。
- 安装/升级/卸载、开始菜单/默认路径、管理员提示、SmartScreen、WebView2、Release UI、主题切换、真实本地数据库写入/重启、原生另存为取消/覆盖/权限失败：全部 **NOT TESTED**。本轮未在真实用户环境安装或启动。
- 公网诊断未执行，系统网络设置未更改。

## 6. 问题分级与已完成/未完成项

| ID | 严重度 | 状态与依据 |
|---|---|---|
| F-02 | **P1 / 未关闭** | 默认摘要/详情省略已验证；但 target_id、run_id、时间戳、版本字段可注入 JSON/Markdown；默认文件名也接受非法 run_id。直接 sanitizer 对 JSON 普通引号键值仍泄漏，并有 Windows 路径边界 panic。 |
| F-07-DATA | **P1 / 新发现** | finish_failure 删除传入的预存用户文件；隔离临时文件已被删除，确认数据损失风险。 |
| F-07-LIMIT | **P2 / 未关闭** | release 后 count=9；8 MiB+1 单快照仍保留；in-flight 快照和 clone 不纳入总量限制。 |
| F-07-TEST | **P2 / 新发现** | cargo test 中 f07_snapshot_survives_history_change 失败；error.summary 已省略后，测试仍假定修改它必定改变报告，时间戳精度又造成同毫秒竞态。 |
| F-08 | **未验证** | 本地依赖 Miniconda openssl.exe；CI 只追加 Git usr/bin 路径，不检查实际可执行文件；Actions NOT TESTED。 |
| SQLITE-CLEANUP | **P2 / 新发现** | 遗留键 DELETE 的错误被忽略；触发器模拟失败后应用初始化成功但旧键静默保留。数据安全与语句回滚通过，失败可见性未通过。 |
| WINDOWS-UI/INSTALL | **NOT TESTED** | 没有隔离安装运行记录；未安装、启动、升级或卸载。 |
| REAL-DB | **未触碰** | 只对独立临时数据库做验证；此前报告里的真实库启动矛盾本轮无法证明或消解。 |

## 7. 最终说明

F-02 的自由文本移除是有效改进，但其余导出字段仍需验证与转义；F-07 的 Available/InFlight/Consumed 生命周期在基本路径和并发单次消费上通过，用户文件清理与资源上限仍有阻断缺陷。当前 Gate A 与完整 Windows Release Gate 均为 **FAIL**。

本报告完成后停止。没有修改产品源码、安装/启动/卸载应用、访问真实用户数据库、push、创建 Release 或修改系统网络配置。
