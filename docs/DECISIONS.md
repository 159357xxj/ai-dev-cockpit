# 决策记录（ADR）（AI Dev Cockpit）

格式：每条含 背景 / 决定 / 后果。状态默认"已接受"；变更时追加新条目并标注替代关系，不原地改写历史结论。
关联：`docs/ARCHITECTURE.md`（结论落地处）、`docs/TEST_PLAN.md`（验证方式）

---

## D-001 采用 Tauri 2 而非 Electron
- 背景：需要 Windows 原生能力（网络探测、系统信息、SQLite）与较小安装包，且项目核心逻辑在 Rust。
- 决定：Tauri 2 + React/TS 前端；Rust 进程独占网络、系统信息、存储与导出。
- 后果：构建依赖 MSVC + Windows SDK（本机已验证就绪，见 ENVIRONMENT.md）；WebView2 依赖系统运行时（本机 155.0.4283.62 已确认）。

## D-002 网络栈使用 Tokio + Reqwest（启用 socks 特性）
- 背景：需要异步并发、HTTPS、SOCKS5/5h 支持，且不绕过 TLS 校验。
- 决定：Reqwest 作为唯一 HTTP 客户端；SOCKS 支持通过官方 `socks` 特性启用；证书校验永不关闭。
- 后果：`library_default` 的代理源语义随版本/特性变化（见风险 R1），Phase 3 实测后固化到 ARCHITECTURE §7.2，在此之前文档只写"待实测"。

## D-003 IPC 载荷统一 snake_case
- 背景：PRD 样例（`target_ids`、`route_mode`）为 snake_case；Rust serde 默认即 snake_case。
- 决定：线上格式 snake_case；TS DTO 显式同名字段，不做运行时大小写转换；枚举一律字符串字面量。
- 后果：牺牲 TS 命名习惯换取零映射魔法；契约测试（TEST_PLAN L3）守护一致性。

## D-004 DNS / TCP / HTTPS 是三次独立测量
- 背景：把三次独立探测拼成"一次请求的分阶段耗时"是常见的诊断工具错误。
- 决定：三个探测各自独立发起、独立计时、独立展示；聚合按 `probe_type` 分开；UI 文案禁用"ICMP RTT""丢包率"。
- 后果：用户能同时看到互相矛盾的事实（如 DNS 成功 + HTTPS 超时），工具不强行合并结论。

## D-005 manual_local_proxy 仅 loopback、无凭证、绝不回落
- 背景：手动指定本地代理是本工具核心场景；任意代理输入会引入 SSRF/凭证泄露面。
- 决定：scheme ∈ {http, socks5, socks5h}，host ∈ {127.0.0.1, localhost, [::1]}，端口必填，禁止 userinfo 与 path/query；连接失败归类 `proxy_unavailable` 并明确提示，绝不静默改走直连或系统路径。
- 后果：死端口场景行为可预测（PRD §8.5）；不接受认证代理留待后续版本显式评估。

## D-006 no_app_proxy ≠ 绕过 TUN/透明代理
- 背景：Reqwest 的 no_proxy 只影响应用层；TUN/透明转发在更底层。
- 决定：UI 与报告固定展示免责声明；工具绝不声称"检测到/未开启 TUN"，不承诺绕过任何透明路径。
- 后果：杜绝误导性"绕过成功/失败"结论；TUN 检测明确列入非目标。

## D-007 HTTP 状态码中性分类，401/403 不等于"服务正常"
- 背景：401/403 只证明"HTTP 栈到端且返回了响应"，不证明登录/业务可用；429/5xx 同理。
- 决定：收到任何 HTTP 响应都不是传输层错误；`success` 仅 2xx 为 true；401/403 → `auth_required_401_403`（"收到响应，但业务能力未验证"），407 → `proxy_auth_407`，429 → `rate_limited_429`；无法归因时用 `unknown`，不强行推断。
- 后果：满足 PRD §8.3/8.4 验收；诊断建议（P4）不得据此推断 IP 信誉/账号状态。

## D-008 存储用 rusqlite + 版本化迁移，前端零 SQL
- 背景：PRD §6 数据模型；前端执行任意 SQL 是注入与越界面。
- 决定：SQLite 独占于 Rust；迁移编号只向前；全部参数化查询；前端仅经白名单 IPC 读写。
- 后果：历史数据可靠（PRD §8.8）；后端可独立测试迁移与清理策略。

