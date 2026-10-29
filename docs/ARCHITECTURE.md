# 架构设计（AI Dev Cockpit）

版本：v1.1（对齐 v0.1.0 Beta 代码状态）｜更新日期：2026-10-29｜上游文档：`PRD.md`、`AGENTS.md`
关联文档：`docs/TEST_PLAN.md`（验证策略）、`docs/DECISIONS.md`（决策记录）、`docs/ENVIRONMENT.md`（环境核查）

---

## 1. 系统总览

Tauri 2 双进程模型。**所有网络探测、系统信息读取、数据持久化、脱敏导出只在 Rust 侧发生**；前端（WebView）仅通过白名单 IPC 调用，永不直接访问网络或系统 API。

```
┌───────────────────────────── WebView (React + TS) ─────────────────────────────┐
│  pages: Dashboard / Diagnostics / Compare / History / Settings                 │
│  features: diagnostics / proxy / history        lib: api.ts / types.ts / format│
└───────────────▲───────────────────────────────▲────────────────────────────────┘
        invoke() 白名单命令                listen() 事件订阅
        (snake_case JSON)                 (diagnostic://*)
┌───────────────▼───────────────────────────────▼────────────────────────────────┐
│                            Rust Core (Tauri 2, Tokio)                          │
│  commands/{system,diagnostics,history,report}     ← IPC 入口 + 参数二次校验      │
│  diagnostic/{scheduler,dns,tcp,https,rules,types} ← 探测编排与执行               │
│  platform/windows/{proxy,network}                 ← 只读系统信息                 │
│  storage/{mod,migrations,models}                  ← SQLite（Phase 5）           │
│  security/{validation,redaction}                  ← 输入校验 / 导出脱敏          │
└────────────────────────────────────────────────────────────────────────────────┘
```

信任边界：
- 前端不可信：所有命令入参在 Rust 侧二次校验（目标 ID 白名单、代理 URL 规则、次数与超时上限）。
- 探测目标只允许预设域名列表（HTTPS 443），不接受任意 URL/主机/端口输入（SSH 面板为远期独立评估项）。
- 应用绝不请求管理员权限，绝不写系统网络配置。

## 2. 后端模块设计

```
src-tauri/src/
  main.rs / lib.rs            # Tauri Builder、状态注入、命令注册
  commands/
    system.rs                 # get_system_snapshot
    diagnostics.rs            # start_diagnostic / cancel_diagnostic / 事件推送
    history.rs                # list_runs / get_run / delete_run / clear_history（P5）
    report.rs                 # preview_sanitized_report / export_sanitized_report（P5）
  diagnostic/
    types.rs                  # 全部 DTO、枚举、错误分类（serde 双向）+ 请求校验 + 聚合计算
    targets.rs                # 预设目标白名单注册表（id → 域名/URL），探测目标唯一来源
    backend.rs                # trait ProbeBackend + 生产/桩实现装配（依赖注入缝，见 2.1）
    scheduler.rs              # 任务编排：并发上限、每请求超时、全局截止、取消令牌
    dns.rs                    # 独立 DNS 查询测量（标准解析器）
    tcp.rs                    # 独立 TCP connect 测量
    https.rs                  # Reqwest HTTPS 请求（三种路由模式、受限重定向/体积）
    rules.rs                  # 规则诊断引擎（Phase 4，纯函数：diagnose(runs)→Vec<Diagnosis>，R01..R12）
  platform/windows/
    proxy.rs                  # 只读读取用户级 WinHTTP/注册表代理与 PAC 状态（Phase 3）
    network.rs                # 适配器/DNS 摘要（合法只读 API）
  storage/
    mod.rs migrations.rs models.rs   # rusqlite、版本化迁移（Phase 5）
  security/
    validation.rs             # 手动代理 URL 校验等
    redaction.rs              # 导出脱敏（userinfo/query token/认证头/本地路径）
```

要点：
- `diagnostic/` 与 `platform/windows/` 严格隔离：探测逻辑不 import 平台模块，平台模块不做网络请求。
- `scheduler` 持有 `CancellationToken`（tokio_util），取消后剩余样本记为 `cancelled`，绝不补齐或伪造。

