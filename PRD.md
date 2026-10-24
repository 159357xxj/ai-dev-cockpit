# AI Dev Cockpit — 开发者网络诊断台

版本：v1.0 需求与架构草案｜2026-10-24

## 1. 产品目标
为 Windows 10/11 开发者提供本地优先、无需登录、只读的网络诊断桌面工具。解决 AI 服务、GitHub 与开发工作流出现网络故障时，DNS/TCP/HTTPS/代理路径等层面的问题定位困难。第一版不提供系统修复、不代替认证检查、不承诺能够诊断所有 TUN、VPN 或应用内部请求。

**目标用户**：Windows 上使用 ChatGPT、Codex、Claude、GitHub、SSH 等的学生与开发者。

**北极星场景**：当 Codex 无法连通时，用户选择 API/登录入口目标，点击诊断，看到分阶段结果；再选择本地 HTTP/SOCKS5 代理做 HTTPS 路径对比；最后获得有证据、有局限说明的故障建议，并能导出脱敏报告。

## 2. 范围与优先级

### P0（基础可用）
1. Windows 桌面应用，简体中文，深色/浅色主题，离线可打开主界面。
2. 本机网络概览：活跃网络适配器的非敏感摘要、当前 DNS、Windows 用户代理设置快照（手动代理、PAC 是否配置）、可识别的代理环境变量（显示时脱敏）。不准确声称 TUN 开启/关闭。
3. 预设目标：`chatgpt.com`、`api.openai.com`、`auth.openai.com`、`claude.ai`、`github.com`、`zcode.z.ai`。使用 HTTPS 安全地址；不发送 API 密钥、登录信息或 Cookies。
4. 独立的 DNS 查询、TCP 443 建连测试、HTTPS 请求测试。DNS/TCP 默认走操作系统路由且无应用层显式代理；标注指标定义与各自独立的探测事实，不能把三次探测伪装成一次 HTTP 请求的内部逐阶段耗时。
5. HTTPS 连接模式：`library_default`（HTTP 库默认路径，可能读取环境/系统代理）、`no_app_proxy`（HTTP 客户端不配置代理；依旧可能经过 TUN/透明路由）、`manual_local_proxy`（HTTP/SOCKS5/SOCKS5h 127.0.0.1/localhost/[::1]:端口；不接受认证信息）。确保代理失败不静默回落到其他路径。
6. 多次请求采样（默认 3 次，可选 5 次）：成功次数、请求失败率、最小/中位/最大耗时。不是 ICMP Ping 或数据包丢包率；每请求最大 10 秒，最多并发 3，整个运行有截止时间并支持取消。
7. 在不同代理模式下只比较 HTTPS 测试；DNS/TCP 属于独立本机基线，不错误标注为已通过 HTTP/SOCKS 代理。
8. 系统状态与请求状态严格区分：2xx/3xx、401/403、407、429、5xx、TLS 失败、超时、DNS 失败、TCP 失败等均给出中性准确说明，不将 403 归类为服务功能正常。

### P1（可发布）
9. SQLite 保存诊断会话、目标、连接模式、时间、脱敏后的指标和故障分类；30 天或最多 500 个会话的本地保留策略，用户可清除。
10. 历史趋势、最近两次运行对比，明确展示仅有相同目标/模式/采样参数时可比较。
11. 规则驱动的诊断建议引擎；建议必须附事实与限制，不能推断 ISP 封锁、IP 污染或账号状态。
12. 导出匿名 JSON 与 Markdown 报告，预览并二次确认；清除代理认证、令牌、主机用户名和本地敏感路径；本机地址如非必要不导出。
13. README、构建脚本、Rust/前端测试、Windows 真实运行检查、Windows 安装包（优先 NSIS）。

### P2（后续版本，不进入 MVP）
- SSH 目标状态面板与自定义端口探测（仅用户授权配置，严禁范围扫描）；Windows WinHTTP 只读状态；ICMP Ping；定时后台监控；通知；真实应用日志导入；AI 云端总结；macOS/Linux；自动更新、崩溃遥测、云同步；更复杂的 PAC 解析和 TUN 检测；系统代理修改与自动修复。

## 3. 技术选型
- **Tauri 2 + Rust**：Windows 原生核心与 IPC 边界。
- **React + TypeScript + Vite**：界面。Tailwind CSS + shadcn/ui + Lucide；图表 Recharts。
- **Rust Tokio + Reqwest（含 SOCKS 支持）**：异步 DNS/TCP/HTTPS 测量。Windows 系统信息用合法、只读的 API 或范围受限的注册表读取，按需要引入兼容库。
- **SQLite + rusqlite**：后端独占数据库写入，使用明确版本迁移；前端不能执行任意 SQL。
- **Vitest + React Testing Library；cargo test**：单元/集成验证；本地模拟 HTTP 服务器，不依赖公网成功作为自动化测试通过的条件。
- **Git + GitHub Actions**：CI，Windows 构建与质量检查。

## 4. 页面信息架构
- **总览 Dashboard**：系统状态摘要、最近诊断、快捷按钮、重要限制说明。
- **服务诊断 Diagnostics**：选择目标、运行模式、次数、开始/取消、每目标阶段指标、错误解释。
- **路径对比 Compare**：对同一 HTTPS 目标用不同请求模式重复测量；展示样本量、成功率与中位数，异常注明无法推断真实底层路由。
- **历史 History**：按时间/目标筛选，可查看、对比、删除、导出。
- **设置 Settings**：主题、采样、超时、数据保留、默认本地代理端口、隐私说明。

