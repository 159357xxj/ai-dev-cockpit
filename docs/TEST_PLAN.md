# 测试计划（AI Dev Cockpit）

版本：v1.0（Phase 0）｜日期：2026-10-24｜上游：`PRD.md` §7–8、`AGENTS.md` 验证基线
关联：`docs/ARCHITECTURE.md`（被测契约）、`docs/DECISIONS.md`

---

## 1. 测试原则

1. **自动化测试永不依赖公网成功**。所有 HTTPS/DNS/TCP 用例基于本地 mock 服务器与本机回环地址；公网预设目标（chatgpt.com 等）只作为人工检查项。
2. **真实执行，不编造**。每个阶段报告只记录真实运行的命令与输出；无法验证的项明确标注"未验证"。
3. 先有失败用例，再修缺陷（可复现问题必须先补测试）。
4. UI 禁止假数据冒充真实结果；Demo 数据必须有显式标识并可用开关关闭。

## 2. 测试分层

| 层 | 工具 | 覆盖对象 | 引入阶段 |
|---|---|---|---|
| L1 Rust 单元测试 | `cargo test` | 错误分类映射、代理 URL 校验、脱敏规则、聚合统计（min/median/max/失败率） | P2 起 |
| L2 Rust 集成测试 | `cargo test` + 本地 mock HTTP/TCP 服务器 | 探测端到端：成功、拒连、超时、401/403/407/429/5xx、取消、非法参数 | P2 起 |
| L3 契约测试 | `cargo test` + 前端 Vitest | serde 序列化 round-trip、snake_case 字段映射、枚举字面量前后端一致 | P2 起 |
| L4 前端组件测试 | Vitest + React Testing Library | 页面状态（空/加载/错误/数据）、演示模式标识、表单校验 | P1 起 |
| L5 手工验收矩阵 | 人工执行（PRD §8） | 真实 Windows 行为、断网场景、真实代理软件场景 | 每 |
| L6 CI | GitHub Actions（windows-latest） | lint/typecheck/test/fmt/clippy/cargo test/build | P6 |

## 3. 本地 mock 服务器设计（P2 实现时冻结细节）

- 极简实现优先：基于 `tokio::net::TcpListener` 手写最小 HTTP 响应（避免引入重型框架），或按需引入 `axum`/`hyper`，以 Phase 2 实际代码为准并记录。
- 能力：可指定返回状态码（200/301/401/403/405→GET 回退/407/429/500）、延迟（模拟超时）、立即断连（模拟 TLS/连接失败）、多请求计数（验证采样次数与并发上限）。
- 保留策略：mock 只监听 `127.0.0.1` 随机端口，测试结束即关闭；不写防火墙规则、不需要提权。

## 4. 各阶段测试清单

### Phase 1（脚手架与界面）
- [ ] ≥3 个前端组件测试可运行（如：侧栏五页切换、诊断页表单校验提示、演示模式角标常驻）。
- [ ] Rust 命令桩返回类型化 DTO（桩内静态值 + 明确"演示"标记，不假装联网）。
- [ ] `npm run lint / typecheck / test` 真实通过；`cargo fmt --check / clippy -D warnings / test` 真实通过；`tauri dev` 真实启动。

### Phase 2（真实探测）
- [ ] L2 用例矩阵：
  - 200 → `success=true, http_outcome=ok_2xx`；
  - 401/403 → `success=false, http_outcome=auth_required_401_403`，error 为空（收到响应≠错误）；
  - 407 / 429 / 500 / 301(达上限) → 对应 outcome，均不标绿；
  - 立即断连 → 分类为 `tcp_failure` 或 `tls_failure`（按阶段），不得误判为 `unknown`；
  - mock 延迟 > timeout_ms → `timeout`；
  - 取消（任务中途） → 剩余样本 `cancelled`，任务状态 `cancelled`，无伪造成功记录；
  - 非法参数（未知 target_id、repeats=4、timeout_ms=20000、manual 模式带 userinfo/非 loopback） → `invalid_input` 且任务不启动。
- [ ] 采样聚合正确性：3/5 次的成功数、请求失败率、min/median/max（含偶数个样本的中位数定义）。
- [ ] 并发上限 ≤3、全局截止 120s 有测试或明确人工验证记录。
- [ ] 人工检查项：对 ≥2 个公网预设目标真实探测（截图/日志留档）；断网时可控失败。