### 2.1 探测后端依赖注入（Phase 1 预留，见 D-012）
- `diagnostic/backend.rs` 定义 `trait ProbeBackend`（Phase 1 为最小形态，Phase 2 扩充为 dns/tcp/https 具体探测方法与调度入参）；`AppState` 持有 `Arc<dyn ProbeBackend + Send + Sync>`，命令层只经由该 trait 调用探测能力。
- 生产构建在 `lib.rs::run()` 组装 `ProductionBackend`：其探测目标**只能来自 `targets.rs` 白名单注册表**与用户表单中的合法参数（次数/超时/loopback 代理），不存在"任意 URL/端口"入口。
- `MockBackend`（指向本地 mock 服务器、绑定 127.0.0.1 随机端口）只在 `#[cfg(test)]` 与集成测试代码中构造；注入点收敛于 `lib.rs` 组装处与测试代码，**Tauri 命令签名与前端 IPC 面不暴露任何注入接口**——正式构建不会因该方案获得任意探测能力。

## 3. 前端模块设计

```
src/
  main.tsx  app/{App.tsx, router.tsx, ErrorBoundary, ThemeProvider}
  pages/{Dashboard,Diagnostics,Compare,History,Settings}/
  components/{MetricCard,ResultTable,StatusBadge,TrendChart}/
  features/{diagnostics,proxy,history}/        # 业务状态与领域逻辑
  lib/{api.ts,types.ts,format.ts}              # api.ts 封装 invoke/listen；types.ts 与 Rust types.rs 一一对应
  tests/                                       # Vitest + Testing Library
```

约定：
- TypeScript `strict`，DTO 字段显式 snake_case 与线上格式一致（见 §5），禁止无理由 `any`。
- `lib/api.ts` 是唯一允许调用 Tauri IPC 的模块；页面/组件不得直接 `invoke`。
- Demo/Mock 数据放在 `features/demo/`，界面常驻"演示数据"角标；发布版默认关闭（DECISIONS D-009）。
- 时间一律 UTC 存储/传输，仅展示层转本地时区。

## 4. 预设诊断目标（P0 冻结）

| target_id | 域名 | 探测地址（HTTPS） | 说明 |
|---|---|---|---|
| `chatgpt` | chatgpt.com | `https://chatgpt.com/` | ChatGPT 网页入口 |
| `openai-api` | api.openai.com | `https://api.openai.com/` | OpenAI API 入口（不携带任何密钥） |
| `openai-auth` | auth.openai.com | `https://auth.openai.com/` | 登录域 |
| `claude` | claude.ai | `https://claude.ai/` | Claude 网页入口 |
| `github` | github.com | `https://github.com/` | GitHub |
| `zcode` | zcode.z.ai | `https://zcode.z.ai/` | ZCode |

规则：仅 HTTPS 443；请求不携带 Cookie、Authorization、任何用户凭证；请求头固定为最小集合（含 UA `ai-dev-cockpit/<version>`）。目标清单调整属于显式代码变更。

## 5. IPC 通信契约

### 5.1 通用约定
- 传输：Tauri 2 `invoke` / `emit`，载荷为 JSON；**字段名统一 snake_case**（与 PRD 样例一致；Rust 结构体使用默认 serde 命名，TS 接口显式同名字段，不做运行时大小写转换）。
- **命令参数命名（Phase 1 修订，见 D-013）**：Tauri 2 命令宏默认按 camelCase 匹配 JS 侧参数键，多单词参数（`job_id`/`run_id`）会与契约错位。因此**所有 `#[tauri::command]` 显式标注 `rename_all = "snake_case"`**，使 JS 侧 `invoke` 参数键与 DTO 字段同为 snake_case；Rust 枚举另加 `#[serde(rename_all = "snake_case")]`（个别变体用显式 `#[serde(rename)]`）保证字面量一致。宏属性的存在性在 Phase 1 以 tauri-macros 源码核查；端到端键名行为由 Phase 2 契约测试实证。
- 命令返回统一为 `Result<T, IpcError>`；`IpcError { code: "invalid_input" | "not_found" | "not_available" | "internal" | "cancelled", message: string, detail?: string }`。`not_available` 为 Phase 1 新增：功能尚未实现或当前模式下不可用，前端据此展示"将在后续阶段提供"空态，而非错误假象。
- 所有枚举以字符串序列化（如 `"no_app_proxy"`），前后端共用同一份枚举字面量。

