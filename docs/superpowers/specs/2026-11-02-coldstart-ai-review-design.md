# ColdStart AI Review 与评审规范库设计

- 日期：2026-11-02
- 状态：已确认，待实现
- 适用阶段：AI Review 与评审规范门禁

## 1. 背景

ColdStart 已经能完成“需求 -> 蓝图 -> 计划 -> Codex/MCP 领取 -> Git 提交 -> 本地验证 -> 任务完成并推送”的闭环。当前完成门禁只能证明配置的验证命令通过，不能发现验证未覆盖的代码质量、安全、可维护性或业务约束问题。

本阶段在本地验证之后增加独立的 AI Review，并允许用户上传或编写代码规范，让 Codex 与 Reviewer 使用同一份规则。AI Review 是最终提交前的补充门禁，不替代确定性验证，也不替代人工对生产发布、提审和不可逆操作的授权。

## 2. 目标

1. 任务完成前，使用真实 Git diff 执行一次结构化 AI Review。
2. 项目通用规范和仓库专属规范都能被版本化保存，并同时供任务上下文和 Reviewer 使用。
3. `critical`、`high` 问题默认阻止任务完成；中低风险只提示。
4. 模型不可用、超时、输出无效或输入超限时默认失败关闭，不静默放行。
5. 用户可以在桌面端对有证据的误报进行人工豁免，但不能通过 MCP 豁免。
6. Review 的配置、规范版本、问题、判定和豁免都有可追踪记录。
7. README 重写为面向使用者的产品说明，覆盖新项目、历史项目、完整工作流、MCP 使用、安全边界和当前限制。

## 3. 非目标

- 不接入 GitHub Checks、PR 评论或第三方代码托管平台的 Review 机器人。
- 不实现自动修复、自动提交修复、多轮 Agent 循环或 Review 通过率优化。
- 不在本阶段实现部署、App 打包、商店提审、小程序发布、自动合并或 Release Tag。
- 不提供通用 RAG、向量检索或对整个仓库建立语义索引。
- 不保证识别所有安全问题；AI Review 是补充门禁，不是安全审计或恶意代码沙箱。
- 不执行仓库中的脚本、可执行文件或模型生成的命令。
- 不把 Review 模型输出当作确定性事实；高风险误报允许桌面用户填写理由后豁免。

## 4. 核心决策

### 4.1 使用独立的完成阶段

编码任务的提交顺序固定为：

```text
校验报告与真实 Git 提交
  -> 校验任务分支、HEAD 和允许修改路径
  -> 执行仓库验证命令
  -> 确认验证未破坏 Git 不变量
  -> 提取真实 Git diff
  -> 执行 AI Review
  -> 通过、豁免或失败
  -> 通过后完成任务并推送任务分支
```

任何一步失败，任务都不能进入 `done`。验证失败时不调用 Review 模型；Review 阻塞或基础设施失败时也不推送任务分支。

### 4.2 Reviewer 使用真实代码，不信任 Agent 自报

Review 输入中的变更必须由 ColdStart 从 Git 仓库重新提取。Agent 报告里的摘要、证据和自测结果只作为辅助信息，不能代替 diff。Review 不允许读取未出现在本次 diff 中的文件内容，也不运行任何仓库代码。

### 4.3 规范按项目与仓库分层

- 项目通用规范适用于所有编码仓库。
- 仓库专属规范只适用于固定逻辑键 `server`、`admin`、`web`、`app`、`mini_program` 对应的仓库。
- 两者同时启用时按“项目通用在前、仓库专属在后”的确定顺序组合；出现冲突时，提示 Reviewer 将仓库专属规范视为更具体的要求，并在问题中说明冲突。
- 规范不确定解析成结构化规则，而是作为带版本的自然语言评审依据。

### 4.4 默认失败关闭

开启 AI Review 后，以下情况都阻止完成：

- 缺少有效配置或用户同意；配置了凭据引用但无法解析时同样阻止。
- 模型请求失败、超时、被中断或响应无法解析。
- diff 或规范超过输入预算。
- 规范快照哈希与领取时不一致。
- 存在达到阻断级别的 Review 问题。

不使用“Review 失败就放行”的降级策略。用户只能对有具体 finding 的误报执行桌面端人工豁免；基础设施错误不能豁免。

## 5. 系统架构

沿用现有 Rust workspace 和分层，不新增独立 crate。

