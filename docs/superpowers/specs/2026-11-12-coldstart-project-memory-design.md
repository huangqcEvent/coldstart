# ColdStart 项目记忆与变更审计设计

- 日期：2026-11-12
- 状态：已完成（2026-11-12 通过全量验证）
- 适用阶段：项目记忆与变更审计

## 1. 背景

ColdStart 已经把项目初始化、需求导入、蓝图发布、计划发布、任务领取、心跳、验证、AI Review、仓库与凭据配置写入 SQLite `events` 表。当前桌面端只显示事件总数，用户无法回答以下问题：

- 这个项目最近发生了什么？
- 蓝图或计划是在什么时候、以哪个版本发布的？
- Codex 领取、阻塞、提交报告、验证和 Review 的顺序是什么？
- 某个仓库、凭据或能力是什么时候配置的？
- 任务为什么没有继续推进？

本阶段把已有事件日志产品化为“项目记忆”：一个本地、只读、可筛选、可分页的项目变更时间线。它不新增第二套审计表，也不允许从界面修改历史。

## 2. 目标

1. 从现有 `events` 表读取当前项目的事件，严格按 `project_id` 隔离。
2. 使用游标分页，默认返回最近 100 条，单页最多 500 条。
3. 后端把原始 `DomainEventKind` 投影为安全、稳定的中文展示模型，前端不接触原始 payload。
4. 为每条事件提供阶段、标题、简短摘要和可选对象引用。
5. 默认隐藏高频 `work_heartbeat`，用户可显式打开“显示系统心跳”。
6. 提供阶段筛选、关键词搜索和“加载更多”，不改变服务端历史顺序。
7. 展示全部事件数、有效变更数、心跳数和各阶段有效变更数。

## 3. 非目标

- 不提供事件编辑、删除、回滚或重放。
- 不做云同步、团队协作或远程审计。
- 不实现自动部署、App 打包、商店提审、小程序发布、自动合并或 Release Tag。
- 不新增全文检索引擎；关键词搜索只在前端对已加载的安全投影做大小写不敏感匹配。
- 不把项目记忆暴露为新的写入能力，也不向 MCP 返回租约 token、凭据内容或原始 `payload_json`。

## 4. 核心决策

### 4.1 事件表是唯一事实来源

项目记忆只读取 `events`。事件写入仍由各领域事务负责，不在本阶段改动业务状态机。这样项目记忆不会与计划、Review 或仓库状态形成第二套竞争性事实。

### 4.2 安全投影，而不是透传

存储层只负责把行还原为 `DomainEvent`。应用层通过穷尽匹配把 `DomainEventKind` 映射为 `ProjectMemoryItem`：

- `eventId`：事件 ID。
- `occurredAt`：发生时间。
- `phase`：阶段。
- `eventType`：稳定的 snake_case 事件类型，用于样式与测试。
- `title`：中文动作标题。
- `summary`：不包含敏感内容的短摘要。
- `reference`：可选的安全对象引用，包含类型、ID 和展示标签。
- `isHeartbeat`：是否为系统心跳。

以下信息不得出现在响应中：

- 原始 `payload_json`。
- 租约 token 或完整租约 ID。
- 凭据 secret、secret ref、API Key 或密码。
- `RepositorySaved.remote_url`；远程 URL 可能内嵌用户名、令牌或本地敏感路径，只展示仓库逻辑键。

### 4.3 阶段划分

阶段固定为五类，前端以稳定顺序展示：

| 阶段 | 事件 |
| --- | --- |
| `requirements_blueprint` | 项目初始化、来源导入、蓝图草稿与发布、人工信息请求与回答 |
| `plan_execution` | 计划草稿与发布、计划替换、任务领取、开始、心跳、阻塞、报告、完成、租约释放与过期、人工任务 |
| `verification_review` | 验证开始、通过、失败、中断；Review 开始、通过、阻塞、失败、中断、豁免 |
| `repository_capability` | 仓库保存、验证命令保存、凭据保存与删除、能力状态、Git 同步 |
| `system_configuration` | 项目设置更新及其他无法归入业务阶段的系统事件 |