### Phase 3（代理与对比）
- [ ] 代理 URL 校验表驱动用例：合法 http/socks5/socks5h loopback；非法（userinfo、非 loopback host、无端口、path/query、错误 scheme）。
- [ ] 手动代理指向未监听端口 → `proxy_unavailable`，**断言不存在回落路径**（对比同目标 `no_app_proxy` 结果不同或均独立失败）。
- [ ] `library_default` 代理源实测记录（本地 mock 代理 + 环境变量开关），结论回写 ARCHITECTURE §7.2。
- [ ] 代理快照脱敏：注入含 userinfo/token 的假配置值，断言展示层已遮蔽；未配置时不崩溃、显示"未启用/未知"。
- [ ] 对比页只纳入 HTTPS 样本的断言；DNS/TCP 显示"独立基线/不适用"。

### Phase 4（规则诊断）
- [ ] 每条规则一个 fixture 样本 + 表驱动测试（DNS 失败、TCP 失败、TLS 失败、超时、401/403、407、429、5xx、代理不可用、样本不足的路径差异）。
- [ ] 同一 fixture 输出字节级稳定（确定性）。
- [ ] 断言：401/403 的建议文案不出现"服务可用/登录正常"；输出含"不能推断事项"；置信标记仅用"证据充分/可能/证据不足"。

### Phase 5（SQLite 与导出）
- [ ] 迁移测试：从 v0 逐版迁移到当前，重复执行幂等。
- [ ] 事务完整性：completed run 必含全部样本；cancelled run 状态如实。
- [ ] 保留策略：>30 天或 >500 会话触发清理的边界用例；删除/清空有确认且生效。
- [ ] 脱敏测试集（表驱动）：Authorization 头、URL userinfo、query token、含用户目录的路径（`C:\Users\xxx\...`）、代理 URL 凭证、特殊字符、二进制垃圾字符——全部遮蔽且不崩溃。
- [ ] 导出文件为合法 JSON/Markdown，含"诊断局限"说明段；重启应用后历史仍在。

### Phase 6（发布）
- [ ] 执行并记录 AGENTS.md 验证基线全部命令；未执行的逐项写明原因。
- [ ] 断网安装启动（PRD §8.1）；NSIS 安装/卸载/启动/历史数据位置检查。
- [ ] Tauri capability 权限最小化审查、依赖来源与锁文件审查、无明文凭证检查。

## 5. 手工验收矩阵（映射 PRD §8 十条）

| # | 场景 | 操作 | 预期 |
|---|---|---|---|
| 1 | 断网首启 | 断网 → 启动应用 | 主界面可用，显示"未检测"，不擅自联网 |
| 2 | 无代理快照 | 系统未配置代理 → 查看代理快照 | 显示未启用/未知，不崩溃 |
| 3 | DNS 通而 HTTPS 超时 | hosts/网络条件构造 | 两类独立结果并列展示，不强行给单一原因 |
| 4 | 401/403/407 分类 | 触达返回这些状态的目标 | 各自独立分类，401/403 不标绿，407 提示代理认证 |
| 5 | 手动代理死端口 | 指向 127.0.0.1 未监听端口 | 明确"指定代理不可用"，无直连回落 |
| 6 | 透明代理/TUN 免责 | 系统处于 TUN 时用 no_app_proxy | 展示固定免责声明 |
| 7 | 中途取消/断网 | 运行中断网或点取消 | 有截止时间，界面可恢复，无虚假成功记录 |
| 8 | 历史持久化 | 重启应用 | 历史可查、清理/删除/导出正常 |
| 9 | 报告脱敏 | 导出报告 | 无 URL 认证、API Key、Cookie、用户名、私钥内容 |
| 10 | 质量基线 | 运行全部基线命令 | 真实通过或逐项解释阻碍 |

## 6. 验证基线命令（每阶段报告必须引用）

```
npm run lint            # P1 起
npm run typecheck       # P1 起
npm run test            # P1 起
cargo fmt --check       # src-tauri，P2 起
cargo clippy -- -D warnings   # src-tauri，P2 起
cargo test              # src-tauri，P2 起
tauri dev / tauri build # P1 / P6；未运行必须写明理由
```

Phase 0 状态：以上命令因尚无工程代码，全部记为**不适用**（不是"通过"）。

## 7. CI 计划（Phase 6 落地）

- 触发：push + PR；runner：`windows-latest`。
- Job 1（前端）：setup-node → `npm ci` → lint → typecheck → test → build。
- Job 2（Rust）：Rust stable-msvc → `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo test`。
- Job 3（构建，可选/手动）：`tauri build` 产物存档。
- 不在 CI 中访问公网预设目标；CI 失败必须修复或显式标注，不允许长期红。
