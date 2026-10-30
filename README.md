# ColdStart / 冷启动

> Vibe Coding 之前，先把项目冷启动。

ColdStart 是一款单机桌面工具，面向 Web、管理后台、App 和小程序组成的互联网产品。它在 AI 编码开始之前整理需求、项目蓝图、技术决策、三方服务、发布前置条件和开发计划，再通过 MCP 把正确的工作与上下文交给 Codex 等外部 Agent。

ColdStart 不生成业务代码，也不替代 Agent。它负责项目的认知与交付控制面：知道项目需要什么、现在该做什么、哪些条件尚未满足、每一项完成依据是什么。

## 当前状态

- 第一阶段 Rust 核心与 CLI 已完成：可创建可迁移的项目主目录，在本地 SQLite 中保存项目和追加式事件，并查询项目状态。
- 第二阶段 Tauri 桌面壳已完成：可通过桌面窗口创建、打开并刷新项目状态。
- 第三阶段需求与蓝图已完成：可导入需求、调用 OpenAI 兼容模型生成结构化蓝图、人工编辑与发布，并在 Inbox 中补充阻塞信息。
- 计划引擎、MCP、Git/GitHub、凭据引用、发布门禁与安装包仍属于后续阶段。

```powershell
cargo run -p coldstart-cli -- init --name "Demo Project" --path D:\work\demo
cargo run -p coldstart-cli -- status --path D:\work\demo --json
```

`coldstart.project.json` 不保存机器绝对路径；移动到其他目录后，项目根路径由该文件所在目录重新推导。

当前 CLI 的 `status --json` 要求项目根路径为 UTF-8；`init` 会在写入清单前以明确错误拒绝非 UTF-8 根路径。

## 桌面应用

本机已将 Rust 与构建工具安装在 D 盘。新终端可先加载环境脚本，再在仓库根目录安装工作区依赖：

```powershell
. D:\DevTools\coldstart-env.ps1
pnpm install
pnpm --dir apps/desktop dev
pnpm --dir apps/desktop build
```

开发窗口同时依赖 Vite 与 Tauri，Tauri CLI 从工作区 `node_modules` 运行；pnpm store 固定在仓库内的 `.pnpm-store`，Rust 构建产物位于 `target`，均留在 D 盘。生产构建输出 `target\release\coldstart-desktop.exe`，当前不生成安装包。

桌面应用现在支持项目创建与打开、需求导入、蓝图生成与人工编辑、不可变版本发布，以及 Inbox 回答与项目事实归档。

## 需求与蓝图

1. 创建或打开项目后进入“需求导入”，导入 Markdown、TXT、DOCX 或 PDF 文档。
2. 配置 OpenAI 兼容服务的 Base URL、模型名称与 API Key，并明确确认将需求文本发送到该服务。
3. 生成结构化蓝图，检查平台矩阵、模块依赖、技术栈、三方服务和发布前置条件，再按需人工修改。
4. 保存会创建新的草稿版本；只有通过结构、来源引用和模块 DAG 校验后才能发布。发布版本只读，后续修改会生成新版本。
5. 阻塞性缺失信息进入人工 Inbox；回答后请求自动归档，并形成带版本的项目事实。

支持单个不超过 20 MiB 的需求文档，单次抽取的锚点文本总量上限为 80,000 字符。扫描型 PDF 当前会返回“无可提取文本”的明确错误，不执行 OCR。

模型 API Key 只存在于当前界面内存和本次 IPC 调用中，不写入项目文件、SQLite、事件、日志或错误消息。切换页面或重启应用后需要重新输入。

## 解决的问题

- 需求、技术栈、账号、密钥、提审材料和发布条件散落在不同文档与对话中。
- Agent 每次都需要重新理解项目，容易丢失约束或重复实现。
- 传统待办列表无法表达前置条件、外部审批、长周期发布任务和完成证据。
- 把全部需求与密钥塞进上下文既昂贵，也不安全。
- Agent 口头报告完成，但项目缺少可追踪的上下文、证据和审计记录。

## 核心闭环

```text
导入需求
  -> 生成并编辑项目蓝图
  -> 发现缺失信息与待授权能力
  -> 编译带依赖关系的开发计划
  -> Codex 通过 MCP 主动领取任务
  -> 提交结构化完成报告与证据
  -> 自动解锁后继任务
  -> 人工只处理信息、决策、授权和风险门禁
```

## 设计原则

- **单机优先**：项目数据、计划和执行记录保存在本地。
- **按需中断**：不要求用户全程盯盘，只在缺信息、缺授权、低置信度决策和不可逆操作时介入。
- **确定性内核**：模板、依赖、门禁和状态机由确定性代码控制；模型只负责提取、生成建议和总结。
- **最小上下文**：每个任务只接收相关需求、契约、约束、代码入口和能力状态。
- **凭据隔离**：真实密钥不进入 Prompt、MCP 响应、日志或 Git；Agent 只看到能力与使用约束。
- **可追踪**：蓝图、计划、任务契约、完成报告和审批都有版本与审计记录。
- **Agent 自证完成**：MVP 不重跑测试，接受结构化 Agent 报告，但保留证据来源和验收策略扩展能力。
- **仓库安全**：任务分支可自动推送；受保护分支、生产发布、提审和破坏性操作必须人工确认。

## MVP 范围

- 单用户、本地桌面客户端。
- 支持多仓库项目，以及 Web、服务端、管理后台、App、小程序平台目标。
- 导入 Markdown、TXT、DOCX、PDF 需求文档。
- 自动或手工生成模块、技术栈、接口、三方能力与发布条件。
- 生成并编辑任务 DAG、验收策略和人工门禁。
- 提供 MCP 任务领取、上下文读取、心跳、阻塞上报和完成报告。
- 支持 GitHub 私有仓库创建、远程仓库配置和任务分支自动推送。
- 管理本地凭据引用、能力状态与发布清单。

明确不做：团队协作、云同步、自动 App 提审、自动小程序发布、生产部署、重跑 Agent 测试和通用行业模板。

## 技术形态

MVP 采用 Tauri 2、Rust、React、TypeScript 和 SQLite。Rust 负责领域模型、状态机、计划引擎、SQLite、Git、凭据、MCP stdio 与 CLI；React/TypeScript 只负责桌面界面。桌面 UI、MCP 入口和 CLI 共享同一套 Rust 核心 crate，不使用后台守护进程。

## 文档

- [产品与系统架构规格](docs/superpowers/specs/2026-10-30-coldstart-design.md)
- [交付路线图](docs/superpowers/plans/2026-10-30-coldstart-delivery-roadmap.md)
- [第一阶段实施计划：Rust 核心与 CLI](docs/superpowers/plans/2026-10-30-coldstart-01-core-foundation.md)
- [第二阶段实施计划：Tauri Desktop Shell](docs/superpowers/plans/2026-10-30-coldstart-02-desktop-shell.md)
- [第三阶段实施计划：需求与蓝图](docs/superpowers/plans/2026-10-30-coldstart-03-requirements-blueprint.md)

## 安全说明

ColdStart 避免把明文凭据交给模型，并限制 MCP 可访问的项目范围。MVP 不是面向恶意本地进程的完整沙箱；运行 Agent 的账户权限仍应与敏感生产凭据隔离。
