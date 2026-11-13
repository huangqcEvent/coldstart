# ColdStart 阶段 20：多仓库交付总览设计

实现状态：已完成（2026-11-13）

## 1. 目标

在“项目总览”中增加只读的多仓库交付视图，让用户不用在计划看板、仓库与能力、AI 评审之间来回切换，就能回答五个问题：

1. 每个仓库还有多少可领取、执行中、阻塞、失败和待验收任务？
2. 每个仓库是否配置了本地验证命令？
3. 每个仓库最近一次任务分支推送是否失败？
4. 每个仓库最近相关的 PR、Checks、合并冲突和 Review 状态是什么？
5. 当前最应该处理哪一件事？

该视图只聚合 ColdStart 已经保存的本地事实，不产生新的业务事实，不触发网络请求，不改变任务状态。

## 2. 设计原则

- Rust 是交付事实的唯一聚合点，前端不自行拼接多个工作区接口。
- 只读优先：打开总览、刷新总览都不写 SQLite、不写项目目录、不产生项目事件。
- 主动领取保持不变：视图只指出“有哪些任务可领取”，不替 Agent 领取，也不改变租约。
- 人工把关保持不变：PR、Checks、冲突和 Review 只用于人工判断，不自动合并、重跑或修复。
- 信息不足时明确显示“未配置”“尚未同步”或“暂无记录”，不把缺失状态推断为成功。
- 单机运行，不引入后台轮询、守护进程、云端服务或新的外部依赖。

## 3. 范围

### 3.1 包含

- 一个新的应用服务 `DeliveryOverviewService`。
- 一个新的 Tauri 命令 `load_delivery_overview`。
- 一个只读的 `DeliveryOverviewView` 返回结构。
- “项目总览”页的多仓库卡片、总体摘要、空态、加载态和错误态。
- 每个仓库的任务计数、验证命令计数、最近 Git 同步、最近 PR/Checks、Review 状态与下一步建议。
- Rust 单元/应用测试与前端组件测试。

### 3.2 不包含

- 不增加任何 GitHub、Git 或其他网络请求。
- 不增加 MCP 工具，MCP 工具总数仍为 11。
- 不升级 SQLite 架构，数据库保持架构 9。
- 不新增数据库表、迁移或持久化快照。
- 不改变计划、任务、租约、报告、Review、PR 或凭据状态。
- 不自动领取、合并、关闭、评论、重跑 Checks、部署、打包、提审或发布。
- 不把 GitHub Project、Issue 或外部协作数据混入交付事实。

## 4. 数据来源

所有数据来自当前项目 SQLite 与现有本地文件定位逻辑：

| 事实 | 来源 |
| --- | --- |
| 当前已发布计划版本 | `latest_plan(project_id, Published)` |
| 工作项与任务契约 | `list_work_items_for_plan`、`task_spec_for_work_item` |
| 已映射仓库 | `list_repositories` |
| 验证命令 | `list_repository_verification_commands` |
| 最近 Git 同步 | `list_git_syncs` |
| PR 记录 | `list_pull_requests` |
| PR 验收快照 | `load_pull_request_insights` |
| Review 状态 | `list_work_reports` 中与 PR 报告关联的 `review_status` |

读取只通过既有查询方法完成。`IntegrationService::load_workspace` 会同步能力状态，因此本视图不调用该服务，避免只读操作产生写入。

## 5. 聚合模型

### 5.1 顶层结构

`DeliveryOverviewView` 包含：

- `projectId`：项目标识。
- `planRevisionNumber`：当前已发布计划版本号；未发布时为 `null`。
- `repositories`：最多 5 个已映射仓库的交付视图，按 `server`、`admin`、`web`、`app`、`mini_program` 的固定顺序排列。
- `unassignedAgentTaskCount`：已发布计划中未关联“已配置仓库”的编码任务数量。

### 5.2 仓库视图

每个 `RepositoryDeliveryView` 包含：

- 仓库键、名称、远程地址、默认分支。
- `state`：`attention`、`active`、`ready`、`complete`、`idle`。
- `tasks`：任务状态计数。
- `verificationCommandCount`：本地验证命令数量。
- `latestGitSync`：最近一次任务分支推送；没有记录时为 `null`。
- `pullRequest`：最近相关 PR；没有记录时为 `null`。
- `nextAction`：面向人工的中文下一步建议。

### 5.3 任务计数

计数只基于当前已发布计划：

- `total`：除 `cancelled` 外的任务总数。
- `done`：`done`。
- `ready`：`ready`。
- `active`：`claimed` 与 `in_progress`。
- `blocked`：`blocked`。
- `failed`：`failed`。
- `submitted`：`submitted`。
- `reopened`：`reopened`。
- `cancelled`：`cancelled`，仅用于说明，不进入 `total`。

`task.repositoryKey` 与已配置仓库匹配时计入对应仓库。编码任务没有仓库键，或引用了未配置仓库时计入 `unassignedAgentTaskCount`，避免静默遗漏。

