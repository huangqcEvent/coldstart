# ColdStart GitHub Project 只读概览设计

- 日期：2026-11-13
- 状态：已完成（阶段 19，2026-11-13）
- 关联：`docs/superpowers/plans/2026-11-13-coldstart-github-projects.md`

## 1. 目标

阶段 19 在“需求导入”页增加 GitHub Projects v2 只读概览，让人工在 ColdStart 内查看已映射仓库关联的项目、条目的有限状态与单选字段，作为外部协作进度的参考。

本阶段不把 Project 当作需求来源、计划来源或任务状态源，也不尝试替换 GitHub 的协作界面。具体能力包括：

- 从已映射的 GitHub.com 仓库读取最多 5 个 Projects v2。
- 每个项目最多展示 30 个条目和 10 个单选项字段。
- 展示项目开放/关闭、标题、更新时间、条目总数与 GitHub 链接。
- 展示 Issue、Pull Request、Draft Issue，以及当前 Token 无法读取内容的受限条目。
- 请求失败时清空当前视图并显示稳定错误，不展示可能过期的部分数据。

## 2. 设计原则

1. 只读：只发送固定 GraphQL 查询，不提供 create、update、delete、archive 或字段写入。
2. 人工触发：只有桌面端点击“读取 Project”才访问 GitHub，不自动刷新、不后台轮询。
3. 不落库：Project 概览不写 SQLite、不写项目事件、不写项目文件，也不升级数据库架构。
4. 不扩大 Agent 权限：不增加 MCP 工具，MCP 总数仍为 11；Agent 不能读取或修改 Project。
5. 有界读取：项目、条目、字段、文本和链接都有硬上限，界面明确标注截断。
6. 外部资料隔离：Project 条目只用于人工参考，不导入需求、不生成蓝图、不改变计划或任务状态。

## 3. 后端数据映射

### 3.1 GraphQL 请求

- 端点：`POST https://api.github.com/graphql`。
- 查询仓库字段：`repository(owner, name) { projectsV2(first: 5) }`。
- 每个项目读取 `id`、`number`、`title`、`url`、`closed`、`updatedAt` 和条目连接。
- 条目连接读取 `totalCount`、`pageInfo.hasNextPage`，以及 `items(first: 30)`。
- 每个条目读取 `id`、`updatedAt`、`fieldValues(first: 10)` 和 `content`。
- `content` 支持 `Issue`、`PullRequest` 与 `DraftIssue`；无法读取或未知类型映射为 `unavailable`。
- 字段值只读取 `ProjectV2ItemFieldSingleSelectValue` 的字段名和值，不读取富文本、附件或评论。
- 查询变量只包含仓库 owner 与名称；Token 只进入 `Authorization: Bearer ...` 请求头，不进入查询正文。

### 3.2 数量与文本限制

- 最多 5 个项目、每个项目最多 30 个条目、每个条目最多 10 个字段。
- 项目标题最多 200 字符，条目标题最多 256 字符，字段名与字段值各最多 80 字符。
- GraphQL 节点 ID 最多 200 字符，拒绝控制字符和空白标题。
- 项目与条目链接必须是 `https://github.com/` 开头的 HTTPS 地址。
- 项目与条目更新时间必须是 RFC 3339；非法时间、非法状态或超限响应按 `github_response_invalid` 处理。

### 3.3 截断与失败语义

- 项目连接、条目连接或字段数量达到上限时设置对应 `truncated` 标记，界面显示项目列表或条目“当前仅显示前 N 个”。
- 条目总数沿用 GitHub 的 `totalCount`，不把已展示数量当成总数。
- GraphQL 返回 `errors`、缺少 `data`、仓库不可见或响应结构非法时，本次读取失败。
- 失败不返回上一次成功快照，也不把部分项目写入界面或本地存储。

## 4. 应用服务与桌面命令

### 4.1 仓库与凭据校验

