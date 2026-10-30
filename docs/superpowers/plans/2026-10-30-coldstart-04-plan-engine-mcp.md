# ColdStart 阶段 4：计划引擎与 MCP 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将已发布蓝图编译为不可变任务 DAG，提供租约、上下文、MCP 领取和结构化报告闭环，并在桌面端展示计划进度。

**Architecture:** 确定性计划编译器在 `coldstart-core` 生成计划领域对象，`coldstart-storage` 以 schema v4 保存不可变计划、工作项、租约和报告，`coldstart-app` 编排状态转换与上下文，独立 `coldstart-mcp` 通过 stdio 暴露七个工作工具，Tauri/React 只调用显式 IPC 命令。

**Tech Stack:** Rust 1.89、edition 2024、SQLite/rusqlite、Tauri 2、React 19、TypeScript、`rmcp 3.8`、Tokio、Vitest。

**Spec:** `docs/superpowers/specs/2026-10-30-coldstart-design.md`，重点阅读第 4、6、11-15、19、21-23、27 节。

## Global Constraints

- 文档使用中文；Rust、JSON、TypeScript 标识保留英文。
- 后端与全部文件、SQLite、租约和 MCP 能力使用 Rust；前端不得直接访问 SQLite、文件系统、Shell 或 Secret。
- Rust workspace 使用 edition 2024、`rust-version = "1.89"`；必须同时通过 stable 与 `cargo +1.89.0`。
- 计划模板固定为 `plan-v1`，上下文默认预算为 12,000 tokens，租约默认 30 分钟。
- 编码任务只能通过有效 MCP 租约提交完成报告；桌面 UI 只能完成人工任务。
- 租约令牌只保存 SHA-256 哈希；不得写入事件、日志、错误、报告或 UI。
- 阶段 4 不实现 Git/GitHub、真实凭据、部署发布、命令自动验证和已激活计划重规划。
- 依赖、缓存和构建产物保持在 D 盘；PowerShell 是默认终端。
- 不提交或推送 Git，除非用户随后明确要求。

---

## 文件结构

- `crates/core/src/entity_id.rs`：新增计划、工作项、租约、运行和报告 ID。
- `crates/core/src/plan.rs`：计划、任务、依赖、状态机、报告与校验。
- `crates/core/src/plan_compiler.rs`：从蓝图确定性生成 `plan-v1` 任务 DAG。
- `crates/core/src/context.rs`：上下文选择、预算和引用模型。
- `crates/core/src/event.rs`：计划与工作事件。
- `crates/storage/src/migrations.rs`：schema v4。
- `crates/storage/src/plan_store.rs`：计划、工作项、租约、运行与报告事务。
- `crates/app/src/plan.rs`：计划用例、领取、上下文、心跳、阻塞、报告和人工完成。
- `crates/app/src/plan_workspace.rs`：桌面计划 DTO。
- `crates/app/src/context_compiler.rs`：把蓝图、事实和任务契约编译为上下文包。
- `crates/mcp/src/*`：MCP stdio 服务和七个工具。
- `apps/desktop/src-tauri/src/commands.rs`：计划 IPC 命令。
- `web/src/components/PlanView.tsx`、`web/src/api/planApi.ts`：计划看板。
- `web/src/components/InboxView.tsx`：统一显示人工计划任务。

### Task 1: 计划领域模型、DAG 校验与报告校验

**Files:**
- Create: `crates/core/src/plan.rs`
- Create: `crates/core/src/context.rs`
- Modify: `crates/core/src/entity_id.rs`
- Modify: `crates/core/src/event.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/plan.rs`
- Test: `crates/core/tests/context.rs`

**Interfaces:**
- Consumes: `BlueprintEntityId`、`ProjectId`、`BlueprintRevisionId`、`sha2`。
- Produces: `PlanRevisionId`、`TaskSpecId`、`WorkItemId`、`LeaseId`、`WorkRunId`、`WorkReportId`；`TaskSpec`、`PlanContent`、`PlanRevision`、`WorkItemStatus`、`WorkReportInput`、`WorkReportRecord`、`TaskContext`、`TaskReference`；`validate_plan`、`TaskSpec::contract_hash`、`WorkItemStatus::can_transition_to`、`estimate_tokens`。

- [x] **Step 1: 写领域测试**