### 4.4 心跳默认隐藏

`work_heartbeat` 可能远多于所有业务事件。主时间线默认在 SQL 层排除该类型，避免第一页被心跳占满；用户打开开关后，分页请求将携带 `includeHeartbeats=true` 重新读取。

阶段计数只统计非心跳事件；心跳单独计数并展示。总事件数仍包含全部事件。

### 4.5 游标分页

游标使用 `events.seq`，排序固定为 `seq DESC`。读取时多取一条判断是否还有下一页：

```sql
SELECT seq, event_id, project_id, occurred_at, event_type, payload_json
FROM events
WHERE project_id = ?1
  AND (?2 IS NULL OR seq < ?2)
  AND (?3 = 1 OR event_type <> 'work_heartbeat')
ORDER BY seq DESC
LIMIT ?4;
```

`nextCursor` 为当前页最后一条的 `seq`，仅当还有更多记录时返回。无效游标或超出范围的 limit 在应用边界返回稳定错误，不静默截断。

## 5. 接口

### 5.1 存储层

```rust
pub struct StoredEvent {
    pub seq: i64,
    pub event: DomainEvent,
}

pub struct EventPage {
    pub events: Vec<StoredEvent>,
    pub next_cursor: Option<i64>,
}

impl Database {
    pub fn list_events(
        &self,
        project_id: &ProjectId,
        before_seq: Option<i64>,
        limit: u32,
        include_heartbeats: bool,
    ) -> Result<EventPage, StorageError>;

    pub fn event_type_counts(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<EventTypeCount>, StorageError>;
}
```

### 5.2 应用层

```rust
pub struct ProjectMemoryService;

impl ProjectMemoryService {
    pub fn read(
        &self,
        project_home: PathBuf,
        before_cursor: Option<i64>,
        limit: Option<u32>,
        include_heartbeats: bool,
    ) -> Result<ProjectMemoryPage, AppError>;
}
```

默认 limit 为 100，范围为 1 到 500。`before_cursor` 必须大于 0。

### 5.3 Tauri

```text
read_project_memory(path, beforeCursor, limit, includeHeartbeats)
  -> ProjectMemoryPage
```

命令使用 `spawn_blocking`，错误沿用稳定的 `{ code, message }` 契约。

## 6. 前端

侧栏新增第 09 项“项目记忆”，顶部阶段文案改为 `PHASE 09 / PROJECT MEMORY`。

页面包含：

- 摘要卡片：全部事件、有效变更、系统心跳、最近活动时间。
- 阶段筛选：全部、需求与蓝图、计划与执行、验证与评审、仓库与能力、系统与配置。
- 关键词搜索：匹配标题、摘要、事件类型和引用标签。
- “显示系统心跳”开关。
- 垂直时间线：时间、阶段、标题、摘要、引用。
- “加载更多”：使用 `nextCursor` 追加，不重复、不打乱顺序。
- 空状态：区分“项目尚未产生事件”和“当前筛选没有匹配结果”。

页面只读，不提供删除、编辑或回滚按钮。

## 7. 安全与验证

必须覆盖以下测试：

1. 事件按 `seq DESC` 返回，游标无重复、无遗漏。
2. 不同项目的事件不会串线。
3. 心跳默认排除，显式打开后可见。
4. 所有事件类型都有阶段、中文标题和非空摘要。
5. 响应 JSON 不包含仓库凭据 URL、租约 ID、secret ref 或原始 payload 字段。
6. 无效 limit、无效游标和缺失项目返回稳定错误码。
7. 前端可进入项目记忆、筛选阶段、搜索、切换心跳并加载下一页。

## 8. 验收标准

- 打开任意已有项目后，可从侧栏进入“项目记忆”。
- 首次加载显示最近 100 条非心跳事件，并显示准确的总数与阶段计数。
- 切换心跳后重新请求服务端，心跳出现在时间线中。
- 选择阶段或输入关键词后，仅过滤已加载的安全投影。
- 点击“加载更多”后追加旧事件，顺序保持从新到旧。
- `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --locked`、前端 typecheck、测试和生产构建通过。