- `GitHubProjectService::list_projects` 接收项目主目录和逻辑仓库键。
- 复用既有 `resolve_github_repository_access`：只接受已映射、远程为 GitHub.com、使用凭据认证的仓库。
- 凭据只从 Windows Credential Manager 读取，支持仓库绑定的 GitHub OAuth Token 与 Personal Access Token。
- 仓库未映射、远程不受支持、凭据缺失或类型不符时返回 `github_project_ineligible`，不发起网络请求。
- 成功后返回仓库键、仓库名、`owner/repository` 标签、项目和整体截断标记。

### 4.2 Tauri 命令

- 新增 `list_github_projects(path, repositoryKey)` 命令，经 `spawn_blocking` 调用 Rust 服务。
- 响应使用 camelCase 视图对象，不包含 Token、凭据引用、查询正文或本地绝对路径。
- 前端只通过 Tauri IPC 调用该命令；不与 GitHub 直接通信。

## 5. 桌面交互

- “需求导入”页在 Issue 收件区之后增加“GitHub Project 只读概览”面板。
- 仓库下拉框复用当前项目已映射的仓库；切换仓库会清空旧概览和错误。
- 只有存在已映射仓库且当前没有其他忙碌操作时才能点击“读取 Project”。
- 加载中显示“正在读取…”，成功后展示项目卡片和条目；失败时显示稳定错误码与错误摘要。
- 项目卡片展示 `Project #编号`、标题、开放/关闭、更新时间、条目总数、截断提示和 GitHub 链接。
- 条目展示类型（Issue、PR、草稿、受限）、编号、标题、状态、更新时间和单选字段。
- 没有可见项目、空项目或无法读取内容时分别提供明确空态，不把空结果描述成同步成功。
- 面板固定标注“只读”，并说明不会创建、修改项目、字段或条目，也不会导入需求或蓝图。

## 6. 权限、安全与隐私

### 6.1 GitHub 权限

- OAuth Device Flow 的 scope 从 `repo`、`read:org` 扩展为 `repo`、`read:org`、`read:project`。
- PAT 必须具备 Projects 只读权限。权限不足时 GitHub GraphQL 错误作为稳定读取失败展示。
- 只支持 GitHub.com，不支持 GitHub Enterprise Server。

### 6.2 数据边界

- Project 概览只存在于当前桌面界面的内存中，不进入 SQLite、事件、日志、MCP 响应、任务上下文或项目文件。
- 不保存 GraphQL 原始响应、查询错误全文、评论、Issue 正文、附件或字段以外的项目数据。
- Token 只进入 Rust 请求头；错误正文会替换 Token，避免凭据出现在 UI 或日志。
- 外部标题、字段和错误摘要按纯文本渲染；链接必须为 GitHub HTTPS 地址并使用安全的新窗口属性。
- 不增加 MCP 工具，Agent 无法通过 ColdStart 读取 Project，也无法获得任何 Project 写权限。

## 7. 明确不做

- 不创建、编辑、关闭、归档或删除 Project。
- 不创建、移动、删除或修改 Project 条目和字段。
- 不把 Project 条目导入需求、蓝图、计划、任务或项目事实。
- 不读取 Project 视图、过滤器、工作流自动化、评论或附件。
- 不同步 ColdStart 计划到 GitHub，也不以 GitHub Project 状态覆盖本地任务状态。
- 不自动翻页、不后台轮询、不订阅 webhook。
- 不调用 GraphQL mutation，不增加合并、部署、打包或提审能力。

## 8. 验收

- 适配器测试覆盖固定 GraphQL 查询、`POST /graphql`、Bearer 头、Token 不进入正文、5/30/10 上限、截断、Issue/PR/草稿解析和非法响应。
- 适配器测试拒绝不安全链接、非法时间、控制字符和超限文本。
- 应用测试覆盖仓库与凭据校验、Project 视图映射、Token 不写入项目目录、读取不新增项目事件。
- 前端测试覆盖仓库选择、加载成功、项目与字段展示、截断提示、错误态和只读说明。
- OAuth 测试或代码检查确认 Device Flow 请求 `read:project`，PAT 权限提示准确。
- 全量 Rust、前端门禁、生产构建、NSIS 打包、D 盘覆盖安装和 11 个 MCP 工具验收通过。
- 旧项目 SQLite 仍为架构 9，原有项目、工作项和 PR 数据保留。
