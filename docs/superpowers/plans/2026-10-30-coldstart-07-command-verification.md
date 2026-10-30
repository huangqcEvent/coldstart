# ColdStart 阶段 7：本地命令验证与完成门禁实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Codex 的结构化报告只能触发 ColdStart 独立执行仓库验证命令，全部通过后才允许任务进入 `done`，并在失败时保留租约供 Codex 修复后重试。

**Architecture:** `coldstart-core` 定义仓库验证命令、规范快照、验证结果和稳定失败类型；`coldstart-storage` 以 SQLite v6 保存命令、领取快照和验证状态，并把“开始验证/成功完成/失败回退/中断恢复”拆成独立事务；`coldstart-adapters` 提供无 Shell 的 Windows 进程执行器、Job Object 进程树终止、输出脱敏和 Git 不变量检查；`coldstart-app` 编排报告、Git 与命令执行；Tauri/React 提供命令编辑与验证摘要；MCP 工具列表不变，但返回快照和验证结果。

**Tech Stack:** Rust 1.89、edition 2024、SQLite/rusqlite、Windows Job Object、Tauri 2、React 19、TypeScript、Vitest。

**Spec:** `docs/superpowers/specs/2026-10-30-coldstart-design.md`，重点阅读第 30 节。

## Global Constraints

- 目标平台固定为 Web/管理后台 + App + 小程序；逻辑仓库键固定为 `server`、`admin`、`web`、`app`、`mini_program`。
- Codex 继续通过 MCP 主动领取；ColdStart 不启动 Codex，不增加 MCP 命令配置工具。
- 每个 `agent_coding` 任务领取前必须有仓库验证命令；无配置返回 `verification_not_configured`，且不得创建租约或改变任务状态。
- 验证配置属于仓库；领取时快照写入 `work_runs`，之后修改配置只影响下一次领取。
- 每条命令名 1-80 字符且仓库内唯一；最多 8 条；单条超时 10-900 秒；总超时不超过 3600 秒；单个参数不超过 2048 字符，参数总量不超过 16 KiB。
- 只执行绝对路径 `.exe` 文件，参数数组独立传递；禁止 `.cmd`、`.bat`、`.ps1`、Shell 字符串、管道、重定向和变量展开。工作目录固定为仓库根目录。
- stdin 为空；stdout/stderr 并发读取；每流最多保留 32 KiB，合并最多 64 KiB 尾部；输出去 ANSI/控制字符并尽力脱敏，原始输出不持久化。
- 环境清空后只注入规格第 30.5 节白名单；不得继承 Token、Secret、Password、API Key、`NODE_OPTIONS` 或 Credential Manager 内容。
- 报告执行顺序固定为：结构/Git 校验 -> 延长租约 -> `submitted` + `running` 验证记录 -> 顺序执行 -> 全部通过才 `done`。
- 非零退出、启动失败、超时、已跟踪文件变化、切换分支或改写 `HEAD` 均失败；失败不消费租约，任务回到 `in_progress`，必须用新幂等键重试。
- 租约过期或进程崩溃时，运行中验证改为 `interrupted`，任务回 `ready`，旧报告不得完成。
- 只有任务成功进入 `done` 后才执行现有远程推送；推送失败只写 Git 同步失败，不回写任务状态。
- SQLite 架构升级到 6；文档中文，代码标识英文；Secret 不进入 SQLite、日志、错误、任务上下文或 MCP。
- 所有 PowerShell 命令先执行 `. 'D:\DevTools\coldstart-env.ps1'`；依赖、构建和安装优先在 D 盘。

## File Structure

