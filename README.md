# ColdStart / 冷启动

> Vibe Coding 之前，先把项目冷启动。

ColdStart 是一款单机桌面工具，面向 Web、管理后台、App 和小程序组成的互联网产品。它在 AI 编码开始之前整理需求、项目蓝图、技术决策、三方服务、发布前置条件和开发计划，再通过 MCP 把正确的工作与上下文交给 Codex 等外部 Agent。

ColdStart 不生成业务代码，也不替代 Agent。它负责项目的认知与交付控制面：知道项目需要什么、现在该做什么、哪些条件尚未满足、每一项完成依据是什么。

## 当前状态

- 第一阶段 Rust 核心与 CLI 已完成：可创建可迁移的项目主目录，在本地 SQLite 中保存项目和追加式事件，并查询项目状态。
- 第二阶段 Tauri 桌面壳已完成：可通过桌面窗口创建、打开并刷新项目状态。
- 第三阶段需求与蓝图已完成：可导入需求、调用 OpenAI 兼容模型生成结构化蓝图、人工编辑与发布，并在 Inbox 中补充阻塞信息。
- 第四阶段计划引擎与 MCP 已完成：可从已发布蓝图编译确定性任务 DAG，通过租约领取工作、编译上下文、提交结构化报告，并在桌面看板观察进度。
- Git/GitHub、凭据引用、发布门禁与安装包仍属于后续阶段。

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

桌面应用现在支持项目创建与打开、需求导入、蓝图生成与人工编辑、不可变版本发布、计划编译与发布、任务状态观察，以及 Inbox 回答、人工计划任务和项目事实归档。

## 需求与蓝图

1. 创建或打开项目后进入“需求导入”，导入 Markdown、TXT、DOCX 或 PDF 文档。
2. 配置 OpenAI 兼容服务的 Base URL、模型名称与 API Key，并明确确认将需求文本发送到该服务。
3. 生成结构化蓝图，检查平台矩阵、模块依赖、技术栈、三方服务和发布前置条件，再按需人工修改。
4. 保存会创建新的草稿版本；只有通过结构、来源引用和模块 DAG 校验后才能发布。发布版本只读，后续修改会生成新版本。
5. 阻塞性缺失信息进入人工 Inbox；回答后请求自动归档，并形成带版本的项目事实。

支持单个不超过 20 MiB 的需求文档，单次抽取的锚点文本总量上限为 80,000 字符。扫描型 PDF 当前会返回“无可提取文本”的明确错误，不执行 OCR。

模型 API Key 只存在于当前界面内存和本次 IPC 调用中，不写入项目文件、SQLite、事件、日志或错误消息。切换页面或重启应用后需要重新输入。

## 开发计划与 MCP

1. 发布蓝图后进入“计划看板”，编译由 `plan-v1` 模板确定性生成的开发计划。模板覆盖平台工程基础、服务端契约、模块纵向切片、三方能力准备与接入、发布条件和平台发布里程碑。
2. 计划草稿可人工调整标题、目标、范围、明确不做、优先级、关键路径、验收标准、证据要求和 `blocks` 依赖；保存时重新校验任务键、DAG、路径边界和验收结构。
3. 发布计划后，任务按 `blocks` 依赖解锁。编码任务只能由持有有效租约的 MCP Agent 提交完成报告；桌面 UI 只允许完成 `human_external` 与 `approval_gate` 任务。
4. 计划看板每 3 秒刷新，窗口重新获得焦点时立即刷新；刷新失败会保留最后一次成功快照并显示错误。

构建 release MCP 二进制：

```powershell
. D:\DevTools\coldstart-env.ps1
cargo build -p coldstart-mcp --release
```

Codex 或兼容 MCP 客户端使用以下配置主动连接项目：

```json
{
  "mcpServers": {
    "coldstart": {
      "command": "D:\\coldstart\\target\\release\\coldstart-mcp.exe",
      "args": ["--project-home", "D:\\work\\your-project"]
    }
  }
}
```

MCP 只暴露七个工作工具：

- `claim_next_work`：按 Agent 与逻辑仓库原子领取下一个可执行任务。
- `get_work_context`：返回所选任务在预算内编译的上下文包。
- `read_task_reference`：读取任务白名单内的额外引用。
- `heartbeat_work`：续租并记录活动。
- `report_work_blocker`：上报阻塞与需要人工提供的最小输入。
- `submit_work_report`：提交带契约哈希、验收结果、证据和幂等键的结构化报告。
- `release_work`：释放无法继续的任务租约。

ColdStart 不启动 Codex，也不常驻后台。默认租约为 30 分钟，建议每 5 分钟心跳；默认上下文预算为 12,000 tokens。租约令牌是 256 位随机值，SQLite 只保存 SHA-256 哈希，令牌不会写入事件、日志、UI、报告或 MCP 回放。授权类项目事实在 Agent 上下文中只标记为“已登记”，具体值不会展开。

阶段 4 不实现 Git/GitHub 操作、真实 Commit 校验、真实 OSS/短信/支付密钥、自动部署、App 打包、商店提审与命令自动验证。

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
- [第四阶段实施计划：计划引擎与 MCP](docs/superpowers/plans/2026-10-30-coldstart-04-plan-engine-mcp.md)

## 安全说明

ColdStart 避免把明文凭据交给模型，并限制 MCP 可访问的项目范围。MVP 不是面向恶意本地进程的完整沙箱；运行 Agent 的账户权限仍应与敏感生产凭据隔离。