### 5.2 命令清单

| 命令 | 签名 | 引入阶段 |
|---|---|---|
| `get_system_snapshot` | `() -> Result<SystemSnapshot, IpcError>` | P1（桩）/P2（真实） |
| `get_proxy_snapshot` | `() -> Result<ProxySnapshot, IpcError>` | P3 |
| `start_diagnostic` | `(request: DiagnosticRequest) -> Result<JobHandle, IpcError>` | P1（桩）/P2（真实） |
| `cancel_diagnostic` | `(job_id: string) -> Result<CancelOutcome, IpcError>` | P2 |
| `diagnose_runs` | `(runs: DiagnosticRun[]) -> Result<Diagnosis[], IpcError>` | P4 |
| `list_runs` | `(query: RunQuery) -> Result<RunSummary[], IpcError>` | P5 |
| `get_run` | `(run_id: string) -> Result<DiagnosticRun, IpcError>` | P5 |
| `delete_run` | `(run_id: string) -> Result<bool, IpcError>` | P5 |
| `clear_history` | `() -> Result<u32, IpcError>` | P5 |
| `get_setting` | `(key: string) -> Result<string\|null, IpcError>` | P5 |
| `set_setting` | `(key: string, value_json: string) -> Result<(), IpcError>` | P5 |
| `preview_sanitized_report` | `(run_id: string, format: "json"\|"markdown") -> Result<ReportPreview, IpcError>`（含不透明 `snapshot_id`） | P5 |
| `export_sanitized_report` | `(run_id: string, format: "json"\|"markdown", snapshot_id: string) -> Result<ExportResult, IpcError>`（F-07：复用预览快照的同一份脱敏字节；快照单次使用、TTL 600s、≤8 份/≤8MiB、取消与写盘失败可重试；目标已存在时返回 `target_exists`） | P5 |

### 5.3 事件

| 事件 | 载荷 | 说明 |
|---|---|---|
| `diagnostic://progress` | `{ job_id, target_id?, probe_type?, attempt?, percent, status, message? }` | status ∈ `running/finished/failed/cancelled` |
| `diagnostic://completed` | `{ job_id, run: DiagnosticRun }` | 终态，含完整结果 |
| `diagnostic://failed` | `{ job_id, error: IpcError }` | 终态，任务级失败（区别于单样本失败） |

### 5.4 核心 DTO

```rust
// —— 请求 ——
struct DiagnosticRequest {
  target_ids: Vec<String>,            // 1..=6 个，必须命中预设白名单，去重
  route_mode: RouteMode,              // 见 §7
  manual_local_proxy_url: Option<String>, // 仅 manual_local_proxy 时必填，规则见 §7
  repeats: u8,                        // 仅允许 3 或 5
  timeout_ms: u32,                    // 1000..=10000，缺省 10000
}

// —— 测量样本（一次探测 = 一条）——
struct ProbeSample {
  target_id: String,
  probe_type: ProbeType,              // "dns" | "tcp" | "https"
  route_mode: RouteMode,
  attempt_no: u32,                    // 1 起
  success: bool,                      // https: 仅 2xx 为 true；dns/tcp: 成功即 true
  duration_ms: Option<u64>,           // None = 未完成（取消/任务失败）
  http_status: Option<u16>,           // 仅 https
  http_outcome: Option<HttpOutcome>,  // 仅 https，见 §6
  error: Option<DiagnosticError>,     // 仅传输层失败；收到 HTTP 响应 ≠ 错误
  tested_at: String,                  // RFC3339 UTC
}

// —— 每目标 × 每探测类型聚合（D-011 口径 + Phase 2 D-014 取消口径）——
struct ProbeAggregate {
  probe_type: ProbeType,
  attempts_total: u32,                // 全部样本（含已取消）
  cancelled_count: u32,               // 用户取消/截止未完成：不进入任何失败率分母（D-014）
  success_count: u32,                 // https: 仅 2xx；dns/tcp: 成功
  transport_failure_count: u32,       // 传输层失败：未收到任何 HTTP 响应（不含已取消样本）
  http_response_count: u32,           // 收到 HTTP 响应的样本数（任意状态码；仅 https 可能 >0）
  request_failure_rate: f64,          // (executed_total - success_count) / executed_total
  duration_min_ms / duration_median_ms / duration_max_ms: Option<u64>, // 仅成功样本有耗时
}
// 逐样本互斥分类（恒等式）：
//   attempts_total = success_count + transport_failure_count + non_2xx_count + cancelled_count
//   其中 non_2xx_count = http_response_count - success_count（仅 https 有意义；dns/tcp 恒为 0）
//   executed_total = attempts_total - cancelled_count（各失败率的分母）
// 派生比率由展示层计算，分母不同，禁止合并成一个口径：
//   传输失败率     = transport_failure_count / executed_total   （链路通不通；全取消时为"不适用"）
//   非 2xx 响应比例 = non_2xx_count / http_response_count        （服务端给出什么答案）

struct DiagnosticRun {
  run_id: String,                     // UUID v4
  started_at: String, finished_at: Option<String>,  // UTC
  status: RunStatus,                  // "running" | "completed" | "cancelled" | "failed"
  route_mode: RouteMode, repeats: u8, app_version: String,
  targets: Vec<TargetResult>,         // target_id + Vec<ProbeSample> + Vec<ProbeAggregate>
}

struct SystemSnapshot { os_version, arch, app_version, adapters: Vec<AdapterSummary>, dns_servers: Vec<String>, captured_at }
struct AdapterSummary { name, adapter_type?, status_up: bool, ipv4?: String }  // 非敏感摘要，导出时 IP 默认移除
struct ProxySnapshot { user_proxy: UserProxyInfo, env_vars: Vec<EnvVarInfo>, captured_at } // 值均脱敏
```