## D-009 Demo/Mock 数据显式标识、发布默认关闭
- 背景：AGENTS.md 禁止假数据冒充真实结果；开发期又需要可运行界面。
- 决定：模拟数据集中在 `features/demo/`，界面常驻"演示数据"角标；由设置项控制，正式构建默认关闭。
- 后果：开发体验与数据诚实性兼得；测试断言角标存在。

## D-010 UI 简体中文；时间 UTC 存储本地展示
- 背景：PRD §2 目标用户；AGENTS.md 要求 UTC 存储。
- 决定：界面文案简体中文；IPC 与数据库一律 RFC3339 UTC；仅展示层转本地时区。
- 后果：跨时区数据可比；`format.ts` 统一时间格式化逻辑。

## D-011 指标口径拆分：传输失败率 ≠ 非 2xx 响应比例（Phase 1 修订）
- 背景：Phase 0 的 `ProbeAggregate` 只有一个 `success_count`（https 仅 2xx）及其派生的总失败率，隐含把"传输失败"（未收到 HTTP 响应）与"收到非 2xx 响应"混入同一口径。Phase 1 审核指出二者语义不同：前者反映链路问题（DNS/TCP/TLS/超时/代理），后者反映服务端给出的答案（401/403/429/5xx 等）。
- 决定：`ProbeAggregate` 增加 `transport_failure_count` 与 `http_response_count` 两个计数；展示层必须同时给出**传输失败率**（分母 = 全部样本）与**非 2xx 响应比例**（分母 = 收到响应的样本），禁止合并展示；`request_failure_rate` 保留为 PRD 总览口径并标注定义。超时样本的耗时不得进入 min/median/max 分布。逐样本互斥恒等式：`attempts_total = success_count + transport_failure_count + non_2xx_count`。
- 后果：ARCHITECTURE §5.4/§6.3 已同步修订；P4 规则引擎可基于更准确的证据分类；TEST_PLAN 增加对应聚合单测。本条修订 D-007 的落地细节，不推翻其结论。

## D-012 探测后端依赖注入预留（Phase 1）
- 背景：TEST_PLAN 要求自动化测试不依赖公网成功；需要本地 mock 注入点。同时正式构建不得因该方案获得任意 URL/端口探测能力。
- 决定：`diagnostic/backend.rs` 定义 `trait ProbeBackend`，`AppState` 持有 `Arc<dyn ProbeBackend>`；生产构建在 `lib.rs::run()` 组装 `ProductionBackend`（目标只能来自 `targets.rs` 白名单注册表与用户表单合法参数）；`MockBackend`（本地 127.0.0.1 随机端口 mock 服务器）只在 `#[cfg(test)]` 与测试代码中构造；Tauri 命令签名与前端 IPC 面不暴露注入接口。
- 后果：P2 起接入真实探测与 mock 测试无需改动业务分层；注入面收敛于组装点，攻击面不扩大。

## D-013 Tauri 命令参数显式 `rename_all = "snake_case"`（D-003 的实现细化）
- 背景：Tauri 2 命令宏默认按 **camelCase** 匹配 JS 侧 `invoke` 参数键，多单词参数（`job_id`、`run_id`、`manual_local_proxy_url`）会与本项目的 snake_case 线上契约错位。
- 决定：所有 `#[tauri::command]` 显式标注 `rename_all = "snake_case"`；Rust 枚举配合 `#[serde(rename_all = "snake_case")]`（个别变体显式 `rename`）；TS 侧 `invoke` 一律传 snake_case 键。
- 后果：宏属性存在性在 Phase 1 通过 tauri-macros 源码核查；端到端参数键行为由 Phase 2 契约测试实证（P1 无真实 IPC 数据流）。

