# ColdStart GitHub Issue 需求收集与导入设计

- 日期：2026-11-13
- 状态：已完成（阶段 17，2026-11-13）
- 关联：阶段实施计划仅保留在本地，不随开源仓库发布。

## 1. 目标

阶段 17 为已有项目和迭代需求增加一个受控入口：从已映射的 GitHub 仓库读取开放 Issue，由人在桌面端选择并导入为本地需求来源，再沿用现有蓝图与计划流程继续工作。

目标是缩短“新需求散落在 GitHub”到“进入 ColdStart 可追踪需求”的手工复制步骤，同时保持以下边界：

- 只读取 Issue 的标题、正文、标签、状态、更新时间、评论数和链接。
- 只导入用户明确选择的单个 Issue。
- 导入后仍由现有文档锚点、蓝图生成、人工校正和计划发布流程处理。
- 不自动生成蓝图、不自动领取任务、不自动修改 GitHub Issue。

## 2. 设计原则

1. Issue 是可追溯的外部需求来源，不是已确认项目需求。导入后必须经过现有蓝图流程。
2. GitHub Token 只进入 Rust HTTP Authorization 头，不进入前端、SQLite、事件、日志或错误。
3. Issue 正文属于不受信任的外部文本，只能作为需求资料展示和模型输入，不能触发命令、工具或自动操作。
4. 列表读取和导入都只使用 GitHub REST 只读 GET。
5. 不新增 MCP 工具；Agent 不能拉取、选择或导入 Issue。
6. 不升级 SQLite 架构，复用现有 `artifacts`、锚点和来源文档存储。

## 3. GitHub 数据映射

### 3.1 开放 Issue 列表

使用：

```text
GET /repos/{owner}/{repository}/issues?state=open&sort=updated&direction=desc&per_page=100
```

- 该端点也会返回 Pull Request；任何包含 `pull_request` 字段的条目都必须在解析时过滤。
- 最多向桌面端返回 50 条 Issue，按 GitHub 返回顺序保留。
- 原始响应达到 100 条或过滤后超过 50 条时，列表标记为已截断。
- 标题最多 256 字符，正文最多 30,000 字符，标签最多保留 20 个且每个最多 50 字符。
- 链接必须是有界 HTTPS URL；更新时间必须是有效 RFC 3339。

### 3.2 单个 Issue

导入前使用：

```text
GET /repos/{owner}/{repository}/issues/{issue_number}
```

- 导入使用单条 Issue 的最新快照，不复用可能过期的列表正文。
- 若目标条目实际是 Pull Request，则拒绝导入。
- 只接受 `open` 和 `closed` 两种状态；其他状态视为非法响应。

## 4. 本地需求来源

导入时把 Issue 转换为 Markdown：

```markdown
# #123 Issue 标题

- 来源：owner/repository
- 状态：open
- 链接：https://github.com/owner/repository/issues/123
- 更新时间：RFC3339
- 标签：标签 A、标签 B

> 本来源只包含 Issue 标题与正文，不包含评论。完整讨论请回到 GitHub 查看。

## 正文

Issue 正文或“无正文”
```

- 原始 Markdown 写入新的 artifact 目录，文件名固定为 `github-issue-<number>.md`。
- `importSource` 固定为 `github://<owner>/<repository>/issues/<number>`，用于显示已导入状态和避免重复导入。
- 复用现有 Markdown 锚点提取和来源哈希。
- 同一个仓库 Issue 已导入时返回稳定错误 `github_issue_already_imported`，不静默创建重复来源。
- 导入成功后返回完整项目工作区，使需求页立即显示新来源。

## 5. 桌面交互

- 需求导入页增加“从 GitHub Issue 导入”区域。
- 用户先选择已映射的逻辑仓库，再点击“读取开放 Issue”。
- 列表展示 Issue 编号、标题、标签、更新时间、评论数和限长正文预览。
- 每个 Issue 提供“导入为需求来源”按钮；已导入条目禁用并显示状态。
- 未配置仓库、仓库非 GitHub、凭据缺失、网络失败或响应非法时显示稳定错误，不覆盖现有来源。
- 不提供创建、编辑、关闭、评论、分配或合并 Issue 的操作。

## 6. 安全边界

- Token 从仓库绑定的 GitHub OAuth Token 或 Personal Access Token 读取，沿用现有 Credential Manager。
- 不记录原始 GitHub 响应、Issue 原始 JSON 或 Token。
- 正文控制字符和超长文本在适配器边界拒绝或限长；正文只以 React 文本节点渲染，不注入 HTML。
- 不自动执行 Issue 中的命令、链接或代码块。
- 不增加 MCP 工具，MCP 总数仍为 11。

## 7. 明确不做

- 不支持 GitHub Enterprise Server、Issue 评论、附件、Projects、Milestones 或搜索 API。
- 不自动生成蓝图、计划或代码。
- 不自动刷新 Issue 列表，不建立后台轮询。
- 不创建、编辑、关闭、评论或分配 Issue。
- 不导入 Issue 评论；需要讨论上下文时由用户补充文档。

## 8. 验收

- 适配器测试覆盖固定查询参数、PR 条目过滤、50 条上限、正文与标签限长、非法 URL、非法时间和单条 Issue 解析。
- 应用测试覆盖仓库/凭据校验、Issue 转 Markdown、artifact 与来源哈希落库、`importSource`、重复导入拒绝和 Token 不落库。
- 前端测试覆盖仓库选择、列表加载、已导入状态、稳定错误和只读说明。
- 全量 Rust、前端门禁、生产构建、安装包、D 盘覆盖安装与 11 个 MCP 工具验收通过。
