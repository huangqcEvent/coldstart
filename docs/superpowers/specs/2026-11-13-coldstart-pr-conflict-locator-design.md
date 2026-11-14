# ColdStart GitHub PR 冲突文件本地定位设计

- 日期：2026-11-13
- 状态：已完成（阶段 15，2026-11-13）
- 关联：阶段实施计划仅保留在本地，不随开源仓库发布。

## 1. 目标

阶段 15 在阶段 14 的合并就绪快照上增加“冲突文件本地定位”。当 GitHub 明确返回合并冲突时，桌面端使用本地 Git 仓库计算冲突路径，帮助人工快速判断应该查看哪些文件。

这项能力只回答“哪些路径发生冲突”，不读取补丁内容，不解决冲突，也不改变 GitHub 状态。

## 2. 设计原则

1. 只读诊断：不修改工作区、索引、当前分支、远程分支或 Git 配置。
2. 不新增 GitHub 请求：冲突文件来自本地 Git，不依赖 GitHub 未提供的冲突文件接口。
3. 不把本地诊断失败当成 GitHub 状态变化：GitHub 仍标记冲突时，本地无法定位只显示为“不可用”。
4. 不把“本地未复现”当成“冲突已解决”：GitHub 状态仍是最终判断依据。
5. 不保存补丁、blob 内容、Git 原始输出或 stderr；只保存经过限长和脱敏的仓库相对路径。
6. 不执行仓库自定义 merge driver；不能因为一次只读刷新运行仓库脚本。

## 3. 数据模型

`PullRequestInsights` 增加带默认值的 `conflictDetails` 字段：

```text
PullRequestConflictDetails
  state: not_checked | not_applicable | detected | not_reproduced | unavailable
  paths: string[]
  truncated: boolean
```

状态含义：

| 状态 | 含义 |
| --- | --- |
| `not_checked` | 旧快照没有冲突定位数据；刷新后才可能产生 |
| `not_applicable` | PR 已合并/关闭，或 GitHub 未标记合并冲突 |
| `detected` | 本地 merge-tree 报告冲突，`paths` 为冲突路径 |
| `not_reproduced` | GitHub 标记冲突，但本地 merge-tree 未复现冲突 |
| `unavailable` | 本地缺少提交对象，或 Git 诊断不可用/失败 |

冲突路径最多 100 条，每条最多 1,024 字符；路径必须是相对的仓库路径，不含控制字符。只有 `detected` 可以携带路径和 `truncated=true`。

## 4. 诊断流程

1. 先完成现有四个 GitHub 只读请求，并计算合并状态。
2. 只有 PR 为开放状态且 `mergeability=conflicted` 时才执行本地诊断。
3. 使用 GitHub PR 详情中的 base SHA 和 head SHA；不把分支名作为诊断输入。
4. 先检查两个提交对象是否存在于映射的本地仓库。
5. 执行 `git merge-tree --write-tree --name-only --no-messages <base> <head>`。
6. 退出码 `0` 映射为 `not_reproduced`；退出码 `1` 映射为 `detected` 并读取冲突路径；其他退出码映射为 `unavailable`。
7. 任何本地诊断失败都不影响本次 GitHub 快照落库。

`merge-tree` 只做对象层面的合并，不切换分支、不改工作区，也不改索引；它可能在 `.git` 中写入临时树对象，但不产生业务文件变更。

## 5. Git 脚本隔离

实验证明 `git merge-tree` 默认可能执行仓库 `.gitattributes` 指定的自定义 merge driver。诊断必须执行以下隔离：

- 设置 `GIT_CONFIG_NOSYSTEM=1`，禁用系统配置。
- 将 `GIT_CONFIG_GLOBAL` 指向平台空设备，禁用全局配置。
- 枚举仓库本地 `merge.*.driver`，对每个键追加 `-c <key>=false`，覆盖自定义驱动。
- 强制 `merge.default=text`，避免未命名属性回落到自定义默认驱动。
- 清除继承的 `GIT_CONFIG_COUNT`、`GIT_CONFIG_KEY_*` 和 `GIT_CONFIG_VALUE_*`。

这些规则必须有适配器测试证明：配置了带副作用的 merge driver 时，诊断不会创建副作用文件。

## 6. 快照与界面

- 冲突详情写入既有 `pull_request_insights` JSON，不升级 SQLite 架构。
- 旧快照缺少该字段时默认 `not_checked`，刷新后更新。
- 仅在开放且 GitHub 标记合并冲突的 PR 上显示“本地冲突定位”区域。
- `detected` 显示冲突路径列表和是否截断；`not_reproduced`、`unavailable`、`not_checked` 显示明确的中文原因提示。
- 所有文案明确说明：GitHub 状态是最终依据，ColdStart 不自动解决冲突。

## 7. 安全边界

- 不新增 GitHub 端点，仍只有阶段 13 的四个只读 GET。
- 不读取冲突补丁内容，不保存 blob、树对象或 Git 原始输出。
- 不执行 fetch、pull、merge、rebase、checkout、reset 或写分支操作。
- 不执行仓库自定义 merge driver。
- 不增加 MCP 工具，MCP 总数仍为 11。

## 8. 明确不做

- 不自动解决冲突、不自动更新目标分支、不自动合并。
- 不读取部署状态、Issue 或 Projects。
- 不把本地未复现冲突解释为 GitHub 冲突已经消失。

## 9. 验收

- 领域测试覆盖默认值、状态校验、路径上限、控制字符和旧快照兼容。
- Git 适配器测试覆盖冲突路径、无冲突、缺失提交和自定义 merge driver 隔离。
- 应用测试覆盖刷新后冲突详情持久化，以及本地诊断失败不阻断 GitHub 快照。
- 前端测试覆盖冲突路径、未复现、不可用、旧快照与只读说明。
- 全量 Rust、前端门禁通过，安装包更新到 D 盘，MCP 仍为 11 个工具。