### 5.4 Git 同步

- 每个仓库只展示 `created_at` 最新的一条 `GitSyncRecord`。
- 展示状态、分支、提交 SHA、时间和已脱敏错误摘要。
- 不重新执行 Git，不判断远程实时状态；视图只说明“ColdStart 最近一次记录了什么”。

### 5.5 PR 与 Review

- 同一仓库优先选择最近创建的开放 PR；没有开放 PR 时选择最近创建的 PR。
- 只读取该 PR 已保存的本地 `PullRequestInsights`，没有快照时 `checks` 与 `mergeability` 为 `null`。
- `checks` 包含总数、失败数、等待数及 `none`/`pending`/`success`/`failure` 汇总。
- `mergeability` 原样使用现有枚举，不新增推断。
- 只展示冲突文件数量，不复制路径列表；完整路径继续在 AI 评审页查看。
- `reviewStatus` 使用该 PR 对应报告的本地 Review 状态。
- `changesRequestedReviewCount` 汇总 PR 快照中 `changes_requested` 的 Review 数。

### 5.6 状态判定

仓库状态的优先级：

1. `attention`：存在阻塞/失败任务、推送失败、开放 PR 冲突、Checks 失败、Review 阻断或待处理修改，或仍有未完成任务但没有验证命令。
2. `active`：存在执行中/待验收任务，或存在开放 PR。
3. `ready`：存在可领取或已重开任务。
4. `complete`：有任务且所有有效任务已完成。
5. `idle`：没有有效任务。

### 5.7 下一步建议优先级

同一仓库按以下顺序给出唯一建议：

1. 阻塞或失败任务。
2. 已重开任务。
3. 最近 Git 推送失败。
4. 开放 PR 冲突。
5. 开放 PR Checks 失败。
6. 开放 PR Review 阻断或要求修改。
7. 开放 PR 状态仍在计算。
8. 开放 PR 尚未同步验收快照。
9. 仍有未完成任务但验证命令未配置。
10. 可领取任务。
11. 执行中任务。
12. 待验收任务。
13. 没有任务。
14. 全部完成。
15. 继续推进当前计划。

## 6. 接口

Rust：

```text
DeliveryOverviewService::load(project_home) -> DeliveryOverviewView
load_delivery_overview(path) -> Result<DeliveryOverviewView, CommandError>
```

前端：

```text
readDeliveryOverview(path) -> DeliveryOverviewView
```

命令只接受项目主目录。所有错误沿用现有 `CommandError` 契约。读取失败时前端清空旧快照并展示稳定错误，不保留过期交付状态。

## 7. 界面

“项目总览”在现有指标卡和项目状态之后增加“多仓库交付总览”：

- 总体摘要：当前计划版本、已配置仓库数、有效任务完成数、需关注仓库数、开放 PR 数。
- 每个仓库一张卡片：进度条、状态徽标、五类任务计数、验证状态、最近推送、最近 PR、下一步。
- 空态：没有已映射仓库时说明原因，并提供“前往仓库与能力”入口。
- 加载态：显示“正在聚合多仓库交付状态…”。
- 错误态：显示错误信息和“重新读取”按钮，不展示旧快照。
- 未关联任务提示：`unassignedAgentTaskCount > 0` 时显示明确警告。

所有远程链接使用现有 HTTPS 地址，在新窗口打开并设置 `noopener noreferrer`。界面不渲染 HTML。

## 8. 安全与边界

- 不读取凭据值，不接触 Windows Credential Manager。
- 不返回远程仓库的认证信息。
- 只展示现有 `GitSyncRecord` 的脱敏错误摘要和现有 PR 快照字段。
- 不增加 MCP 工具，不进入 MCP 响应、任务上下文、项目简报或项目记忆。
- 不新增背景任务、轮询或自动刷新。
- 不采集遥测。

## 9. 测试与验收

- 单元测试覆盖：任务计数、仓库顺序、未关联任务、验证缺失、Git 失败、PR 选择、Checks/冲突/Review 风险、状态与下一步优先级。
- 应用测试覆盖：真实 SQLite 查询接线、只读行为、无新增事件、空项目与仓库为空。
- 前端测试覆盖：加载、成功卡片、空态、错误清空旧快照、仓库入口、风险与下一步展示。
- 全量门禁：`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --locked`、前端 typecheck、前端测试与生产构建。
- 安装验收：生成 NSIS 包，覆盖安装到 `D:\DevTools\ColdStart`，启动成功，安装版 MCP 仍为 11 个工具，旧项目 SQLite 架构仍为 9。

## 10. 完成标准

- 项目总览能在不发起网络请求的情况下展示每个已配置仓库的交付状态。
- 缺失、空值、失败和未同步状态均有真实、明确的展示。
- 现有计划、验证、Git、PR、Review 与 MCP 行为不变。
- 所有门禁和安装验收通过。
