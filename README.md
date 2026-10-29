# AI Dev Cockpit — 开发者网络诊断台

Windows 10/11 本地优先、无需登录、**只读**的网络诊断桌面工具。面向在 Windows 上使用 AI 开发服务的开发者，帮助定位 DNS / TCP / HTTPS / 代理路径层面的故障，并导出脱敏诊断报告。

内置 6 个探测目标：ChatGPT（chatgpt.com）、OpenAI API（api.openai.com）、OpenAI 登录（auth.openai.com）、Claude（claude.ai）、GitHub（github.com）、ZCode（zcode.z.ai）。暂不支持自定义目标。

> 本工具**只做诊断**：不修改系统代理、DNS、路由、防火墙、注册表；不请求管理员权限；不读取或存储 Cookie、API Key、代理订阅或私钥；不上传本地历史记录或导出报告。用户主动运行诊断时，应用会向所选内置目标发送不携带用户凭证的 HTTPS 测量请求。

## 当前状态

**v0.1.0 Beta 已于 2026-10-28 以 Pre-release 形式发布。**

- Gate A：PASS（契约 GA-V6-1.0，提交 `4f7f337`；见 [docs/RELEASE_ACCEPTANCE_REPORT_V8.md](docs/RELEASE_ACCEPTANCE_REPORT_V8.md)）。
- Gate C：PASS，仅指 CI 质量检查（run [40688859414](https://github.com/159357xxj/ai-dev-cockpit/actions/runs/40688859414)，提交 `ede0f59`；见 [docs/RELEASE_ACCEPTANCE_REPORT_GATE_C.md](docs/RELEASE_ACCEPTANCE_REPORT_GATE_C.md)）。
- Gate B：WAIVED；安装、升级、卸载和桌面界面验收未执行（见 [docs/GATE_B_WAIVER.md](docs/GATE_B_WAIVER.md)）。

| 已实现 | 未实现（按阶段推进） |
|---|---|
| Tauri 2 + React 19 + TS strict + Vite 工程与锁文件 | 发布签署（安装包未签名，SmartScreen 提示见 INSTALL） |
| 五页界面 + 深浅主题 + 演示模式（仅开发构建可开启） | Gate B 安装、升级、卸载和桌面界面验收（WAIVED，未执行） |
| 真实探测：DNS（系统解析器）/ TCP 443 / HTTPS（Reqwest + 证书校验） | 外部漏洞数据库审计（npm audit / cargo audit） |
| 调度器：3/5 次采样、并发 ≤3、单请求 1–10s、任务截止 120s、取消 | |
| 事件流：progress/completed/failed，终态恰好一个，订阅先于启动 | |
| 错误分类：dns/tcp/tls 失败、超时、取消、代理不可用、401/403/407/429/5xx 中性细分 | |
| Windows 代理快照（WinINet/WinHTTP/环境变量，只读、脱敏、来源分别标注） | |
| 三种 HTTPS 路由模式（no_app_proxy / library_default / manual_local_proxy，IPC 三模式全通） | |
| 路径对比页接入真实测量（多模式顺序执行、样本互不共享） | |
| 规则诊断引擎：12 条规则（R01–R12）、确定性输出、证据分层（事实/解释/局限）；R11 非 2xx 误判已修复关闭 | |
| 诊断结论卡：严重程度排序、置信徽标、可展开细节（两页接入） | |
| SQLite 历史（筛选/趋势/删除/清空）+ 脱敏导出（JSON/MD，先预览后确认） | |
| 保留策略（30 天/500 次，可配置） | |
| 报告快照（F-07：状态机 Available/InFlight/Consumed，预览导出同字节，取消/写盘失败可重试，TTL+限额） | |
| 导出保存原子不覆盖：目标已存在时拒绝保存并提示换名；导出仅使用白名单 DTO | 代码签名、npm audit / cargo audit、CI 中的 NSIS 打包作业（从未运行） |
| 本机 Rust：170 passed / 0 failed / 4 ignored；test-hooks 原子保存套件：9 passed / 0 failed / 1 ignored | SSH 面板、自定义目标（PRD P2，尚未实现） |

## 环境要求（本机核查，2026-10-29）

本轮重新核实了 Node.js、npm 与 Rust 工具链；Windows、MSVC、Windows SDK 和 WebView2 的版本沿用 2026-10-24 环境记录，未在本轮重测。

| 组件 | 版本 |
|---|---|
| Windows | 11 23H2（10.0.22631） |
| Node.js / npm | 24.19.0 / 11.17.0（本机）；CI 使用 Node 22 |
| Rust / Cargo | 1.97.1，由仓库根目录 `rust-toolchain.toml` 固定（本地与 CI 一致） |
| MSVC Build Tools | VS 2022 BuildTools |
| Windows SDK | 10.0.26100（安装于 `E:\Windows Kits\10`） |
| WebView2 Runtime | 155.0.4283.62 |
| OpenSSL CLI | 测试依赖（TLS mock 生成测试证书；随 Git for Windows 分发；生产应用运行时不依赖） |

详细核查记录（命令与输出）：[docs/ENVIRONMENT.md](docs/ENVIRONMENT.md)

## 开发命令

```bash
npm install          # 安装前端依赖（见 package-lock.json）
npm run dev          # 仅前端 Vite 开发
npm run tauri dev    # 开发模式桌面运行；不代表 Release UI 验收（Gate B 未执行）
npm run lint         # ESLint（实测通过）
npm run typecheck    # tsc --noEmit（实测通过）
npm run test         # Vitest
npm run build        # tsc + vite build
cd src-tauri
cargo fmt --check            # 实测通过
cargo clippy --all-targets -- -D warnings
cargo test --all
cargo test --all --features test-hooks --test v6_atomic_save   # 含提交前竞态回归
cargo test --test v6_atomic_save -- --ignored                   # 符号链接用例，需管理员终端或开发人员模式
npm run tauri build  # Windows 发布构建
```

版本以 `package-lock.json` / `src-tauri/Cargo.lock` 为准。主要依赖：React 19、react-router 8、recharts 3、Tailwind CSS 4、shadcn/ui（组件源码方式集成）、lucide-react、Tauri 2、serde、url。

## 测试结果（2026-10-29 本轮复核）

| 套件 | 结果 |
|---|---|
| `cargo test --all`（src-tauri） | 170 passed / 0 failed / 4 ignored（公网用例及需提权的符号链接用例默认忽略） |
| `cargo test --all --features test-hooks --test v6_atomic_save` | 9 passed / 0 failed / 1 ignored；包含提交前竞态回归 |
| `npm run test`（Vitest，本机） | 33 passed / 2 failed / 0 ignored；导航和演示模式各有一个 5 秒超时。GitHub Actions Gate C run 40688859414 为 35/35，通过。 |

Gate C 的 run 40688859414 在提交 `ede0f59` 上通过 Rust 170/0/4 与前端 35/35；NSIS CI 打包作业 skipped，尚未运行。测试明细和 Gate 边界见 [docs/RELEASE_ACCEPTANCE_REPORT_GATE_C.md](docs/RELEASE_ACCEPTANCE_REPORT_GATE_C.md)。本地复跑的两个前端超时如上所列，不用 CI 结果覆盖本地输出。

## 文档索引

- [PRD.md](PRD.md) — 产品需求
- [AGENTS.md](AGENTS.md) — 工程 Agent 约束（每一阶段的长期规则）
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — 架构、IPC 契约、数据结构与错误分类
- [docs/TEST_PLAN.md](docs/TEST_PLAN.md) — 测试策略与验收矩阵
- [docs/DECISIONS.md](docs/DECISIONS.md) — 技术决策（ADR，D-001..D-026）与风险登记
- [docs/ENVIRONMENT.md](docs/ENVIRONMENT.md) — 开发环境核查表
- [docs/INSTALL.md](docs/INSTALL.md) — 安装说明（WebView2/数据位置/升级）
- [docs/GATE_A_CONTRACT_V6.md](docs/GATE_A_CONTRACT_V6.md) — Gate A 验收契约
- [docs/RELEASE_ACCEPTANCE_REPORT_V8.md](docs/RELEASE_ACCEPTANCE_REPORT_V8.md) — Gate A 最终验收
- [docs/RELEASE_ACCEPTANCE_REPORT_GATE_C.md](docs/RELEASE_ACCEPTANCE_REPORT_GATE_C.md) — Gate C CI 验收
- [docs/GATE_B_WAIVER.md](docs/GATE_B_WAIVER.md) — Gate B 豁免记录
- [CHANGELOG.md](CHANGELOG.md) — 更新日志
- [ZCODE_PROMPTS.md](ZCODE_PROMPTS.md) — 历史：早期分阶段开发提示词

## 安装（Windows）

从 [Releases](https://github.com/159357xxj/ai-dev-cockpit-releases/releases) 下载 `AI-Dev-Cockpit_<版本>_x64-setup.exe`，核对 SHA-256 后安装，详见 [docs/INSTALL.md](docs/INSTALL.md)。
历史数据库由 Rust 通过 Tauri `app_data_dir()` 定位为应用数据目录下的 `history.db`（应用标识 `com.aidevcockpit.app`）；卸载保留数据的行为未在真实 Gate B 测试中验证。

## 隐私声明

AI Dev Cockpit 是**本地优先**的只读诊断工具：

- **本地数据不上传**：诊断记录、历史和导出报告保存在本机；用户主动诊断时，应用会向所选内置目标发送不携带用户凭证的 HTTPS 测量请求。
- **不采集凭证**：不读取/存储 Cookie、API Key、OAuth 令牌、代理订阅、SSH 私钥；诊断请求从不携带用户凭证。
- **不遥测**：无崩溃上报、无使用统计、无账号体系。
- **只读诊断**：不修改系统代理/DNS/路由/防火墙/注册表；不请求管理员权限；不做端口扫描或抓包。
- **数据保留**：本地历史默认保留 30 天或 500 次（可配置）；卸载应用不自动删除该数据。
- 完整边界见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) §9 与 [AGENTS.md](AGENTS.md)。

## 隐私与安全边界（摘要）

1. 只读诊断，仅经用户点击触发网络测试；启动离线可用。
2. 手动代理仅支持 loopback（http/socks5/socks5h），不带凭证，失败不回落。
3. 401/403 等响应按中性分类展示，不判定"服务可用"；传输失败率与非 2xx 响应比例分口径统计（D-011/D-014/D-017）。
4. 代理快照为只读本地检测，配置存在 ≠ 真实请求使用该配置；不下载/不执行 PAC。
5. 导出报告先本地脱敏并预览确认；认证材料与敏感路径永不出现在报告中。
6. 不做端口扫描、抓包、提权或系统设置修改。
7. 导出使用系统原生"另存为"对话框，路径选择在本地完成；历史数据和导出文件不会上传。
8. 正式（release）构建强制关闭演示模式；启动不自动联网探测，仅用户主动运行诊断时向所选目标发起 HTTPS 测量。
