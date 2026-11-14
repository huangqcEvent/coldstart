# ColdStart GitHub 草稿 PR 交付设计

- 日期：2026-11-13
- 状态：已实现（阶段 12）
- 关联：阶段实施计划仅保留在本地，不随开源仓库发布。

## 1. 目标

阶段 12 为已经完成本地验证、AI Review 和任务分支推送的编码任务，增加一个人工触发的 GitHub 草稿 PR 交付出口。ColdStart 负责整理任务、报告、验证、Review、变更文件和已知问题，但不会替用户合并、转为正式 PR、创建 Release Tag 或改写本地 Git。

设计原则：

1. 不可逆的平台操作只由桌面端人工触发。
2. MCP 不增加 PR 写入工具，Agent 仍只能领取、报告和读取上下文。
3. GitHub Token 只从系统凭据库临时读取，只用于 Rust 适配器请求。
4. PR 正文必须经过长度限制和脱敏，不包含 Secret、租约或本地绝对路径。
5. PR 状态只在用户主动刷新时读取；不伪装成实时同步。

## 2. 前置条件

创建草稿 PR 必须同时满足：

- 任务为 `agent_coding`，且当前状态为 `done`。
- 报告为 `completed`、已接受，且 Review 状态为 `not_required`、`passed` 或 `waived`。
- 报告带有任务分支和 Commit SHA。
- 同一工作项、仓库、分支和 Commit 存在成功的 `pushed` Git 同步记录。
- 仓库远程地址是 GitHub HTTPS 或 SSH 地址。
- 仓库使用凭据认证，凭据类型为 OAuth Token 或 Personal Access Token。
- 同一报告尚未关联草稿 PR；重复点击返回已有记录，不重复调用 GitHub。

## 3. 数据与事件

SQLite 架构升级到 8，新增 `pull_requests` 表：

- 唯一约束：`report_id`；同一仓库内 `number` 唯一。
- 关联项目、工作项、报告和仓库，删除项目或关联对象时级联删除。
- 保存编号、HTTPS 链接、标题、头分支、基分支、状态、草稿标记和时间。
- 状态固定为 `open`、`closed`、`merged`。

新增事件：

- `PullRequestOpened`：首次保存 PR 记录。
- `PullRequestStateChanged`：刷新后状态或草稿标记发生变化。

事件进入项目记忆的安全投影，不包含 Token、请求正文或本地路径。

## 4. GitHub 边界

适配器只实现两个操作：

- `POST /repos/{owner}/{repository}/pulls`，请求固定包含 `draft: true`、报告分支和仓库默认分支。
- `GET /repos/{owner}/{repository}/pulls/{number}`，只读刷新状态。

远程地址只接受 `github.com` 的 HTTPS 或 SSH 形式；拒绝其他主机、额外路径段和带用户信息的 URL。响应必须包含非零编号、HTTPS 链接、标题、头分支、基分支和合法状态。

不实现：

- 合并、关闭、转换正式 PR、修改标题或正文。
- PR 评论、Checks、提交状态回写、Issue 或 Projects 集成。
- Release Tag、部署、App 打包、商店提审和小程序发布。
- 删除远程分支或修改本地 Git 引用。

## 5. 应用层与安全

`PullRequestService` 只接受项目主目录和报告或 PR 标识；不接受任意仓库地址、分支、标题或请求正文。

PR 正文由服务端从已存在记录生成：

- 任务键、标题、目标、验收结果和 Commit。
- 本地验证状态和脱敏命令摘要。
- Review 结论、问题数、模型和人工豁免理由。
- 变更文件、已知问题和后续事项，均有数量和长度上限。
- 固定说明“由 ColdStart 创建为草稿，未自动合并，未创建 Release Tag”。

正文生成前复用验证输出脱敏器；请求正文上限 60,000 字符。Token 只出现在 HTTP Authorization 头，不进入 SQLite、事件、Tauri 返回、前端状态、错误消息或测试快照。

## 6. 桌面交互

“AI 评审”历史项在满足条件时显示“创建草稿 PR”。创建后显示编号、标题、分支方向、状态、GitHub 链接和“刷新状态”。错误使用稳定错误码显示，不展示 HTTP 正文或 Secret。

当前没有后台轮询。用户需要手动刷新；刷新只更新 ColdStart 的本地状态和审计事件，不在 GitHub 上执行写操作。

## 7. 验收

- 领域测试覆盖 URL、分支、标题和状态更新。
- 存储测试覆盖架构 8、唯一约束、状态变更和事件。
- 适配器测试覆盖创建请求、刷新响应、远程解析和 Token 脱敏。
- 应用测试覆盖完整闭环、幂等复用、状态刷新、门禁拒绝和 Secret 扫描。
- 前端测试覆盖可创建、不可创建、链接安全属性和手动刷新。
- 全量门禁通过后构建并安装 NSIS 包；MCP 工具仍为 11 个。

## 8. 已知限制

- 只支持 GitHub.com，不支持 GitHub Enterprise Server。
- 只支持人工创建草稿 PR，不提供批量或自动创建。
- 如果 GitHub 已创建成功但本地数据库保存失败，重试可能产生重复远程 PR；当前以本地幂等为主，用户需在 GitHub 侧确认。
- 不读取 Checks、Review 评论或合并冲突；这些留给后续阶段。