## D-014 取消样本口径：不入失败率分母；耗时仅成功样本（Phase 2，修订 D-011 落地细节）
- 背景：Phase 2 引入真实取消后，"用户取消的样本"若计入传输失败会污染链路结论（用户主动中止 ≠ 网络故障）；同时实测发现失败样本（如拒连 ~0ms）若计入耗时分布会拉低最小值、扭曲统计。
- 决定：`ProbeAggregate` 增加 `cancelled_count`；失败率分母为 `executed_total = attempts_total - cancelled_count`；取消前未开始的任务不产生样本，在途任务产生 `cancelled` 样本并保留展示；`duration_ms` 仅在 `success=true` 时有值。恒等式更新为 `attempts_total = success + transport_failure + non_2xx + cancelled`。
- 后果：前端 metrics.ts 与 Rust types.rs 镜像测试同步修订；ARCHITECTURE §5.4/§6.3 已更新。不推翻 D-011 的拆分原则，仅细化其分母定义。

## D-015 DNS 探测使用系统标准解析器（Phase 2）
- 背景：PRD 要求"dns：本机 DNS 查询"；引入 hickory 等自研解析器会偏离系统真实行为并增加依赖。
- 决定：`probe_dns` 使用 `tokio::net::lookup_host`（系统解析器），测得耗时为"系统解析器完成一次解析的总时长"，**不是原始 DNS 报文 RTT**；展示层如实标注。TCP 探测做一次不计时前置解析（失败归类 `dns_failure`），connect 只计建连耗时。
- 后果：离线/缓存/hosts/DoH 等系统行为会影响读数——这是特性而非缺陷（反映系统真实解析路径）；测试限制：本机运营商解析器通配解析不存在域名（NXDOMAIN 重定向），DNS 失败分类无法离线稳定构造，记录为人工验证项。

## D-016 HTTPS 实现细节：native-tls、错误链分类、重定向规则注入（Phase 2）
- 背景：Reqwest 需要确定 TLS 后端与传输错误分类策略；重定向白名单规则在测试中需指向本地 mock。
- 决定：
  1. TLS 后端用 Reqwest 默认 native-tls（Windows SChannel，校验系统证书库），**永不绕过校验**；
  2. 传输错误经 reqwest 错误链识别：`is_timeout` → 超时；connect 链含 "failed to lookup address information" → `dns_failure`；`ConnectionRefused` → `tcp_failure`；其余 connect 错误（TLS 握手失败为主）→ `tls_failure`；无法归因如实 `unknown`；
  3. 重定向策略 `Policy::custom`：达上限或目标越界（非白名单主机/非 https/非 443）一律 `stop()`，保留可分类的 3xx 终态；
  4. 重定向规则经 `RedirectRules` 注入（生产锁定白名单，测试指向 mock）；`JobSpec` 经 `ProductionBackend::with_spec_builder` 注入——**注入面只在 lib.rs 组装与测试代码，IPC 面不可达**（延续 D-012）。
- 后果：E2E 测试实证 TLS 失败/拒连/越界重定向分类正确（Windows 实测）；`library_default` 的代理源语义仍留待 Phase 3 实测固化。

---

## 技术风险登记表（Phase 0 版）

| # | 风险 | 等级 | 应对 | 状态 |
|---|---|---|---|---|
| R1 | Reqwest `library_default` 代理源语义随版本/特性变化 | 中 | P3 本地 mock 实测后固化文档；此前不写死结论 | 待 P3 |
| R2 | Windows SDK / MSVC 工具链缺失导致无法构建 | 高 | 环境核查 + 临时目录链接测试 | **已排除**（SDK 在 `E:\Windows Kits\10`，链接测试通过） |
| R3 | TUN/透明代理误判误导用户 | 高 | 只出免责声明；TUN 检测列为非目标 | 设计已规避 |
| R4 | SOCKS5 与 SOCKS5h 解析位置差异被误读 | 中 | 模式说明固定展示；对比页仅同模式可比 | 设计已规避 |
| R5 | 自动化测试依赖公网导致假失败 | 中 | 测试全走本地 mock；公网仅人工检查项 | 设计已规避 |
| R6 | 本机 WebView2 注册表标准键缺失（文件系统已证实安装） | 低 | P6 复核 NSIS bootstrap 判据，不用注册表键做唯一判据 | 已记录 |
| R7 | Node 24 / Rust 1.97 较新的依赖兼容性 | 低 | 锁文件固定；CI 覆盖 | 持续观察 |

