# AI Dev Cockpit — Release Acceptance Report V3

独立复验日期：2026-10-25（Asia/Tokyo）  
验收范围：F-02/F-07 专项独立复验、文档与 OpenSSL 依赖核查、前后端质量基线、Release 构建和候选包复算。  
本轮没有修改产品源码，没有安装/启动/卸载应用，没有访问当前用户真实数据库，没有 push 或创建 Release。

## 1. 发布判定

# **FAIL**

**依据：** F-02 仍能被独立构造的假凭证绕过，且畸形/普通错误文本可触发 redaction.rs panic；F-07 的全新快照会被生产 take 逻辑立即判为过期，当前无法到达保存对话框。另有报告设置键未清理、写盘失败丢失快照和文档残留不一致。自动化质量基线通过不能覆盖这些发布阻断问题。安装版 UI、真实另存为交互、GitHub CI 尚未实测。

## 2. 环境、版本与工作区

| 项目 | 实测结果 |
|---|---|
| 仓库根目录 | E:\ai-dev-cockpit |
| 操作系统 | Windows 11 家庭中文版，10.0.22631，Build 22631，x64 |
| Node.js / npm | v24.19.0 / 11.17.0 |
| Rust / Cargo | rustc 1.97.1 / cargo 1.97.1，MSVC x64 |
| 分支 / HEAD | main / e99b82edea05fc723a49f773909a0740e7e7b637 |
| 产品修复提交 | 68a5e59（V2 专项修复） |
| 远端状态 | 本地 main ahead 6，未推送；本轮没有 GitHub Actions run |
| 工作区 | 验收开始时干净；临时独立 QA 测试文件已移除。交付本报告后仅有报告文件变更 |
| 锁文件 | package-lock.json、src-tauri/Cargo.lock 均存在 |
| 版本 | 前端与 Rust/Tauri 包版本均为 0.1.0 |

git diff 68a5e59..HEAD 实测只有 docs/RELEASE_REMEDIATION_REPORT_V3.md 的新增，不包含产品源码变更。因此本轮在 e99b82e 上重建的产品代码与修复提交 68a5e59 相同；构建没有签名或可复现构建证明。

## 3. 安装包与 Release 构建

本轮真实执行 npm run tauri build，退出码 **0**。Tauri 输出 Target: x64 并成功生成 NSIS 安装包。

| 项目 | 本轮实测 |
|---|---|
| 安装包绝对路径 | E:\ai-dev-cockpit\src-tauri\target\release\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe |
| 大小 | 2,788,955 字节（2.66 MiB） |
| SHA-256（Get-FileHash -Algorithm SHA256） | B5AFCA890A71B196B88136B9951245F2600E8E07D37357F7101B64892028F917 |
| FileVersion / ProductVersion | 0.1.0 / 0.1.0 |
| Authenticode | NotSigned |
| 主程序 | E:\ai-dev-cockpit\src-tauri\target\release\ai-dev-cockpit.exe，7,428,608 字节，SHA-256 9DD5405AE763EC731DA6F2EDA232ED73CDADB1929A97D171886031AFD73884BF，NotSigned |

构建前候选包哈希为开发报告所列 03F9112D287621BA71251F99A6A603864FCC8DC9EC859C84C7BD4EE219D20E01，本轮重建后新哈希为上表值；旧哈希不代表当前文件。构建日志另有两项提示：Tauri 提示 identifier com.aidevcockpit.app 以 .app 结尾；Rust linker 输出创建 .lib/.exp 的 warning。均未使构建失败。前端 Vite 生产构建 2,630 个模块完成，没有 Vite 构建 warning。

未运行安装器、未启动 EXE；SmartScreen、安装向导、默认路径、快捷方式、WebView2 引导及运行 UI 均 **NOT TESTED**。

## 4. 自动化质量基线

下表为本轮真实命令结果；Rust 全量测试时确认 OPENSSL_CONF 未设置。

| 命令 | 退出码 | 实际结果 |
|---|---:|---|
| npm run lint | 0 | ESLint 通过 |
| npm run typecheck | 0 | TypeScript 检查通过 |
| npm run test | 0 | 9 个测试文件，35 passed、0 failed |
| npm run build | 0 | TypeScript + Vite 生产构建通过；2,630 modules transformed |
| cargo fmt --check（src-tauri） | 0 | 通过 |
| cargo clippy --all-targets -- -D warnings（src-tauri） | 0 | 通过 |
| cargo test --quiet（src-tauri，无 OPENSSL_CONF） | 0 | 130 passed、0 failed、3 ignored |
| npm run tauri build | 0 | Release x64 + NSIS 构建成功，见 §3 |

