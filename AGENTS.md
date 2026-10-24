# AI Dev Cockpit — Agent 项目约束

你是此仓库的工程开发 Agent。请将本文件作为每一阶段的长期约束。

## 项目边界
- Windows 10/11 优先，Tauri 2 + React/TypeScript + Rust + SQLite。
- 本地优先、无账号、无云端 API 依赖、无遥测；默认不后台联网。
- 项目是**只读诊断工具**，绝不自动修改系统网络设置、代理、DNS、防火墙、路由、hosts、SSH 或用户代理软件配置。
- 不自动采集或读取浏览器 Cookie、API Key、OAuth token、代理订阅、SSH 私钥和明文密码。
- 不提供任意命令执行、端口段扫描、嗅探、提权或无确认的系统设置更改。
- 不绕过 TLS 证书校验，不跳过身份认证，不关停安全软件。

## 技术与正确性
- 必须明确区分：DNS、TCP connect、HTTP(S) 响应、ICMP Ping、业务可用性；不同测量的耗时绝不可混用。
- `no_app_proxy` 指禁用应用层 HTTP 客户端代理，不承诺绕过 TUN/透明代理。
- `manual_local_proxy` 仅允许 loopback HTTP/SOCKS5/SOCKS5h，且失败绝不静默回落。
- HTTP 401/403 等视为“收到响应但业务能力未验证”，不视为业务正常。
- DNS/TCP 独立探测不可展示成代理请求内部的阶段时长。
- UI 禁止假数据冒充真实结果；可用明确标识的 Demo/Mock，发布时正式模式默认关闭。
- 每个诊断任务有超时、取消、并发上限、准确错误状态，禁止无上限重试。
- Rust 负责本地网络、系统信息、数据持久化和导出脱敏；前端仅调白名单 IPC。
- 任何导出先本地脱敏并预览，认证材料和个人路径不能出现在报告里。

## 工作方法
1. 每阶段开始先扫描现有文件、锁文件、README、docs，简述修改计划和影响范围。
2. 仅实现本阶段要求，避免重写已完成模块、无关重构及改动不必要配置。
3. 未经确认不要运行需要管理员权限的命令、全局安装、修改 PATH/注册表/系统代理/防火墙的命令、远端部署或 push。
4. 根据检测到的环境安装项目本地依赖；如缺少 MSVC/Rust/WebView2 等系统前置条件，说明步骤，等待人工完成。
5. 按项目已存在的脚本执行 lint/typecheck/tests；根据变更补齐测试。
6. 真实执行可用的构建/测试命令，不可编造通过记录；不能验证则明确记为未验证。
7. 结束时给出：已修改文件、实现内容、实际执行命令与结果、遗留问题、下一阶段入口。
8. 所有可复现的问题先增加测试用例再修复；每阶段鼓励小而可回滚的 Git 提交。

## 代码规范
- TypeScript strict，不使用无理由的 any；Rust 使用显式错误类型与结构化结果。
- UI 按 feature 切分，网络与 Windows API 的平台依赖明确隔离。
- IPC 请求与响应有共享的类型契约，使用 serde 序列化，注意 snake_case/camelCase 映射。
- 使用稳定版本与锁文件；避免引入过多重复依赖；所有实际使用的命令和版本记入 README。
- 时间以 UTC 存储、本地展示；不会将基于少量样本的结果描述为统计显著。

## 验证基线
- `npm run lint`
- `npm run typecheck`
- `npm run test`
- `cargo fmt --check`（在 src-tauri 执行）
- `cargo clippy -- -D warnings`（在 src-tauri 执行）
- `cargo test`（在 src-tauri 执行）
- Windows 真实 `tauri dev` 与 `tauri build`：如未运行必须说明理由。