```text
React 桌面端
  -> Tauri commands
  -> coldstart-app::review 编排
  -> coldstart-core::review 领域规则
  -> coldstart-storage::review_store SQLite 持久化
  -> coldstart-adapters::review Git diff/模型 HTTP/脱敏

MCP submit_work_report
  -> coldstart-app::plan
  -> coldstart-app::review
```

职责边界：

- `core`：Review 配置、规范元数据、快照、finding、状态、阻断策略和输入校验。
- `storage`：规范版本、Review 配置、工作领取快照、Review 记录和事件的事务持久化。
- `adapters`：从 Git 提取受控 diff、调用 OpenAI 兼容模型、限制响应大小和脱敏。
- `app`：把 Review 接入领取、报告提交、验证、完成、推送和人工豁免流程。
- `mcp`：不增加新工具；扩展 `get_work_context` 和 `submit_work_report` 的结果与稳定错误码。
- `web/Tauri`：管理规范库、Review 配置、隐私同意、结果查看和人工豁免。

## 6. 评审规范库

### 6.1 支持的来源

- 上传 Markdown、TXT、DOCX、PDF，复用现有文档提取器。
- 桌面端直接编写 Markdown 规范。
- 扫描版 PDF 的 OCR 不在支持范围内。

### 6.2 规范实体

每个规范包含：

- 稳定 `standard_id`、名称、适用范围和排序位置。
- 项目通用或指定仓库两种范围。
- 启用状态和软删除状态。
- 当前修订和不可变历史修订。
- 原始文件名、来源类型、格式、创建时间、提取文本大小和内容哈希。

修订是不可变的。重新上传、编辑名称之外的正文或改变文件都会创建新修订；已领取任务仍引用旧修订。

### 6.3 存储

- SQLite 保存规范元数据、修订元数据和选择快照。
- 原始文件及提取文本保存在项目主目录的 `.coldstart/review-standards/<standard-id>/<revision-id>/`。
- 软删除只隐藏规范，不物理删除历史修订，避免已领取任务的快照失效。
- 使用修订内容哈希校验磁盘文本；哈希不一致时 `review_standard_invalid`，禁止评审。

### 6.4 输入限制

- 每个规范提取后的文本最多 `60,000` 个字符。
- 单个任务最多启用 `20` 个规范。
- 单个任务组合后的规范文本最多 `60,000` 个字符。
- 空文本、无法读取的文档、未知格式和超过限制的内容在上传或启用时返回明确错误。
- 不静默截断规范；超限时必须由用户拆分、停用或缩小范围。

## 7. Review 配置

每个项目只有一份当前 Review 配置：

- `enabled`：是否启用 AI Review。
- `base_url`：OpenAI 兼容服务的 Base URL。
- `model`：模型名称。
- `credential_id`：可选的 Windows Credential Manager 凭据引用。
- `blocking_severities`：默认 `critical`、`high`。
- `consent_fingerprint`：确认将 diff 和规范发送给模型服务时绑定的配置指纹。

约束：

- Base URL 必须是 `http://` 或 `https://`。
- 模型名称不能为空。
- 真实 API Key 不写入 SQLite、事件、日志、错误、MCP 响应、Prompt 或 Git。
- 保存凭据只保存系统凭据和元数据引用，界面不回读明文。
- 修改 Base URL、模型或凭据引用后，原同意失效，必须重新确认。
- 未配置凭据引用时不发送 `Authorization` 头，适用于不需要鉴权的本地兼容服务；配置了引用但系统凭据不存在时 `review_credential_missing`。
- 规范增删改不重置模型配置同意，但每次任务领取仍记录实际使用的规范修订快照。
- AI Review 只对 `agent_coding` 任务生效；人工任务和审批任务保持现有语义。

## 8. 领取快照

编码任务领取时原子保存 Review 快照，至少包含：

- 是否启用、配置指纹、模型名称和阻断级别。
- 本次任务仓库对应适用的项目规范与仓库规范。
- 每个规范的 `standard_id`、修订 ID、名称、范围、排序、内容哈希和提取文本大小。
- 任务基准 Commit SHA，用于之后提取稳定 diff。

快照写入 `work_runs`，并在任务上下文中暴露不含 Secret 的只读视图。领取后的配置或规范变更不影响该任务。

开启 AI Review 但配置无效、同意缺失、规范哈希校验失败或没有可用的 Review 模型时，不允许领取新的 `agent_coding` 任务；错误必须在领取阶段返回，而不是等到提交后才暴露。