Rust 测试输出分组：40 + 19 + 19 + 13 + 31 + 8 = **130 passed**；另有 3 个公网用例 ignored。测试/Release 编译日志存在 #[warn(linker_messages)]，不是 Clippy 错误。实际总数比 RELEASE_REMEDIATION_REPORT_V3.md 的 127 多 3：当前导出回归组实测 19 项，报告列为 16 项。

### 独立 QA 探针（不计入产品基线）

临时新增并运行 cargo test --test qa_v3_independent_tmp -- --nocapture，退出码 **1**，4 tests：**0 passed、4 failed、0 ignored**。所有输入都是独立构造的假凭证；测试文件在运行后已删除。

| 探针 | 实际结果 |
|---|---|
| 混合凭证的 JSON/Markdown 报告 | **FAIL**：JSON 泄漏 192.168.77.4、10.9.8.7、eth0、小写 c:\users\... 用户名、D:\Users\... 用户名及 JSON token 值；直接脱敏输出也保留这些值。JSON 外层仍可解析，Markdown 未包含错误摘要/详情中的 canary，统计字段仍在 |
| 畸形/普通头名称输入 | **FAIL**：Cookie、Cookieé、Authorization🎉、ordinary error mentions cookie、x-api-key 五种输入全部 panic；panic 位置 redaction.rs:133 |
| 新建快照立即消费及字节 | 临时文件字节与缓存预览字节相同（TEMP_FILE_BYTES_EQUAL=true），但 take 立即失败（VALID_IMMEDIATE_TAKE_OK=false） |
| 三个新快照共存 | **FAIL**：插入三个后长度为 1，且 ID 对应项无法正常消费 |

另运行临时合成 target ID 探针，退出码 0：将 db.internal.corp 作为合成历史中的 target_id 后，JSON 与 Markdown 都原样输出该内部主机名。正常诊断命令对目标 ID 有白名单校验，但报告构造器对已存目标 ID 不再校验/脱敏；这是受正常入口约束的残余数据风险，不能当成公开目标的泄漏已被排除。

### F-02 输入覆盖及边界

独立混合输入覆盖 CRLF 与折叠 Cookie、多种连续敏感头、百分号编码且含多个 @ 的 URL userinfo、敏感查询参数、İ?ToKeN= Unicode 前缀、私有 IPv4/IPv6、IPv6 scope ID、多个大小写/盘符 Windows 用户目录、JSON 转义和 Markdown 特殊字符。观察到：

- **有效遮蔽：** 常规 Authorization/Cookie/API Key/Proxy-Authorization/Set-Cookie 及折叠 Cookie 值；多 @ URL userinfo；token/API_KEY 查询值；Unicode 前缀查询键。公网 8.8.8.8、正常目标 github 和 Markdown 样本统计保留，JSON 外层有效。
- **仍泄漏：** 带端口的私有 IPv4（192.168.77.4:8443、10.9.8.7:443，后置边界逻辑把 : 当作 IP token）；IPv6 scope 后缀 eth0；小写盘符路径和非 C 盘 D:\Users\...；JSON 文本字段中的敏感键值（"token":"..." 的值原样留存，且文本变成缺少冒号的畸形片段）；db.internal.github.com 因 .github.com 白名单而未被内部主机规则遮蔽。
- **可导致 panic：** mask_headers_folded 未验证敏感头名称后确有冒号，就切 text[colon + 1..]。普通错误摘要只要以 cookie 等头名结束也会越界；多字节字符后切片可落在 UTF-8 字符中间。
- **诊断字段与新字段：** report.rs 对 diagnosis 的 title/facts/causes/steps/limitations 都调用 sanitize_and_cap；R04 事实现为结构化计数，不再复制原始错误摘要。生成的 Markdown 不包含原始错误摘要/详情。本轮未能通过外部 build_report API 注入自定义 diagnosis 文本（诊断对象由规则引擎静态生成），因此这四类自由文本的自定义异常载荷覆盖为 **NOT TESTED**，只能确认源码有 sanitizer 调用。合成 target_id 的探针另证实非自由文本标识字段不能依赖 renderer 自动脱敏。
- **格式和截断限制：** serde 让外层 JSON 有效，但不会阻止错误摘要字符串里的 canary 泄漏或 sanitizer 产生畸形片段。300 字符截断发生在脱敏之后，本轮没有穷举每个字段截断边界；任意自由文本不能由有限规则证明完全安全，建议对无法可靠净化的错误摘要/详情采用省略或结构化允许列表，而非继续堆叠替换规则。

## 5. F-01～F-07 独立复验

