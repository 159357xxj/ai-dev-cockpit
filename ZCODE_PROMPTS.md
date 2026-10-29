历史文档：早期使用 ZCode 时的分阶段提示词，不再维护。
# 直接按顺序交给 ZCode Agent 的阶段提示词

使用方式：在 ZCode 新建名为 `ai-dev-cockpit` 的工作区，在根目录放置 `PRD.md` 与 `AGENTS.md`；按顺序粘贴 P0–P6。每阶段检查结果、运行测试、提交版本后再继续。首轮建议 Plan mode，常规界面改动可改用 Edit automatically。下面每个提示词都能独立复制。

---

## P0 — 项目审查与环境检查（Plan mode）

你现在负责从零开发 AI Dev Cockpit，请先阅读工作区根目录的 AGENTS.md 与 PRD.md。此阶段只做工程前置检查和文档，不生成 UI 和业务代码。

任务：
1. 检查当前系统是否为 Windows，检查 Git、Node LTS、npm、Rust stable-msvc、cargo、Microsoft C++ Build Tools、WebView2、Tauri 开发环境是否可用。不要假设已安装；执行只读检查命令。
2. 核实 PRD 中的 Tauri 2/React/Rust/Reqwest/SQLite 方案是否存在明显实现风险。重点解释默认代理、无应用代理、指定本地代理和 TUN 的语义区别。
3. 生成 docs/ARCHITECTURE.md、docs/TEST_PLAN.md、docs/DECISIONS.md，覆盖页面、后端模块、IPC 数据契约、错误分类、测试策略、隐私边界。
4. 列出 Phase 1 最小文件清单与可执行验收命令。不要擅自全局安装、修改系统配置或申请管理员权限。

验收：提交环境核查表，区分已验证/未验证；完成文档且与 PRD 不矛盾。结束时列出实际执行的命令与输出摘要。不要进入 Phase 1。

---

## P1 — 工程脚手架与界面（Edit automatically，安装依赖需确认）

阅读 AGENTS.md、PRD.md、docs 下的方案，基于现有工作区搭建 AI Dev Cockpit 的 Tauri 2 + React + TypeScript + Vite 工程。使用 Tailwind CSS、shadcn/ui、Lucide 和必要的 Recharts 组件。不要先开发网络探测逻辑；界面使用明确标注的模拟数据。

必须完成：
1. 可运行的 Tauri Windows 桌面应用；React 严格类型、路由、全局布局、主题切换、错误边界。
2. 五个页面：总览、服务诊断、路径对比、历史、设置。实现侧栏导航、空状态、加载状态、错误状态、响应式布局。
3. 诊断目标选择、模式选择、3/5 次采样选择、开始/取消按钮；此阶段允许按钮运行模拟状态，但必须显眼显示“演示模式”，不能冒充真实测量。
4. 定义前后端共享的 TypeScript DTO，并在 Rust 预留 `get_system_snapshot`、`start_diagnostic` 等类型化命令接口，不实现假联网。
5. npm scripts：dev、build、lint、typecheck、test、tauri。编写至少 3 个可运行的 UI 测试。
6. 在 README 写明开发启动命令、环境需求及当下已实现/未实现功能。

验收：真实执行依赖安装、lint、typecheck、前端测试、Tauri 开发运行（如系统条件不足，给出明确阻碍）。界面五页可切换，所有模拟数据清晰标示；不可声称“网络功能已完成”。不要进入 P2。

---

## P2 — 真正的 DNS/TCP/HTTPS 诊断

读取 AGENTS.md、PRD.md 和现有代码，只实现独立真实网络诊断，保留后续代理对比接口。必须由 Rust 执行测试，不允许前端浏览器 fetch 冒充原生测试。