## 5. 后端与 IPC
建议模块：
```
src-tauri/src/
  lib.rs
  commands/{system.rs,diagnostics.rs,history.rs,report.rs}
  diagnostic/{dns.rs,tcp.rs,https.rs,scheduler.rs,rules.rs,types.rs}
  platform/windows/{proxy.rs,network.rs}
  storage/{mod.rs,migrations.rs,models.rs}
  security/{validation.rs,redaction.rs}
src/
  app/{App.tsx,router.tsx}
  pages/{Dashboard,Diagnostics,Compare,History,Settings}/
  components/{MetricCard,ResultTable,StatusBadge,TrendChart}/
  features/{diagnostics,proxy,history}/
  lib/{api.ts,types.ts,format.ts}
  tests/
docs/{ARCHITECTURE.md,TEST_PLAN.md,DECISIONS.md}
AGENTS.md
```

建议命令契约（最终由实现自证与文档同步）：
- `get_system_snapshot() -> SystemSnapshot`
- `get_proxy_snapshot() -> ProxySnapshot`
- `start_diagnostic(request: DiagnosticRequest) -> {job_id}`
- `cancel_diagnostic(job_id) -> {cancelled}`
- `list_runs(query) -> RunSummary[]`
- `get_run(run_id) -> DiagnosticRun`
- `delete_run(run_id)`, `clear_history()`
- `preview_sanitized_report(run_id) -> ReportPreview`
- `export_sanitized_report(run_id, format) -> ExportResult`
- 事件：`diagnostic://progress`，包括 job_id、目标、阶段、百分比、状态。

样例输入：
```json
{
  "target_ids": ["openai-api", "github"],
  "route_mode": "no_app_proxy",
  "manual_local_proxy_url": null,
  "repeats": 3,
  "timeout_ms": 10000
}
```

**探测语义**：
- `dns`：本机 DNS 查询，不代表 HTTPS 请求的真实解析路径。
- `tcp`：客户端对目标主机的独立 TCP connect 耗时，不能叫 ICMP RTT。
- `https`：整个 HTTP(S) 请求耗时，响应 401/403 = 已获得 HTTP 响应，功能层不可认定可用。
- `manual_local_proxy`：仅 HTTPS 通过指定代理，DNS/TCP 显示“不适用/独立基线”。SOCKS5 与 SOCKS5h 的 DNS 处理区别须有说明。
- `no_app_proxy`：仅不配置 HTTP 客户端代理；透明 TUN/系统路由仍可生效。
- 对 405 HEAD，允许一次 GET 回退（限制读取响应体）；重定向次数有限，禁止向任何目标发送身份认证信息。

## 6. 数据模型（建议）
`runs`：id, started_at, finished_at, status, route_mode, repeats, app_version, notes
`results`：id, run_id, target_id, probe_type, attempt_no, success, duration_ms, http_status, error_kind, error_summary
`settings`：key, value_json（不得储存凭证）
建立 run_id、started_at、target_id 索引；可选开启 WAL；数据库 schema migration 必须向前兼容。所有时间 UTC 存储、UI 转本地时间。

## 7. 质量 / 安全 / 隐私
- 启动离线可用；首次启动不自动扫描网络、不自动检测外网、不上传遥测。
- 仅通过用户操作触发网络测试；支持取消和全局截止时间；不进行端口段扫描、IP 枚举或抓包。
- 不读取/缓存/导出 SSH 私钥、API Key、Cookie、浏览器会话、代理订阅内容。
- Rust IPC 参数校验，Tauri 最小权限；无任意 shell 命令执行入口；不要求管理员权限。
- 用户输入的手动代理首版限 loopback，无账号密码；界面不允许输入任意命令。
- 脱敏在 Rust 导出层执行，脱敏逻辑单测覆盖 URL 中 userinfo、query token、认证头、本地用户名/路径等。
- 预设目标可能调整 HTTP 策略；报告只展示当前测量事实，不能承诺服务可实际登录使用。

## 8. MVP 验收场景
1. 新安装后断网打开，UI 可工作并显示未检测；不会擅自联网。
2. Windows 系统未设置代理时，代理快照显示未启用/未知，不崩溃。
3. DNS 成功而 HTTPS 超时时，能同时显示两种独立结果，不强行给定单一原因。
4. 401/403 有单独分类，不标绿成“服务完全正常”；407 显示代理认证相关。
5. 127.0.0.1:不监听端口时，手动代理测试明确失败，且没有回退为直连结果。
6. 系统处于透明代理/TUN 路径时，`no_app_proxy` 具有清楚免责声明。
7. 运行过程中关闭网络或点击取消，任务有截止时间、界面可恢复，不写入虚假成功记录。
8. SQLite 重新打开能查询历史；清理策略、删除、导出正常。
9. 脱敏报告不包含 URL 认证、API Key、Cookie、用户名、私钥内容。
10. `npm run lint`、`npm run typecheck`、`npm run test`、`cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`、Windows 安装构建真实通过或逐项解释阻碍。

## 9. 开发节奏
- Phase 0：环境检查、仓库规则与文档
- Phase 1：Tauri 框架、前端界面、模拟数据
- Phase 2：真实 DNS/TCP/HTTPS 探测与进度/取消
- Phase 3：Windows 代理信息、HTTP/SOCKS 本地路径对比
- Phase 4：规则诊断、结果汇总与风险提示
- Phase 5：SQLite 历史、趋势、清理、脱敏导出
- Phase 6：测试、发布、README、CI
- Phase 7（可选）：SSH 自定义主机状态面板，独立评估安全边界

## 10. 非目标
不是 VPN/机场客户端；不是可以判断出口 IP 是否“干净”的信誉检测器；不是 ChatGPT/Codex 登录检测器；不是 WAF 绕过工具；不是代理配置自动修改器；不是通用安全扫描或抓包工具。
