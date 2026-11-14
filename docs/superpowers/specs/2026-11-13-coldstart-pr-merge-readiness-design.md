# ColdStart GitHub PR 合并就绪只读展示设计

- 日期：2026-11-13
- 状态：已完成（阶段 14，2026-11-13）
- 关联：阶段实施计划仅保留在本地，不随开源仓库发布。

## 1. 目标

阶段 14 在阶段 13 的 PR 验收快照上增加合并就绪信息，让桌面端在人工最终验收时同时看到：

- GitHub 返回的 PR 合并状态。
- 合并冲突、分支落后、草稿、状态计算中等已知阻塞信号。
- 失败的 Checks、未完成的 Checks 和要求修改的 Review。

这些信息只帮助人工判断，不授予合并权限，也不会自动执行任何 GitHub 写操作。

## 2. 设计原则

1. 复用现有 PR 详情请求，不增加 GitHub 端点或额外网络调用。
2. 只保存稳定枚举，不保存 GitHub 原始状态文本。
3. 旧快照缺少合并状态时默认 `unknown`，不把缺失当作可合并。
4. 最终是否允许合并仍由 GitHub 分支保护、审批规则和 Checks 决定。
5. 不增加 MCP 工具，不允许 Agent 读取或触发 PR 操作。

## 3. 数据映射

PR 详情响应中的 `mergeable` 与 `mergeable_state` 映射为稳定状态：

| GitHub 信号 | ColdStart 状态 | 界面文案 |
| --- | --- | --- |
| `mergeable_state=clean` | `mergeable` | 合并状态：可合并 |
| `mergeable_state=dirty` | `conflicted` | 合并冲突 |
| `mergeable_state=blocked` 或 `has_hooks` | `blocked` | 合并状态：暂不可合并 |
| `mergeable_state=behind` | `behind` | 分支落后目标分支 |
| `mergeable_state=unstable` | `unstable` | 合并状态不稳定 |
| `mergeable_state=draft` | `draft` | 草稿 PR |
| `mergeable_state=unknown` | `checking` | GitHub 正在计算合并状态 |
| 无法识别或响应缺少字段 | `unknown` | 合并状态未知 |

如果 `mergeable_state` 缺失，则 `mergeable=true` 映射为可合并、`false` 映射为暂不可合并、缺失映射为未知。

`mergeable_state` 在适配器层限制为 40 字符并拒绝控制字符；不进入事件、日志或错误详情。

## 4. 快照与兼容性

- `PullRequestInsights` 增加 `mergeability` 字段，随既有 JSON 快照保存，不升级 SQLite 架构。
- 该字段使用 serde 默认值 `unknown`，阶段 13 创建的旧快照可以直接反序列化。
- 合并状态与 Checks、Review、行内评论在同一次完整刷新中更新；失败时继续保留旧快照。
- 项目审计事件结构不变，不新增合并状态事件，避免每次 GitHub 重新计算状态都产生噪音。

## 5. 桌面展示

- PR 卡片显示合并状态徽标。
- 开放 PR 显示“合并前检查”：列出合并冲突、暂不可合并、分支落后、草稿、状态未知、Checks 失败/未完成、Review 要求修改等阻塞项。
- 没有已知阻塞项时显示“当前快照未发现阻塞项”，并明确说明这不是自动合并许可。
- 已合并或已关闭 PR 显示对应终态，不再展示合并前检查。
- 所有判断只基于当前快照和已有只读数据，不推断分支保护或审批是否满足。

## 6. 明确不做

- 不调用合并、关闭、更新分支、重跑 Checks 或回复评论接口。
- 不读取或展示具体冲突文件、部署状态、Issue 或 Projects。
- 不把“未发现阻塞项”称为“已批准合并”。
- 不增加 MCP PR、合并或工作流工具。

## 7. 验收

- 领域测试覆盖全部映射、缺失字段和旧快照兼容。
- 适配器测试覆盖 `mergeable`、`mergeable_state` 和非法状态长度。
- 应用测试覆盖刷新后快照持久化合并状态。
- 前端测试覆盖合并冲突、合并前阻塞项、已合并终态和只读说明。
- 全量 Rust、前端门禁通过，安装包更新到 D 盘，MCP 仍为 11 个工具。
