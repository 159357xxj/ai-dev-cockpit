# Gate B 豁免记录 — v0.1.0 Beta

| 项 | 内容 |
|---|---|
| 日期 | 2026-10-28 |
| 决定人 | 项目负责人 |
| 决定 | v0.1.0 Beta 不执行 Gate B（Windows 安装、升级、卸载以及打包后的桌面 UI 验收），直接发布 |
| Gate B 结论 | **WAIVED（未执行）**，不等于 PASS |

## 未经验证的范围

安装向导与安装路径；打包后应用的启动、WebView2 和主题显示；原生"另存为"对话框和"文件已存在"提示（契约 GA-V6-1.0 中 F-10 的 UI 部分）；安装版中的真实网络探测，以及手动代理失败时不回落直连（后端逻辑已有自动化测试覆盖，但未在安装版中操作验证）；安装版中历史记录的持久化；覆盖安装、卸载、重装和数据保留；版本升级。

## 已有证据

- Gate A：PASS（GA-V6-1.0，`4f7f337`，见 `RELEASE_ACCEPTANCE_REPORT_V8.md`）
- Gate C：PASS，仅指 CI 质量检查（run 40688859414，`ede0f59`，见 `RELEASE_ACCEPTANCE_REPORT_GATE_C.md`）
- `4f7f337` 之后没有改动产品源码与依赖，工具链固定为 Rust 1.97.1

## 发布的安装包

`AI Dev Cockpit_0.1.0_x64-setup.exe`，从 `4f7f337` 用 Rust 1.97.1 构建，未签名，
SHA-256 `CA792870B5732C938AF47DAD26DD05530EA57386E8F11A6D15B5C10BE6EDE0BE`。
发布时只重命名为 `AI-Dev-Cockpit_0.1.0_x64-setup.exe`，不重新构建。

## 对发布的影响

Release 标记为 Pre-release，发布说明中列出未经验证的范围。下一个版本发布前，或首次收到安装、界面类问题反馈时，补做 Gate B。