## D-017 耗时口径修订：收到响应即保留真实耗时（Phase 3，细化 D-014）
- 背景：Phase 2 曾规定"耗时仅成功样本"；Phase 3 审计指出收到 401/403/5xx 的请求同样是**完成的 HTTP 事务**，丢弃其耗时丢失了服务端响应性证据；而真正会扭曲分布的是传输失败（拒连 ~0ms）与未完成（超时/取消）。
- 决定：`duration_ms` 在"收到 HTTP 响应（任意状态码）"或"dns/tcp 成功"时有值；传输失败/超时/取消一律 None。耗时分布 = 完成事务的分布。
- 后果：types.rs 与 metrics.ts 测试同步修订；不推翻 D-014 的取消/传输失败分母规则。

## D-018 显式禁用 Reqwest 默认隐式重试（Phase 3，源码实证）
- 背景：审计发现锁定的 reqwest 0.12.28 默认把客户端包进 `tower::retry::Retry`（`Builder::default()` → `Classifier::ProtocolNacks`），对 **HTTP/2/3 协议层 NACK**（`h2::Error` 的 REFUSED_STREAM/GOAWAY 等）会静默重发请求——与"一次采样 = 恰好一次请求"契约冲突（http2 为默认特性）。
- 决定：所有模式的客户端显式 `.retry(reqwest::retry::never())`（reqwest 官方 API，文档原文"disable the Client's default behavior of retrying protocol nacks"）；连接错误/超时/HTTP 状态码本就不在默认重试范围（源码 `is_retryable_error` 仅匹配 h2/h3 错误）。测试以"mock 立即断开 → 服务端只看到 1 次连接"守护。
- 后果：采样语义严格；若未来升级 reqwest 需复查该默认值（风险登记表持续跟踪）。
- **措辞澄清（Phase 4 补强）**：`never()` 禁止的是隐式传输层重试；按白名单规则跟随 3xx 重定向与 HEAD→GET 一次性回退是显式策略下的**新请求**，不属于重试范畴。

## D-019 Windows 代理快照来源与 library_default 语义（Phase 3，源码实证 + 实测）
- 决定：
  1. 代理快照三来源分别标注：用户 WinINet 配置（`WinHttpGetIEProxyConfigForCurrentUser`）、WinHTTP 机器级配置（`WinHttpGetDefaultProxyConfiguration`）、代理相关环境变量；读取失败一律 `status="unknown"`，绝不伪造"未启用"；全量脱敏（`security::redaction`）后才出进程；PAC 只展示 URL，**不下载不执行**。
  2. `library_default` 的实际代理解析链（reqwest 0.12.28 → hyper-util 0.1.21 `Matcher::from_system`，源码实证 + 本机 mock 实测）：**`HTTPS_PROXY`/`HTTP_PROXY` > `ALL_PROXY` > 注册表 `ProxyServer`（需 `ProxyEnable=1`）**；绕行列表：**`NO_PROXY` 环境变量优先，未设时用注册表 `ProxyOverride`**（两槽位独立！）；`REQUEST_METHOD` 存在（CGI）时 env 代理全部失效；**PAC/AutoConfigURL 完全不参与**。
  3. 本机实测：系统代理开启（Clash 系 127.0.0.1:7890）且 `ProxyOverride` 含 `127.*` 时，library_default 对 127.0.0.1 目标即使设置了 `HTTPS_PROXY` 也直连（`no` 槽位被注册表例外占据）——该行为已固化为条件执行的观测测试。
- 后果：ARCHITECTURE §7.2 已固化；`no_app_proxy` 不受 env/注册表影响的反向证明完成。
- **范围限定（Phase 4 补强）**：上述优先级链结论仅对**当前锁定版本**（reqwest 0.12.28 + hyper-util 0.1.21）与**已验证环境**（Windows 11 23H2 + Clash 系系统代理 + 本注册表状态）成立；不得泛化为所有 Windows 环境或未来版本，依赖升级必须重新实测。