- `crates/core/src/verification.rs`：验证命令、限制、路径规范化、快照和哈希。
- `crates/core/src/plan.rs`：`WorkRun`、`WorkReportRecord`、`ClaimFailure` 的验证字段。
- `crates/core/src/context.rs`：任务上下文验证快照。
- `crates/core/src/event.rs`：验证开始、通过、失败、中断和命令配置事件。
- `crates/storage/src/migrations.rs`：v6 表、列、索引。
- `crates/storage/src/verification_store.rs`：命令 CRUD、领取快照、验证状态事务。
- `crates/storage/src/plan_store.rs`：严格领取和租约过期中断。
- `crates/adapters/src/verification.rs`：Windows 执行器、Job Object、输出脱敏。
- `crates/adapters/src/git.rs`：验证期间的分支/HEAD/已跟踪文件不变量。
- `crates/adapters/src/bin/coldstart-verification-probe.rs`：真实 `.exe` 测试探针。
- `crates/app/src/plan.rs`：报告与验证编排、失败回退、成功后推送。
- `crates/app/src/integration.rs`：仓库验证命令读写服务。
- `crates/app/src/error.rs`：六个稳定验证错误码和详情。
- `crates/mcp/src/error.rs`、`crates/mcp/src/server.rs`：结构化错误与工具结果。
- `apps/desktop/src-tauri/src/commands.rs`、`lib.rs`：明确验证命令 IPC。
- `web/src/components/RepositoryVerificationEditor.tsx`：新增/编辑/删除/排序。
- `web/src/components/IntegrationView.tsx`、`PlanView.tsx`：不可领取警告与验证摘要。
- `crates/app/tests/mvp_acceptance.rs`：真实 Git + 真实 `.exe` 的阶段闭环。
- `README.md`、规格、路线图、阶段台账：阶段 7 状态与限制。

---

### Task 1: 验证领域模型、校验与快照

**Files:**
- Create: `crates/core/src/verification.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/src/plan.rs`
- Modify: `crates/core/src/context.rs`
- Modify: `crates/core/src/event.rs`
- Test: `crates/core/src/verification.rs`

**Interfaces:**
- Produces `RepositoryVerificationCommand { name: String, executable: PathBuf, args: Vec<String>, timeout_seconds: u32, position: u32 }`。
- Produces `VerificationSnapshot { schema_version: u32, repository_key: String, commands: Vec<RepositoryVerificationCommand>, total_timeout_seconds: u32 }`。
- Produces `VerificationSnapshot::new(...) -> Result<Self, VerificationCommandError>`、`content_hash() -> String`、`canonical_json() -> Result<Vec<u8>, serde_json::Error>`。
- Produces `normalize_verification_commands(Vec<RepositoryVerificationCommand>) -> Result<Vec<RepositoryVerificationCommand>, VerificationCommandError>`。
- Produces `VerificationStatus`、`VerificationFailureKind`、`VerificationCommandResult`、`VerificationResult`。
- Adds `WorkRun.verification_snapshot: Option<VerificationSnapshot>`、`WorkRun.verification_snapshot_hash: Option<String>`。
- Adds `WorkReportRecord.verification_status: VerificationStatus`、`WorkReportRecord.verification: Option<VerificationResult>`。
- Adds `TaskContext.verification: Option<VerificationSnapshot>`、`ClaimFailure::VerificationNotConfigured`。

- [x] **Step 1: 写失败测试**

