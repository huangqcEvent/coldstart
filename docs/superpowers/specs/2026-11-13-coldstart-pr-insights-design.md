# ColdStart GitHub PR 验收信息只读同步设计

- 日期：2026-11-13
- 状态：已完成（阶段 13，2026-11-13）
- 关联：`docs/superpowers/plans/2026-11-13-coldstart-pr-insights.md`

## 1. 目标

阶段 13 在阶段 12 的草稿 PR 交付之后，为已存在的 GitHub PR 增加人工触发的只读验收同步：

- PR 当前状态与草稿标记。
- 提交上的 GitHub Checks。
- Pull Request Review 结论。
- Pull Request 行内 Review 评论。

同步结果进入本地 SQLite，供桌面端在“AI 评审”历史中审阅。ColdStart 不写回 GitHub，不自动修复失败检查，不回复评论，不合并 PR，也不创建 Release Tag。

## 2. 设计原则

1. 仍然只有桌面端人工点击“刷新状态”才访问 GitHub。
2. 所有请求都是 `GET`；适配器不提供评论、Checks 写接口或合并接口。
3. Token 只在 Rust 进程内临时读取，只进入 Authorization 头。
4. 外部文本先脱敏、再限长；详情链接只接受 HTTPS。
5. 一次刷新只有在全部读取成功后才写入状态与快照，避免把半套数据当成完整验收结果。
6. Checks 与评论内容变化时才写入项目审计事件；单纯刷新时间不制造事件噪音。
7. MCP 工具数量保持 11 个，不增加 PR 相关读取或写入工具。

## 3. GitHub 只读接口

同步使用四个端点：

1. `GET /repos/{owner}/{repository}/pulls/{number}`：PR 状态、草稿标记和当前 head SHA。
2. `GET /repos/{owner}/{repository}/commits/{head_sha}/check-runs?filter=all&per_page=100`：Checks。
3. `GET /repos/{owner}/{repository}/pulls/{number}/reviews?per_page=100`：Review 结论。
4. `GET /repos/{owner}/{repository}/pulls/{number}/comments?per_page=100`：行内 Review 评论。

只读取第一页，最多各 100 项。超过部分不自动翻页，界面明确显示“最多展示 100 项”的限制。

## 4. 数据模型与存储

新增领域对象：

- `PullRequestCheck`：名称、GitHub 状态、结论、详情链接、提供方名称。
- `PullRequestReview`：评审人、结论、正文、GitHub 链接。
- `PullRequestReviewComment`：评审人、文件路径、行号、正文、GitHub 链接。
- `PullRequestInsights`：关联 PR、三类列表和同步时间。

SQLite 升级到架构 9，新增 `pull_request_insights` 表。每个 PR 最多一行，保存：

- `pull_request_id`、`project_id`。
- 脱敏后的快照 JSON。
- 内容哈希，用于判断是否变化。
- `synced_at`。

删除 PR、项目或关联仓库时，快照随外键级联删除。

新增事件 `PullRequestInsightsSynced`，只包含 PR 编号、Checks 汇总、评审数和评论数，不包含评论正文、URL 或 Token。事件进入项目记忆的“验证与评审”阶段。

## 5. 状态与展示

Checks 汇总规则：

- 无 Checks：`none`。
- 任意失败、超时、取消或需要操作：`failure`。
- 仍有排队或运行中的检查：`pending`。
- 有检查且全部完成、无失败：`success`。

桌面端显示：

- Checks 汇总徽标与详细检查列表。
- Review 结论摘要与正文。
- 行内评论的文件路径、行号、作者和正文。
- 最近同步时间。
- 旧 PR 尚未同步时显示明确提示，用户需要手动点击刷新。

所有外部链接使用 `target="_blank"` 与 `rel="noreferrer"`，正文由 React 以纯文本渲染，不注入 HTML。

## 6. 错误处理

- 任一 GitHub 请求失败时，本次刷新返回稳定错误码，不写入 PR 状态或快照。
- 响应缺少必要字段、head SHA 非法、可选链接不是 HTTPS、列表超过上限时，拒绝整次同步。
- GitHub 返回的未知 Checks 状态或 Review 状态映射为 `unknown`，不执行任何推断操作。
- 失败检查只展示，不触发自动修复、重试、合并或 Agent 任务。

## 7. 明确不做

- 不回复、解决或删除 PR 评论。
- 不重跑 Checks，不创建 Check Run，不写 Commit Status。
- 不合并、关闭、转正式 PR、更新分支或创建 Tag。
- 不读取或同步冲突状态、部署状态、Issue、Projects。
- 不提供 MCP PR 工具，不允许 Agent 触发同步。

## 8. 验收

- 领域测试覆盖快照上限、非法链接和 Checks 汇总。
- 适配器测试覆盖四个 GET 端点、响应校验和 Token 脱敏。
- 存储测试覆盖架构 9、快照覆盖、级联删除和变化事件。
- 应用测试覆盖完整刷新、失败时不落半套数据、重复同步幂等。
- 前端测试覆盖 Checks 汇总、Review 评论、未同步提示和刷新后的更新。
- 全量 Rust 与前端门禁通过，安装包更新到 D 盘，MCP 仍为 11 个工具。