### 5.5 校验规则（Rust 侧为准，前端仅做同规则预检）

- `target_ids` 非空且全部命中 §4 白名单，去重后 ≤6。
- `repeats ∈ {3,5}`；`timeout_ms ∈ [1000,10000]`。
- `route_mode = manual_local_proxy` 时 `manual_local_proxy_url` 必填，且必须满足：scheme ∈ {`http`,`socks5`,`socks5h`}；host ∈ {`127.0.0.1`,`localhost`,`[::1]`}；端口必填 1–65535；禁止 userinfo；禁止 path/query/fragment。违规 → `invalid_input`，任务不启动。
- `route_mode ≠ manual_local_proxy` 时若携带该字段 → `invalid_input`。
- 同一 UI 时刻至多一个活动任务；后端允许并发任务但每任务独立截止时间。

## 6. 错误分类（冻结，Phase 2 实现）

**核心原则：收到 HTTP 响应不是错误**。传输层失败才产生 `DiagnosticError`；任何 HTTP 状态码都属于"测量事实"。

### 6.1 ErrorKind（传输层）

| kind | 触发 | 中性文案方向 |
|---|---|---|
| `dns_failure` | 域名解析失败（仅 dns/https 解析阶段） | 本机 DNS 未能解析该域名；不代表服务异常 |
| `tcp_failure` | TCP connect 失败/拒绝（独立测量） | 无法建立到目标 443 的 TCP 连接 |
| `tls_failure` | TLS 握手失败/证书错误（不绕过校验） | TLS 握手未完成；可能是网络拦截或证书问题 |
| `timeout` | 单请求超时（≤10s）或任务截止 | 在限定时间内未完成本次测量 |
| `cancelled` | 用户取消或任务截止取消 | 任务已被取消，本次样本无效 |
| `proxy_unavailable` | manual_local_proxy 连不上（绝不回落） | 指定的本地代理不可用，未产生直连结果 |
| `invalid_input` | 参数校验失败 | 请求参数不合法 |
| `internal` | 本应用内部错误 | 工具内部错误（附脱敏 detail） |
| `unknown` | 无法可靠归因 | 原因未明，不强行推断 |

### 6.2 HttpOutcome（HTTPS 收到响应时的细分，success 仅 2xx）

