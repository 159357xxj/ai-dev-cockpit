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

## 变更流程
新决策追加新编号条目（如 D-011）；推翻旧决策时新条目标注"替代 D-xxx"，旧条目状态改为"已替代（见 D-xxx）"。
