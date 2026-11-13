# ColdStart 阶段 21：MCP 受限凭据代理设计

实现状态：已完成（2026-11-13）

## 1. 目标

阶段 21 让 Codex 能在已领取任务的授权范围内调用三方能力 API，同时保持真实 Secret 不离开 Windows Credential Manager。

ColdStart 不开放本地 HTTP 端口，不启动后台代理服务，也不把 Secret 写入 MCP 参数、任务上下文、项目 SQLite、事件日志、报告、Git 仓库或 MCP 响应。Codex 只通过一个新增的 MCP 工具发起受限请求，由 Rust 在本机完成租约校验、能力授权、Secret 注入、HTTPS 请求、响应限长和脱敏。

## 2. 数据模型

新增 `capability_api_endpoints` 表，SQLite 架构从 9 升到 10。每个端点绑定：

- 项目、蓝图能力 ID 和环境（`development`、`staging`、`production`）。
- 人类可读名称和 HTTPS Base URL。
- 允许的路径前缀。
- 允许的 HTTP 方法集合。
- 可选凭据引用。
- 可选鉴权头名称和前缀，例如 `Authorization` + `Bearer` 或 `X-API-Key` + 空前缀。

端点只在任务上下文中暴露 ID、名称、方法、路径前缀和凭据引用，不暴露 Secret。删除凭据时，数据库外键将端点的 `credential_id` 置空。

## 3. MCP 契约

MCP 新增第 12 个工具 `call_capability_api`：

```text
capability_id
environment
endpoint_id
work_id
lease_token
method
path
query?
body?
content_type?
```

工具只返回结构化状态码、内容类型、响应正文、是否截断、耗时和是否发生脱敏。响应正文最多保留 128 KiB；请求正文最多 256 KiB；请求整体超时 20 秒。

## 4. 授权链

调用必须同时满足：

1. `work_id` 存在，租约 token 哈希匹配，租约未过期，工作项处于 `claimed` 或 `in_progress`。
2. 工作项来自当前活动计划，任务为 Agent 编码任务。
3. 任务的 `context_refs` 明确包含该能力 ID。
4. 能力状态存在，且为 `configured` 或 `verified`。
5. 端点存在，且项目、能力、环境完全匹配。
6. 请求方法和路径命中端点白名单。
7. 端点凭据属于当前项目，凭据环境为 `shared` 或与端点环境一致，系统凭据库能读取到 Secret。

只读租约校验不会刷新心跳、推进工作项状态或产生项目事件。代理调用失败不会改变任务状态。

## 5. 请求安全

- 只允许 HTTPS；拒绝 URL 内嵌用户名密码、查询、片段、非默认 Host、私网 IP 字面量和 `localhost`。
- 拒绝重定向，禁止跨 Host 跳转。
- 路径必须是绝对路径，拒绝 `//`、反斜杠、控制字符、`.`、`..` 和编码形式的 `%2e`、`%2f`、`%5c`。
- 查询参数使用结构化键值，不允许自己拼接完整 URL。
- 禁止覆盖 `Host`、`Content-Length`、`Transfer-Encoding`、`Connection`、`Proxy-*` 等传输头。
- Secret 只注入到配置的鉴权头；错误信息必须脱敏。
- 返回正文如果包含注入的 Secret，替换为 `[REDACTED]`。
- 本地 HTTP 只允许适配器测试显式开启，生产客户端始终只接受 HTTPS。

## 6. 桌面配置

“仓库与能力”页新增“能力 API 白名单”面板：

- 选择能力与环境。
- 配置名称、Base URL、路径前缀、允许方法、凭据、鉴权头和鉴权前缀。
- 支持编辑和删除。
- 页面只显示配置，不显示 Secret。

## 7. 错误码

至少包含：

- `capability_api_endpoint_not_found`
- `capability_api_not_authorized`
- `capability_api_unavailable`
- `capability_api_method_not_allowed`
- `capability_api_path_not_allowed`
- `capability_api_credential_missing`
- `capability_api_request_too_large`
- `capability_api_timeout`
- `capability_api_redirect_not_allowed`
- `capability_api_request_failed`
- `invalid_capability_api_endpoint`

## 8. 非目标

- 不自动部署、打包 App、商店提审、小程序发布、合并 PR 或创建 Release Tag。
- 不自动启动 Codex，不开放本地端口。
- 不代理文件二进制流或浏览器会话 Cookie。
- 不支持 OSS 签名算法等需要 SDK 计算的鉴权方案；第一版只支持静态 Header 注入。
- 不把三方 API 响应写入 SQLite、项目记忆或报告。

## 9. 验收

- 全量 Rust 测试、Clippy、格式检查、前端类型检查、前端测试和生产构建通过。
- 测试覆盖端点校验、数据库升级、租约/任务/能力授权、Secret 注入、响应脱敏、方法/路径拒绝和重定向拒绝。
- 真实项目从架构 9 自动升级到 10，旧数据保留。
- MCP 工具总数为 12，旧工具行为不变。
- Secret 不出现在 SQLite、事件日志、任务上下文、MCP 响应、报告和安装目录。