## D-020 规则诊断引擎：纯函数、证据分层、确定性（Phase 4）
- 背景：PRD P4 要求"可解释、确定性、离线"的诊断建议，禁止 AI/云端；建议必须区分事实与推断。
- 决定：
  1. `diagnostic/rules.rs` 为**纯函数引擎**（`diagnose(&[DiagnosticRun]) -> Vec<Diagnosis>`）：只消费结构化测量结果，无网络、无状态、无随机数，同输入必同输出（测试断言字节级一致）；
  2. 每条诊断七段式：rule_id / title / severity（info|warning|critical）/ **confidence（evidence_sufficient|possible|insufficient，仅证据充分程度，非根因概率）** / observed_facts（仅引用测量） / possible_causes（≤3 条解释）/ verification_steps（优先只读）+ limitations（不可推断事项）；
  3. 12 条核心规则 R01..R12 按固定顺序求值，target 字典序，输出按 rule_id 排序；每 (规则, 目标) 至多一条，任务级规则每任务至多一条；
  4. 取消样本不触发任何传输失败规则（D-014 延续）；传输失败与 HTTP 非 2xx 分口径；R10/R11 明确"独立测量不强行归因"；R12 用 insufficient 表达"样本不支持结论"；
  5. 禁止断言（IP 信誉/ISP 封锁/TUN 绕过/账号可登录/节点必然更快/服务器宕机）由**测试扫描**守护：标题与 possible_causes 中出现即失败（为此规则文案避免使用这些词的否定句式）。
- 后果：IPC 新增 `diagnose_runs(runs) -> Vec<Diagnosis>`（纯函数、限 16 运行）；前端结论卡只做展示与排序去重；TS DTO 同步（types.ts Diagnosis/Severity/Confidence）。
- **验收记录（2026-10-25）**：Phase 4 正式通过功能验收——用户在真实 Tauri 桌面窗口完成三项人工验证（Windows 代理快照 / GitHub 真实诊断与规则结论 / 取消恢复，全部 PASS）。**遗留**：R11 对"HTTPS 全部非 2xx 响应"场景的误判风险登记为 Phase 5 开工前置项（详见 TEST_PLAN Phase 4 验收小节），持久化实施前必须先验证并处理。

## D-021 规则引擎专项修复：R11/R10 的口径修正（Phase 5 前置，2026-10-25）
- 背景：Phase 4 验收通过的遗留待复核项——R11 以 `https_successes()`（仅 2xx）判定"零成功"，
  混合结果（收到非 2xx 响应 + 传输失败并存）会误称"均未获得 HTTP 响应"，与 D-007/D-011 矛盾；
  R10 以 `executed > success` 推断"存在传输失败"，把非 2xx 响应混入传输失败口径，且未要求跨模式。
- 决定（测试先行：7 项回归测试先复现 4 项失败，再修复）：
  1. R11 触发条件 = 基线正常 ∧ executed>0 ∧ **全部已执行样本均为传输失败**（无任何样本收到
     HTTP 响应）；事实明确"全部为传输层失败"；非 2xx 场景归 R05/R07/R08。
  2. R10 差异判定改用 `transport_failure_count`；要求参与比较的运行覆盖 ≥2 种不同 route_mode；
     事实行含各模式传输失败计数与中文模式标签。
  3. 规则测试夹具 `status_sample` 对齐 D-017（非 2xx 保留真实耗时）。
- 后果：rules_engine 31 项全绿（24 原有 + 7 回归，原有测试无一修改断言）；
  全基线复跑通过（cargo 102 / npm 35 / 八命令）。引擎层对多运行同目标的逐运行规则仍可能
  产生同 rule_id 重复条目（由前端 sortAndDedup 去重展示），引擎级合并留待后续迭代。

## D-022 SQLite 持久化与脱敏导出（Phase 5，2026-10-25）
- 决定：rusqlite bundled；runs/results/settings 三表 + user_version 版本化迁移（只向前、幂等）；全 SQL 参数化；新增 8 个白名单 IPC 命令；运行结束经 `on_run_complete` 钩子事务写入（注入面收敛于 lib.rs，D-012 延续）；保留策略默认 30 天/500 次、settings 白名单键覆盖、启动与保存后执行；导出先预览（含局限说明）后写应用数据目录 exports/（零上传）；`sanitize_text` 遮蔽 userinfo/敏感查询参数/本地用户路径/Authorization 头。
- 修复缺陷：遮蔽循环同一匹配点无限重复（位置推进修复，storage 测试复现）；rusqlite 无 u64（i64 转换）。
- 后果：Rust 110 项全绿（新增 storage 8 项）；存储不可用时其余功能不受影响（命令返回 internal）。

## 变更流程
新决策追加新编号条目（如 D-011）；推翻旧决策时新条目标注"替代 D-xxx"，旧条目状态改为"已替代（见 D-xxx）"。
