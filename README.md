# ColdStart / 冷启动

> Vibe Coding 之前，先把项目冷启动。

[GitHub 仓库](https://github.com/huangqcEvent/coldstart) · [MIT License](LICENSE) · [参与贡献](CONTRIBUTING.md) · [安全说明](SECURITY.md)

ColdStart 是一款单机 Windows 桌面工具。它把需求、需求图片溯源、历史项目只读代码调查、项目蓝图、技术决策、三方能力、仓库配置、本地验证、AI Review、GitHub 草稿 PR、只读验收同步、冲突定位与部署状态、GitHub Project 协作概览、多仓库交付总览、开发计划和项目记忆组织成一套可追踪的工程控制面，再通过 MCP 让 Codex 主动领取任务或调查、读取上下文、按需查看任务图片、调用受限能力 API 和提交完成报告。

ColdStart 本身不生成业务代码，也不替你启动 Agent。它解决的是编码之前与编码之后最容易失真的问题：现在到底要做什么、依赖是否满足、密钥和提审材料是否就绪、Agent 是否真的完成了任务，以及本次改动是否值得进入主分支。

## 它解决什么问题

- 需求、模块划分、技术栈和三方服务散落在聊天、文档与个人记忆里。
- App、小程序、管理后台和 Web 同时开发时，发布前置条件与账号资质容易遗漏。
- Agent 每次都需要重新理解项目，容易越过范围或重复实现。
- Agent 口头说“完成了”，但缺少独立验证、真实 diff 检查和可审计记录。
- 团队代码规范只存在于 Wiki 或个人习惯中，AI 评审无法稳定参照。
- 真实密钥容易进入 Prompt、日志或 Git，缺少本地隔离边界。

## 适合谁

ColdStart 当前面向个人开发者、小团队中的单机使用场景，覆盖以下产品形态：

- Web 前台
- 管理后台
- 移动 App
- 微信小程序
- 服务端

仓库逻辑键固定为 `server`、`admin`、`web`、`app`、`mini_program`。一个项目可以使用其中任意组合，不要求五个仓库都存在。

### 适合的场景

**新项目从 0 到 1**

从需求文档开始，生成并人工校正模块、平台、技术栈、三方能力、发布条件和任务 DAG，再把任务交给 Codex 逐项实现。

**已有项目继续迭代**

把 ColdStart 当作长期项目的交付控制面。每次迭代导入新的变更需求，发布新的蓝图与计划版本；稳定的 `taskKey` 会继承已完成状态，契约变化的任务会重新打开，删除的任务会取消。

**第一次接入历史项目**

不需要把整个历史代码库重新生成一遍。创建 ColdStart 项目，映射现有仓库，导入当前架构说明与本次变更需求，把蓝图范围收敛到本次迭代，再生成任务计划。

ColdStart 不会自行扫描整库，也不会替你自动还原完整架构。需求页支持创建“只读代码调查”：Codex 通过 MCP 主动领取问题，在已映射的 Git 工作区中查清入口、调用链、影响范围和风险，再提交带路径与行号的结构化证据；证据只有经人工确认后才会进入下一次蓝图生成。历史项目的基线信息仍应通过架构文档、接口说明、代码入口清单和人工校正提供。

### 明确不做什么

- 不做团队协作、云同步、远程执行器或云 CI。
- 不自动部署生产环境。
- 不自动打包 App、提交商店审核或发布小程序。
- 不自动合并分支、创建 Release Tag 或强推远程历史。
- 不替代架构师、代码审查者或安全审计。
- 不承诺 AI Review 能证明代码“没有漏洞”。

## 工作方式

```text
导入需求
  -> 可选：人工查看需求图片，或由 Codex 按任务权限读取
  -> 可选：为历史项目创建只读代码调查，由 Codex 主动领取并提交证据
  -> 人工确认调查证据后，才作为下一次蓝图生成的参考
  -> 生成并人工校正蓝图
  -> 编译带依赖关系的开发计划
  -> 配置仓库、凭据、验证命令与代码规范
  -> 可选：只读查看仓库关联的 GitHub Project 外部进度
  -> Codex 通过 MCP 主动领取任务
  -> 提交结构化报告、分支、Commit 与证据
  -> ColdStart 校验真实 Git 变更并独立执行验证命令
  -> 从真实 diff 执行 AI Review
  -> passed / waived 后完成任务并推送任务分支
  -> 人工在桌面端创建 GitHub 草稿 PR
  -> 手动刷新 PR 合并就绪、Checks、Review 与行内评论
  -> GitHub 标记冲突时，仅从本地定位冲突路径
  -> 只读查看 PR 头提交的部署记录与最新状态
  -> 在 GitHub 人工审查并决定是否合并
  -> 在计划看板人工确认合入，或刷新已合入的 GitHub PR 自动写入合入时间
  -> 解锁依赖该编码任务的后继任务
  -> 在项目总览只读汇总各仓库任务、验证、推送与 PR 状态
  -> 在项目记忆中复盘每次变更与门禁结果
```

人工只在必须提供信息或承担风险时介入，例如补充阻塞信息、确认外部资质、授权三方能力、确认重规划、处理 Review 误报或执行不可逆操作。

## 安装与启动

### 使用 Windows 安装包

从 [GitHub Releases](https://github.com/huangqcEvent/coldstart/releases/latest) 下载最新安装包。

构建好的 NSIS 安装包位于：

```text
target\release\bundle\nsis\ColdStart_0.1.1_x64-setup.exe
```

例如安装到 D 盘独立目录：

```powershell
& '.\target\release\bundle\nsis\ColdStart_0.1.1_x64-setup.exe' /S /D=D:\Apps\ColdStart
```

安装目录包含桌面程序、`coldstart-mcp.exe`、Web 资源和卸载器。卸载不会删除 ColdStart 项目目录、SQLite 数据库、报告或 Git 仓库。

安装包尚未代码签名，Windows SmartScreen 可能显示“未知发布者”。自动更新、崩溃上报和安装统计均未启用。

### 从源码运行

本机开发依赖：

- Windows 10/11
- Rust 1.89 或更高版本
- Node.js 与 pnpm 11
- WebView2（Windows 11 通常已安装）

安装工作区依赖并启动桌面开发模式：

```powershell
pnpm install
pnpm --dir apps/desktop dev
```

只构建桌面二进制：

```powershell
pnpm --dir apps/desktop build
```

构建 NSIS 安装包：

```powershell
pnpm --dir apps/desktop build:installer
```

## 创建或打开项目

ColdStart 的“项目主目录”与业务代码仓库可以分开存放。推荐为每个产品建立独立目录，例如：

```text
D:\work\your-project
```

1. 启动 ColdStart。
2. 选择“新建项目”，输入名称并选择项目主目录。
3. 或选择“打开项目”，定位已有 ColdStart 主目录。

如果误选的是只包含一个 ColdStart 子项目的上层容器目录，客户端会自动定位该唯一子项目；若上层目录下存在多个项目清单，仍会要求进入具体项目目录，避免打开错误项目。

项目清单、SQLite 状态、规范文件和执行记录都在本地。清单不保存机器绝对路径，项目整体移动后根路径由清单所在目录重新推导；当前 CLI 的 JSON 状态输出仍要求根路径是 UTF-8。

### 最近项目与多项目切换

ColdStart 会在应用数据目录记录最近打开的项目，最多保留 12 个。下次启动时，如果上次项目仍然可用，客户端会直接恢复该项目，不再要求重新选择目录。侧边栏顶部的项目切换器可以在已登记项目之间快速切换；欢迎页也会列出最近项目，并标记已经移动、删除或数据库缺失的失效路径。

应用同一时间只保持一个活动项目，切换时会重新加载目标项目的清单、SQLite 状态和计划上下文。项目数据仍按目录隔离，不会把不同项目的任务、凭据或事件混在同一个数据库里。这样可以避免多个项目同时争用 SQLite、Git 工作区和任务租约；需要并行查看时，建议分别打开两个 ColdStart 客户端实例，而不依赖同一窗口内的并发写入。

“清除访问历史”只删除应用级最近项目索引，不会删除 `coldstart.project.json`、`.coldstart/state.db`、需求制品或业务 Git 仓库。应用级索引默认位于 Tauri 的应用数据目录，文件名是 `project-history.json`；其中不保存 Secret、Token、租约或项目内容。

## 完整工作流

### 1. 导入需求、GitHub Issue 与 Project 概览

在“需求导入”中导入一个或多个 Markdown、TXT、DOCX、PDF 文档。单个文档不超过 20 MiB，单次抽取锚点文本总量不超过 80,000 字符。扫描型 PDF 暂不执行 OCR。

Markdown 中使用本地相对路径引用的图片，以及 DOCX 内嵌图片，会随需求文档一起固化到项目制品目录并绑定到所在章节锚点。当前支持 PNG、JPEG、GIF 和 WebP；每份文档最多 50 张，单张和整份文档的图片总量都不超过 10 MiB。远程 URL、`data:`、绝对路径、文档目录外路径和非图片内容会被忽略，PDF 中图片暂不提取。远程图片不会由 ColdStart 下载。

需求页默认只展示图片数量、文件名、章节、MIME、大小和哈希，不展示原图，也不会把图片二进制发送给蓝图模型。点击“查看原图”后才从本地制品目录读取，并再次校验路径边界、文件大小和 SHA-256。图片不做 OCR，不参与自动需求分析；人工确认和 Agent 按需查看是两条独立入口。

同一页面还可以从已映射的 GitHub.com 仓库只读读取开放、已关闭或全部 Issue，每个状态最多 50 条最近更新记录。列表会过滤 Pull Request，并展示标题、正文预览、标签、更新时间、评论数和链接。读取后可再按编号、标题、标签或正文预览搜索，按标签过滤，或只查看尚未导入的记录；这些筛选只作用于当前快照。人工选择单条 Issue 后，ColdStart 使用最新快照生成 `github-issue-<number>.md`，复用本地锚点与来源哈希；同一 Issue 不能重复导入。Issue 只作为外部需求资料，评论、附件和 Projects 不会导入，正文不会触发命令，也不会自动生成蓝图。

同一页面还提供“GitHub Project 只读概览”。选择已映射的 GitHub.com 仓库后，可以手动读取最多 5 个 Projects v2；每个项目最多展示 30 个条目（Issue、PR、草稿或受限内容）和 10 个单选项字段，并标注项目或条目截断。该概览只用于人工确认外部协作进度，不写入 SQLite、不产生项目事件、不导入需求、蓝图或计划，也不会改变 ColdStart 任务状态。OAuth 授权需要 `read:project` scope，PAT 需要 Projects 只读权限。

#### 历史项目只读代码调查

历史项目迭代时，如果需求文档不足以说明现有实现，可以先在“需求导入”页创建只读代码调查：

1. 选择已映射的仓库和基线分支，填写需要查清的问题，例如现有入口、事务边界、鉴权链路或可能受影响的模块。
2. 桌面端只保存调查请求，不会启动 Codex，也不会自行扫描仓库。
3. Codex 通过 MCP 调用 `claim_next_code_investigation` 主动领取，在对应本地 Git 工作区只读排查后，用 `submit_code_investigation` 提交 Commit、摘要、路径、行号、符号、风险、建议修改范围和调查限制。
4. 状态进入 `submitted` 后由人工在桌面端“确认证据”或填写原因后“驳回报告”。只有 `confirmed` 调查会进入下一次蓝图模型上下文；`requested`、`claimed`、`submitted` 和 `rejected` 都不会作为修改依据。
5. 驳回后可重新创建问题；已提交报告是不可变快照，相同幂等键重复提交返回原结果，不同内容重复提交会被拒绝。

调查不下载整库，不保存源码正文、完整 Git 输出、Secret 或本地绝对路径。领取令牌只返回一次，SQLite 只保存其哈希；租约默认 60 分钟。蓝图生成最多带入最近 3 个已确认调查，代码证据总预算为 35,000 字符，超限时会明确标注省略数量。

生成蓝图前需要配置 OpenAI 兼容服务的 Base URL、模型名称和 API Key，并明确同意将需求文本发送到该服务。需求模型 Key 只存在于当前界面内存和本次 IPC 调用中，不写入 SQLite、事件、日志或项目文件；切换页面或重启后需要重新输入。

### 2. 生成并发布蓝图

蓝图包含：

- 产品目标、范围与非目标
- 角色与平台矩阵
- 模块划分及依赖
- 技术栈选择与理由
- 文件存储、短信、支付等三方能力
- App、小程序、商店账号等发布前置条件
- 假设与风险

生成前只发送当前勾选的需求来源，图片元数据和图片二进制都不会进入蓝图模型。已确认的项目事实与已确认的代码调查证据会作为独立参考输入；未确认调查不会参与生成。

生成结果只是草稿。手工修正模块、平台、技术栈和范围后，ColdStart 会校验来源引用、结构完整性和模块 DAG。发布版本不可变；后续修改会创建新版本。阻塞性缺失信息会进入“人工 Inbox”，回答后形成带版本的项目事实。

蓝图发布门禁按项目内全部“开放且阻塞”的信息请求计算，而不是只看当前草稿引用的修订号；因此保存新草稿不能绕过 Inbox。必须逐条回答，或对已经过时、重复、不再适用的问题填写原因后关闭，才能发布蓝图。关闭会保留原因和审计事件，但不会生成项目事实。

### 3. 编译与发布计划

“计划看板”会把已发布蓝图编译为确定性的任务 DAG，覆盖平台工程基础、服务端契约、模块纵向切片、三方能力准备、发布条件和平台里程碑。

草稿可调整标题、目标、范围、明确不做、优先级、关键路径、验收标准、证据要求和阻塞依赖。发布前会重新校验任务键、DAG、路径边界与验收结构。

替换活动计划时，ColdStart 会展示新增、变更、未变化、删除任务，以及继承完成、重新打开、取消和活动租约数量。用户确认后才执行替换；存在活动租约时禁止重规划。

### 4. 映射仓库与能力

在“仓库与能力”中：

1. 按 `server`、`admin`、`web`、`app`、`mini_program` 映射已有 Git 工作区。
2. 配置远程 URL、默认分支、保护分支、认证方式和同步策略。
3. 在 Windows Credential Manager 中保存 Git、云存储、短信、支付等 Secret 引用。
4. 按 `development`、`staging`、`production` 跟踪三方能力状态与关联凭据。
5. 在“能力 API 白名单”中登记 HTTPS Base URL、路径前缀、允许方法、鉴权头和凭据引用。

白名单只保存配置和凭据引用，不保存 Secret。Codex 调用时仍需通过有效租约、任务能力授权、能力状态、端点环境、方法/路径和凭据环境检查。

领取编码任务时，ColdStart 会检查工作区、获取远程、验证基准分支并创建 `codex/*` 任务分支。工作区不干净、分支冲突、远程不可用或认证失败会释放租约，不覆盖本地或远程历史。

### 5. 配置本地验证命令

每个编码仓库必须配置受约束的验证命令，否则任务不能领取。规则如下：

- 每个仓库 1 到 8 条命令，顺序执行。
- 每条命令使用绝对路径 `.exe` 和独立参数数组。
- 不经过 Shell，不接受 `.cmd`、`.bat`、`.ps1`、管道、重定向或变量展开。
- 工作目录固定为仓库根目录。
- 配置在任务领取时固化为快照，后续修改只影响下一次领取。

Codex 提交报告后，ColdStart 先校验真实分支、HEAD、Commit 和变更路径，再独立执行全部命令。任何非零退出、启动失败、超时或 Git 不变量破坏都会保留任务为 `in_progress`、保留租约且不推送。

### 6. 启用 AI Review

在“AI 评审”中配置模型与规则：

- Review 开关
- OpenAI 兼容 Base URL
- 模型名称
- Windows 凭据库中的模型凭据引用（本地模型可以不绑定凭据）
- 阻断完成的 finding 级别
- 明确的隐私同意

默认阻断 `critical` 与 `high`。模型不可用、超时、输入超限或响应格式非法时始终失败关闭，不提供“忽略失败”开关。

### 7. 让 Codex 通过 MCP 领取工作

Codex 可以先通过 MCP 读取项目简报和项目记忆恢复上下文，再主动领取下一个可执行任务。领取后读取编译后的任务上下文；如果当前任务与前序工作相关，还可以读取任务接力摘要。需要访问已配置的三方服务时，调用 `call_capability_api`，由本机注入 Secret。完成后提交结构化报告。ColdStart 不启动 Codex，也不要求 Codex 与桌面 UI 同时打开。

### 8. 完成与推送

只有以下条件全部满足，编码任务才会进入 `done`：

1. 报告中的分支、Commit 与变更文件通过真实 Git 校验。
2. 仓库验证命令全部独立通过。
3. AI Review 状态为 `not_required`、`passed` 或人工 `waived`。

完成后 ColdStart 执行普通推送。推送失败不会回滚已完成任务，而是写入 Git 同步风险记录。禁止强推、删除远程分支和写入保护分支。

`done` 表示任务交付物已完成：报告通过真实 Git 校验、验证命令全部通过、Review 达到完成条件，并且任务分支已成功推送。它不表示代码已经进入默认分支。编码任务还有独立的合入确认：

- 如果创建了 GitHub 草稿 PR，在 PR 合入默认分支后于桌面端刷新状态，ColdStart 会写入 `integrated_at` 和 `GitHub PR #<number>` 引用。
- 非 GitHub 场景或人工直接合入时，在“计划看板”该任务卡片中点击“确认已合入”，填写合入分支或 Commit 等引用。
- 依赖某个编码任务的后继任务，只有在它既为 `done` 又已确认合入后才会解锁；仅完成但未合入时，领取结果会提示“等待合入基线”。

合入确认只接受已推送的任务，重复确认是幂等的；它不会执行合并、改写远程历史或创建额外 Git 操作。

### 9. 创建 GitHub 草稿 PR 并同步验收信息

任务分支成功推送后，可以到“AI 评审”历史中点击“创建草稿 PR”。创建由桌面端人工触发，固定为草稿状态，不会合并或创建 Release Tag。

前置条件：

1. 编码任务已经是 `done`，报告已接受。
2. Review 状态为 `not_required`、`passed` 或 `waived`。
3. 报告分支和 Commit 存在成功的推送记录。
4. 仓库远程地址是 GitHub.com 的 HTTPS 或 SSH 地址。
5. 仓库使用凭据认证，凭据类型为 OAuth Token 或 Personal Access Token。

PR 正文包含任务、验收、验证、Review、变更文件和已知问题，并经过脱敏和长度限制。创建成功后可以打开 GitHub 链接，也可以回到 ColdStart 手动“刷新状态”。

每次刷新会以 PR 当前 head 提交为边界，先发起四个必需的只读 `GET` 请求，读取并保存：

1. PR 开放、关闭、已合并状态、草稿标记与合并状态。
2. 该提交上的 GitHub Checks。
3. PR Review 结论与正文。
4. PR 行内 Review 评论、文件路径和行号。

四个请求全部成功后才写入核心验收快照；任一步失败时，本次刷新不覆盖已有 PR 状态或验收信息。Checks 汇总区会分别显示无 Checks、运行中、失败与全部通过；失败或待处理状态只用于人工判断，不会触发自动修复、重跑或合并。

核心快照成功后，ColdStart 还会用同一个 head SHA 读取最多 10 条 GitHub Deployment，并读取每个部署的最新状态。部署列表请求失败时，只把部署区域标记为“不可用”，不会阻断 PR、合并状态、Checks、Review 与评论落库；单个部署状态读取失败时，该环境显示“状态未知”。部署区域展示成功、失败、错误、进行中、排队、等待、停用和未知状态，不保存 payload、日志链接或原始响应。

PR 卡片还会展示 GitHub 合并状态与“合并前检查”。合并冲突、暂不可合并、分支落后、草稿、GitHub 正在计算、Checks 失败或未完成、Review 要求修改都会列为待人工确认项。没有已知阻塞项也不代表已获准合并；最终仍受 GitHub 分支保护、审批和 Checks 规则约束。已合并或已关闭 PR 显示终态，不再执行合并前检查。

当 GitHub 明确标记合并冲突时，ColdStart 会使用 PR 详情中的 base/head 提交，在映射的本地仓库执行只读 `git merge-tree`，展示冲突文件路径。该诊断不新增 GitHub 请求，不切换分支、不修改工作区或索引，并会禁用系统/全局 Git 配置、覆盖仓库本地自定义 merge driver，避免只读刷新执行仓库脚本。若本地缺少对应提交，界面显示“无法在本地定位”；若本地未复现冲突，也不会宣称冲突已解决。冲突路径最多保存 100 条，不保存补丁内容或 Git 原始输出。

Checks、Review 和评论只读取第一页，每类最多展示 100 项；部署最多展示当前 head 提交最近 10 条。外部正文按纯文本显示，详情链接只接受 HTTPS。GitHub 返回未知状态时显示“未知”，ColdStart 不推断为通过或部署成功。

ColdStart 不自动合并、不关闭 PR、不转正式 PR、不写评论、不重跑 Checks、不创建 Commit Status、不自动解决冲突或更新目标分支，也不创建、审批、取消、重试或回滚部署，不创建 Release Tag。PR、合并状态、Checks、评论、冲突文件和部署状态同步只能在桌面端人工触发，不属于 MCP 工具。

### 10. 查看多仓库交付总览

“项目总览”中的多仓库交付总览会把当前已发布计划、仓库验证配置、最近 Git 同步、PR/Checks、合并冲突与 AI Review 快照按仓库集中展示：

- 总体展示有效任务完成度、可领取、执行中、需关注仓库和开放 PR 数量。
- 每个仓库展示任务计数、验证命令数量、最近任务分支推送、最近相关 PR、Checks、合并状态、Review 和下一步建议。
- 仓库固定按 `server`、`admin`、`web`、`app`、`mini_program` 排序；未映射的仓库不会伪造空卡片。
- 编码任务没有仓库键或引用了未配置仓库时，会显示“未关联任务”提示，避免静默遗漏。
- 同一仓库优先展示最近开放 PR；没有开放 PR 时展示最近一次 PR 记录。
- 点击“刷新状态”会重新读取项目工作区和交付总览；“重新读取”只刷新交付总览。

该视图只读取 ColdStart 已经保存在本地的数据，不发起 GitHub 或 Git 网络请求，不写 SQLite、不产生项目事件，也不改变任务、租约、PR 或 Review 状态。PR 的外部状态仍需先到“AI 评审”页手动刷新；总览展示的是最近一次成功保存的本地快照，不后台轮询，也不通过 MCP 暴露给 Agent。

### 11. 复盘项目记忆

“项目记忆”把 SQLite 事件日志整理成只读时间线，覆盖需求导入、信息请求回答与关闭、蓝图发布、计划发布、任务领取、心跳、阻塞、验证、AI Review、仓库与凭据配置、Git 同步等变更。

- 默认显示最近 100 条有效变更，隐藏高频任务心跳；可手动打开心跳。
- 按五个阶段筛选，并按标题、摘要、事件类型或对象搜索已加载事件。
- 使用游标向前加载更早的事件，不重复、不打乱从新到旧的顺序。
- 显示全部事件数、有效变更数、心跳数与各阶段计数。
- 只展示安全摘要，不返回原始 payload、仓库凭据 URL、租约 ID 或 Secret 引用。

Codex 可以通过 `get_project_memory` 读取同一安全投影；新会话可以用 `get_project_brief` 获取项目、计划、当前工作和最近变更摘要，任务执行中可以用 `get_work_handoff` 读取相关前序工作、已知问题和阻塞。

## 接入历史项目的建议步骤

### 同一产品继续迭代

1. 打开原 ColdStart 项目，保留已有蓝图、计划、任务与仓库配置。
2. 在“需求导入”中加入本次变更需求、缺陷说明或接口变更文档。
3. 如果现有代码入口或影响范围不清楚，创建只读代码调查；Codex 提交后人工确认证据。
4. 生成新的蓝图版本，人工收敛本次迭代的平台、模块、范围与非目标。
5. 发布新蓝图，编译新计划并在预览中检查任务继承、重新打开和取消情况。
6. 更新仓库验证命令与本次适用的代码规范。
7. 发布计划，让 Codex 领取新任务。

### 首次把历史项目纳入 ColdStart

1. 新建一个独立 ColdStart 项目，不把控制面文件混入业务仓库。
2. 映射现有 `server`、`admin`、`web`、`app`、`mini_program` 仓库。
3. 导入现有架构说明、接口约定、目录约定和本次需求变更。
4. 对本次需求涉及的入口、调用链、事务或鉴权边界创建只读代码调查，并逐条确认 Codex 提交的证据。
5. 手工补齐关键模块、三方能力、账号资质与发布前置条件。
6. 把蓝图范围限制在本次增量，不要试图一次描述完所有历史细节。
7. 为每个仓库配置真实可运行的验证命令，再发布本轮计划。

对于历史项目，最重要的不是让模型重写过去，而是明确“这次改什么、不能碰什么、如何验证、谁来提供必要信息”。

## 真实项目实战验收

2026-11-13 使用一份真实历史项目的隔离副本完成了端到端验收，原始仓库未被修改，客户名称、业务源码与内部路径未进入开源仓库。验收覆盖：

- 导入 6 份真实需求、测试用例和架构约束，人工收敛并发布蓝图。
- 配置隔离 Git 远程、服务端仓库、Maven 定向验证、OSS 测试凭据和仓库级代码规范。
- 将模板计划收敛为单一真实任务，由安装版 Codex MCP 主动领取。
- 先补失败测试确认 RED，再修复一个真实业务兼容接口并确认 GREEN。
- 提交报告后由 ColdStart 独立执行 JUnit 验证、调用 AI Review 并推送任务分支。
- 重启客户端后核对 `done` 状态、无活动租约、报告状态和完整项目记忆。

本轮 Review 使用本机 OpenAI 兼容测试端点，证明的是 Review 流程、快照、状态机和门禁集成，不代表模型评审质量。具体客户信息和内部验收证据不随开源仓库发布。

## 项目记忆与变更审计

项目记忆不是第二套业务状态，而是对已有 `events` 表的产品化读取。它让用户和后续维护者能够回答：

- 蓝图与计划分别是哪个版本、什么时候发布的？
- Codex 领取后经历了开始、心跳、阻塞、报告、验证和 Review 的什么顺序？
- 仓库、验证命令、凭据和三方能力是什么时候配置的？
- 某次任务为什么没有完成，门禁最后停在哪一步？

安全边界：

- 只读，不提供编辑、删除、回滚或重放。
- 严格按 `project_id` 隔离，不会读取其他项目事件。
- 应用层对全部事件类型做显式中文投影；前端不接触原始 `payload_json`。
- `work_heartbeat` 默认在 SQL 查询层排除，避免占满第一页。
- `RepositorySaved` 不展示远程 URL；任务心跳与领取事件不展示租约 ID；凭据只展示提供方、环境与安全对象引用。
- 分页大小限制为 1 到 500，默认 100；游标必须为正整数。

## AI Review 与代码规范库

### 评审对象

AI Review 不接受 Codex 自报的“已完成”，也不把 Agent 提交的 diff 文本当作证据。ColdStart 会：

1. 校验报告中的分支、Commit 和变更路径。
2. 在任务分支上独立执行仓库验证命令。
3. 以领取时固化的基准 Commit 和报告 Commit 为边界，从本地 Git 提取真实 diff。
4. 把真实 diff、任务验收标准、启用规范和脱敏验证摘要发送给 Review 模型。
5. 校验模型输出结构、finding 路径、行号、数量和长度。

Git diff 使用固定安全参数，禁用外部 diff、textconv、颜色和重命名推断。diff 最多 160,000 字符、1,000 个文件。

### 规范库

“AI 评审”页面支持：

- 上传 Markdown、TXT、DOCX、PDF 规范文件。
- 直接新建 Markdown 规范。
- 基于当前版本创建新修订。
- 项目级或单仓库范围。
- 启用、停用、排序和软删除。
- 查看原始文件名、提取字符数、内容哈希、修订号和更新时间。

规范限制：

- 最多 20 个规范。
- 单个规范最多 60,000 字符。
- 启用规范合计最多 60,000 字符。
- 正文不会静默截断；超限会返回 `review_input_too_large`。
- 原始文件与提取文本保存在项目 `.coldstart/review-standards/` 下，读取时校验 SHA-256 和字符数。

Review 领取任务时会原子固化配置、启用规范和基准 Commit。评审过程中使用快照，之后修改规范或 Review 配置只影响下一次领取。

### 状态与阻断

| 状态 | 是否允许完成 | 含义 |
| --- | --- | --- |
| `not_required` | 是 | 领取时 Review 未启用 |
| `passed` | 是 | 评审完成且没有命中阻断级别 |
| `waived` | 是 | `blocked` 后由桌面端人工豁免 |
| `running` | 否 | 评审进行中 |
| `blocked` | 否 | 存在达到阻断级别的 finding |
| `failed` | 否 | 模型不可用、超时、响应非法或输入超限 |
| `interrupted` | 否 | 租约过期或进程中断导致评审未完成 |

`failed` 和 `interrupted` 不允许人工豁免。用户需要修复模型配置、缩小任务或规范，或重新领取后再次提交。

### 误报人工豁免

只有 `blocked` Review 可以提供人工豁免入口，而且只在桌面端出现，MCP 不提供豁免工具。

流程：

1. 打开“AI 评审”历史，核对 finding、文件、严重级别和规范依据。
2. 点击“人工豁免”。
3. 填写非空理由。
4. 进入二次确认，明确任务将被接受并进入推送流程。
5. ColdStart 记录理由、本机操作者、时间、finding 哈希和规范快照哈希。

豁免不可撤销。租约过期或报告已不是当前有效报告时，存储层会拒绝豁免。

### 隐私边界

发送到 Review 模型的内容仅包括：

- 本次真实 Git diff
- 任务标题、目标和验收标准
- 启用规范的提取文本
- 脱敏验证摘要

不会发送：

- API Key 或其他 Secret
- Windows Credential Manager 中的值
- 完整环境变量
- 项目 SQLite 数据库
- diff 之外的项目文件
- 租约令牌

模型输出只能形成结构化 summary 和 findings，不能触发 Git、Shell、网络下载、文件写入或任意 MCP 工具。错误、事件、UI 与数据库只保留稳定错误码和脱敏摘要。

### 常见 Review 错误

- `review_consent_required`：更换模型、Base URL 或凭据后没有重新确认隐私边界。
- `review_credential_missing`：凭据元数据不存在，或 Windows 凭据库中的 Secret 不可读。
- `review_not_configured`：领取 `agent_coding` 任务时 Review 已启用但配置不完整。
- `review_input_too_large`：diff 或规范超过限制，需要拆分任务或缩小规范。
- `review_timeout`：模型调用超过 180 秒；任务保持进行中且保留租约。
- `review_response_invalid`：模型响应不是合法 JSON，或 finding 不符合路径、长度、数量约束。
- `review_failed`：网络或模型 HTTP 调用失败；不会保存原始响应正文。
- `review_blocked`：存在阻断 finding；修复后使用新的幂等键重新提交，或由桌面端人工豁免。
- `review_interrupted`：评审运行中遇到租约过期或进程中断。
- `review_standard_invalid`：规范文件缺失、哈希不一致或修订无效。

Review 成功后推送失败不会重新打开任务。风险会保留在“Git 同步记录”中。

## MCP 与 Codex

构建 release MCP 二进制：

```powershell
cargo build -p coldstart-mcp --release
```

Codex 或兼容 MCP 客户端使用以下配置连接某个 ColdStart 项目：

```json
{
  "mcpServers": {
    "coldstart": {
      "command": "D:\\path\\to\\coldstart\\target\\release\\coldstart-mcp.exe",
      "args": ["--project-home", "D:\\work\\your-project"]
    }
  }
}
```

安装版通常使用：

```json
{
  "mcpServers": {
    "coldstart": {
      "command": "D:\\Apps\\ColdStart\\coldstart-mcp.exe",
      "args": ["--project-home", "D:\\work\\your-project"]
    }
  }
}
```

实际路径以安装目录或 `target\release` 为准。计划看板提供“复制 MCP 配置”，会生成当前项目的版本。

### MCP 工具

当前实际暴露 16 个工具：

- `get_project_brief`：领取任务前读取有界的项目简报，包括当前蓝图、计划状态、当前工作、最近完成、阻塞、未决信息、项目事实和最近记忆；不返回本地路径、远程 URL、凭据引用或租约。
- `get_project_memory`：按游标分页读取安全项目记忆，支持 `limit` 和 `include_heartbeats`，默认隐藏心跳。
- `claim_next_code_investigation`：按 Agent 与可选逻辑仓库原子领取一个等待处理的只读代码调查，返回一次性领取令牌和最小问题上下文；不返回本地路径。
- `submit_code_investigation`：使用领取令牌提交不可变的结构化代码证据，包括 Commit、摘要、路径、行号、符号、风险、建议修改范围和限制；拒绝源码正文与 Secret。
- `claim_next_work`：按 Agent 与逻辑仓库原子领取下一个可执行任务，使用 `requestId` 保证重试幂等。
- `get_work_context`：用租约令牌读取预算内的任务上下文、验证快照和 Review 规范引用。
- `get_work_handoff`：在有效租约下读取当前任务的相关前序工作、最新报告、已知问题、后续事项和阻塞；不返回租约令牌。
- `read_task_reference`：读取任务白名单内的额外引用，包括当前 Review 快照中的规范修订。
- `list_task_images`：列出当前任务契约引用的模块、平台或上下文实体所属章节中的需求图片元数据；不返回二进制、制品相对路径或本地绝对路径。
- `read_task_image`：在有效租约和任务蓝图锚点双重授权下读取一张原图，返回标准 MCP 图片内容块与安全溯源元数据；每次读取都会重新校验文件边界、大小和 SHA-256。
- `heartbeat_work`：续租并记录 Agent 活动。
- `report_work_blocker`：上报阻塞与需要人工提供的最小输入。
- `submit_work_report`：提交契约哈希、验收结果、分支、Commit、变更文件、证据和幂等键；触发真实 Git 校验、本地验证、AI Review 和推送。
- `release_work`：释放无法继续的任务租约。
- `get_capability_status`：读取能力与环境状态、提供方和凭据引用标识，永不返回 Secret。
- `call_capability_api`：在有效租约和能力授权范围内调用已登记的三方 HTTPS API；由本机注入 Secret，响应限长并脱敏。

调查的创建、确认与驳回只在桌面端开放；MCP 只提供领取和提交，不提供自我确认。GitHub 草稿 PR 的创建与刷新也只在桌面端开放，不属于 MCP 工具。Agent 仍只能提交任务报告，不能合并、关闭或刷新 PR。

默认租约为 30 分钟，建议每 5 分钟心跳；默认上下文预算为 12,000 tokens。租约令牌是随机值，SQLite 只保存 SHA-256 哈希，不写入事件、日志、UI 或报告。

### 能力 API 受限代理

`call_capability_api` 不接受完整 URL，也不接受 Secret。输入为：

```json
{
  "capability_id": "capability_xxx",
  "environment": "development",
  "endpoint_id": "00000000-0000-4000-8000-000000000000",
  "work_id": "00000000-0000-4000-8000-000000000001",
  "lease_token": "当前租约令牌",
  "method": "POST",
  "path": "/v1/files",
  "query": [{ "key": "mode", "value": "upload" }],
  "body": "{\"name\":\"demo\"}",
  "content_type": "application/json"
}
```

调用必须同时满足：租约为当前活动租约；工作项是 Agent 编码任务；任务契约明确引用该能力；能力状态为 `configured` 或 `verified`；端点项目、能力和环境完全匹配；方法和路径命中白名单；凭据环境为共享或与端点一致。只读授权校验不会刷新心跳、推进任务或写事件。

代理只允许 HTTPS，拒绝重定向、URL 内嵌凭据、非绝对路径、`..` 和编码路径穿越。请求正文最多 256 KiB，响应正文最多保留 128 KiB，超时 20 秒。响应若回显注入的 Secret，会替换为 `[REDACTED]`。API 响应不写入 SQLite、项目记忆、报告或 Git。

### Codex 的推荐执行循环

```text
get_project_brief（可选，恢复项目上下文）
  -> get_project_memory（可选，分页查看历史）
claim_next_work
  -> get_work_context
  -> get_work_handoff（可选，继续相关任务时）
  -> list_task_images（可选，查看当前任务授权图片）
  -> read_task_image（可选，按需读取单张原图）
  -> 按允许路径修改代码
  -> 运行必要检查
  -> heartbeat_work（长任务期间）
  -> submit_work_report
  -> 若 verification/review 未通过：读取错误、修复、用新幂等键再次提交
  -> 任务完成后继续领取下一个
```

历史项目需要先查清现状时，使用独立调查循环：

```text
claim_next_code_investigation
  -> 在已映射仓库只读排查，不提交、不推送、不修改业务文件
  -> submit_code_investigation（一次性领取令牌与幂等键）
  -> 等待桌面端人工确认
  -> 人工确认后，再由桌面端生成下一代蓝图
```

遇到缺少账号、外部授权、产品决策或不可逆操作时，使用 `report_work_blocker`，不要猜测或绕过。

## 凭据与安全边界

### 凭据

- Git 与三方服务的 Secret 保存在 Windows Credential Manager。
- SQLite、事件、日志、错误、任务上下文、MCP 响应和 Git 仓库只保存凭据引用或状态。
- UI 支持写入和删除凭据，不支持回读明文。
- 删除凭据元数据时会同步删除对应系统凭据条目。
- Review 模型凭据通过引用选择；实际值只在执行评审时读取到内存。
- 需求生成蓝图的 API Key 只存在于当前界面内存与本次 IPC 调用中，重启后清空。
- 读取 GitHub Issue 与 Project 使用仓库已绑定的 OAuth/PAT 凭据；Token 只进入 Rust 请求头，不进入前端、SQLite、事件、日志或项目文件。
- 能力 API Secret 只在 Rust 代理发请求时从 Windows Credential Manager 读取；MCP 入参、任务上下文、SQLite、事件、日志、报告和响应都不包含明文 Secret。
- 能力 API 只允许静态鉴权 Header 注入，不支持 OSS SDK 签名等需要额外算法计算的方案；第一版仍存在“公网域名在 DNS 解析后指向内网地址”的理论 SSRF 边界，部署时应避免为不可信域名登记白名单。

### 仓库安全

- 自动创建 `codex/*` 任务分支，不直接修改保护分支。
- 拒绝强推、删除远程分支和越过任务允许路径的变更。
- 推送失败只记录风险，不伪造成功或回滚已完成状态。
- 创建 GitHub 私有仓库需要桌面端二次确认。
- 草稿 PR 只能由桌面端人工创建；请求正文由 ColdStart 生成，Token 不进入正文或本地记录。
- PR 刷新只通过只读请求同步状态、合并就绪、Checks、Review、行内评论和当前 head 提交的部署状态，不执行合并、关闭、评论、重跑、Checks 写入或部署操作。
- GitHub Issue 导入只能由桌面端人工触发，可只读开放、已关闭或全部 Issue；不创建、编辑、关闭、评论或分配 Issue。
- GitHub Project 只能由桌面端人工触发只读概览；不写 Project、字段或条目，不导入需求或计划，也不增加 MCP 工具。
- 多仓库交付总览只聚合本地已保存事实；不读取 Secret、不新增网络请求、不写项目状态，也不自动领取、合并、重跑、部署、打包、提审或发布。
- 历史代码调查只能由桌面端创建、确认和驳回；MCP 仅提供主动领取与提交，不能自我确认或改写他人报告。
- 信息请求的关闭只能由桌面端人工执行，必须填写非空原因；MCP 不提供关闭工具，也不能用关闭绕过需要确认的业务决策。
- 调查报告只有 Commit、摘要、路径、行号、符号、风险、建议修改范围和限制等结构字段，不提供源码正文入口；疑似 Secret 的内容会被拒绝保存。ColdStart 不创建调查分支、Commit 或 PR，也不触发验证、Review、推送或任务状态变更。

### 执行安全

- 本地验证只接受绝对路径 `.exe` 与参数数组，不经过 Shell。
- Review 模型输出不能执行命令或修改文件。
- MCP 不提供仓库 CRUD、凭据回读、Review 配置管理、项目记忆写入或人工豁免。
- 项目简报、项目记忆和任务接力都是只读安全投影，不返回租约令牌、Secret、凭据引用、远程 URL 或本地绝对路径。
- 调查领取令牌只返回一次，SQLite 只保存哈希；调查提交必须匹配当前租约，租约过期或报告冲突会失败关闭。调查证据在人工确认前不会进入蓝图模型上下文。
- 需求图片不自动进入任务上下文、项目简报或蓝图提示词。MCP 只向持有有效租约的当前任务暴露其蓝图实体来源锚点关联的图片，且必须显式调用 `read_task_image` 才返回二进制。
- 项目数据库与规范文件都在本地。

MVP 不是面向恶意本地进程的完整沙箱。运行 Agent 的账户权限仍应与生产密钥隔离；高风险生产凭据不应交给日常 Coding Agent 使用。

## 数据、备份与迁移

每个 ColdStart 项目包含：

```text
your-project/
  coldstart.project.json
  .coldstart/
    state.db
    artifacts/
    snapshots/
    review-standards/
```

- `coldstart.project.json`：项目清单，不保存机器绝对路径。
- `.coldstart/state.db`：SQLite 状态、蓝图、计划、任务、报告、验证、Review、草稿 PR、验收快照、历史代码调查与能力 API 白名单记录。
- `.coldstart/artifacts/`：导入的需求原文、提取文本、章节锚点、来源图片、`images.json` 元数据与完整性哈希。
- `.coldstart/review-standards/`：规范原文件、提取文本和修订目录。

备份时应整体复制项目主目录。业务 Git 仓库是独立目录，需要按原有 Git 策略单独备份。

## 已知限制

- 只支持 Windows 单机桌面。
- 同一窗口同一时间只显示一个活动项目；支持最近项目登记与快速切换，但不提供多项目并行写入或跨项目聚合看板。
- 只覆盖 Web/管理后台 + App + 小程序 + 服务端，不提供通用平台模板。
- 扫描型 PDF 不执行 OCR。
- 需求图片只提取 Markdown 文档目录内的本地相对路径图片和 DOCX 内嵌图片；不下载远程图片，不提取 PDF 图片，不做 OCR、视觉理解或自动截图比对。
- 历史项目只读代码调查用于回答本次迭代的具体问题，不会自动还原完整架构；完整基线仍需要文档、入口清单和人工收敛。
- 历史代码调查不提供操作系统级只读沙箱。ColdStart 不启动 Agent，也不会为调查创建任务分支；Codex 应自行遵守只读排查约束，不得提交、推送或修改业务文件。
- 没有团队权限、多人并发审批或云端同步。
- 不自动部署、打包、提审、发布小程序、合并分支或创建 Release Tag。
- GitHub 草稿 PR 与验收信息同步只支持 GitHub.com 和桌面端人工触发；每次最多读取 100 个 Checks、100 条 Review、100 条行内评论和 10 条 Deployment，不自动翻页。
- 在 GitHub 标记 PR 冲突时，使用本地 `merge-tree` 只读定位冲突路径；本地缺少提交时降级为不可用。部署状态只读取当前 PR 头提交，不读取 Actions 日志或部署 payload，也不会根据失败的 Checks 或部署自动修复、重跑、部署或合并。
- GitHub Issue 导入只支持 GitHub.com 上与已映射仓库关联的开放、已关闭或全部 Issue，每种状态最多读取 50 条；关键词、标签和导入状态筛选只作用于当前快照，不调用 Search API、不自动翻页、不导入评论、附件、Projects 或 Milestones，也不会自动刷新或生成蓝图。
- GitHub Project 概览只支持 GitHub.com，手动读取最多 5 个项目、每个项目 30 个条目和 10 个单选项字段；不支持视图、评论、过滤自动化，不自动翻页、不后台轮询，也不会把条目导入 ColdStart。
- 多仓库交付总览是本地只读快照，不联网、不后台轮询、不跨项目聚合；GitHub PR、Checks、Review 与部署变化需要先在“AI 评审”页手动刷新，再刷新总览。
- 能力 API 代理只支持 HTTPS 与静态鉴权 Header，不支持 OSS 等 SDK 签名、二进制流、Cookie 会话或自动重定向；DNS 解析到内网地址的理论 SSRF 风险仍依赖白名单治理。
- Review 依赖外部 OpenAI 兼容模型；模型服务故障会失败关闭而不是跳过。
- AI Review 不能证明代码没有漏洞，也不能替代人工安全审计。
- MCP 项目简报只基于当前已发布计划；更早计划的历史需要通过项目记忆分页查看。
- 编码任务 `done` 后若仍未确认合入，依赖它的后继任务不会解锁；需要刷新已合入的 GitHub PR，或在计划看板人工确认合入。
- 安装包未签名。

## 常见故障

### 打开项目提示清单或数据库缺失

- `manifest_missing`：目标目录下没有 `coldstart.project.json`。如果只包含一个直接子项目，客户端会自动定位；如果包含多个候选，请选择具体项目目录（例如 `...\control`），不要选择业务仓库根目录。
- `database_missing`：`.coldstart/state.db` 不存在或被移动，恢复完整项目备份。
- `project_metadata_mismatch`：项目清单与 SQLite 元数据不属于同一项目，不要强行混用。

### 无法生成蓝图

- `no_requirement_sources`：先导入至少一份需求文档。
- `document_empty`：PDF 可能是扫描件，当前不执行 OCR。
- 模型调用失败：检查 Base URL、模型名、网络和隐私同意；需求模型 Key 不会持久化，重启后需要重新输入。

### 无法发布蓝图

- `unresolved_blocking_info_requests`：项目内仍有开放的阻塞问题。逐条回答，或确认问题已过时后填写原因关闭。
- `info_request_dismiss_reason_empty`：关闭问题必须填写非空原因。
- `info_request_not_open`：该问题已经回答或关闭，不能重复处理。

### 需求图片无法查看

- `requirement_image_not_found`：图片不存在，或不属于当前任务蓝图实体所引用的章节；Agent 也不能跨任务读取其他章节或文档的图片。
- `requirement_image_invalid`：制品文件缺失、大小或 SHA-256 与导入记录不一致，或解析后的路径越出该来源的图片目录。需要恢复原始图片或重新导入需求文档。
- 来源页只展示元数据且按钮可用时，原图仍可能因本地制品被移动而读取失败；远程 URL、`data:`、绝对路径和文档目录外的 Markdown 图片不会出现在清单中。

### 任务无法被 Codex 领取

- 没有已发布计划，或依赖任务尚未完成。
- 依赖的编码任务已经 `done` 但尚未确认合入，返回“等待合入基线”；需要刷新已合入 PR 或在计划看板人工确认。
- 仓库没有配置有效的 `.exe` 验证命令，返回 `verification_not_configured`。
- 工作区不干净、任务分支已存在、远程不可达或 Git 认证失败。
- Review 已启用但没有有效隐私同意，返回 `review_consent_required`。
- Review 凭据缺失，返回 `review_credential_missing`。
- 规范正文哈希不一致，返回 `review_standard_invalid`。

### 历史代码调查无法领取或提交

- `repository_not_found`：调查仓库未映射或逻辑仓库键不匹配，先到“仓库与能力”配置。
- `code_investigation_claim_invalid`：领取令牌错误、已失效或不属于该调查；重新领取，不要复用旧令牌。
- `code_investigation_lease_expired`：超过 60 分钟租约；调查会回到队列，重新领取后再提交。
- `code_investigation_report_conflict`：同一调查已提交不同报告；旧报告不可覆盖，需要人工驳回后重新发起调查。
- `code_investigation_contains_secret`：证据文本疑似包含 Secret；移除后再提交。
- 调查状态不是 `submitted` 时不能确认或驳回；先检查 Codex 是否已完成提交。

### 提交后一直无法完成

按以下顺序检查：

1. 计划看板的仓库验证状态；失败会保留租约。
2. AI 评审历史中的状态、模型错误与 findings。
3. 任务是否仍在当前活动计划中，契约是否已过期。
4. 租约是否过期；过期后需要重新领取。
5. 修复后使用新的 `idempotencyKey` 再提交，不要复用已处理报告。

### Review 显示 failed

`failed` 不是可豁免的 finding，而是评审基础设施或输出失败。检查模型地址、凭据、网络、响应格式与输入大小。任务会保持 `in_progress` 且保留租约，不推送。

### 任务已完成但推送失败

这是预期风险记录，不是任务回滚。到“仓库与能力”的 Git 同步记录查看错误摘要，修复远程认证后按正常 Git 流程处理。

### 无法创建草稿 PR

按以下顺序检查：

1. 报告是否已接受、任务是否为 `done`、Review 是否为允许完成的状态。
2. 报告分支和 Commit 是否有成功推送记录；仅本地提交不能创建 PR。
3. 仓库远程是否为 GitHub.com，且仓库是否绑定 GitHub OAuth Token 或 PAT。
4. 同一报告是否已有 PR；已有记录时按钮会替换为链接和刷新操作。

常见错误码：`pull_request_ineligible`、`pull_request_report_not_found`、`pull_request_not_found`、`github_remote_unsupported`、`github_token_invalid` 和 `github_request_failed`。

### 无法刷新 PR 验收信息

刷新先使用 PR 状态、Checks、Review 和行内评论四个必需只读请求。出现以下情况时，本次同步不会覆盖当前快照：

- GitHub 网络、认证或任一接口失败，通常返回 `github_request_failed` 或 `github_token_invalid`。
- 响应缺少必要字段、head SHA 非法、链接不是 HTTPS 或返回超过上限，通常返回 `github_response_invalid` 或 `pull_request_invalid`。
- 本地 PR 或仓库映射已不存在，返回 `pull_request_not_found` 或仓库相关错误。
- 仓库不再使用凭据认证，或 Windows Credential Manager 中缺少 Token，返回 `pull_request_ineligible` 或相关凭据错误。

冲突文件定位是附带诊断，不参与上述 GitHub 请求的原子边界。本地缺少 base/head 提交或 `git merge-tree` 不可用时，GitHub 状态、Checks、Review 和评论仍会保存，冲突区域显示“无法在本地定位冲突文件”。本地未复现冲突也不会覆盖 GitHub 的冲突状态。

部署状态也是附加只读信息。部署列表请求失败时，PR 验收快照仍会保存，部署区域显示“部署状态不可用”；单个部署状态请求失败时，该环境显示“状态未知”。这些降级不会触发部署、审批、取消或重试。

修复后重新点击“刷新状态”。旧快照会保留到下一次完整同步成功。

### 无法读取 GitHub Project

按以下顺序检查：

1. 仓库是否已映射，远程是否为 GitHub.com，并使用 OAuth Token 或 PAT 凭据认证。
2. OAuth 授权是否包含 `read:project`；使用 PAT 时，Token 是否具备 Projects 只读权限。
3. 项目是否已关联到该仓库；组织级 Project 需要通过仓库可见的 Projects v2 连接访问。
4. 是否把“无可见项目”误认为加载失败。空结果会显示独立空态，不会伪造项目。

常见错误码：`github_project_ineligible` 表示仓库或凭据不符合条件，`github_response_invalid` 表示 GraphQL 返回错误、权限不足或响应结构非法，`github_request_failed` 表示网络或 GitHub 请求失败。失败时当前 Project 视图会清空，不会保留上次成功数据。

### 安装后找不到 MCP

安装版计划看板会优先定位桌面 EXE 同级的 `coldstart-mcp.exe`，也会识别 Tauri 的 `resources\coldstart-mcp.exe`。复制 MCP 配置前确认文件存在；源码运行时可先执行 `cargo build -p coldstart-mcp --release`。

## 开发与验证

### Rust

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

### Web 前端

```powershell
pnpm --dir web typecheck
pnpm --dir web test
pnpm --dir web build
```

### 代码注释

```powershell
pnpm comments:check
```

所有第一方生产源文件都必须有模块说明，解释职责、边界和关键不变量；包含多个分支、事务或安全校验的公开函数与组件必须有行为注释。简单 getter、纯转发和 DTO 由命名与类型表达，不用行尾注释凑数量。

注释门禁只统计模块与复杂公开行为，门槛均为 70%；统计脚本同时输出全部公开行为和公开类型的覆盖率作为辅助指标。物理行注释比例不作为门禁，避免为了数字堆砌低信息量注释。

### 桌面与安装包

```powershell
pnpm --dir apps/desktop build
pnpm --dir apps/desktop build:installer
```

构建脚本会先把 `coldstart-mcp.exe` 放入安装资源，再执行 Tauri NSIS 构建并输出 SHA-256。若构建环境无法稳定下载 NSIS，可预先准备 `nsis-3.13` 与 `nsis_tauri_utils.dll`，放到 `target\.tauri\NSIS`，并使用本地工具目录。

## 文档索引

- [产品与系统架构规格](docs/superpowers/specs/2026-10-30-coldstart-design.md)
- [AI Review 与规范库设计](docs/superpowers/specs/2026-11-02-coldstart-ai-review-design.md)
- [项目记忆与变更审计设计](docs/superpowers/specs/2026-11-12-coldstart-project-memory-design.md)
- [MCP 项目记忆与接力设计](docs/superpowers/specs/2026-11-13-coldstart-mcp-memory-handoff-design.md)
- [GitHub 草稿 PR 交付设计](docs/superpowers/specs/2026-11-13-coldstart-draft-pull-request-design.md)
- [GitHub PR 验收信息只读同步设计](docs/superpowers/specs/2026-11-13-coldstart-pr-insights-design.md)
- [GitHub PR 合并就绪只读展示设计](docs/superpowers/specs/2026-11-13-coldstart-pr-merge-readiness-design.md)
- [GitHub PR 冲突文件本地定位设计](docs/superpowers/specs/2026-11-13-coldstart-pr-conflict-locator-design.md)
- [GitHub PR 部署状态只读展示设计](docs/superpowers/specs/2026-11-13-coldstart-pr-deployment-status-design.md)
- [GitHub Issue 需求收集与导入设计](docs/superpowers/specs/2026-11-13-coldstart-github-issue-intake-design.md)
- [GitHub Issue 收件箱筛选增强设计](docs/superpowers/specs/2026-11-13-coldstart-issue-inbox-filters-design.md)
- [GitHub Project 只读概览设计](docs/superpowers/specs/2026-11-13-coldstart-github-projects-design.md)
- [多仓库交付总览设计](docs/superpowers/specs/2026-11-13-coldstart-delivery-overview-design.md)
- [MCP 受限凭据代理设计](docs/superpowers/specs/2026-11-13-coldstart-mcp-credential-proxy-design.md)

阶段实施计划与内部验收记录仅保留在本地，不随开源仓库发布。

## 技术形态

Tauri 2 + Rust + React 19 + TypeScript + SQLite。Rust 负责领域模型、状态机、计划引擎、SQLite、Git、历史代码调查、GitHub Issue 导入、Project 只读 GraphQL、草稿 PR、验收同步、合并就绪、冲突定位与部署状态、多仓库交付聚合、能力 API 受限代理、凭据、验证、AI Review、MCP stdio 与 CLI；React/TypeScript 只负责桌面界面。桌面 UI、MCP 入口和 CLI 共享同一套 Rust 核心 crate，不使用后台守护进程。

## 开源与反馈

ColdStart 以 [MIT License](LICENSE) 开源。欢迎各位开发者提交 Issue、发起 Pull Request、补充文档，或直接指出设计缺陷与安全风险。

- **发现问题**：请附上版本、复现步骤和脱敏日志，通过 [GitHub Issues](https://github.com/huangqcEvent/coldstart/issues) 反馈。
- **改进代码**：请先阅读 [CONTRIBUTING.md](CONTRIBUTING.md)，保持改动聚焦并附上验证结果。
- **安全漏洞**：不要公开披露，请按 [SECURITY.md](SECURITY.md) 私下报告。
- **功能讨论**：欢迎说明真实使用场景、涉及平台和现有替代方案，再讨论是否进入路线图。

如果 ColdStart 对你有帮助，欢迎 Star、分享给正在做 Web、管理后台、App 或小程序项目的开发者，也欢迎直接批评：这个工具最应该被指正的地方，往往比赞美更有价值。
