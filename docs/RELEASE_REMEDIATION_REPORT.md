# Release Gate FAIL 专项修复报告（F-01 ~ F-06）

修复日期：2026-10-25（Asia/Shanghai）
依据：`docs/RELEASE_ACCEPTANCE_REPORT.md`（独立验收判定 FAIL，F-01/F-02 为 P1 阻断）
修复提交：`e400d218b17c815125de9433240fd0d1aec21db8`（本地 main，领先 origin/main 3 个提交，未推送）
约束遵守：未修改用户真实数据库/未启动会触发清理的应用实例做测试/未安装卸载/未清理 AppData/未改网络配置；全部数据库与导出测试使用内存库或假凭证。

## 1. F-01 ~ F-06 处理状态

| ID | 状态 | 处理 |
|---|---|---|
| F-01 IPC 模式门控 | **已修复** | 移除 Phase 2 过期限制；三模式合法到达后端；全部校验保留 |
| F-02 JSON 敏感泄漏 | **已修复** | 结构化安全导出 + sanitize_text 全面加固；7 项假凭证回归测试零泄漏 |
| F-03 WAL 备份说明 | **已修复** | INSTALL.md 明确三文件备份/Backup API/退出后备份 |
| F-04 文档一致性 | **已修复** | README/CHANGELOG/ARCHITECTURE/INSTALL 与现状对齐 |
| F-05 CI 最小权限 | **已修复** | ci.yml 显式 `permissions: contents: read` |
| F-06 commit-to-artifact | **已解决** | 从 `e400d21` 干净重建（旧包已删除），新 SHA-256 见 §5 |

## 2. 每项修复的文件与核心改动

### F-01（`src-tauri/src/commands/diagnostics.rs`）

- **删除**：`route_mode != NoAppProxy → not_available("Phase 3 提供")` 的过期门控。
- **保留**：`request.validate()`（目标白名单/去重/次数 3|5/超时 1000..=10000/模式-代理地址互斥）+ `manual_local_proxy` 的 `validate_manual_proxy_url` 严格校验（loopback host、端口 1..=65535、http/socks5/socks5h、禁 userinfo/path/query）。
- **未开放**：任意目标 URL（白名单不变）、非回环代理（校验不变）；底层 `proxy_unavailable` 不回落逻辑不变（P3 已有测试守护）。

### F-02（`src-tauri/src/security/redaction.rs` + `src-tauri/src/commands/report.rs`）

- **report.rs**：新增 `secure_run_json` / `secure_diagnoses_json`——**不再序列化内部对象**，逐字段挑选可导出内容（目标 ID、探测类型、路由模式、采样号、成功、耗时、状态码、结果分类、错误类别、统计口径），自由文本（error_summary/error_detail/诊断四类文本字段）全部经 `sanitize_text`。诊断在**原始**数据上求值（脱敏文本不影响规则匹配），输出再安全化。
- **redaction.rs sanitize_text 加固**：
  - 敏感头（大小写不敏感，值至行尾）：`Authorization`/`Cookie`/`Set-Cookie`/`X-API-Key`/`Api-Key`/`X-Auth-Token`/`Proxy-Authorization` → `Name: [已隐藏]`；
  - URL userinfo（仅 authority 形态，不误伤邮件地址类文本）；
  - 敏感键值（`?k=`/`&k=`/`"k":`/`k:`，键集 token/key/secret/password/apikey/api_key/access_token/auth）；
  - 私有 IP：IPv4 10/8、127/8、172.16/12、192.168/16、169.254/16（带边界判定，版本号如 1.2.3.4 不误伤）+ IPv6 `::1`/`fe80::`/`fc00::`/`fd00::` → `[内网地址]`；
  - 内部主机名：`localhost` 及含 internal/corp/lan/local 标签的域名 → `[内部主机]`（诊断白名单域名 github.com 等保留）；
  - 全部规则**位置推进式**实现，无死循环；多次出现全部处理。
- **修复中发现的额外缺陷**：Markdown 报告的事实/原因/步骤/局限四行此前**未**过脱敏（验收探针未覆盖到的通道）——已修复并在回归测试中覆盖。

### F-03（`docs/INSTALL.md`）

新增"升级前备份"章节：明确 WAL 模式下只复制 `history.db` **不构成可靠备份**；给出三种可靠方式（退出进程后三文件一起复制 / SQLite Backup API / 以三文件为默认做法）。

### F-04（README / CHANGELOG / ARCHITECTURE / INSTALL）

- README：测试数更新为 117→119（含导出回归）；隐私口径改为"启动不自动探测，用户主动诊断时发起所选目标 HTTPS 测量"（纠正"零网络请求"绝对化表述）。
- CHANGELOG：同步纠正同一表述。
- ARCHITECTURE：R11 由"待复核"改为"已通过专项修复关闭"（D-021）。

### F-05（`.github/workflows/ci.yml`）

顶层显式 `permissions: contents: read`；NSIS 构建仍仅 `workflow_dispatch` 手动触发、只上传 artifact——CI 结构上无法创建 Release 或改仓库设置。

## 3. 失败复现与修复后结果