```rust
#[test]
fn validates_command_names_paths_arguments_timeouts_and_totals() {
    let exe = std::env::current_exe().expect("test exe");
    let valid = RepositoryVerificationCommand {
        name: "cargo-test".to_owned(),
        executable: exe.clone(),
        args: vec!["test".to_owned(), "--workspace".to_owned()],
        timeout_seconds: 600,
        position: 0,
    };
    assert_eq!(normalize_verification_commands(vec![valid.clone()]).expect("valid"), vec![valid]);
    let mut duplicate = valid.clone();
    duplicate.position = 1;
    assert!(matches!(normalize_verification_commands(vec![valid.clone(), duplicate]), Err(VerificationCommandError::DuplicateName(name)) if name == "cargo-test"));
    assert!(matches!(normalize_verification_commands(vec![RepositoryVerificationCommand { timeout_seconds: 9, ..valid.clone() }]), Err(VerificationCommandError::TimeoutOutOfRange { .. })));
    assert!(matches!(normalize_verification_commands(vec![RepositoryVerificationCommand { executable: "cargo.exe".into(), ..valid.clone() }]), Err(VerificationCommandError::ExecutableNotAbsolute(_))));
    assert!(matches!(normalize_verification_commands(vec![RepositoryVerificationCommand { args: vec!["x".repeat(2049)], ..valid }]), Err(VerificationCommandError::ArgumentTooLong { .. })));
}

#[test]
fn snapshot_hash_is_stable_and_covers_command_order() {
    let exe = std::env::current_exe().expect("test exe");
    let one = RepositoryVerificationCommand { name: "a".into(), executable: exe.clone(), args: vec![], timeout_seconds: 10, position: 0 };
    let two = RepositoryVerificationCommand { name: "b".into(), executable: exe.clone(), args: vec![], timeout_seconds: 20, position: 1 };
    let first = VerificationSnapshot::new("server", vec![one.clone(), two.clone()]).expect("snapshot");
    let same = VerificationSnapshot::new("server", vec![one.clone(), two.clone()]).expect("snapshot");
    let reversed = VerificationSnapshot::new("server", vec![RepositoryVerificationCommand { position: 0, ..two }, RepositoryVerificationCommand { position: 1, ..one }]).expect("snapshot");
    assert_eq!(first.content_hash(), same.content_hash());
    assert_ne!(first.content_hash(), reversed.content_hash());
    assert_eq!(first.total_timeout_seconds, 30);
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-core verification --locked`
Expected: FAIL，`RepositoryVerificationCommand` 和 `VerificationSnapshot` 尚不存在。

- [x] **Step 3: 实现最小领域模型**

实现所有常量、错误枚举、`normalize_verification_commands` 和 `VerificationSnapshot`。名称先 `trim` 再要求等于原值；路径用 `dunce::canonicalize`，要求绝对、普通文件、扩展名为不区分大小写的 `.exe`；位置必须为连续 `0..n`；快照至少一条命令。规范 JSON 使用结构体字段顺序与 `serde_json::to_vec`，哈希为 SHA-256。给 `WorkRun`、`WorkReportRecord`、`TaskContext` 和 `ClaimFailure` 增加上述字段，并补齐所有构造点。

- [x] **Step 4: 运行测试确认通过**

Run: `cargo test -p coldstart-core --locked`
Expected: PASS，现有领域测试与新增测试全部通过。

---

### Task 2: SQLite v6、仓库命令与严格领取快照

**Files:**
- Modify: `crates/storage/src/migrations.rs`
- Create: `crates/storage/src/verification_store.rs`
- Modify: `crates/storage/src/lib.rs`
- Modify: `crates/storage/src/database.rs`
- Modify: `crates/storage/src/integration_store.rs`
- Modify: `crates/storage/src/plan_store.rs`
- Modify: `crates/storage/tests/support/mod.rs`
- Create: `crates/storage/tests/verification.rs`
- Modify: `crates/storage/tests/database.rs`
- Modify: `crates/storage/tests/requirements.rs`
- Modify: `crates/storage/tests/plan.rs`

**Interfaces:**
- Produces `Database::replace_repository_verification_commands(&mut self, &ProjectId, &str, Vec<RepositoryVerificationCommand>) -> Result<Vec<RepositoryVerificationCommand>, StorageError>`。
- Produces `Database::list_repository_verification_commands(&self, &ProjectId, &str) -> Result<Vec<RepositoryVerificationCommand>, StorageError>`。
- `Database::claim_work` 在同一事务内读取候选仓库命令，创建快照并写入 `work_runs.verification_snapshot_json/hash`；无命令时返回 `ClaimResult::Empty(ClaimFailure::VerificationNotConfigured)`。
- 新增 `StorageError::{RepositoryNotFound, VerificationNotConfigured, VerificationInProgress, VerificationCommandInvalid}`。