```rust
use coldstart_core::{
    DependencyKind, TaskDependency, TaskKind, TaskSpec, WorkItemStatus, validate_plan,
};

fn task(key: &str, dependency: Option<&str>) -> TaskSpec {
    TaskSpec {
        task_key: key.to_owned(),
        title: key.to_owned(),
        kind: TaskKind::AgentCoding,
        goal: "实现一个可验收切片".to_owned(),
        scope: vec!["当前模块".to_owned()],
        out_of_scope: vec!["其他模块".to_owned()],
        module_id: None,
        platform_id: None,
        repository_key: Some("server".to_owned()),
        base_ref: Some("main".to_owned()),
        work_branch: Some(format!("codex/{key}")),
        allowed_paths: vec!["src".to_owned()],
        context_refs: vec![],
        acceptance_criteria: vec!["测试通过".to_owned()],
        acceptance_policy: Default::default(),
        evidence_requirements: vec!["测试摘要".to_owned()],
        priority: 100,
        critical_path: true,
        context_budget_tokens: 12_000,
        resource_keys: vec!["server".to_owned()],
        dependencies: dependency
            .map(|key| TaskDependency {
                task_key: key.to_owned(),
                kind: DependencyKind::Blocks,
            })
            .into_iter()
            .collect(),
    }
}

#[test]
fn rejects_a_cycle_in_blocking_dependencies() {
    let error = validate_plan(&[task("a", Some("b")), task("b", Some("a"))])
        .expect_err("a blocks cycle must be rejected");
    assert!(error.to_string().contains("cycle"));
}

#[test]
fn contract_hash_is_stable_and_changes_with_the_contract() {
    let original = task("slice:module:server", None);
    let mut changed = original.clone();
    changed.allowed_paths.push("migrations".to_owned());

    assert_eq!(original.contract_hash(), original.contract_hash());
    assert_ne!(original.contract_hash(), changed.contract_hash());
}

#[test]
fn work_item_status_rejects_skipping_claim() {
    assert!(!WorkItemStatus::Ready.can_transition_to(WorkItemStatus::InProgress));
    assert!(WorkItemStatus::Ready.can_transition_to(WorkItemStatus::Claimed));
    assert!(WorkItemStatus::Claimed.can_transition_to(WorkItemStatus::InProgress));
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-core --test plan`
Expected: FAIL，因为 `TaskSpec`、`validate_plan` 等尚不存在。

- [x] **Step 3: 实现领域类型与校验**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatus {
    Draft,
    Ready,
    Claimed,
    InProgress,
    Submitted,
    Done,
    Blocked,
    Failed,
    Reopened,
    Cancelled,
}

impl WorkItemStatus {
    pub fn can_transition_to(self, next: Self) -> bool {
        use WorkItemStatus::*;
        matches!(
            (self, next),
            (Draft, Ready)
                | (Draft, Cancelled)
                | (Ready, Claimed)
                | (Ready, Blocked)
                | (Ready, Cancelled)
                | (Claimed, InProgress)
                | (Claimed, Ready)
                | (Claimed, Blocked)
                | (InProgress, Submitted)
                | (InProgress, Blocked)
                | (InProgress, Failed)
                | (InProgress, Reopened)
                | (Submitted, Done)
                | (Submitted, InProgress)
                | (Blocked, Ready)
                | (Blocked, Failed)
                | (Failed, Reopened)
                | (Done, Reopened)
                | (Reopened, Ready)
        )
    }
}
```

`validate_plan` 必须检查：任务键唯一且只含安全字符；所有依赖目标存在；禁止自依赖；忽略重复 `blocks` 后做拓扑排序；编码任务必须有逻辑仓库、分支、`main` 基线和非空验收项；`allowed_paths` 不得为绝对路径或包含 `..`；每个 `contract_hash` 对完整 `TaskSpec` 做 SHA-256。

- [x] **Step 4: 写上下文预算测试**

```rust
#[test]
fn token_estimate_counts_ascii_and_cjk_conservatively() {
    assert_eq!(estimate_tokens("abcd"), 1);
    assert_eq!(estimate_tokens("冷启动"), 3);
    assert_eq!(estimate_tokens("abc冷"), 2);
}
```

- [x] **Step 5: 实现上下文模型与事件**

```rust
pub const DEFAULT_CONTEXT_BUDGET_TOKENS: u32 = 12_000;

pub fn estimate_tokens(text: &str) -> u32 {
    let (ascii, non_ascii) = text.chars().fold((0u32, 0u32), |(a, n), c| {
        if c.is_ascii() { (a + 1, n) } else { (a, n + 1) }
    });
    ascii.div_ceil(4) + non_ascii
}
```

新增事件：`PlanDraftCreated`、`PlanPublished`、`WorkClaimed`、`WorkStarted`、`WorkHeartbeat`、`WorkBlocked`、`WorkReportSubmitted`、`WorkCompleted`、`WorkReleased`、`LeaseExpired`、`HumanWorkCompleted`。事件只含 ID、状态、时间和非敏感摘要。

- [x] **Step 6: 运行领域测试**

Run: `cargo test -p coldstart-core --test plan --test context`
Expected: PASS。

### Task 2: 确定性 `plan-v1` 计划编译器

**Files:**
- Create: `crates/core/src/plan_compiler.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/plan_compiler.rs`

**Interfaces:**
- Consumes: `BlueprintContent`、`PlatformKind`、`BlueprintEntityId`。
- Produces: `compile_plan(project_id, blueprint) -> Result<PlanContent, PlanValidationError>`、`repository_key_for_platform`、`PLAN_TEMPLATE_VERSION`。

- [x] **Step 1: 写编译器测试**

```rust
#[test]
fn creates_platform_foundations_module_slices_and_release_gates() {
    let content = compile_plan(&project_id, &blueprint).expect("plan must compile");
    let keys = content.tasks.iter().map(|task| task.task_key.as_str()).collect::<Vec<_>>();

    assert!(keys.iter().any(|key| key.starts_with("foundation:")));
    assert!(keys.iter().any(|key| key.starts_with("contracts:server")));
    assert!(keys.iter().any(|key| key.starts_with("slice:")));
    assert!(keys.iter().any(|key| key.starts_with("release:")));
    assert!(content.tasks.iter().filter(|task| task.kind == TaskKind::AgentCoding)
        .all(|task| task.acceptance_criteria.len() >= 2));
}