实现要求：
1. 使用 Tokio 异步 DNS lookup、TCP connect、Reqwest HTTPS 请求；三个检测为独立测量，不能把测量值错误拼成同一个请求时序。
2. 预设 6 个目标：ChatGPT、OpenAI API、OpenAI 登录域、Claude、GitHub、ZCode。仅访问公开 HTTPS 地址，勿发送用户凭证。HTTP HEAD 如 405 可有限回退 GET，并防止无上限 body 下载和重定向。
3. HTTPS 有请求总超时 10s、全局任务截止时间 120s、最多并发 3、每目标默认 3 次可选 5 次；支持任务取消、进度事件与结构化错误。
4. 数据字段至少包括 target_id、probe_type、route_mode、attempt、success、duration_ms、http_status、error_kind、error_message、tested_at。区分 DNS_FAILURE、TCP_FAILURE、TLS_FAILURE、HTTP_RESPONSE、TIMEOUT、CANCELLED、UNKNOWN 等。错误分类不明确时不要强行推断。
5. UI 用真实测量结果显示每次样本、成功次数、请求失败率、min/median/max 耗时。不要叫 ICMP Ping 或数据包丢包率。
6. 先完成 `no_app_proxy` HTTP 客户端模式（Reqwest 明确不配置应用层代理）及 DNS/TCP 独立基线；注明它不绕过 TUN/透明路由。
7. 加入本地模拟服务器相关单元/集成测试，覆盖成功、拒绝连接、超时、HTTP 401/403/429、取消与非法参数；公网目标测试只用作人工检查。

验收：使用真实 Windows 运行截图/控制台日志证明至少两个目标经过真实探测；断网时可控失败；关闭中途任务可取消；没有静默变为假数据；完成 Rust 单元测试和前端相关测试。结束后总结真实检查结果与未验证项，不进入 P3。

---

## P3 — Windows 代理检测与对比

在不修改用户网络配置、不提权的条件下，为 AI Dev Cockpit 增加代理状态只读快照和 HTTPS 路径对比。

任务：
1. 只读读取 Windows 用户级手动代理配置、PAC 是否存在及相关代理环境变量；所有代理字符串必须隐藏可能包含的用户名、密码和 token，Windows 未配置时能正常显示。不要假装可靠识别 TUN 是否开启。
2. 在 HTTPS 层加入 `library_default`（Reqwest 默认行为）、`no_app_proxy`、`manual_local_proxy` 三种模式。手动代理仅支持 loopback 的 http://、socks5://、socks5h://，校验地址、端口、禁止 userinfo；启用 Reqwest SOCKS 能力。
3. 对相同 HTTPS 目标进行按模式重复对比，显示各模式样本数、成功率、中位数、失败原因；仅将 HTTPS 样本纳入代理对比，DNS/TCP 独立基线不得宣称经过 SOCKS/HTTP 代理。
4. `manual_local_proxy` 未监听或无法连接时明确提示“指定代理不可用”，绝不可静默降级到系统路径或无代理路径。
5. 在界面上以明确文字解释：`no_app_proxy` 只是不使用 HTTP 客户端显式代理，Windows TUN 或透明转发仍可能生效；默认库路径也不保证与任一特定桌面应用一致。
6. 为代理地址验证、失败不回退、脱敏、路径对比统计写测试。任何手动测试均由用户点击触发。

验收：本地无代理配置不报错；错误端口不回退；两条可用/不可用请求路径可分别展示结果；没有触碰代理软件和 Windows 配置。请报告真实测得数据以及环境限制，不进入 P4。

---

## P4 — 故障解释与诊断规则

实现可解释、确定性的离线规则诊断引擎，不接入大模型和云端服务。阅读 AGENTS.md 与 PRD.md 的科学测量边界。

