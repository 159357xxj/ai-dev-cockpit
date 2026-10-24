# AI Dev Cockpit — 开发者网络诊断台

Windows 10/11 本地优先、无需登录、**只读**的网络诊断桌面工具。面向在 Windows 上使用 ChatGPT、Codex、Claude、GitHub、SSH 的开发者，帮助定位 DNS / TCP / HTTPS / 代理路径层面的故障，并导出脱敏诊断报告。

> 本工具**只做诊断**：不修改系统代理、DNS、路由、防火墙、注册表；不请求管理员权限；不采集任何凭证（Cookie、API Key、订阅、私钥）；不上传任何数据。

## 当前状态

**Phase 1 已完成（工程脚手架与五页界面，尚无真实网络探测）。**

| 已实现 | 未实现（按阶段推进） |
|---|---|
| Tauri 2 + React 19 + TS strict + Vite 工程与锁文件 | P2：真实 DNS/TCP/HTTPS 探测、进度事件与取消 |
| 五页界面：总览 / 服务诊断 / 路径对比 / 历史记录 / 设置 | P3：Windows 代理快照与三模式路径对比 |
| 深色优先 + 浅色主题、响应式布局、错误边界 | P4：规则诊断引擎（离线） |
| 演示模式（显式开启、默认关闭，全部数据带「演示数据」标识） | P5：SQLite 历史、趋势、脱敏导出 |
| Rust 侧类型化 DTO、目标白名单、请求校验、注入缝（D-012） | P6：NSIS 安装包、CI、发布 |
| 命令桩：校验真实执行，探测诚实返回 not_available，不假装联网 | |
| 前端测试 14 个 + Rust 测试 17 个全部通过 | |

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

## 文档索引

- [PRD.md](PRD.md) — 产品需求
- [AGENTS.md](AGENTS.md) — 工程 Agent 约束（每一阶段的长期规则）
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — 架构、IPC 契约、数据结构与错误分类
- [docs/TEST_PLAN.md](docs/TEST_PLAN.md) — 测试策略与验收矩阵
- [docs/DECISIONS.md](docs/DECISIONS.md) — 技术决策（ADR，D-001..D-013）与风险登记
- [docs/ENVIRONMENT.md](docs/ENVIRONMENT.md) — 开发环境核查表
- [ZCODE_PROMPTS.md](ZCODE_PROMPTS.md) — 分阶段开发提示词（工作流备忘）

## 隐私与安全边界（摘要）

1. 只读诊断，仅经用户点击触发网络测试；启动离线可用。
2. 手动代理仅支持 loopback（http/socks5/socks5h），不带凭证，失败不回落。
3. 401/403 等响应按中性分类展示，不判定"服务可用"；传输失败率与非 2xx 响应比例分口径统计（D-011）。
4. 导出报告先本地脱敏并预览确认；认证材料与敏感路径永不出现在报告中。
5. 不做端口扫描、抓包、提权或系统设置修改。