#[test]
fn client_slice_depends_on_the_matching_server_slice() {
    let content = compile_plan(&project_id, &blueprint).expect("plan must compile");
    let client = content.tasks.iter().find(|task| task.task_key.contains(":admin") && task.kind == TaskKind::AgentCoding && task.task_key.starts_with("slice:"))
        .expect("admin slice must exist");
    assert!(client.dependencies.iter().any(|dependency| dependency.task_key.contains("slice:") && dependency.task_key.contains(":server")));
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-core --test plan_compiler`
Expected: FAIL，因为编译器尚不存在。

- [x] **Step 3: 实现确定性任务生成**

任务键固定为：`foundation:<platform-id>`、`contracts:server`、`slice:<module-id>:<platform-id>`、`capability-prep:<capability-id>`、`capability-integration:<capability-id>`、`release-prereq:<prerequisite-id>`、`prep:<platform-id>:store-account`、`release:<platform-id>`。

生成规则：

```text
for each platform: foundation
if server exists: contracts:server depends on server foundation
for each module ordered by module DAG:
  for each declared platform:
    slice depends on platform foundation, contracts:server,
    same-platform dependency slices, and matching server slice
for each capability:
  capability-prep (human_external)
  capability-integration depends on prep + foundation
for each release prerequisite:
  release-prereq (human_external)
for iOS/Android/MiniProgram platform:
  prep:<platform>:store-account (human_external)
for each platform:
  release milestone depends on all platform slices, platform prep,
  capability integrations, and all release prerequisites
```

编码任务每个都包含至少两条具体验收标准：实现与错误路径；测试或构建摘要；修改路径限制；证据要求。所有跨任务引用只使用稳定 ID 和 `taskKey`。

- [x] **Step 4: 运行编译器测试并修正 DAG**

Run: `cargo test -p coldstart-core --test plan_compiler`
Expected: PASS，并且 `validate_plan` 无循环、无缺失引用。

- [x] **Step 5: 验证完整 core 测试**

Run: `cargo test -p coldstart-core`
Expected: 所有既有与新增测试 PASS。

### Task 3: Schema v4 与计划事务存储

**Files:**
- Modify: `crates/storage/src/migrations.rs`
- Create: `crates/storage/src/plan_store.rs`
- Modify: `crates/storage/src/database.rs`
- Modify: `crates/storage/src/lib.rs`
- Test: `crates/storage/tests/plan.rs`

**Interfaces:**
- Consumes: Task 1-2 的计划实体与 `PlanContent`。
- Produces: `Database::{create_plan_revision, load_plan, latest_plan, list_plans, publish_plan_revision, initialize_work_items, list_work_items, load_work_item, claim_work, heartbeat_lease, expire_leases, submit_report, complete_human_work, release_work}`。

- [x] **Step 1: 写迁移与计划持久化测试**

```rust
#[test]
fn migrates_v3_database_and_preserves_existing_blueprint() {
    let fixture = create_v3_fixture();
    let database = Database::open(&fixture.path).expect("database must migrate");
    assert_eq!(database.list_artifacts(&fixture.project_id).unwrap().len(), 1);
    let version: i64 = database.schema_version_for_test();
    assert_eq!(version, 4);
}

#[test]
fn published_plan_and_work_items_survive_reopen() {
    let fixture = create_project_with_published_blueprint();
    let revision = fixture.database.create_plan_revision(
        &fixture.project_id,
        fixture.blueprint_id,
        compile_plan(&fixture.project_id, &fixture.blueprint).unwrap(),
    ).unwrap();
    fixture.database.publish_plan_revision(&fixture.project_id, &revision.id).unwrap();
    let work_items = fixture.database.list_work_items(&fixture.project_id).unwrap();
    assert!(!work_items.is_empty());
    drop(fixture.database);

    let reopened = Database::open(&fixture.database_path).unwrap();
    assert_eq!(reopened.list_work_items(&fixture.project_id).unwrap(), work_items);
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-storage --test plan`
Expected: FAIL，因为 schema 仍为 v3 且存储方法不存在。

- [x] **Step 3: 创建 schema v4**

```sql
CREATE TABLE plan_revisions (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  revision_number INTEGER NOT NULL,
  blueprint_revision_id TEXT NOT NULL,
  blueprint_content_hash TEXT NOT NULL,
  template_version TEXT NOT NULL,
  status TEXT NOT NULL,
  content_json TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  created_at TEXT NOT NULL,
  published_at TEXT,
  UNIQUE(project_id, revision_number),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (blueprint_revision_id) REFERENCES blueprint_revisions(id)
);

CREATE TABLE task_specs (
  id TEXT PRIMARY KEY NOT NULL,
  plan_revision_id TEXT NOT NULL,
  task_key TEXT NOT NULL,
  contract_hash TEXT NOT NULL,
  kind TEXT NOT NULL,
  spec_json TEXT NOT NULL,
  UNIQUE(plan_revision_id, task_key),
  FOREIGN KEY (plan_revision_id) REFERENCES plan_revisions(id) ON DELETE CASCADE
);

CREATE TABLE work_items (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  plan_revision_id TEXT NOT NULL,
  task_spec_id TEXT NOT NULL,
  task_key TEXT NOT NULL,
  status TEXT NOT NULL,
  contract_state TEXT NOT NULL,
  priority INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  UNIQUE(project_id, plan_revision_id, task_key),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (plan_revision_id) REFERENCES plan_revisions(id) ON DELETE CASCADE,
  FOREIGN KEY (task_spec_id) REFERENCES task_specs(id) ON DELETE CASCADE
);

CREATE TABLE leases (
  id TEXT PRIMARY KEY NOT NULL,
  work_item_id TEXT NOT NULL,
  agent_profile TEXT NOT NULL,
  token_hash TEXT NOT NULL,
  status TEXT NOT NULL,
  issued_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  last_heartbeat_at TEXT NOT NULL,
  released_at TEXT,
  FOREIGN KEY (work_item_id) REFERENCES work_items(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX one_active_lease_per_work_item
  ON leases(work_item_id) WHERE status = 'active';

CREATE TABLE work_runs (
  id TEXT PRIMARY KEY NOT NULL,
  work_item_id TEXT NOT NULL,
  lease_id TEXT NOT NULL,
  agent_profile TEXT NOT NULL,
  claim_request_id TEXT NOT NULL,
  status TEXT NOT NULL,
  started_at TEXT NOT NULL,
  ended_at TEXT,
  UNIQUE(claim_request_id),
  FOREIGN KEY (work_item_id) REFERENCES work_items(id) ON DELETE CASCADE,
  FOREIGN KEY (lease_id) REFERENCES leases(id) ON DELETE CASCADE
);

CREATE TABLE work_reports (
  id TEXT PRIMARY KEY NOT NULL,
  work_item_id TEXT NOT NULL,
  work_run_id TEXT NOT NULL,
  contract_hash TEXT NOT NULL,
  idempotency_key_hash TEXT NOT NULL,
  report_json TEXT NOT NULL,
  report_hash TEXT NOT NULL,
  accepted INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(work_item_id, idempotency_key_hash),
  FOREIGN KEY (work_item_id) REFERENCES work_items(id) ON DELETE CASCADE,
  FOREIGN KEY (work_run_id) REFERENCES work_runs(id) ON DELETE CASCADE
);
```

迁移链必须支持 `0 -> 4`、`1 -> 4`、`2 -> 4`、`3 -> 4`，且使用 `BEGIN IMMEDIATE`。发布计划时在同一事务内把旧发布计划标为 `superseded`、初始化全部工作项、计算初始 `ready` 并写事件；阶段 4 若已有活动租约则返回 `plan_already_active`。

- [x] **Step 4: 实现存储方法与行映射**

`create_plan_revision` 验证计划结构、计算规范化 JSON 的 SHA-256，并写入 `task_specs`。`publish_plan_revision` 校验蓝图是已发布版本且哈希一致。`list_work_items` 返回按优先级、键排序的稳定结果。任何写操作失败都回滚整个事务。

- [x] **Step 5: 运行存储测试**

Run: `cargo test -p coldstart-storage --test plan --test database`
Expected: PASS，旧 schema 测试不回归。

### Task 4: 租约、工作状态与报告事务

**Files:**
- Modify: `crates/storage/src/plan_store.rs`
- Modify: `crates/storage/src/database.rs`
- Test: `crates/storage/tests/leases.rs`
- Test: `crates/storage/tests/reports.rs`

**Interfaces:**
- Consumes: Task 3 的 schema 与计划实体。
- Produces: `ClaimedWork { work_item, lease, lease_token, run }`、`ClaimFailure`、`ReportOutcome`；原子领取、心跳、过期、释放、报告幂等与人工完成。

- [x] **Step 1: 写并发领取与过期测试**

```rust
#[test]
fn only_one_of_two_concurrent_database_handles_can_claim_the_same_work() {
    let fixture = published_single_task_fixture();
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for index in 0..2 {
        let path = fixture.database_path.clone();
        let project_id = fixture.project_id;
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let mut database = Database::open(&path).unwrap();
            barrier.wait();
            database.claim_work(
                &project_id,
                "codex",
                Some("server"),
                &format!("request-{index}"),
                Utc::now(),
                Duration::minutes(30),
            ).is_ok()
        }));
    }
    barrier.wait();
    let successes = handles.into_iter().filter(|handle| handle.join().unwrap()).count();
    assert_eq!(successes, 1);
}

#[test]
fn expired_lease_returns_work_to_ready_without_completing_it() {
    let fixture = published_single_task_fixture();
    let claimed = fixture.database.claim_work(
        &fixture.project_id, "codex", Some("server"), "claim-1",
        Utc::now() - Duration::minutes(31), Duration::minutes(30),
    ).unwrap();
    let expired = fixture.database.expire_leases(&fixture.project_id, Utc::now()).unwrap();
    assert_eq!(expired.len(), 1);
    let item = fixture.database.load_work_item(&claimed.work_item.id).unwrap().unwrap();
    assert_eq!(item.status, WorkItemStatus::Ready);
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-storage --test leases`
Expected: FAIL，因为领取与租约方法尚不存在。

- [x] **Step 3: 实现事务性领取**

领取流程必须在 `BEGIN IMMEDIATE` 中完成：先惰性过期租约；对 `status IN ('draft','ready','reopened')` 的任务按 `priority DESC, task_key ASC` 扫描；校验所有 `blocks` 依赖为 `done`；校验同仓库资源锁无其他活动租约；生成 32 字节随机令牌、保存哈希、插入 `leases` 和 `work_runs`、任务转 `claimed`、写事件。无任务时返回 `all_done`、`waiting_dependencies`、`waiting_human`、`no_plan` 或 `no_matching_repository` 之一。

令牌生成使用 `uuid::Uuid::new_v4()` 两次拼接为 64 个十六进制字符，并只把 `sha256_hex` 写库。比较时重新哈希后做常量时间等价判断。

- [x] **Step 4: 写报告幂等与路径校验测试**

```rust
#[test]
fn accepts_a_complete_report_once_and_unlocks_the_next_task() {
    let fixture = two_task_chain_fixture();
    let claimed = fixture.claim_first();
    let report = fixture.report(&claimed, "report-1");
    let first = fixture.database.submit_report(&claimed.work_item.id, &claimed.lease_token, report.clone()).unwrap();
    let second = fixture.database.submit_report(&claimed.work_item.id, &claimed.lease_token, report).unwrap();
    assert_eq!(first.report_id, second.report_id);
    assert_eq!(fixture.status_of("foundation:one"), WorkItemStatus::Done);
    assert_eq!(fixture.status_of("slice:next"), WorkItemStatus::Ready);
}

#[test]
fn rejects_changed_files_outside_the_contract_paths() {
    let fixture = single_task_fixture_with_allowed_path("apps/server");
    let claimed = fixture.claim();
    let mut report = fixture.report(&claimed, "report-1");
    report.changed_files = vec!["C:/outside.txt".to_owned()];
    let error = fixture.database.submit_report(&claimed.work_item.id, &claimed.lease_token, report).unwrap_err();
    assert!(error.to_string().contains("outside allowed paths"));
}
```

- [x] **Step 5: 实现报告校验与幂等**

报告校验顺序为：租约有效、任务 `claim/in_progress`、契约哈希一致、幂等键非空、摘要非空、每条验收标准恰有结果、每个修改路径在允许路径内、每个证据引用非空且安全。接受后报告转 `accepted`，任务 `submitted -> done`，写 `WorkCompleted`，同事务重算后继任务。拒绝时写拒绝报告，任务回到 `in_progress` 并返回稳定错误；不删除有效工作。

- [x] **Step 6: 实现心跳、释放、阻塞与人工完成**

- 心跳必须匹配活动租约；把 `last_heartbeat_at` 更新到当前时间并把 `expires_at` 延长 30 分钟。
- `release_work` 将租约标为 `released`、运行标为 `released`、任务回到 `ready`。
- 阻塞报告把任务转 `blocked`、租约释放、写入阻塞原因并创建人工任务关联；阶段 4 的阻塞记录进入计划看板。
- 人工完成只允许 `human_external` 或 `approval_gate`，要求非空说明，接受后任务 `done` 并解锁后继，拒绝后任务保持 `blocked`。

- [x] **Step 7: 运行租约与报告测试**

Run: `cargo test -p coldstart-storage --test leases --test reports`
Expected: PASS；并发测试只成功一次领取。

### Task 5: 应用层计划服务与上下文编译

**Files:**
- Create: `crates/app/src/plan.rs`
- Create: `crates/app/src/plan_workspace.rs`
- Create: `crates/app/src/context_compiler.rs`
- Modify: `crates/app/src/workspace.rs`
- Modify: `crates/app/src/error.rs`
- Modify: `crates/app/src/lib.rs`
- Modify: `crates/app/src/requirements.rs`
- Test: `crates/app/tests/plan.rs`
- Test: `crates/app/tests/mcp_contract.rs`

**Interfaces:**
- Consumes: Task 2-4 的编译器与存储。
- Produces: `PlanService::{load_workspace, generate_draft, save_draft, publish, complete_human, claim_next, get_context, read_reference, heartbeat, report_blocker, submit_report, release}`；`PlanWorkspace` 和 `ContextPackage`。

- [x] **Step 1: 写应用闭环测试**

```rust
#[test]
fn publishes_a_plan_claims_context_and_completes_the_first_task() {
    let fixture = project_with_published_blueprint();
    let service = PlanService::new();
    let draft = service.generate_draft(fixture.home.clone()).unwrap();
    service.publish(fixture.home.clone(), draft.draft.unwrap().id).unwrap();

    let claimed = service.claim_next(fixture.home.clone(), ClaimRequest {
        agent_profile: "codex".to_owned(),
        repository: Some("server".to_owned()),
        request_id: "claim-1".to_owned(),
    }).unwrap();
    let context = service.get_context(fixture.home.clone(), claimed.work_item.id, claimed.lease_token.clone()).unwrap();
    assert!(context.context_markdown.contains("验收标准"));
    assert!(!context.context_markdown.contains("Bearer "));

    let report = report_for(&context.task, "report-1");
    service.submit_report(fixture.home.clone(), claimed.work_item.id, claimed.lease_token, report).unwrap();
    let workspace = service.load_workspace(fixture.home).unwrap();
    assert!(workspace.work_items.iter().any(|item| item.status == WorkItemStatus::Done));
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-app --test plan`
Expected: FAIL，因为 `PlanService` 尚不存在。

- [x] **Step 3: 实现计划工作区与命令编排**

`PlanWorkspace` 包含 `draft`、`published`、`work_items`、`active_leases`、`runs`、`reports`、`blockers` 和 `mcp` 启动信息。`ProjectWorkspace` 增加 `plan: PlanWorkspace`，需求页面的所有动作返回同一完整快照。

`PlanService::generate_draft` 只允许存在已发布蓝图；`save_draft` 重新校验；`publish` 委托存储事务。人工完成、阻塞报告和报告提交都在服务层把稳定 `AppError` 映射到 UI。

错误码至少新增：`published_blueprint_required`、`invalid_plan`、`plan_not_found`、`plan_not_draft`、`plan_already_active`、`no_claimable_work`、`work_not_found`、`work_not_claimable`、`lease_invalid`、`lease_expired`、`contract_stale`、`report_invalid`、`report_idempotency_conflict`、`human_work_required`。

- [x] **Step 4: 实现上下文编译器**

上下文按固定顺序拼接：任务契约、验收与证据、相关模块和平台蓝图、技术选择、项目事实、能力准备状态、依赖完成摘要、人工门禁、引用目录、省略清单。每段加入后重新估算 tokens；超过预算时先移除低优先级细节，再截断长文本并记录 `omitted`，绝不截断租赁令牌或用户提供的 Secret（后者不会进入上下文）。

`read_reference` 只接受任务 `context_refs`、依赖任务键或已发布蓝图中确实存在的实体 ID。未知 ID 返回 `reference_not_found`；路径型引用在阶段 4 不开放。

- [x] **Step 5: 写上下文预算与引用测试**

```rust
#[test]
fn context_respects_budget_and_lists_omissions() {
    let fixture = oversized_context_fixture();
    let package = fixture.context_with_budget(80);
    assert!(package.estimated_tokens <= 80);
    assert!(!package.omitted.is_empty());
    assert!(package.markdown.contains("省略"));
}

#[test]
fn refuses_a_reference_not_whitelisted_by_the_task() {
    let fixture = published_context_fixture();
    let error = fixture.service.read_reference(
        fixture.home, fixture.work_id, fixture.lease_token, "module_unknown".to_owned(),
    ).unwrap_err();
    assert_eq!(error.code(), "reference_not_found");
}
```

- [x] **Step 6: 运行应用测试**

Run: `cargo test -p coldstart-app --test plan --test mcp_contract --test requirements`
Expected: PASS。

### Task 6: MCP stdio 服务与七个工具

**Files:**
- Create: `crates/mcp/Cargo.toml`
- Create: `crates/mcp/src/lib.rs`
- Create: `crates/mcp/src/main.rs`
- Create: `crates/mcp/src/server.rs`
- Create: `crates/mcp/src/types.rs`
- Create: `crates/mcp/src/error.rs`
- Modify: `Cargo.toml`
- Test: `crates/mcp/tests/stdio.rs`

**Interfaces:**
- Consumes: `coldstart-app::PlanService` 公共方法。
- Produces: 二进制 `coldstart-mcp --project-home <path>`；MCP 工具 `claim_next_work`、`get_work_context`、`read_task_reference`、`heartbeat_work`、`report_work_blocker`、`submit_work_report`、`release_work`。

- [x] **Step 1: 添加 workspace 成员与测试驱动**

```toml
rmcp = { version = "3.8.0", features = ["server", "macros", "transport-io"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "io-std", "io-util", "sync", "time"] }
```

测试必须启动真实子进程，通过 stdin 写 MCP JSON-RPC，通过 stdout 读取响应，不使用内部函数模拟协议。

- [x] **Step 2: 写真实 stdio 契约测试**

```rust
#[test]
fn stdio_server_lists_all_work_tools_and_rejects_an_unknown_tool() {
    let project = published_plan_project();
    let mut child = Command::new(env!("CARGO_BIN_EXE_coldstart-mcp"))
        .args(["--project-home", project.path.to_str().unwrap()])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    let mut client = McpStdioClient::new(&mut child);
    client.initialize();
    let tools = client.list_tools();
    let names = tools.iter().map(|tool| tool["name"].as_str().unwrap()).collect::<Vec<_>>();
    assert_eq!(names, vec![
        "claim_next_work", "get_work_context", "read_task_reference", "heartbeat_work",
        "report_work_blocker", "submit_work_report", "release_work",
    ]);
    let error = client.call_tool_error("delete_repository", json!({}));
    assert!(error.message.contains("unknown") || error.message.contains("not found"));
}
```

- [x] **Step 3: 运行测试确认失败**

Run: `cargo test -p coldstart-mcp --test stdio`
Expected: FAIL，因为 crate 与协议尚不存在。

- [x] **Step 4: 实现参数校验和工具 schema**

工具入参使用 snake_case。`claim_next_work` 需要 `agent_profile`、可选 `repository`、`request_id`；上下文、心跳、引用、阻塞、提交和释放都需要 `work_id` 与 `lease_token`。报告 schema 与 `WorkReportInput` 完全一致，`idempotency_key` 必填。

任何工具响应不得包含 `token_hash`、数据库路径、绝对项目路径以外的机器信息或 Secret。错误返回稳定的 MCP tool error content，包含 `code`、`message` 和可选 `details`。

- [x] **Step 5: 实现服务生命周期**

`main` 解析唯一 `--project-home`，验证项目和蓝图状态，初始化 Tokio 与 stdio transport。MCP 进程按需运行，不启动 Tauri、不启动 Codex、不写 stdout 日志。所有诊断写 stderr，且脱敏。

- [x] **Step 6: 运行 MCP 测试**

Run: `cargo test -p coldstart-mcp`
Expected: PASS；七个工具、错误码和 stdio 握手均通过。

### Task 7: Tauri 命令、计划看板与统一 Inbox

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `web/src/api/types.ts`
- Create: `web/src/api/planApi.ts`
- Create: `web/src/components/PlanView.tsx`
- Modify: `web/src/components/AppSidebar.tsx`
- Modify: `web/src/components/InboxView.tsx`
- Modify: `web/src/components/ProjectOverview.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/styles.css`
- Test: `web/src/App.test.tsx`

**Interfaces:**
- Consumes: Task 5 的 `PlanWorkspace` 与 IPC 服务。
- Produces: `read_plan_workspace`、`generate_plan_draft`、`save_plan_draft`、`publish_plan`、`complete_human_work` Tauri 命令；`PlanView` 和计划 API。

- [x] **Step 1: 写前端计划流程测试**

```tsx
it('generates and publishes a plan, then displays ready work', async () => {
  const user = await openProject(makeWorkspace({ published: makeRevision() }));
  const draft = makePlanWorkspace({ draft: makePlanRevision({ status: 'draft' }) });
  vi.mocked(planApi.generatePlanDraft).mockResolvedValue(withPlan(makeWorkspace({ published: makeRevision() }), draft));
  vi.mocked(planApi.publishPlan).mockResolvedValue(withPlan(makeWorkspace({ published: makeRevision() }), makePlanWorkspace({
    published: makePlanRevision({ status: 'published' }),
    workItems: [makeWorkItem({ status: 'ready' })],
  })));

  await user.click(screen.getByRole('button', { name: '计划看板' }));
  await user.click(screen.getByRole('button', { name: '编译开发计划' }));
  expect(await screen.findByText('计划草稿 v1')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '发布计划' }));
  expect(await screen.findByText('可领取')).toBeInTheDocument();
  expect(planApi.publishPlan).toHaveBeenCalledWith(projectPath, draft.draft!.id);
});

it('completes a human task from inbox and unlocks the plan', async () => {
  const user = await openProject(makeWorkspace({ plan: makePlanWorkspace({ workItems: [makeHumanWorkItem()] }) }));
  vi.mocked(planApi.completeHumanWork).mockResolvedValue(withPlan(makeWorkspace(), makePlanWorkspace({
    workItems: [makeHumanWorkItem({ status: 'done' })],
  })));

  await user.click(screen.getByRole('button', { name: '人工 Inbox' }));
  await user.type(screen.getByLabelText('完成说明：准备应用商店账号'), '账号已可用');
  await user.click(screen.getByRole('button', { name: '确认完成：准备应用商店账号' }));
  expect(planApi.completeHumanWork).toHaveBeenCalledWith(projectPath, humanWorkId, '账号已可用', true);
});
```

- [x] **Step 2: 运行前端测试确认失败**

Run: `pnpm --dir web test -- App.test.tsx`
Expected: FAIL，因为计划视图与 API 尚不存在。

- [x] **Step 3: 实现 Tauri 命令与 DTO**

所有计划命令沿用 `spawn_blocking`，参数为显式结构体，不暴露数据库句柄。`read_plan_workspace` 可直接复用完整 `ProjectWorkspace`；写命令返回完整 `ProjectWorkspace`，保证界面快照一致。

- [x] **Step 4: 实现计划看板**

页面包含：编译、编辑、发布操作；修订元数据；按状态分组的任务；每个任务的类型、模块、平台、仓库、依赖、验收标准、租约和最近报告；DAG 依赖文本；MCP 配置复制按钮；人工任务操作。草稿编辑只开放标题、目标、范围、明确不做、优先级、关键路径、验收标准、证据要求和 `blocks` 依赖，保存前重新编译校验。

MCP 配置使用可复制文本：

```json
{
  "mcpServers": {
    "coldstart": {
      "command": "D:\\coldstart\\target\\release\\coldstart-mcp.exe",
      "args": ["--project-home", "<项目绝对路径>"]
    }
  }
}
```

- [x] **Step 5: 实现统一 Inbox 与轮询**

`InboxView` 在信息请求之外增加“计划人工任务”分组，显示任务目标、原因、验收标准、状态和完成说明输入。计划页面打开时每 3 秒刷新一次，窗口重新获得焦点时立即刷新；失败刷新保留最后一次成功快照并显示错误。

- [x] **Step 6: 运行前端与 Tauri 检查**

Run: `pnpm --dir web typecheck`
Run: `pnpm --dir web test`
Run: `pnpm --dir web build`
Run: `pnpm --dir apps/desktop tauri build --no-bundle`
Expected: 全部退出码 0。

### Task 8: 文档、版本标识与最终质量门禁

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-10-30-coldstart-delivery-roadmap.md`
- Modify: `web/src/App.tsx`
- Modify: `web/src/components/WelcomeView.tsx`
- Modify: `web/src/components/ProjectOverview.tsx`
- Test: `crates/core/tests/plan.rs`
- Test: `crates/core/tests/plan_compiler.rs`
- Test: `crates/storage/tests/plan.rs`
- Test: `crates/storage/tests/leases.rs`
- Test: `crates/storage/tests/reports.rs`
- Test: `crates/app/tests/plan.rs`
- Test: `crates/mcp/tests/stdio.rs`
- Test: `web/src/App.test.tsx`

**Interfaces:**
- Consumes: Task 1-7 全部交付。
- Produces: 阶段 4 可演示文档、校准的阶段标识和最终门禁证据。

- [x] **Step 1: 更新中文文档与阶段标识**

README 增加计划编译、计划看板、MCP 配置、七个工具、租约规则和阶段 4 边界。路线图把阶段 4 标为已完成、阶段 5 标为下一步。欢迎页与顶栏标识改为 `PHASE 04 / PLAN + MCP`。

- [x] **Step 2: 运行格式化**

Run: `cargo fmt --all`
Expected: 退出码 0。

- [x] **Step 3: 运行全部 Rust 门禁**

Run: `cargo fmt --all -- --check`
Run: `cargo clippy --workspace --all-targets --locked -- -D warnings`
Run: `cargo test --workspace --locked`
Run: `cargo +1.89.0 test --workspace --locked`
Expected: 四条命令退出码均为 0，测试数量在最终输出中记录。

- [x] **Step 4: 运行全部前端门禁**

Run: `pnpm --dir web typecheck`
Run: `pnpm --dir web test`
Run: `pnpm --dir web build`
Run: `pnpm --dir apps/desktop tauri build --no-bundle`
Expected: 全部退出码 0。

- [x] **Step 5: 运行真实桌面与 MCP 闭环验收**

1. 使用现有已发布蓝图项目打开 release 桌面二进制。
2. 在计划看板编译并发布计划，确认任务数、依赖和人工任务真实可见。
3. 用 release `coldstart-mcp.exe` 跑初始化、`tools/list`、领取、上下文、心跳、提交报告；确认任务变为已完成且后继任务解锁。
4. 并发启动两个领取请求，确认只有一个成功。
5. 重启桌面应用，确认计划、任务、报告和人工状态持久化。
6. 在 Inbox 完成一个人工任务，确认后继任务解锁。
7. 搜索事件、日志、MCP 响应、SQLite 文本列，确认不存在租约原始令牌或测试 Secret。

将截图与验收记录保存到 `.superpowers/sdd/2026-10-30-coldstart-04-plan-engine-mcp/`，该目录被本地 Git exclude 忽略。

- [x] **Step 6: 生成交付摘要**

记录 release 二进制大小、SHA-256、Rust/前端测试数量、MCP 七工具契约结果、桌面验收截图路径和本阶段明确遗留项。若任一命令失败，不宣称阶段完成。

## 计划自检

- 规格覆盖：第 11-15 节由 Task 1-6 实现；第 19 节由 Task 7 实现；第 21-23 节由 Task 4、5、8 实现；第 27 节全部映射到 Task 1-8。
- 占位符检查：所有任务均列出文件、接口、测试代码、运行命令和预期结果；没有 TODO、TBD 或“稍后补充”。
- 类型一致性：`PlanContent`、`PlanRevision`、`TaskSpec`、`WorkItem`、`Lease`、`WorkReportInput` 在 Task 1 定义，后续任务使用同名类型；`PlanService` 方法在 Task 5 固定，Task 6-7 只调用这些方法。