任务：
1. 设计数据驱动规则表：DNS lookup 失败、TCP connect 失败、HTTPS/TLS 握手失败、超时、401/403、407、429、5xx、指定代理不可连接、路径间结果显著不同但样本不足。
2. 每条规则输出：事实证据、可能原因（不超过 3 条）、验证步骤（优先只读操作）、不能推断的事项、置信标记（例如“证据充分/可能/不足”；不用伪精确百分比）。
3. 401/403 不可直接标记为服务或登录可用；API 401 可提示“接口可返回 HTTP，但身份/功能不可由此确认”；403 可能有网络边缘安全拦截等不同原因。
4. 输出诊断结论卡，允许“目前证据不足”，说明后续应做什么测试。
5. 建立 fixture 样本和表驱动测试，覆盖上述每类错误及混合情况；确保没有把 TCP RTT 等同 HTTPS 总时间，也没有从少量样本推断 IP 信誉。

验收：给定测试 fixture 可稳定生成相同说明；无 AI API、无额外联网；UI 明确区分测量事实与推断。测试通过后停止，等待 P5。

---

## P5 — SQLite 历史、趋势与脱敏导出

请阅读已有诊断架构并完成本地数据功能，不触碰云端平台。

任务：
1. Rust 后端通过 rusqlite 管理 SQLite：runs、results、settings，含带版本的迁移，索引及错误处理；所有 SQL 参数化，前端只能调用白名单 IPC。
2. 按测试任务成功、部分失败、取消记录正确状态；事务提交确保历史不出现“已完成但缺少样本”的假象。记录 UTC 时间与 app_version。
3. 历史页面支持目标、模式、时间筛选；展示最近两次可比运行趋势和样本数量。无历史显示空状态。
4. 默认保留 30 天或 500 次运行（可配置）；手动清除历史与单次删除有确认，并编写迁移与清理测试。
5. 实现 JSON/Markdown 两种本地导出；先生成脱敏预览让用户确认，再另存为文件。不导出授权头、Cookie、密钥、完整本机用户名路径与代理认证信息。
6. 建立自动化脱敏测试集，包括 Authorization、URL userinfo、query token、包含用户目录的路径、特殊字符等。

验收：重启后历史保留；清理与删除成功；导出的文件可读且含明确诊断局限；所有样例敏感信息都被遮蔽。不可将“本地导出”实现为后台上传。完成后停止。

---

## P6 — 质量、安装包与开源发布

这是发布阶段。审查 AGENTS.md、PRD.md 与完整仓库，不增加 MVP 范围以外的新功能。

任务：
1. 审查 Windows 安装运行、无网络启动、只读权限、代理失败回退、超时/取消、HTTP 状态解释、报告脱敏等端到端场景。
2. 执行并记录实际命令：npm run lint、npm run typecheck、npm run test、npm run build、cargo fmt --check、cargo clippy -- -D warnings、cargo test、Tauri Windows build。未执行的命令必须明示原因。
3. 进行安全检查：Tauri capability 权限最小化；不要开启通用 shell 执行；检查依赖来源，版本锁定，确认没有明文凭证与意外联网。
4. 输出 README.md（功能、安装、截图占位、隐私声明、已知限制、开发方式）、CHANGELOG.md、LICENSE 建议、docs/TROUBLESHOOTING.md，并生成 GitHub Actions Windows CI。
5. 产出可安装的 Windows 包（优先 NSIS），检查安装、卸载、启动、历史数据位置。若当前环境无法实际打包，列出明确缺失前提和复现命令，不得声称构建成功。
6. 只在用户批准后创建 Git commit；不要擅自 git push、发布 Release 或改用户环境。

验收：逐项给出 PASS/FAIL/NOT TESTED 的发布检查表、失败项定位和重现步骤；至少 1 份真实脱敏诊断报告示例；没有虚假截图或虚构测试通过记录。停在可供人工复审和发布的状态。

---

## P7（可选）— 自定义 SSH 主机连通性（严格只读）

在 v1.0 已验收后评估此功能。用户只允许手工添加单个主机与端口，做 DNS/TCP 连通性、SSH banner 可达性检查。不登录服务器，不收集密码或密钥，不发起端口范围扫描，不变更 SSH 配置。先提出设计、隐私和安全评审，再实现并测试；未获确认不要开始。
