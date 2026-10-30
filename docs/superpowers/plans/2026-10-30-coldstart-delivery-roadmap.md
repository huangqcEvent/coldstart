# ColdStart 交付路线图

这是实施路线图，不是可直接执行的任务计划。每个阶段另有一份详细计划，并在上一阶段接口稳定后编写，避免提前假设尚未实现的接口。

## 计划拆分

1. **核心基础与 CLI（已完成）**：Rust workspace、领域模型、SQLite、事件日志、项目主目录、`coldstart init` 和 `coldstart status`。阶段计划与执行记录：[2026-10-30-coldstart-01-core-foundation.md](2026-10-30-coldstart-01-core-foundation.md)。
2. **Tauri Desktop Shell（已实现）**：Tauri 2、React/TypeScript、项目创建、打开与状态刷新界面、Tauri 命令边界。阶段计划：[2026-10-30-coldstart-02-desktop-shell.md](2026-10-30-coldstart-02-desktop-shell.md)。
3. **Requirements and Blueprint**：文档导入、模型适配器、结构化蓝图、来源追踪、缺失信息与 Inbox 基础。
4. **Plan Engine and MCP**：计划编译、DAG、状态机、租约、上下文编译、MCP stdio 和完成报告。
5. **Git, GitHub, Capabilities and Release Gates**：仓库远程、GitHub OAuth、任务分支推送、凭据引用、三方能力、发布清单与审批。
6. **Integration and Hardening**：跨子系统端到端测试、安全测试、变更重规划、打包和 MVP 验收。

## 阶段门禁

- 每阶段结束时 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 必须通过；前端阶段同时运行 TypeScript 检查、前端单元测试和生产构建。
- 每阶段产出可独立演示和验证的软件，不把未完成的多层功能混在同一次提交中。
- 规格变更先更新 `docs/superpowers/specs/2026-10-30-coldstart-design.md`，再修改后续计划。