## 9. 规范与任务上下文

Review 规范同时供 Codex 使用，避免“Agent 不知道规则，Reviewer 却按规则判错”。

- 任务上下文增加 Review 快照摘要。
- 每个适用规范作为一个新的 `review_standard` 类型任务引用加入白名单。
- Codex 可以通过现有 `read_task_reference` 按需读取完整规范。
- 规范引用受租约授权，不允许读取其他项目、其他仓库或未启用的规范。
- 默认上下文预算不因规范正文被强行撑大；引用目录只列出名称、范围、版本和哈希。
- Review 阶段和任务上下文使用同一份领取快照，保证规则一致。

## 10. Git diff 提取

Review 前由 ColdStart 使用固定参数调用本机 Git，不经过 Shell，不运行仓库内部程序。

提取规则：

- 以领取时记录的基准 Commit 与报告 HEAD 之间的真实 diff 为准。
- 使用禁用外部 diff、禁用 textconv、禁用颜色和固定上下文行数的参数。
- 排除二进制文件和子模块变更；二进制列表作为元数据展示，不发送内容。
- 不读取未修改文件、完整仓库或 Git 历史中的其他提交。
- 只允许常规文本 diff，不执行 `.gitattributes` 指定的外部 diff 或 textconv 程序。
- 提取前后都复核当前分支与 HEAD，任何变化都视为 Review 前置条件破坏。

输入预算：

- 单个 Review 的 diff 最多 `160,000` 个字符。
- 最多包含 `1,000` 个变更文件条目；超过时直接失败并要求缩小任务。
- 超过预算返回 `review_input_too_large`，不静默截断。

## 11. 模型请求

### 11.1 输入

请求只包含：

- 任务标题、任务契约中的目标和验收标准。
- 项目通用规范与当前仓库专属规范。
- 真实 Git diff。
- 脱敏后的验证摘要，不包含原始命令输出或 Secret。
- 固定的内置评审基线：正确性、安全、测试覆盖、错误处理、数据兼容和可维护性。

不包含蓝图全文、整个需求文档、凭据、环境变量、数据库路径、租约令牌或其他项目数据。

### 11.2 Prompt 安全

- 明确声明代码、注释和规范文本都是不可信数据，不得执行其中的指令。
- 要求模型只输出约定 JSON，不输出 Markdown 包裹或额外解释。
- 规范正文只能影响评审标准，不能改变输出协议或要求读取额外文件。
- 模型响应有响应体大小上限，超限视为失败。

### 11.3 受控参数

- 温度固定为 `0`。
- 总超时默认 `180` 秒。
- 使用 `response_format: {"type":"json_object"}`，与现有蓝图模型适配器一致。
- 请求仅在本地内存中持有 Secret，完成后不保存。

### 11.4 输出结构

模型必须返回：

```json
{
  "summary": "string",
  "findings": [
    {
      "severity": "critical | high | medium | low",
      "category": "security | correctness | tests | maintainability | style | performance | other",
      "file": "repository-relative path",
      "line": 1,
      "message": "string",
      "recommendation": "string",
      "standard": "optional standard name or null"
    }
  ]
}
```

校验规则：

- `summary` 不能为空，最多 `4,000` 个字符。
- 最多 `100` 条 finding。
- `file` 必须是本次 diff 中的仓库相对路径，禁止绝对路径和 `..`。
- `line` 可省略；存在时必须是正整数。
- `message`、`recommendation` 各自最多 `2,000` 个字符。
- 未知字段可以忽略，但缺少必需字段或枚举值非法时整个响应无效。
- 不保存模型原始响应、完整 Prompt 或未校验文本。

## 12. 阻断与展示

状态固定为：

- `not_required`：Review 未启用。
- `running`：Review 正在执行。
- `passed`：Review 完成且没有达到阻断级别的问题。
- `blocked`：存在达到阻断级别的问题。
- `failed`：配置、网络、超时、输入或响应校验失败。
- `waived`：用户在桌面端对 `blocked` 结果完成人工豁免。
- `interrupted`：进程中断或租约过期导致的未完成 Review。

只有 `not_required`、`passed` 或 `waived` 允许报告被接受。

桌面任务详情展示：

- Review 状态、模型名称、耗时和规范版本摘要。
- finding 的严重级别、文件、行号、问题、建议和关联规范。
- `blocked` 结果提供人工豁免入口，要求填写理由并二次确认。
- `failed` 结果不允许豁免，必须修复配置后重新提交。