| 项目 | 状态 | 独立结论 |
|---|---|---|
| F-01 命令入口三模式 | **NOT TESTED（本轮未复验）；桌面 UI NOT TESTED** | 本轮没有重新执行入口/代理专项测试，也没有启动安装版。此前报告的结果不作为本轮独立通过证据。 |
| F-02 JSON/Markdown 脱敏 | **FAIL，P1 阻断** | 本轮假凭证探针复现泄漏与 panic；合成 target ID 也可进入两个格式。详见 §4。 |
| F-03 SQLite WAL 备份文档 | **静态审查 PASS；真实备份/升级 NOT TESTED** | 文档审查不构成生产 SQLite 备份或跨版本升级验证。本轮未打开、复制、哈希或迁移用户真实数据库。 |
| F-04 文档/IPC 一致性 | **FAIL，P2** | ARCHITECTURE.md 同时保留带 snapshot_id 的新签名和不带该参数的旧 export_sanitized_report 签名；TEST_PLAN.md Phase 6 脱敏表仍描述旧字段范围与旧嵌套字段；README 声称 F-07 字节一致，但真实 take 不能消费快照。README 内 2026-10-24 的 47/22 标为历史记录；当前 README 只写 Rust 119+、前端 35，非本轮实测的精确统计。 |
| F-05 CI 最小权限 | **YAML 静态审查 PASS；实际 CI NOT TESTED** | .github/workflows/ci.yml 声明 permissions: contents: read，使用 Windows runner、Node 22、Rust stable；没有自动 Release 步骤。CI Rust 测试需 openssl.exe，workflow 未显式安装/定位它。Phase 6 未推送，结果为 **NOT TESTED — 等待推送后实际运行**。 |
| F-06 安装包来源 | **本地构建 PASS；签名/复现证明 NOT TESTED** | 68a5e59..e99b82e 只有 V3 remediation 文档变更；本轮从 e99b82e 真正重建，哈希和版本见 §3。未取得 CI artifact、签名链或可复现构建证明。 |
| F-07 报告快照 | **FAIL，P1 阻断** | 精确实现和独立结果见下节。 |

## 6. F-07 ReportSnapshot 实现复验

### 已确认的实现事实

- preview_sanitized_report 调用 build_report 一次，生成包含 exported_at 的内容，再把同一字符串复制进 ReportSnapshot 并返回预览。故 exported_at 在生成该快照时确定；读取历史的后续变化不会改写已缓存字符串。
- 缓存字段是报告内容字符串，不是原始 DiagnosticRun 对象；但 F-02 的脱敏缺陷会让敏感值进入该“已脱敏”字符串和快照。
- take 按 snapshot_id + run_id + format + exported 标记查找，并在 mutex 内移除匹配项，静态看有 ID 隔离。**没有可成功消费的快照**，因此并发预览/导出隔离、单次消费成功路径均未由运行时验证。

### 发现的阻断与恢复缺陷

1. **新快照立即被当作过期。** insert/take 取 SystemTime 的 Unix 秒 now_secs()，再计算 now.saturating_sub(instant.elapsed().as_secs()) < 600。一个刚创建的 Instant 年龄为 0 秒，左侧仍约为当前 Unix epoch 秒，远大于 600；所以插入第二个快照时会先淘汰前一个，take 对第一个新快照也立即返回 invalid_input。独立实测：新快照 take=false，三个插入后 count=1。TTL、8 项数量上限和 8 MiB 总量上限因此没有按预期工作；字节上限未能在有效生命周期下独立压测。
2. **取消保存流程不可达。** 源码在原生 Save As 取消后将快照 push_back，但 take 的即时失效使实际路径到不了对话框。本轮未运行桌面 UI，也未验证取消后重试。
3. **写盘失败不可重试。** take 在打开对话框前已经移除快照；std::fs::write(...)? 失败时没有重新放回，因此失败后同一 snapshot_id 无法再次导出。没有实际制造磁盘权限/写入失败，因为有效快照目前不可达。
4. **报告 ID 设置键无清理。** 每次预览向 SQLite settings 写入 report.snapshot.<uuid>；仓库中 delete_setting 只有定义，没有从预览/导出调用。导出成功、取消、过期、快照超限或应用重启都没有对应清理，产生持久、无界的无用键。V3 remediation 报告声称导出后清理，与源码不符。

### 预览/实际文件字节

独立探针把缓存快照的 content.as_bytes() 写入系统临时文件并重新读取，逐字节与 preview 相等（PASS）。这只验证缓存 clone 的字节一致性；不是正式 export_sanitized_report 文件保存验证。正式 take 会立即失败，因此本轮无法通过命令/UI 写出真实报告文件。仓库原回归测试手动从队列移除快照，而没有调用生产 SnapshotStore::take，不能证明实际导出可用。源码上的 std::fs::write 使用缓存内容本身，若快照生命周期修正，静态路径将写相同字节，但当前端到端结论仍为 **FAIL / NOT TESTED**。