| outcome | 状态 | UI 语义 |
|---|---|---|
| `ok_2xx` | 2xx | 收到响应（success=true） |
| `redirect_3xx` | 3xx（重定向上限耗尽后仍为 3xx） | 收到响应，未跟随更多重定向 |
| `auth_required_401_403` | 401/403 | **收到响应，但业务能力未验证**——绝不标绿 |
| `proxy_auth_407` | 407 | 代理要求认证（与手动代理"无凭证"策略一致，预期失败） |
| `rate_limited_429` | 429 | 收到限流响应 |
| `server_error_5xx` | 5xx | 服务端返回错误响应 |
| `client_error_other_4xx` | 其他 4xx | 收到其他客户端错误响应 |

禁止事项：不把 401/403/429/5xx 翻译成"服务正常/异常"的二元结论；不把 `tcp` 耗时称作 ICMP RTT；不把"请求失败率"称作"丢包率"。

### 6.3 指标口径（D-011 + Phase 2 D-014 修订）
- **传输失败率** = `transport_failure_count / executed_total`：样本未到达 HTTP 响应层（DNS/TCP/TLS 失败、超时、代理不可用）的比例，衡量"链路通不通"。**用户取消与任务截止不计入分母与分子**（D-014）。
- **非 2xx 响应比例** = `non_2xx_count / http_response_count`：在**收到响应**的样本中非 2xx 状态码的占比，衡量"服务端给出什么答案"。
- 两者分母不同（`executed_total` vs `http_response_count`），**禁止合并为一个"失败率"口径展示**。PRD 的 `request_failure_rate`（`(executed-success)/executed`）作为总览指标保留，但界面必须同时给出上述两个拆分口径。
- **耗时分布只包含成功样本**（`duration_ms` 仅在 `success=true` 时有值）：失败/超时/取消一律为 None——失败瞬时返回（如拒连 ~0ms）不得拉低最小值。
- 取消语义（Phase 2 冻结）：取消/截止前**未开始**的任务不产生样本；**在途**任务产生 `cancelled` 样本并保留展示；已完成的样本原样保留。

## 7. 探测语义与路由模式（冻结）

### 7.1 三次独立测量
- `dns`：标准解析器对本域名的查询耗时。**不代表 HTTPS 请求实际经历的解析路径**（连接复用/缓存/DoH 均可能造成差异）。
- `tcp`：客户端对目标 `host:443` 的独立 TCP connect 耗时。独立于任何代理模式。
- `https`：完整 HTTP(S) 请求耗时（含连接建立）。禁止把三次测量拼装成"一次请求的分阶段耗时"。

### 7.2 路由模式（仅影响 HTTPS；DNS/TCP 永远是本机独立基线）

| mode | 行为 | 必须展示的说明 |
|---|---|---|
| `library_default` | Reqwest 0.12.28 默认（`system-proxy` 特性默认启用）。**优先级链（源码实证 + mock 实测，D-019）**：`HTTPS_PROXY`/`HTTP_PROXY` > `ALL_PROXY` > 注册表 `ProxyServer`（需 `ProxyEnable=1`）；绕行：`NO_PROXY` env 优先，未设时用注册表 `ProxyOverride`（两槽位独立）；CGI 环境（`REQUEST_METHOD` 存在）env 全失效；**PAC 不参与** | **结论限定范围（D-019）**：仅适用于当前锁定的 reqwest 0.12.28 + hyper-util 0.1.21 与已验证环境（Windows 11 23H2 + Clash 系系统代理），不得泛化为所有 Windows 环境或其它 reqwest 版本；升级依赖必须复测。也不保证与任何特定桌面应用行为一致 |
| `no_app_proxy` | 构建客户端时显式 `no_proxy()` | 仅指不使用应用层显式代理；TUN/透明代理仍可能生效；**绝不声称"未经过代理"** |
| `manual_local_proxy` | 仅 HTTPS 走用户指定 loopback 代理 | 仅接受 `http://`、`socks5://`、`socks5h://`（socks5=本地解析域名，socks5h=由代理解析域名，差异须展示）；代理失败 → `proxy_unavailable`，**绝不静默回落**；DNS/TCP 显示"不适用（独立基线）" |