- [x] **Step 1: 写迁移和命令 CRUD 失败测试**

```rust
#[test]
fn migrates_to_v6_and_replaces_ordered_commands() {
    let fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    assert_eq!(fixture.database.schema_version().expect("version"), 6);
    let exe = std::env::current_exe().expect("exe");
    let commands = vec![
        RepositoryVerificationCommand { name: "test".into(), executable: exe.clone(), args: vec!["test".into()], timeout_seconds: 600, position: 0 },
        RepositoryVerificationCommand { name: "build".into(), executable: exe, args: vec![], timeout_seconds: 300, position: 1 },
    ];
    let saved = fixture.database.replace_repository_verification_commands(&fixture.project_id, "server", commands).expect("save");
    assert_eq!(saved.len(), 2);
    assert_eq!(fixture.database.list_repository_verification_commands(&fixture.project_id, "server").expect("list"), saved);
}
```

- [x] **Step 2: 写严格领取失败测试**

```rust
#[test]
fn claim_without_verification_commands_creates_no_lease_or_run() {
    let mut fixture = PlanFixture::without_verification(vec![agent_task("foundation:demo", &[])]);
    let result = fixture.database.claim_work(&fixture.project_id, "codex", Some("server"), "no-verification", Utc::now(), Duration::minutes(30)).expect("claim");
    assert_eq!(result, ClaimResult::Empty(ClaimFailure::VerificationNotConfigured));
    assert_eq!(fixture.database.list_work_runs(&fixture.project_id).expect("runs").len(), 0);
    assert_eq!(fixture.database.list_leases(&fixture.project_id).expect("leases").len(), 0);
}
```

- [x] **Step 3: 写快照固化和原子回滚测试**

领取后断言 `WorkRun.verification_snapshot` 等于领取时命令，哈希匹配，任务由 `ready` 变 `claimed`。再写一个使用损坏仓库外键或重复位置的配置测试，断言命令与租约均未部分写入。

- [x] **Step 4: 运行测试确认失败**

Run: `cargo test -p coldstart-storage --test verification --locked`
Expected: FAIL，v6 表、命令 API 和严格领取尚不存在。

- [x] **Step 5: 实现迁移和存储**

`SCHEMA_VERSION=6`；`create_schema_v6` 创建 `repository_verification_commands`，扩展两个运行/报告表并创建“每个工作项最多一条 running”部分唯一索引。更新所有旧版本迁移链和测试版本断言。CRUD 在事务中先校验仓库归属与全部命令，再删除旧集合并插入新集合，写入 `repository_verification_commands_updated` 事件。`claim_work` 在选择候选后构造 `VerificationSnapshot`；任何失败都回滚。

- [x] **Step 6: 更新测试夹具并运行存储测试**

`PlanFixture` 默认创建临时仓库映射和一条使用 `std::env::current_exe()` 的验证命令；`without_verification` 清空命令。Run: `cargo test -p coldstart-storage --locked`
Expected: PASS。

---

### Task 3: 验证报告状态机、失败回退与中断恢复

**Files:**
- Modify: `crates/storage/src/verification_store.rs`
- Modify: `crates/storage/src/plan_store.rs`
- Modify: `crates/storage/src/database.rs`
- Modify: `crates/storage/tests/verification.rs`
- Modify: `crates/storage/tests/reports.rs`

**Interfaces:**
- Produces `BeginVerification::{Started(PendingVerification), AlreadyResolved(WorkReportRecord)}`。
- Produces `PendingVerification { report: WorkReportRecord, snapshot: VerificationSnapshot }`。
- Produces `begin_work_verification(...) -> Result<BeginVerification, StorageError>`。
- Produces `finish_work_verification_success(..., verification: VerificationResult, now) -> Result<WorkReportRecord, StorageError>`。
- Produces `finish_work_verification_failure(..., verification: VerificationResult, now, default_lease_duration) -> Result<WorkReportRecord, StorageError>`。

