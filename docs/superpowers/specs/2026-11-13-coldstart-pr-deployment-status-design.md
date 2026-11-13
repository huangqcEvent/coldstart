# ColdStart GitHub PR 部署状态只读展示设计

- 日期：2026-11-13
- 状态：已完成（阶段 16，2026-11-13）
- 关联：`docs/superpowers/plans/2026-11-13-coldstart-pr-deployment-status.md`

## 1. 目标

阶段 16 在现有 PR 验收快照中增加 PR 头提交的 GitHub Deployment 记录与最新部署状态，让桌面端人工验收时能看到：

- 哪些环境针对当前 PR 头提交创建过部署。
- 每个部署的最新状态，例如成功、失败、进行中、排队或未知。
- 部署描述与最后一次状态更新时间。

部署信息只用于人工判断，不构成合并许可，也不会触发任何部署写操作。

## 2. 设计原则

1. 只读取 PR 头提交对应的部署，不读取仓库默认分支或任意历史提交的部署。
2. 只使用 GitHub REST 只读 GET，不创建部署、不更新状态、不审批环境、不取消或重试工作流。
3. 部署读取是附加信息：列表读取失败时只将部署区域标记为不可用，不阻断 PR 状态、合并就绪、Checks、Review 与行内评论落库。
4. 单个部署状态读取失败时，该部署标记为未知；其他部署继续展示。
5. 不新增 MCP 工具，不把部署信息或相关 Token 暴露给 Agent。
6. 不升级 SQLite 架构；字段写入既有 `pull_request_insights` JSON 快照。

## 3. GitHub 数据映射

### 3.1 部署列表

使用：

```text
GET /repos/{owner}/{repository}/deployments?sha={head_sha}&per_page=11
```

- `sha` 必须使用当前 PR 详情的 head SHA。
- 最多展示 10 条部署；请求 11 条仅用于判断是否还有更多记录。
- 只保存部署的环境名、描述、状态和时间，不保存 payload、creator、ref、原始响应或日志链接。
- 环境名最多 200 字符，描述最多 500 字符。

### 3.2 最新部署状态

对每个保留的部署使用：

```text
GET /repos/{owner}/{repository}/deployments/{deployment_id}/statuses?per_page=1
```

- 状态映射为 `success`、`failure`、`error`、`inactive`、`in_progress`、`queued`、`pending`、`unknown`。
- 没有状态记录或状态读取失败时使用 `unknown`。
- 只保存状态、脱敏限长的描述和状态更新时间，不保存 `environment_url`、`log_url`、`target_url` 或原始响应。

## 4. 领域模型

`PullRequestInsights` 增加带默认值的 `deploymentDetails`：

```text
PullRequestDeploymentDetails
  state: not_checked | no_deployments | available | unavailable
  deployments: PullRequestDeployment[]
  truncated: boolean

PullRequestDeployment
  environment: string
  state: success | failure | error | inactive | in_progress | queued | pending | unknown
  description: string | null
  statusUpdatedAt: RFC3339 timestamp | null
```

- 旧快照缺少字段时默认为 `not_checked`。
- `available` 必须包含至少一个部署；其他状态不得携带部署或截断标记。
- 最多保存 10 个部署；环境名和描述拒绝控制字符。
- 状态集合只接受稳定枚举，未知 GitHub 状态降级为 `unknown`。

## 5. 刷新与失败语义

- 原有 PR、Checks、Review、评论四个请求仍保持原子性；任一失败时本次刷新不写入新快照。
- 四个请求成功后，再读取当前 head SHA 的部署列表。
- 部署列表失败时保存 `unavailable`，其他验收信息仍正常落库。
- 单个部署状态请求失败时保存该部署为 `unknown`，其他部署继续处理，整体仍为 `available`。
- 部署刷新不新增审计事件字段，也不改变历史事件反序列化契约；现有验收事件仍以 Checks、Review 和评论为内容哈希边界。

## 6. 桌面展示

- PR 头部增加部署汇总徽标：成功、存在失败、进行中、状态未知、无部署、不可用或尚未同步。
- 部署详情展示环境名、状态、描述和状态更新时间。
- 明确说明部署状态只基于当前 PR 头提交和 GitHub 当前快照，不证明生产发布已获批准。
- 不提供部署、审批、取消、重试、回滚或跳转日志按钮。
- 旧快照显示“部署状态尚未同步，刷新后读取”。

## 7. 明确不做

- 不创建 Deployment、Deployment Status、Environment Approval 或 Workflow Run。
- 不自动部署、不提审 App、不发布小程序、不合并 PR、不创建 Release Tag。
- 不读取 Actions 日志、部署 payload、环境 Secret 或生产凭据。
- 不将部署状态加入合并阻塞项或自动合并判断。
- 不增加 MCP 工具，不允许 Agent 主动刷新 PR 或部署状态。

## 8. 验收

- 领域测试覆盖状态映射、默认值、数量上限、状态一致性、文本与时间校验。
- GitHub 适配器测试覆盖精确 head SHA 查询、10 条上限、最新状态解析和非法响应。
- 应用测试覆盖成功落库、无部署、列表失败降级、单状态失败降级与旧快照兼容。
- 前端测试覆盖成功、失败、进行中、未知、无部署、不可用和旧快照提示。
- 全量 Rust、前端门禁、安装包与 D 盘覆盖安装通过，MCP 仍为 11 个工具。