### 7.3 请求策略
- 方法：HEAD 优先；405 时允许**一次** GET 回退，响应体读取上限 64 KiB 后丢弃。
- 重定向：最多 5 次；到达上限仍 3xx → `redirect_3xx`。
- 超时：单请求 `timeout_ms`（≤10s）；任务全局截止 120s；HTTPS 并发 ≤3。
- 重试语义（D-018 措辞澄清）：`.retry(never())` 禁止的是**隐式传输层重试**（同一次采样内静默重发请求，包括 reqwest 默认的 h2/h3 协议 NACK 重发）；**协议内的合法行为不属于重试**——按白名单规则跟随 HTTP 3xx 重定向（最多 5 次，每次是新的合法请求）与 HEAD 被拒后的一次 GET 回退（显式策略、可观测、计入采样事实）均被允许。同一采样绝不静默产生第二次传输尝试（测试守护：服务端立即断开 → 只看到 1 次连接）。
- 取消：`CancellationToken`，取消后未完成样本记 `cancelled`，已落库数据不回填、不伪造。
- manual_local_proxy 不可达：`proxy_unavailable`（含代理 407 / SOCKS 连接失败），绝不回落直连（D-005，mock 实测守护）。

### 7.5 规则诊断引擎（Phase 4，D-020）
- `rules.rs::diagnose(runs)`：纯函数，输入 1..16 个运行结果，输出确定性诊断列表。
- 规则清单：R01 DNS 失败 / R02 TCP 失败 / R03 TLS 失败 / R04 HTTPS 超时 / R05 401·403（中性）/
  R06 407 / R07 429 / R08 5xx / R09 指定代理不可用（模式门控） / R10 跨路径不一致（跨运行） /
  R11 基线正常但 HTTPS 全失败 / R12 样本不足或取消（insufficient）。
- 置信分层：全部执行且一致失败 → evidence_sufficient；部分失败/跨路径差异 → possible；
  取消/样本不足 → insufficient。事实（observed_facts）只引用测量；解释（possible_causes）≤3；
  每条必附 limitations。禁止断言（信誉/封锁/绕过/登录/节点更快/宕机）由测试扫描守护。
- IPC：`diagnose_runs(runs) -> Vec<Diagnosis>`（白名单命令，无网络）。前端结论卡仅排序、
  去重与展示；空结果不渲染。
- **验收状态（2026-10-25）**：Phase 4 已通过功能验收（真机三项验证 PASS）。
  R11 非 2xx 误判风险已通过专项修复关闭（2026-10-25，见 TEST_PLAN 与 ADR D-021）：
  触发条件改为"全部已执行样本均为传输失败"，非 2xx 响应归 R05/R07/R08 覆盖。

### 7.4 Windows 代理配置快照（Phase 3）
三来源分别检测、分别标注（`platform/windows/proxy.rs`，全部只读 API）：
1. 用户 WinINet 配置（`WinHttpGetIEProxyConfigForCurrentUser`）：手动代理 / PAC URL / 自动检测；
2. WinHTTP 机器级配置（`WinHttpGetDefaultProxyConfiguration`）；
3. 环境变量（`HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY`/`NO_PROXY` 及小写，存在状态 + 脱敏值）。
读取失败 → `status="unknown"`（不伪造"未启用"）；所有值经 `security::redaction` 脱敏出进程；PAC 仅展示 URL 不下载不执行；**配置存在 ≠ 真实请求使用**（各应用解析规则不同）。

## 8. 存储设计（Phase 5 实现）

```
runs(id TEXT PK, started_at TEXT, finished_at TEXT?, status TEXT, route_mode TEXT,
     repeats INTEGER, app_version TEXT, notes TEXT?)
results(id INTEGER PK AUTOINCREMENT, run_id TEXT FK, target_id TEXT, probe_type TEXT,
        attempt_no INTEGER, route_mode TEXT, success INTEGER, duration_ms INTEGER?,
        http_status INTEGER?, http_outcome TEXT?, error_kind TEXT?, error_summary TEXT?,
        error_detail TEXT?, tested_at TEXT)
settings(key TEXT PK, value_json TEXT)          -- 不储存任何凭证
索引：idx_results_run(run_id)、idx_results_target(target_id)、idx_runs_started_at(started_at)
```
- 时间全部 UTC RFC3339；UI 转本地展示。
- 数据库由 Rust 通过 Tauri `app_data_dir()` 下的 `history.db` 打开；迁移使用 `PRAGMA user_version` 版本化、只向前且幂等；WAL 固定开启（`journal_mode=WAL`）。
- 写入事务保证"completed 状态的 run 必含全部已采样本"；取消/失败的 run 状态如实标注。
- 保留策略：30 天或 500 个会话（可配置），删除/清空需用户确认。