| 场景 | 修复前 | 修复后 |
|---|---|---|
| F-01 复现 | `impl_rejects_phase3_modes_with_not_available`（旧测试**确认拒绝行为**） | 改为 `impl_accepts_all_three_route_modes`：三模式到达后端（Stub 以 not_available 应答代表"已过门控"）；+ 非法代理 4 例 invalid_input；+ 非手动模式携带地址拒绝 |
| F-02 复现（四类假凭证） | Cookie `FAKESESSION...`、`sk-FAKEKEY...`、`192.168.1.1`/`10.0.0.5`、`db.internal.corp.example` 原样出现在 JSON | `regression_f02_no_leak_in_json_and_markdown`：JSON 与 Markdown 均零泄漏；头值/内网地址/内部主机均有遮蔽标记 |
| F-02 诊断字段重漏 | Markdown 事实行嵌入原始错误摘要（含 API Key） | `regression_f02_diagnoses_fields_sanitized`：JSON+MD 均不含假凭证 |
| F-02 对抗输入 | —（新增） | `regression_f02_no_hang_on_adversarial_input`：8 类对抗输入（含 200 次重复 token、500 次 Cookie:、控制字符）总耗时 < 2s，无死循环无崩溃 |
| 大小写/多次出现 | —（新增） | `cOoKiE`/`COOKIE` 三次出现全遮蔽；`10.1.2.3`×2 全遮蔽 |
| 白名单不误伤 | —（新增） | 公网 `8.8.8.8`、`github.com`/`api.openai.com`、版本号 `1.2.3.4` 保留 |
| 预览=导出 | —（新增） | 同一 `build_report` 纯函数生成，两次调用确定性一致 |

**测试驱动过程**：F-01 先改旧测试（复现门控缺陷）→ 修复；F-02 回归测试在修复中真实失败过两次（内部主机规则未覆盖 `db.internal.corp.example` 中段标签；Markdown 事实行漏脱敏），逐一定位修复后全绿——证明测试有效，非"先修后补"。

## 4. 自动化测试统计（2026-10-25 实际执行）

| 命令 | 退出码 | 结果 |
|---|---:|---|
| `npm run lint` | 0 | 通过 |
| `npm run typecheck` | 0 | 通过 |
| `npm run test` | 0 | **35/35**（9 文件） |
| `npm run build` | 0 | 主包 431 KB + 图表 chunk 351 KB |
| `cargo fmt --check` | 0 | 通过 |
| `cargo clippy --all-targets -- -D warnings` | 0 | 0 错误 |
| `cargo test` | 0 | **119 项全绿**：41 单元（+2 F-01）+ 19 E2E + 13 代理行为 + 31 规则 + 8 存储 + 7 导出回归（新增）+ 0 公网 ignored 3 |
| `npm run tauri build` | 0 | NSIS 2.65 MiB（见 §5） |

本轮 `cargo test` 未需设置 `OPENSSL_CONF`（TLS mock 13 项全部通过）；如 QA 环境复现 OpenSSL 配置问题，可按验收报告方式仅对测试进程设置，不影响生产证书校验。

## 5. 新安装包元数据（替换旧 SHA-256）

| 项 | 值 |
|---|---|
| Git Commit | `e400d218b17c815125de9433240fd0d1aec21db8` |
| 版本 | 0.1.0（三处一致） |
| 绝对路径 | `E:\ai-dev-cockpit\src-tauri\target\release\bundle\nsis\AI Dev Cockpit_0.1.0_x64-setup.exe` |
| 大小 | 2,783,923 字节（2.65 MiB） |
| **SHA-256** | `5624A3BE576CB6672FBACD536F0F5FE51D3C8CE8198C86E21C7B0041DC506FAD` |
| 构建 | `npm run tauri build` 退出码 0（构建日志尾：`Finished 1 bundle at: ...AI Dev Cockpit_0.1.0_x64-setup.exe (2.65 MiB)`）；构建于提交后、工作区干净状态，commit-to-artifact 对应关系成立 |
| Release exe 启动 | 通过（启动→进程存活→正常终止；用户真实 DB 已存在，启动仅打开与执行保留策略——该行为已经用户 Phase 5 验收确认无害） |
| 旧 SHA-256 | `2AF65668...6E14EBA` **已作废**（旧包文件已删除） |

## 6. 未执行的人工验收（保持 NOT TESTED）

安装向导全流程/升级/卸载与数据保留、干净系统与断网首启、WebView2 引导、Release UI 三模式真实运行（IPC 门控已修，待 UI 复验）、另存为对话框真实交互（取消/权限/覆盖）、应用进程网络流量观察、推送后 GitHub CI 首跑。真实安装验收建议在 Windows Sandbox/VM 执行并先获授权。

## 7. 剩余安全风险

1. 安装包与应用**未签名**——供应链与 SmartScreen 体验风险（文档已说明；签名需证书，超出本轮）。
2. 外部依赖未做漏洞数据库审计（锁文件版本固定，CI 未含 audit 步骤——可作为后续项）。
3. `rfd` 另存为允许用户选择任意可写路径（这是功能本意）；写入内容已全量脱敏。
4. 脱敏为保守规则集，无法保证覆盖未来新增的自由文本形态——结构性缓解（逐字段白名单导出）已落地，新增导出字段必须显式加入 `secure_*_json`。

## 8. 结论

F-01~F-05 已修复并有回归测试守护，F-06 通过干净重建解决；八项质量基线真实执行全 PASS。**具备重新启动 Release Acceptance Test 的条件**；v1.0 未发布，等待独立 QA 复验。