- [x] **Step 1: 写成功、失败和幂等失败测试**

测试必须断言：开始验证后任务为 `submitted`、租约延长到总预算 + 5 分钟、报告状态 `running`；成功写 `passed`、消费租约、任务 `done`；非零失败写 `failed`、报告拒绝、任务 `in_progress`、租约仍有效；同幂等键运行中返回 `VerificationInProgress`，完成后返回原记录；失败后同键不重跑，新键可再次开始。

- [x] **Step 2: 写并发唯一性和过期中断失败测试**

用手工 SQL 尝试插入第二条 `running` 报告，断言部分唯一索引拒绝。过期测试把租约时间拨到未来后调用 `expire_leases`，断言报告 `interrupted`、运行 `expired`、任务 `ready`、旧报告 `accepted=false`。

- [x] **Step 3: 运行测试确认失败**

Run: `cargo test -p coldstart-storage --test verification verification_ --locked`
Expected: FAIL，开始/完成 API 和中断逻辑尚不存在。

- [x] **Step 4: 实现事务拆分**

保留结构拒绝路径；结构通过后不再直接完成。`begin_work_verification` 先查幂等记录，再授权并校验报告，读取运行快照，把报告以 `accepted=false`、`verification_status=running` 插入，更新任务 `submitted` 和运行 `submitted`，写开始事件。成功/失败事务分别完成或回退；失败把租约延长默认时长且运行恢复 `running`。`expire_active_leases` 必须先把关联 `running` 报告改 `interrupted` 再改任务状态。

- [x] **Step 5: 运行全部存储测试**

Run: `cargo test -p coldstart-storage --locked`
Expected: PASS。

---

