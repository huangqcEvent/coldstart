# ColdStart / 冷启动

> Vibe Coding 之前，先把项目冷启动。

ColdStart 是一款单机桌面工具，面向 Web、管理后台、App 和小程序组成的互联网产品。它在 AI 编码开始之前整理需求、项目蓝图、技术决策、三方服务、发布前置条件和开发计划，再通过 MCP 把正确的工作与上下文交给 Codex 等外部 Agent。

ColdStart 不生成业务代码，也不替代 Agent。它负责项目的认知与交付控制面：知道项目需要什么、现在该做什么、哪些条件尚未满足、每一项完成依据是什么。

## 当前状态

- 第一阶段 Rust 核心与 CLI 已完成：可创建可迁移的项目主目录，在本地 SQLite 中保存项目和追加式事件，并查询项目状态。
- 第二阶段 Tauri 桌面壳已完成：可通过桌面窗口创建、打开并刷新项目状态。
- 第三阶段需求与蓝图已完成：可导入需求、调用 OpenAI 兼容模型生成结构化蓝图、人工编辑与发布，并在 Inbox 中补充阻塞信息。
- 第四阶段计划引擎与 MCP 已完成：可从已发布蓝图编译确定性任务 DAG，通过租约领取工作、编译上下文、提交结构化报告，并在桌面看板观察进度。
- 第五阶段仓库、能力与发布门禁已完成：可映射多仓库、使用 Windows Credential Manager 保存 Secret、验证并推送任务分支、配置 GitHub 与三方能力，并在只读发布中心汇总上架和审批状态。
- 第六阶段集成、重规划与安全加固已完成：活动计划可在无租约时安全替换，历史任务与旧契约不能推进新计划；跨子系统 MVP 验收通过，并生成包含桌面端与 MCP 的 Windows NSIS 安装包。
- 第七阶段本地命令验证与完成门禁已完成：仓库可配置受约束的 `.exe` 验证命令，Codex 的结构化报告只有在 ColdStart 独立重跑全部命令并确认 Git 不变量后才进入 `done`；失败保留租约并返回脱敏摘要。
- 自动部署、App 打包、商店提审、小程序发布、自动合并和 Release Tag 仍明确不在当前 MVP 内。

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
pnpm --dir apps/desktop build:installer
```

开发窗口同时依赖 Vite 与 Tauri，Tauri CLI 从工作区 `node_modules` 运行；pnpm store 固定在仓库内的 `.pnpm-store`，Rust 构建产物位于 `target`，均留在 D 盘。只构建桌面二进制时输出 `target\release\coldstart-desktop.exe`；完整安装包由 `build:installer` 生成。

桌面应用现在支持项目创建与打开、需求导入、蓝图生成与人工编辑、不可变版本发布、计划编译与发布、重规划差异确认、任务状态与验证摘要观察、Inbox 回答与人工计划任务，以及仓库映射、仓库验证命令编辑、GitHub 授权、系统凭据、三方能力状态和发布中心。

## Windows 安装包

完整安装包位于 `target\release\bundle\nsis\ColdStart_0.1.0_x64-setup.exe`，使用 Tauri NSIS 为当前用户安装，不要求管理员权限，并允许把安装目录改到 D 盘：

```powershell
& 'D:\coldstart\target\release\bundle\nsis\ColdStart_0.1.0_x64-setup.exe' /S /D=D:\DevTools\ColdStart
```

安装目录同时包含 `coldstart-desktop.exe`、`coldstart-mcp.exe`、Web 资源和卸载器。安装版计划看板会优先定位桌面 EXE 同级的 MCP 二进制，也会识别 Tauri 的 `resources\coldstart-mcp.exe`。卸载会移除程序文件和开始菜单入口，但不会删除用户项目目录、SQLite 数据库、报告或 Git 仓库。

安装包尚未代码签名，Windows SmartScreen 可能提示“未知发布者”。这是当前已知限制，不代表安装包已经签名。自动更新、崩溃上报和安装统计均未启用。

若构建环境无法稳定下载 NSIS，可预先准备 `nsis-3.13` 与 `nsis_tauri_utils.dll`，放到 `target\.tauri\NSIS`，并保持 `bundle.useLocalToolsDir=true`；构建脚本会先把 MCP release 二进制放进安装资源，再执行 Tauri NSIS 构建并输出 SHA-256。

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
4. 需要替换已激活计划时，桌面端先展示新增、变更、未变化和删除任务，以及继承完成、重新打开、取消与活动租约数量；用户确认后才提交替换事务。存在活动租约时禁止发布。
5. 新计划按稳定 `task_key` 继承安全状态；契约变化的旧工作项标记为过期并重新打开，删除任务会取消。旧计划、报告和事件永久保留，但计划看板、Inbox、发布中心、任务上下文和 MCP 引用只读取当前活动计划。
6. 计划看板每 3 秒刷新，窗口重新获得焦点时立即刷新；刷新失败会保留最后一次成功快照并显示错误。

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

MCP 暴露七个工作工具和一项只读能力状态查询：

- `claim_next_work`：按 Agent 与逻辑仓库原子领取下一个可执行任务。
- `get_work_context`：返回所选任务在预算内编译的上下文包。
- `read_task_reference`：读取任务白名单内的额外引用。
- `heartbeat_work`：续租并记录活动。
- `report_work_blocker`：上报阻塞与需要人工提供的最小输入。
- `submit_work_report`：提交带契约哈希、验收结果、证据和幂等键的结构化报告，并由 ColdStart 独立执行仓库验证命令。
- `release_work`：释放无法继续的任务租约。
- `get_capability_status`：按能力与环境读取状态、提供方和凭据引用标识，永不返回值。

ColdStart 不启动 Codex，也不常驻后台。默认租约为 30 分钟，建议每 5 分钟心跳；默认上下文预算为 12,000 tokens。租约令牌是 256 位随机值，SQLite 只保存 SHA-256 哈希，令牌不会写入事件、日志、UI、报告或 MCP 回放。授权类项目事实在 Agent 上下文中只标记为“已登记”，具体值不会展开。

## 仓库、凭据与发布

1. 在“仓库与能力”中按固定逻辑键 `server`、`admin`、`web`、`app`、`mini_program` 映射已有 Git 工作区，配置远程 URL、默认分支、保护分支、认证方式与同步策略。
2. 领取编码任务时，ColdStart 使用系统 `git` 检查工作区、获取远程、验证基准分支并创建 `codex/*` 工作分支。工作区不干净、分支已存在、远程不可用或认证失败会释放租约，不覆盖本地或远程历史。
3. Agent 报告完成后，ColdStart 验证任务分支、Commit HEAD 和实际变更路径，并在仓库根目录顺序执行已配置的 `.exe` 验证命令；全部通过后才完成任务并执行普通推送。推送失败不会回滚已完成任务，但会在 Git 同步记录中保留风险和错误摘要。禁止强推、删除远程分支和写入保护分支。
4. 明文 Secret 只写入 Windows Credential Manager，SQLite、事件、日志、错误、任务上下文、MCP 响应和 Git 仓库只保留引用或状态。界面提供写入和删除，不提供回读；删除凭据元数据时会同步删除系统凭据条目。
5. 三方能力按 `development`、`staging`、`production` 跟踪，状态为 `missing`、`configured`、`verified`、`auth_required` 或 `disabled`，可关联一个系统凭据。
6. GitHub 支持导入本机 `gh` 已登录账号，也支持通过用户提供的 OAuth App Client ID 启动 Device Flow。创建私有仓库必须由桌面界面二次确认，成功后只更新对应本地仓库的 `origin`；MCP 不提供仓库 CRUD。
7. 发布中心只汇总 `release-prereq:*`、`release:*`、人工审批闸门和人工外部任务。它不部署、不打包、不提审，也不提供绕过 Inbox 的高风险任务完成入口。

当前 MVP 仍不执行自动合并、创建 Release Tag、生产部署、App 打包、商店提审和小程序发布。ColdStart 会执行仓库绑定的本地验证命令，但不提供云 CI、远程执行器或自动修复；MCP 不会获得 Secret，只会获得能力状态、环境变量名称和领取时的验证快照。

## 本地命令验证

1. 在“仓库与能力”的“仓库验证门禁”中为每个编码仓库配置 1-8 条命令。命令按顺序执行，每条包含名称、绝对路径 `.exe`、独立参数数组、超时和执行位置。
2. 命令不经过 Shell；不接受 `.cmd`、`.bat`、`.ps1`、管道、重定向、变量展开或 Credential Manager Secret 注入。工作目录固定为仓库根目录。
3. 配置在任务领取时固化为快照并写入 `work_runs`。之后修改仓库配置只影响下一次领取，已经领取的 Codex 任务仍使用原快照。
4. Codex 提交完成报告后，ColdStart 先校验真实 Git 分支、HEAD 和变更范围，再运行全部命令。执行前后都检查分支、HEAD 与已跟踪文件未变化。
5. 全部通过后写入验证通过记录、接受报告、完成任务并推送任务分支。非零退出、启动失败、超时或 Git 不变量破坏会把任务退回 `in_progress`，保留租约供修复后使用新幂等键重试。
6. 失败结果包含快照哈希、失败类型、命令名、退出码、耗时和最多 64 KiB 的脱敏尾部摘要；不会持久化或返回原始输出、租约令牌、Secret 或完整环境变量。
7. 稳定错误码包括 `verification_not_configured`、`verification_in_progress`、`verification_failed`、`verification_timeout`、`verification_interrupted` 和 `verification_command_invalid`。租约过期或进程崩溃会把运行中的验证标记为 `interrupted`，任务回到 `ready`。

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
  -> ColdStart 独立执行仓库验证命令
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
- **独立验证完成**：Agent 报告不等于完成；ColdStart 在任务分支上重跑仓库配置的验证命令，全部通过后才接受完成结果。
- **仓库安全**：任务分支可自动推送；受保护分支、生产发布、提审和破坏性操作必须人工确认。

## MVP 范围

- 单用户、本地桌面客户端。
- 支持多仓库项目，以及 Web、服务端、管理后台、App、小程序平台目标。
- 导入 Markdown、TXT、DOCX、PDF 需求文档。
- 自动或手工生成模块、技术栈、接口、三方能力与发布条件。
- 生成并编辑任务 DAG、验收策略和人工门禁。
- 提供 MCP 任务领取、上下文读取、心跳、阻塞上报和完成报告。
- 支持 GitHub 私有仓库创建、远程仓库配置和任务分支自动推送。
- 支持仓库级本地命令验证、领取时快照、失败重试和脱敏验证摘要。
- 管理本地凭据引用、能力状态与发布清单。

明确不做：团队协作、云同步、自动 App 提审、自动小程序发布、生产部署、云 CI、远程执行器、失败自动修复和通用行业模板。

## 技术形态

MVP 采用 Tauri 2、Rust、React、TypeScript 和 SQLite。Rust 负责领域模型、状态机、计划引擎、SQLite、Git、凭据、MCP stdio 与 CLI；React/TypeScript 只负责桌面界面。桌面 UI、MCP 入口和 CLI 共享同一套 Rust 核心 crate，不使用后台守护进程。

## 文档

- [产品与系统架构规格](docs/superpowers/specs/2026-10-30-coldstart-design.md)
- [交付路线图](docs/superpowers/plans/2026-10-30-coldstart-delivery-roadmap.md)
- [第一阶段实施计划：Rust 核心与 CLI](docs/superpowers/plans/2026-10-30-coldstart-01-core-foundation.md)
- [第二阶段实施计划：Tauri Desktop Shell](docs/superpowers/plans/2026-10-30-coldstart-02-desktop-shell.md)
- [第三阶段实施计划：需求与蓝图](docs/superpowers/plans/2026-10-30-coldstart-03-requirements-blueprint.md)
- [第四阶段实施计划：计划引擎与 MCP](docs/superpowers/plans/2026-10-30-coldstart-04-plan-engine-mcp.md)
- [第五阶段实施计划：仓库、能力与发布门禁](docs/superpowers/plans/2026-10-30-coldstart-05-git-capabilities-release.md)
- [第六阶段实施计划：集成、重规划与安全加固](docs/superpowers/plans/2026-10-30-coldstart-06-integration-hardening.md)
- [第七阶段实施计划：本地命令验证与完成门禁](docs/superpowers/plans/2026-10-30-coldstart-07-command-verification.md)

## 安全说明

ColdStart 避免把明文凭据交给模型，通过 Windows Credential Manager 保存 Secret，并限制 MCP 可访问的项目范围。MVP 不是面向恶意本地进程的完整沙箱；运行 Agent 的账户权限仍应与敏感生产凭据隔离。Git 推送主动拒绝保护分支、强推和越过任务范围的 Commit 变更。安装包未签名，分发时不要把它误标为已通过代码签名或商店审核。