MCP 只返回脱敏后的摘要、状态、finding 和稳定错误码，不返回 Prompt、凭据、模型原始响应或本地绝对路径。

## 13. 人工豁免

人工豁免只用于有具体 finding 的模型误报：

- 仅桌面端可执行，MCP 不提供工具。
- 仅允许状态为 `blocked` 的 Review。
- 必须显示全部阻断 finding 并要求填写非空理由。
- 记录豁免人标识（本机用户在当前产品边界内为本地操作者）、时间、理由、Review ID（复用关联 work report 的 ID）、finding 哈希和规范快照哈希。
- 豁免后任务按正常完成与推送流程处理；已经存在的租约被消费。
- 只有报告仍是当前任务的有效报告且租约未过期时才能豁免；租约过期后必须按新的任务运行重新提交。
- 审计记录永久保留，后续不能在原报告上撤销为未发生。

## 14. SQLite 架构

架构版本升级到 7。

新增表：

- `review_configurations`：项目级 Review 配置和同意指纹。
- `review_standards`：规范稳定身份、范围、启用与软删除状态、排序。
- `review_standard_revisions`：不可变修订、文件位置、哈希和大小。

扩展表：

- `work_runs`：增加 `review_snapshot_json`、`review_snapshot_hash`。
- `work_reports`：增加 `review_status`、`review_json`、`review_started_at`、`review_finished_at`、`review_waiver_json`。

数据库约束：

- 同一个 `work_item_id` 最多一条 `review_status = 'running'` 的报告。
- 快照哈希和规范修订哈希使用 SHA-256。
- Review 状态与报告接受状态在同一事务中更新。
- Review 失败、阻塞或中断不能把任务写成 `done`。

## 15. MCP 契约

不增加新的 MCP 工具。保留现有八个工具及任务租约模型。

`get_work_context`：

- 返回 Review 快照摘要和规范引用目录。
- Review 未启用时显式返回 `not_required` 或省略不影响兼容性。

`submit_work_report`：

- 验证通过后同步执行 Review，并返回 Review 状态与脱敏 findings。
- 只有 Review 允许通过时报告才被接受。
- 失败后保留租约，Codex 可使用新的幂等键修复并重新提交。

新增稳定错误码：

- `review_not_configured`
- `review_consent_required`
- `review_credential_missing`
- `review_standard_invalid`
- `review_in_progress`
- `review_input_too_large`
- `review_timeout`
- `review_failed`
- `review_response_invalid`
- `review_blocked`
- `review_interrupted`

## 16. 桌面界面

新增“AI 评审”视图：

- Review 开关、模型 Base URL、模型名称、凭据选择和隐私同意。
- 明确显示“本次 diff 与启用的规范将发送至该模型服务”。
- 配置校验和当前适用规范预览。
- 规范库：上传、直接新建 Markdown、编辑正文、仓库范围、启用/停用、排序、查看版本和软删除。
- 规范预览：原始文件名、提取字符数、内容哈希和更新时间。
- Review 历史：按任务查看状态、时间、模型和 finding；只在 `blocked` 时显示豁免入口。

仓库与能力页的仓库项可以显示适用的规范数量，但规范正文和编辑入口集中在 AI 评审页。

## 17. README 重写

实现完成后同步重写 `README.md`，使其成为当前产品文档，而不是阶段开发日志。

README 必须包含：

1. 一句话定位和解决的问题。
2. 适合的新项目与历史项目场景，以及它不会做什么。
3. 安装、启动和创建项目。
4. 从需求导入、蓝图、计划、仓库配置、验证命令、AI Review、MCP 到任务完成的完整工作流。
5. 历史项目接入步骤：映射现有仓库、导入变更需求、限定本次迭代范围。
6. 代码规范库和 AI Review 的使用、阻断规则、误报豁免与隐私说明。
7. MCP 配置示例和八个工具说明。
8. 凭据、安全边界、已知限制和常见故障。
9. 开发构建、安装包和文档索引。

README 不再以“第 1 至第 7 阶段”作为首屏结构；阶段历史保留在路线图和实施计划中。

## 18. 安全与隐私