## 9. 安全与隐私边界（与 AGENTS.md 对齐）

1. 只读诊断：不修改代理/DNS/路由/防火墙/注册表/hosts；不请求管理员权限；无任意命令执行入口；Tauri capability 按需最小化。
2. 不采集：Cookie、API Key、OAuth token、代理订阅、SSH 私钥、浏览器会话。
3. 手动代理仅 loopback 且不带凭证；输入框不允许任意命令/任意地址。
4. 导出：仅序列化 Rust 侧白名单 DTO，不含原始错误摘要/详情或引擎自由文本；预览后由原生对话框选择本地路径。保存使用同目录临时文件与 `persist_noclobber` 原子提交，不覆盖已有文件（返回 `target_exists`）；历史记录与导出文件不上传。
5. 启动离线可用；首次启动不自动联网、不扫描；网络请求仅由用户点击触发。
6. 代理快照展示值一律脱敏（隐藏 userinfo/token）。

## 10. 技术风险评估

| # | 风险 | 等级 | 缓解 | 状态 |
|---|---|---|---|---|
| 1 | `library_default` 的代理源语义随 Reqwest 版本/特性变化，易在文档中写错 | 中 | Phase 3 对锁定依赖和 Windows 环境用本地 mock 实测，结论固化于 §7.2 / D-019 | 已处理；依赖升级后需复验 |
| 2 | Windows SDK 缺失导致链接失败 | 高→**已排除** | 链接测试通过；SDK 位于 `E:\Windows Kits\10`（26100） | 已关闭 |
| 3 | TUN/透明代理不可可靠检测，UI 若断言会误导 | 高 | 只出免责声明，绝不声称"检测到/未开启 TUN" | 设计已规避 |
| 4 | SOCKS5 与 SOCKS5h 域名解析位置不同，结果不可比却易被误读 | 中 | 模式说明固定展示差异；对比页仅比较同模式样本 | 设计已规避 |
| 5 | 公网目标结果天然不稳定，自动化测试会假失败 | 中 | 自动化测试全部使用本地 mock 服务器；公网探测仅人工检查项 | 设计已规避 |
| 6 | WebView2 注册表标准检测键缺失（本机已证实） | 低 | 文件系统证据已记录；CI NSIS 打包和安装版 WebView2 行为未运行（Gate B WAIVED） | 未验证 |
| 7 | Node 24 / Rust 1.97 较新的依赖兼容性 | 低 | 锁文件固定依赖；Rust 工具链由 `rust-toolchain.toml` 固定为 1.97.1，CI 使用 Node 22 | 持续观察 |

## 11. Phase 1 最小文件清单与验收命令

```
package.json  vite.config.ts  tsconfig.json  tailwind 配置  index.html  .gitignore(已有)
src/main.tsx  src/app/{App.tsx,router.tsx}  src/pages/{Dashboard,Diagnostics,Compare,History,Settings}/
src/components/{MetricCard,ResultTable,StatusBadge}/  src/lib/{api.ts,types.ts,format.ts}
src/tests/（≥3 个可运行 UI 测试）
src-tauri/{Cargo.toml,tauri.conf.json,build.rs,src/main.rs,src/lib.rs,src/commands/system.rs}
              （类型化命令桩：get_system_snapshot / start_diagnostic / cancel_diagnostic，不实现假联网）
README.md（更新：开发命令、环境需求、已实现/未实现清单）
```

验收命令（真实执行并记录输出）：`npm install`（安装前征求确认）、`npm run dev`、`npm run lint`、`npm run typecheck`、`npm run test`、`npm run tauri dev`。

## 12. 阶段实施顺序

P1 脚手架与五页界面（Demo 数据显式标识）→ P2 真实 DNS/TCP/HTTPS + 进度/取消/错误分类 → P3 Windows 代理快照 + 三模式对比 → P4 规则诊断引擎（离线、可解释）→ P5 SQLite 历史/趋势/脱敏导出 → P6 测试/NSIS 打包/CI/README 发布。与 PRD §9 一致，每阶段结束创建 Git 提交并 push。
