# AI Dev Cockpit — 开发者网络诊断台

Windows 10/11 本地优先、无需登录、**只读**的网络诊断桌面工具。面向在 Windows 上使用 ChatGPT、Codex、Claude、GitHub、SSH 的开发者，帮助定位 DNS / TCP / HTTPS / 代理路径层面的故障，并导出脱敏诊断报告。

> 本工具**只做诊断**：不修改系统代理、DNS、路由、防火墙、注册表；不请求管理员权限；不采集任何凭证（Cookie、API Key、订阅、私钥）；不上传任何数据。

## 当前状态

**Phase 5 已通过功能验收（2026-10-25 真机四项 PASS：重启历史保留 / 取消状态正确 / 报告预览导出 / 趋势与删除）。**

| 已实现 | 未实现（按阶段推进） |
|---|---|
| Tauri 2 + React 19 + TS strict + Vite 工程与锁文件 | P3：Windows 代理快照；`library_default` / `manual_local_proxy` 模式对比 |
| 五页界面 + 深浅主题 + 演示模式（显式开启、默认关闭） | P4：规则诊断引擎（离线） |
| 真实探测：DNS（系统解析器）/ TCP 443 / HTTPS（Reqwest + 证书校验） | P5：SQLite 历史、趋势、脱敏导出 |
| 调度器：3/5 次采样、并发 ≤3、单请求 1–10s、任务截止 120s、取消 | P6：NSIS 安装包、CI、发布 |
| 事件流：progress/completed/failed，终态恰好一个，订阅先于启动 | |
| 错误分类：dns/tcp/tls 失败、超时、取消、代理不可用、401/403/407/429/5xx 中性细分 | |
| Windows 代理快照（WinINet/WinHTTP/环境变量，只读、脱敏、来源分别标注） | |
| 三种 HTTPS 路由模式（no_app_proxy / library_default / manual_local_proxy） | |
| 路径对比页接入真实测量（多模式顺序执行、样本互不共享） | |
| 规则诊断引擎：12 条规则（R01–R12）、确定性输出、证据分层（事实/解释/局限） | R11 非 2xx 误判风险待复核（P5 前置项） |
| 诊断结论卡：严重程度排序、置信徽标、可展开细节（两页接入） | |
| SQLite 历史（筛选/趋势/删除/清空）+ 脱敏导出（JSON/MD，先预览后确认） | —— |
| 保留策略（30 天/500 次，可配置） | |
| Rust 测试 117（单元/E2E/代理/规则/存储/导出脱敏）+ 前端 35 全部通过 | |

## 环境要求（本机实测，2026-10-24）

| 组件 | 版本 |
|---|---|
| Windows | 11 23H2（10.0.22631） |
| Node.js / npm | 24.19.0 / 11.17.0 |
| Rust / Cargo | 1.97.1（stable-x86_64-pc-windows-msvc，含 rustfmt/clippy 组件） |
| MSVC Build Tools | VS 2022 BuildTools |
| Windows SDK | 10.0.26100（安装于 `E:\Windows Kits\10`） |
| WebView2 Runtime | 155.0.4283.62 |

详细核查记录（命令与输出）：[docs/ENVIRONMENT.md](docs/ENVIRONMENT.md)

## 开发命令（Phase 1 实测可用）

```bash
npm install          # 安装前端依赖（见 package-lock.json）
npm run dev          # 仅前端 Vite 开发
npm run tauri dev    # 桌面应用开发运行（实测可启动窗口）
npm run lint         # ESLint（实测通过）
npm run typecheck    # tsc --noEmit（实测通过）
npm run test         # Vitest，14 个测试（实测通过）
npm run build        # tsc + vite build（实测通过）
cd src-tauri
cargo fmt --check            # 实测通过
cargo clippy -- -D warnings  # 实测通过
cargo test                   # 17 个测试（实测通过）
npm run tauri build  # 发布构建（Phase 6 验证）
```

版本以 `package-lock.json` / `src-tauri/Cargo.lock` 为准。主要依赖：React 19、react-router 8、recharts 3、Tailwind CSS 4、shadcn/ui（组件源码方式集成）、lucide-react、Tauri 2、serde、url。

## 测试结果（2026-10-24 实测）

| 套件 | 结果 |
|---|---|
| `cargo test`（src-tauri） | 47 通过 = 30 单元（DTO 契约/校验/聚合口径）+ 17 端到端（本地 mock 服务器，全离线）；公网验证 2 项默认 `#[ignore]` |
| `npm run test`（Vitest） | 22 通过（页面导航/表单校验/演示模式开关/IPC snake_case 契约/真实模式事件流/聚合口径） |
| `cargo fmt --check`、`cargo clippy -- -D warnings`、`npm run lint`、`npm run typecheck`、`npm run build` | 全部通过 |
| 公网人工验证（已授权） | github.com 与 zcode.z.ai 各一次 DNS/TCP/HTTPS：全部成功，HTTPS 200 `ok_2xx`（3060ms / 414ms）——200 仅代表"收到响应"，不代表登录或业务可用 |

自动化测试不依赖公网；真实网络探测仅由用户点击触发或显式运行 `--ignored` 用例。

## 文档索引

- [PRD.md](PRD.md) — 产品需求
- [AGENTS.md](AGENTS.md) — 工程 Agent 约束（每一阶段的长期规则）
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — 架构、IPC 契约、数据结构与错误分类
- [docs/TEST_PLAN.md](docs/TEST_PLAN.md) — 测试策略与验收矩阵
- [docs/DECISIONS.md](docs/DECISIONS.md) — 技术决策（ADR，D-001..D-013）与风险登记
- [docs/ENVIRONMENT.md](docs/ENVIRONMENT.md) — 开发环境核查表
- [docs/INSTALL.md](docs/INSTALL.md) — 安装说明（WebView2/数据位置/升级）
- [CHANGELOG.md](CHANGELOG.md) — 更新日志
- [ZCODE_PROMPTS.md](ZCODE_PROMPTS.md) — 分阶段开发提示词（工作流备忘）

## 安装（Windows）

从 [Releases](https://github.com/159357xxj/ai-dev-cockpit/releases) 下载 NSIS 安装包
（核对 SHA-256 后安装，无需管理员权限），详见 [docs/INSTALL.md](docs/INSTALL.md)。
卸载不会删除本地历史数据库（`%APPDATA%\com.aidevcockpit.app\history.db`）。

## 隐私声明

AI Dev Cockpit 是**本地优先**的只读诊断工具：

- **不联网上传**：所有数据（诊断记录、历史、报告）仅保存在本机；导出经系统原生"另存为"完成，无任何网络上传。
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
4. 导出报告先本地脱敏并预览确认；认证材料与敏感路径永不出现在报告中。
5. 不做端口扫描、抓包、提权或系统设置修改。
6. 导出使用系统原生"另存为"对话框，路径选择在本地完成，无任何网络上传。
7. 正式（release）构建强制关闭演示模式；启动不自动联网探测，仅用户主动运行诊断时发起所选目标的 HTTPS 测量。