- 发送到模型服务的内容只包括本次 diff、任务验收标准、启用规范和脱敏验证摘要。
- 用户必须对新配置做显式同意；没有同意不能领取开启 Review 的编码任务。
- API Key 只从 Windows Credential Manager 读取到内存，不记录、不显示、不进入 MCP。
- 错误、事件、UI 和数据库只保留稳定错误码与脱敏摘要。
- diff、规范、finding 和模型响应都设置长度和数量上限。
- Review 模型输出不能触发 Git、Shell、网络下载、文件写入或任意 MCP 工具。
- Review 不提供“忽略失败”选项；只有针对具体 `blocked` finding 的人工豁免。
- 产品不宣称 AI Review 能证明代码没有漏洞，也不宣称它是完整安全沙箱。

## 19. 失败与恢复

- 模型超时或网络错误：Review 标记 `failed`，任务保持 `in_progress`，保留租约和已提交报告；不推送。
- 响应格式无效：`review_response_invalid`，不保存原始响应。
- 输入超限：`review_input_too_large`，用户拆分任务或缩小规范后重新提交。
- 进程崩溃：下一次访问把运行中的 Review 标为 `interrupted`；租约有效时允许修复后重新提交，租约过期时按现有租约规则回收。
- 规范文件哈希不一致：`review_standard_invalid`，阻止评审并提示重新导入或修复规范。
- 配置变更：只影响之后领取的任务，已领取任务继续使用快照。
- Review 成功后推送失败：任务保持完成，沿用现有 Git 同步风险记录，不重新打开任务。

## 20. 测试策略

### 20.1 领域测试

- 配置、规范、范围、排序、同意指纹和快照校验。
- 阻断级别与 Review 状态转换。
- finding 路径、行号、长度和数量校验。
- 规范冲突与项目/仓库组合顺序。

### 20.2 存储测试

- v6 到 v7 迁移和全新数据库建表。
- 规范 CRUD、版本化、软删除、启用顺序和哈希。
- 领取时原子快照、并发领取和无有效配置时拒绝领取。
- Review 成功、阻塞、失败、中断、豁免和幂等重试。
- 同一个任务不能存在两条运行中的 Review。
- 所有失败路径都不能完成任务或推送。

### 20.3 适配器测试

- 真实本地 Git 仓库的 diff、基准 Commit、二进制排除、超限以及外部 diff/textconv 禁用。
- 本地 HTTP 假模型服务覆盖成功、超时、HTTP 错误、响应过大和非法 JSON。
- 发送内容不包含 Secret、环境变量或项目数据库路径。
- 代码中的提示注入文本不能改变输出协议。

### 20.4 应用与 MCP 测试

- 验证失败时不调用 Review 模型。
- 验证通过且 Review 通过时完成任务并推送。
- Review `blocked`、`failed`、`interrupted` 时保持 `in_progress` 且不推送。
- 桌面豁免后完成任务并推送，MCP 不能调用豁免。
- 配置或规范修改不影响已领取任务。
- MCP 工具列表不变，错误码稳定，响应不泄露 Secret。

### 20.5 前端测试

- Review 配置、隐私同意、模型凭据和规范库交互。
- 上传、Markdown 新建、修订、仓库范围、排序和软删除。
- finding 展示、阻断提示、失败关闭和人工豁免确认。
- 历史项目与新项目说明可被用户找到。

### 20.6 端到端验收

使用本地裸 Git 远程、真实验证 `.exe` 和本地假模型服务完成：

```text
配置规范 -> 领取任务 -> Codex 提交 -> 验证通过
  -> Review 阻塞 -> 修复 -> 新报告 -> Review 通过
  -> 任务完成 -> 推送 -> 桌面查看记录
```

并覆盖一次模型不可用导致的失败关闭，以及一次桌面人工豁免。

## 21. 阶段验收

- `cargo fmt --check` 通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- Rust stable 与 Rust 1.89 的 `cargo test --workspace` 全部通过。
- 前端 TypeScript 检查、单元测试和生产构建通过。
- 生成新的 NSIS 安装包，并完成安装、启动、Review 配置和卸载保留数据验收。
- README 重写完成，中文使用说明与当前实现一致。
- Git 工作树干净，提交和远程 SHA 一致。

## 22. 明确不做

- 不增加 GitHub PR 或 Checks 集成。
- 不增加 MCP Review 管理工具或豁免工具。
- 不自动修复 finding，不自动创建修复提交。
- 不调用多个模型投票，不做模型路由或成本优化平台。
- 不做自定义规则 DSL、AST 分析器或本地静态分析器集成。
- 不上传整个仓库，不做向量检索。
- 不在本阶段恢复已搁置的部署、打包、商店提审、小程序发布、自动合并和 Release Tag。