### Task 4: Windows 进程执行器、输出边界与 Git 不变量

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/adapters/Cargo.toml`
- Create: `crates/adapters/src/verification.rs`
- Create: `crates/adapters/src/bin/coldstart-verification-probe.rs`
- Modify: `crates/adapters/src/lib.rs`
- Modify: `crates/adapters/src/git.rs`
- Create: `crates/adapters/tests/verification.rs`
- Modify: `crates/adapters/tests/git.rs`

**Interfaces:**
- Produces `VerificationCommandExecutor: Send + Sync`，方法 `execute(&RepositoryVerificationCommand, &Path) -> VerificationCommandExecution`。
- Produces `WindowsVerificationExecutor`。
- `VerificationCommandExecution { exit_code: Option<i32>, duration_ms: u64, output_summary: String, truncated: bool, timed_out: bool, launch_error: Option<String>, infrastructure_error: Option<String> }`。
- Produces `sanitize_verification_output(&str) -> String`。
- `GitAdapter::ensure_verification_checkout(&Path, branch: &str, commit_sha: &str) -> Result<(), GitError>` 校验分支、HEAD 和 `git status --porcelain=v1 --untracked-files=no`。

- [x] **Step 1: 写真实探针与失败测试**

探针支持 `success`、`fail`、`sleep`、`spawn-child <pid-file>`、`large-output`、`secret-output`、`print-env`。适配器测试使用 `env!("CARGO_BIN_EXE_coldstart-verification-probe")`，覆盖退出码、超时、子进程树终止、每流/合并截断、ANSI 与 Secret 脱敏、白名单存在且 `NODE_OPTIONS/GITHUB_TOKEN` 不存在。

```rust
#[test]
fn timeout_terminates_the_child_tree() {
    let dir = tempfile::tempdir().expect("temp dir");
    let pid_file = dir.path().join("child.pid");
    let command = probe_command("spawn-child", vec![pid_file.to_string_lossy().into_owned()], 1);
    let result = WindowsVerificationExecutor.execute(&command, dir.path());
    assert!(result.timed_out);
    assert!(!process_is_alive(wait_for_pid(&pid_file)));
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-adapters --test verification --locked`
Expected: FAIL，执行器与探针尚不存在。

- [x] **Step 3: 实现 Windows 执行器**

使用 `std::process::Command`，`env_clear`、白名单、相对/绝对 `.exe` 直接启动、`current_dir`、`stdin(Stdio::null())`。创建 `windows-sys` Job Object，设置 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`，启动后立即分配；Job 创建/分配失败时终止并回收子进程，返回 infrastructure 错误。主线程轮询 `try_wait`；超时调用 `TerminateJobObject` 并等待回收；两个读取线程并发读取，各自保留 32 KiB 尾部并记录截断。拼接后保留 64 KiB 尾部，先移除 ANSI/CSI/OSC 和控制字符，再用 `regex` 脱敏 Bearer、Token、Password、Secret、API Key 和常见 Token 前缀。原始缓冲只活在执行器内。

- [x] **Step 4: 实现 Git 不变量检查**

新增 Git 错误变体并映射稳定错误摘要：当前分支必须等于任务分支，`HEAD` 必须等于报告 Commit，工作区不得有已跟踪差异。未跟踪构建产物允许。增加真实临时仓库测试覆盖分支切换、HEAD 改写和 tracked 修改，确认未跟踪文件不误报。

- [x] **Step 5: 运行适配器测试**

Run: `cargo test -p coldstart-adapters --locked`
Expected: PASS，包括真实 `.exe` 和 Job Object 测试。

---

### Task 5: 应用层编排、Git 依赖注入与集成服务

**Files:**
- Create: `crates/app/src/git_gateway.rs`
- Modify: `crates/app/src/lib.rs`
- Modify: `crates/app/src/plan.rs`
- Modify: `crates/app/src/integration.rs`
- Modify: `crates/app/src/context_compiler.rs`
- Modify: `crates/app/src/plan_workspace.rs`
- Modify: `crates/app/src/error.rs`
- Create: `crates/app/tests/support/mod.rs`
- Modify: `crates/app/tests/plan.rs`
- Modify: `crates/app/tests/integration.rs`
- Modify: `crates/app/tests/mvp_acceptance.rs`

**Interfaces:**
- Produces `PlanGitGateway`（生产实现委托 `GitAdapter`），让应用测试注入 fake Git。
- `PlanService::new()` 使用 `WindowsSecretStore + GitAdapter + WindowsVerificationExecutor`；新增 `with_gateways(secrets, Arc<dyn PlanGitGateway>, Arc<dyn VerificationCommandExecutor>)`。
- `PlanService::submit_report` 调用 `begin_work_verification`，顺序执行快照命令并在每步前后校验 Git；成功写 `passed` 后推送，失败写 `failed` 后返回稳定错误。
- `IntegrationService::load_verification_commands(path, key)` 与 `save_verification_commands(path, key, commands)`。
- `AppError` 新增六个验证错误及 `verification_details() -> Option<serde_json::Value>`。

- [x] **Step 1: 写应用失败回退与重试测试**

使用 fake Git + 可编程 fake executor：首个命令失败时断言任务 `in_progress`、租约未消费、写入 `failed`；同一租约用新幂等键改为成功后断言 `done` 且只推送一次；桌面修改仓库命令后重新读取旧运行，快照哈希不变；成功后才 push，push 错误不改变 `accepted`。

- [x] **Step 2: 写上下文快照和错误码测试**

断言 `get_context` 的 JSON 与 Markdown 均含快照命令但不含令牌/Secret；无配置领取返回 `verification_not_configured`；超时、非零、启动失败分别映射到规定错误码；错误详情含命令名、类型、退出码、耗时和有限摘要。

- [x] **Step 3: 运行测试确认失败**

Run: `cargo test -p coldstart-app --test plan --locked`
Expected: FAIL，严格门禁和新构造函数尚未接线。

- [x] **Step 4: 实现编排**

先完成报告结构/Git 校验，再调用开始事务，随后按快照顺序执行。每个命令前后调用 `ensure_verification_checkout`；命令结果转成 `VerificationCommandResult`；首个失败立即停止但保留已完成命令；总耗时与脱敏摘要写入 `VerificationResult`。成功事务完成后再推送。中断/长期 running 由存储过期路径处理。

- [x] **Step 5: 实现集成服务并更新夹具**

命令保存前由核心规范化，仓库必须存在；读取按 position 排序。应用测试夹具默认创建仓库与一条命令，并使用 fake Git/executor；MCP 与真实 Git 测试改为显式配置。Run: `cargo test -p coldstart-app --locked`
Expected: PASS。

---

### Task 6: Tauri、前端类型、命令编辑器与看板摘要

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `web/src/api/types.ts`
- Modify: `web/src/api/integrationApi.ts`
- Create: `web/src/components/RepositoryVerificationEditor.tsx`
- Create: `web/src/components/RepositoryVerificationEditor.test.tsx`
- Modify: `web/src/components/IntegrationView.tsx`
- Modify: `web/src/components/PlanView.tsx`
- Modify: `web/src/App.test.tsx`
- Modify: `web/src/styles.css`

**Interfaces:**
- `IntegrationWorkspace.verificationCommands: Record<string, RepositoryVerificationCommand[]>`。
- Tauri 命令 `read_repository_verification_commands(path, repositoryKey)` 与 `save_repository_verification_commands(path, repositoryKey, commands)`；无工作目录、环境变量或 Shell 参数。
- `RepositoryVerificationEditor` 使用文件选择器选 `.exe`，参数“每行一个”，支持新增、编辑、删除、上移/下移和保存。
- `WorkReportRecord` 类型增加 `verificationStatus` 和 `verification`。

- [x] **Step 1: 写前端失败测试**

组件测试覆盖：新增命令、编辑名称/参数/超时、删除、上移下移后保存顺序、选择 `.exe`；空列表显示“不可领取”警告。看板测试构造 failure 结果，断言显示 `failed`、失败命令、退出码和脱敏摘要，且不显示租约令牌。

- [x] **Step 2: 运行测试确认失败**

Run: `pnpm --dir web test --run web/src/components/RepositoryVerificationEditor.test.tsx`
Expected: FAIL，组件与类型尚不存在。

- [x] **Step 3: 实现 IPC 和组件**

Tauri 只调用 IntegrationService 的两个显式方法。`adoptWorkspace` 保留命令映射。编辑器保存时把 UI 行转为 position `0..n`、timeout 数字和参数数组；成功后采用返回工作区。IntegrationView 在仓库卡片下渲染编辑器，空命令显示醒目警告。PlanView 增加中文状态、失败命令、退出码、耗时和摘要。

- [x] **Step 4: 更新类型与 API，运行前端测试**

Run: `pnpm --dir web typecheck`
Run: `pnpm --dir web test --run`
Expected: PASS。

---

### Task 7: MCP 契约与错误详情

**Files:**
- Modify: `crates/mcp/src/error.rs`
- Modify: `crates/mcp/src/server.rs`
- Modify: `crates/mcp/tests/stdio.rs`

**Interfaces:**
- 工具列表保持不变。
- `get_work_context` 结构化结果包含 `verification`。
- `submit_work_report` 成功结果包含 `verificationStatus`/`verification`；失败返回稳定 `code` 与 `details`。

- [x] **Step 1: 写 MCP 契约失败测试**

断言工具列表仍为现有工具集合；上下文含快照、无 Secret/令牌；验证失败返回 `verification_failed`，超时返回 `verification_timeout`，无配置返回 `verification_not_configured`；返回 JSON 不含完整环境、原始输出、租约令牌或数据库路径。

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-mcp --test stdio --locked`
Expected: FAIL，上下文与错误详情尚未接线。

- [x] **Step 3: 实现错误详情**

`app_error_result` 调用 `AppError::code()` 与 `verification_details()`；无详情时保持现有结构。`execute` 不改变工具集合或参数 schema。

- [x] **Step 4: 运行 MCP 测试**

Run: `cargo test -p coldstart-mcp --locked`
Expected: PASS。

---

### Task 8: 真实本地闭环、文档、阶段门禁与安装验收

**Files:**
- Modify: `crates/app/tests/mvp_acceptance.rs`
- Modify: `README.md`
- Modify: `web/src/components/WelcomeView.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/App.test.tsx`
- Modify: `docs/superpowers/specs/2026-10-30-coldstart-design.md`（仅补充阶段 7 实现状态）
- Modify: `docs/superpowers/plans/2026-10-30-coldstart-delivery-roadmap.md`
- Create: `.superpowers/sdd/2026-10-30-coldstart-07-command-verification/progress.md`
- Create: `.superpowers/sdd/2026-10-30-coldstart-07-command-verification/acceptance-summary.json`

- [x] **Step 1: 写端到端失败测试**

使用本地裸 Git 远程、真实任务分支和真实探针 `.exe`，完成“配置命令 -> 领取 -> 失败报告 -> 同租约修复 -> 成功报告 -> done -> 推送”。断言首次失败不消费租约、远程没有任务分支；成功后远程 SHA 等于报告 Commit；修改已跟踪文件/HEAD 的命令导致失败。

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-app --test mvp_acceptance phase7 --locked -- --nocapture`
Expected: FAIL，直到真实执行与推送顺序全部接线。

- [x] **Step 3: 完成闭环测试并更新欢迎页**

阶段标识改为 `PHASE 07 / COMMAND VERIFICATION`，说明“报告不再等于完成，本地命令验证决定完成”。同步更新 README 当前状态、使用流程、错误码、安全边界和“仍不执行部署/提审/自动修复”。

- [x] **Step 4: 运行完整自动化门禁**

```powershell
. 'D:\DevTools\coldstart-env.ps1'
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo +1.89.0 test --workspace --locked
pnpm --dir web typecheck
pnpm --dir web test --run
pnpm --dir web build
pnpm --dir apps/desktop tauri build
cargo build -p coldstart-mcp --release --locked
```

Expected: 全部退出码 0；记录测试套件/用例数、Clippy、前端、安装包大小与 SHA-256。

- [x] **Step 5: D 盘安装与卸载验收**

用 NSIS `/S /D=D:\DevTools\ColdStart` 安装；启动安装版，打开验收项目，确认验证状态与失败摘要可读；卸载后确认项目数据仍保留。记录截图、安装路径、卸载结果、安装包哈希。

- [x] **Step 6: 生成阶段台账**

仅在所有必需门禁通过后标记完成；记录失败修复、测试证据、安装包哈希、手动验收结果和明确遗留项（不自动修复、不打包、不提审、不注入 Secret）。

## 计划自检

- 规格覆盖：30.1/30.3 由 Task 2、5 实现；30.2 由 Task 1、2、6 实现；30.4 由 Task 3、5 实现；30.5 由 Task 4 实现；30.6 由 Task 2、3、6 实现；30.7 由 Task 5、7 实现；30.8 由所有测试与 Task 8 实现；30.9 边界未被任何任务突破。
- 安全边界：命令只能由桌面写入，MCP 只读快照；命令执行不经过 Shell；Secret 不进入命令、上下文、报告、日志或错误。
- 类型一致性：`RepositoryVerificationCommand`、`VerificationSnapshot`、`VerificationResult`、`VerificationStatus`、`verification_snapshot_hash` 在 core/storage/app/Tauri/React/MCP 全程同名映射。
- 状态一致性：`running -> passed|failed|interrupted`；失败保留租约并回 `in_progress`；过期中断回 `ready`；只有 `passed` 才消费租约并写 `done`。