## 7. OpenSSL、CI 与文档差异

本轮运行 Rust 测试时 OPENSSL_CONF_PRESENT=False，cargo test --quiet 成功；但 Get-Command openssl 实际找到 E:\Miniconda3\Library\bin\openssl.exe。tls_mock.rs 仍用 Command::new("openssl") 生成证书，只为子进程设置临时 OPENSSL_CONF。因此验证结果是“父进程不需设置 OPENSSL_CONF，当前 PATH 上有外部 OpenSSL 时可运行”，**不是无外部 OpenSSL 依赖**。GitHub workflow 没有显式安装或固定 OpenSSL，因此干净 Windows runner 上的依赖可用性仍需真实 CI 验证；V3 remediation 报告宣称完全无 Conda/Git Bash 依赖过强。

文档复核发现：

- docs/ARCHITECTURE.md 的报告导出表有两行同名接口；新行含 snapshot_id，紧邻旧行不含 snapshot_id。
- docs/TEST_PLAN.md 的 Phase 6 脱敏审计表仍只列 userinfo/查询参数/用户路径/Authorization，并称嵌套自由文本只有 error.summary/detail；当前实现还处理诊断 title/facts/causes/steps/limitations，且已有服务端快照，原表未反映这些接口与风险。
- README.md 仍称 F-07 预览导出字节完全一致；这与当前快照即时失效的实测矛盾。README 的 2026-10-24 47/22 是明确标日期的历史数据；本轮当前值应参考 §4，而不是把历史行当作最新结果。V3 remediation 报告的 Rust 127 项与本轮 130 项实测不符。
- CI YAML 的权限范围最小，未发现自动 Release 或明文 secret；这只是配置检查，**NOT TESTED — 等待推送后实际运行**。

## 8. 用户数据、安装与人工验收边界

- 本轮没有打开、读取内容、复制、哈希、迁移或修改当前用户真实 SQLite 数据库；也没有启动任何 Release/安装版应用。此前报告关于某次修复期间是否启动过 Release EXE 的矛盾仍无法由本轮证据消解；没有启动前快照就不能证明当时数据库完全未变。本轮没有通过再次启动应用调查。
- 隔离 Windows Sandbox/VM 安装验收本轮没有执行。基于此前 V2 报告，当前环境未找到可用 Sandbox/VM；本轮不在真实用户环境安装。
- 安装器启动、向导、默认路径、管理员提示、快捷方式、Release UI 白屏/主题、任务退出清理、真实安装版 SQLite、用户取消/文件权限错误/同名覆盖、两个不同版本升级、卸载与重装数据读取：全部 **NOT TESTED**。
- 公网测试没有执行；没有扩大用户授权范围。
- GitHub CI 没有运行；main 领先远端 6 个提交。

## 9. 缺陷与剩余发布门槛

| ID | 严重度 | 证据 | 发布影响 |
|---|---|---|---|
| F-02（未关闭） | **P1 / 高** | 独立假凭证测试复现 JSON 泄漏；五类头名/Unicode/普通文本导致 panic；私网 IP、IPv6 scope、用户目录、JSON 键值、内部域名均有残留。合成报告 target ID 可在 JSON/Markdown 泄漏内部主机名。 | 阻断：导出隐私边界不可靠，报告预览还可能 panic。 |
| F-07（未关闭） | **P1 / 高** | Unix 秒与 Instant::elapsed() 年龄混算，新快照即时被判过期；实际保存路径不可达。写盘失败又会永久消费快照。 | 阻断：正式报告不能导出，取消/失败恢复未满足。 |
| F-04（残留） | **P2 / 中** | ARCHITECTURE 有冲突的重复 IPC 签名；TEST_PLAN 与 README 对脱敏/快照状态未更新；V3 测试计数低报 3 项。 | 发布文档及接口契约不一致。 |
| F-08（环境依赖） | **P2 / 中** | 当前无 OPENSSL_CONF 时通过，但 PATH 依赖 Miniconda 的 openssl.exe；CI 未声明提供 OpenSSL。 | Windows CI 可移植性尚未证明。 |

**解除发布阻断所需：** 修正 F-02 的 panic 与信息泄漏边界，确保报告不把不可可靠脱敏的任意错误文本写入导出；修正 F-07 时间基准并用生产 take、真实字节写入、取消重试、写入失败、TTL/限额、并发 ID 隔离测试；清理持久快照 ID；同步文档；在目标 Windows CI 上真实运行。安装、升级、卸载和 Release UI 仍须在隔离 Windows 环境实际验收。

本报告是本轮最终产物。没有修改产品代码、安装/启动应用、访问真实数据库、push、创建 Release 或改变系统网络配置。
