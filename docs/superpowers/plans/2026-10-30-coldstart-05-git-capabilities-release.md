# ColdStart 阶段 5：仓库、能力与发布门禁实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 ColdStart 增加真实仓库映射、Git 领取与推送、系统凭据、三方能力状态、GitHub 私有仓库创建和只读发布中心。

**Architecture:** `coldstart-core` 定义仓库、凭据元数据、能力状态和 Git 同步记录；`coldstart-storage` 以 schema v5 持久化并产生事件；`coldstart-adapters` 封装系统 Git、Windows Credential Manager 和 GitHub HTTP；`coldstart-app` 编排仓库配置、领取准备、报告验证、推送与集成工作区；Tauri/React 只通过显式命令调用。

**Tech Stack:** Rust 1.89、edition 2024、SQLite/rusqlite、Tauri 2、React 19、TypeScript、`ureq`、`keyring`、系统 Git 2.55、Vitest。

**Spec:** `docs/superpowers/specs/2026-10-30-coldstart-design.md`，重点阅读第 7、8、9、11、13-17、19、21-23、28 节。

## Global Constraints

- 文档使用中文；Rust、JSON、TypeScript 标识保留英文。
- 明文 Secret 只存在 Windows Credential Manager；SQLite、事件、日志、错误、上下文和 MCP 响应不得包含 Secret。
- Git 操作只调用系统 `git`，使用参数数组而非 Shell；禁止强推、删除远程分支和自动写保护分支。
- GitHub 私有仓库创建只能由桌面 UI 显式发起，MCP 不增加仓库 CRUD 工具。
- 测试使用临时本地裸仓库、内存 SecretStore 和本地 HTTP 模拟 GitHub；自动测试不得创建真实远程仓库。
- 阶段 5 不实现部署、App 打包、商店/小程序提审、自动合并保护分支和从空目录生成业务代码。
- 依赖与构建产物保持在 D 盘；不提交或推送 Git，除非用户明确要求。

## 文件结构

- `crates/core/src/integration.rs`：仓库、凭据元数据、能力状态、Git 同步和 GitHub 连接 DTO。
- `crates/core/src/entity_id.rs`：仓库、凭据和 Git 同步 ID。
- `crates/core/src/event.rs`：集成配置与同步事件。
- `crates/storage/src/migrations.rs`：schema v5。
- `crates/storage/src/integration_store.rs`：仓库、凭据、能力、设置与 Git 同步事务。
- `crates/adapters/src/git.rs`：系统 Git 检查、分支、报告验证与推送。
- `crates/adapters/src/secret.rs`：SecretStore、Windows Credential Manager 与内存测试实现。
- `crates/adapters/src/github.rs`：Device Flow、GitHub REST、私有仓库创建和 gh Token 导入。
- `crates/app/src/integration.rs`：集成工作区、凭据、能力、GitHub 和仓库用例。
- `crates/app/src/plan.rs`：领取准备、报告 Git 验证、推送与同步记录。
- `crates/app/src/context_compiler.rs`：仓库配置和能力状态上下文。
- `crates/mcp/src/*`：新增 `get_capability_status`。
- `apps/desktop/src-tauri/src/commands.rs`：集成与发布 IPC。
- `web/src/components/IntegrationView.tsx`、`web/src/api/integrationApi.ts`：集成与发布界面。

### Task 1: 集成领域模型与 schema v5

**Files:**
- Create: `crates/core/src/integration.rs`
- Modify: `crates/core/src/entity_id.rs`
- Modify: `crates/core/src/event.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/storage/src/migrations.rs`
- Create: `crates/storage/src/integration_store.rs`
- Modify: `crates/storage/src/lib.rs`
- Test: `crates/core/tests/integration.rs`
- Test: `crates/storage/tests/integration.rs`

**Interfaces:**
- Produces: `RepositoryId`、`CredentialId`、`GitSyncId`；`Repository`、`RepositoryAuthMode`、`RepositorySyncPolicy`、`CredentialMetadata`、`CredentialKind`、`CredentialEnvironment`、`CapabilityStatus`、`CapabilityState`、`GitSyncRecord`、`GitSyncStatus`。
- Produces storage methods: `save_repository`、`list_repositories`、`repository_by_key`、`save_credential`、`list_credentials`、`credential`、`delete_credential`、`save_capability_state`、`list_capability_states`、`set_project_setting`、`project_setting`、`record_git_sync`、`list_git_syncs`。

- [x] **Step 1: 写领域与迁移测试**

```rust
#[test]
fn repository_keys_and_remote_urls_are_validated() {
    assert!(validate_repository_key("server").is_ok());
    assert!(validate_repository_key("../server").is_err());
    assert!(validate_remote_url("https://user:password@example.com/repo.git").is_err());
    assert!(validate_remote_url("https://github.com/acme/repo.git").is_ok());
}
```

```rust
#[test]
fn migrates_v4_to_v5_and_persists_integration_state() {
    let database = Database::open(&database_path).unwrap();
    assert_eq!(database.schema_version().unwrap(), 5);
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-core --test integration --locked`
Run: `cargo test -p coldstart-storage --test integration --locked`
Expected: FAIL，因为类型、迁移和存储方法尚不存在。

- [x] **Step 3: 实现领域类型、事件和 schema v5**

创建 `repositories`、`credentials`、`capability_states`、`git_syncs`、`project_settings` 表；删除凭据时 `ON DELETE SET NULL`，所有写方法在 `BEGIN IMMEDIATE` 中追加事件。

- [x] **Step 4: 运行领域与存储测试**

Run: `cargo test -p coldstart-core --test integration --locked`
Run: `cargo test -p coldstart-storage --test integration --locked`
Expected: PASS。

### Task 2: 系统凭据与 GitHub 适配器

**Files:**
- Create: `crates/adapters/src/secret.rs`
- Create: `crates/adapters/src/github.rs`
- Modify: `crates/adapters/Cargo.toml`
- Modify: `crates/adapters/src/lib.rs`
- Test: `crates/adapters/tests/secret.rs`
- Test: `crates/adapters/tests/github.rs`

**Interfaces:**
- Produces: `SecretStore`、`WindowsSecretStore`、`MemorySecretStore`、`SecretStoreError`。
- Produces: `GitHubClient::{start_device_flow, poll_device_flow, authenticated_user, create_private_repository}`、`GitHubConnection`、`DeviceFlowStart`、`DeviceFlowPoll`、`GitHubError`、`import_gh_token`。

- [x] **Step 1: 写 SecretStore 与 GitHub HTTP 测试**

```rust
#[test]
fn memory_secret_store_never_returns_secret_in_debug_output() {
    let store = MemorySecretStore::default();
    store.set("ref", "sensitive-value").unwrap();
    assert_eq!(store.get("ref").unwrap(), "sensitive-value");
    assert!(!format!("{store:?}").contains("sensitive-value"));
}
```

使用 `tiny_http` 模拟 Device Flow、用户信息与创建仓库响应，断言 Authorization 头存在且错误响应不把 Token 写入错误文本。

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-adapters --test secret --locked`
Run: `cargo test -p coldstart-adapters --test github --locked`
Expected: FAIL。

- [x] **Step 3: 实现适配器**

Windows SecretStore 使用 `keyring::Entry::new("ColdStart", secret_ref)`；GitHub 使用 `ureq`，端点可在测试中替换；`gh auth token --hostname github.com` 只读取 stdout，Token 不写入错误。

- [x] **Step 4: 运行适配器测试与 Clippy**

Run: `cargo test -p coldstart-adapters --locked`
Run: `cargo clippy -p coldstart-adapters --all-targets --locked -- -D warnings`
Expected: PASS。

### Task 3: 系统 Git 适配器

**Files:**
- Create: `crates/adapters/src/git.rs`
- Modify: `crates/adapters/src/lib.rs`
- Modify: `crates/adapters/Cargo.toml`
- Test: `crates/adapters/tests/git.rs`

**Interfaces:**
- Produces: `GitAdapter`、`GitRepositoryStatus`、`GitCommitVerification`、`GitAuth`、`GitError`。
- Methods: `inspect`、`validate_remote`、`fetch`、`ensure_clean`、`create_work_branch`、`verify_report_commit`、`push_branch`。

- [x] **Step 1: 写本地裸仓库测试**

测试创建临时 bare remote 和工作仓库，验证干净状态、分支创建、无强制推送、Commit 存在、实际变更文件校验、保护分支拒绝和失败错误中不含认证 Token。

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-adapters --test git --locked`
Expected: FAIL。

- [x] **Step 3: 实现参数数组式 Git 调用**

使用 `Command::new(git).args(...)`，设置 `GIT_TERMINAL_PROMPT=0`；GitHub HTTPS 认证通过 `GIT_CONFIG_COUNT/KEY/VALUE` 环境变量传递，不放入命令参数或错误。

- [x] **Step 4: 运行 Git 测试**

Run: `cargo test -p coldstart-adapters --test git --locked`
Expected: PASS。

### Task 4: 集成服务与计划联动

**Files:**
- Create: `crates/app/src/integration.rs`
- Modify: `crates/app/src/plan.rs`
- Modify: `crates/app/src/context_compiler.rs`
- Modify: `crates/app/src/lib.rs`
- Modify: `crates/app/src/error.rs`
- Test: `crates/app/tests/integration.rs`
- Modify: `crates/app/tests/plan.rs`

**Interfaces:**
- Produces: `IntegrationService::{load_workspace, save_repository, validate_repository, save_credential, delete_credential, update_capability, get_capability_status, import_github_session, start_github_device_flow, poll_github_device_flow, create_github_private_repository}`。
- Extends `PlanService` to prepare a configured repository after claim, validate report Commit/files before storage acceptance, push branch after acceptance and record sync.

- [x] **Step 1: 写集成应用测试**

覆盖：凭据只保存到内存 SecretStore；能力合并最新蓝图；仓库保存与验证；GitHub 导入不返回 Token；领取创建任务分支；不干净工作区释放租约；报告 Commit 文件越界拒绝；推送失败仍保持任务完成并写入 sync 记录。

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-app --test integration --locked`
Run: `cargo test -p coldstart-app --test plan --locked`
Expected: FAIL。

- [x] **Step 3: 实现应用编排**

未配置仓库时保持阶段 4 兼容行为；配置后启用 Git 强校验。上下文只提供仓库路径、分支、远程、保护分支和能力状态，不提供 Secret。

- [x] **Step 4: 运行应用测试**

Run: `cargo test -p coldstart-app --locked`
Expected: PASS。

### Task 5: MCP 能力状态工具

**Files:**
- Modify: `crates/mcp/src/server.rs`
- Modify: `crates/mcp/src/types.rs`
- Modify: `crates/mcp/tests/stdio.rs`
- Modify: `README.md`

**Interfaces:**
- Adds `get_capability_status(capability_id, environment)`。
- Returns `{ capabilityId, name, provider, environment, status, credentialRef, note, updatedAt }`。

- [x] **Step 1: 写 stdio 契约测试**

断言 `tools/list` 含第八个工具；已知能力返回状态，未知能力返回 `capability_not_found`，响应不含测试 Secret。

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-mcp --test stdio --locked`
Expected: FAIL。

- [x] **Step 3: 实现工具并运行测试**

Run: `cargo test -p coldstart-mcp --test stdio --locked`
Expected: PASS。

### Task 6: Tauri 命令与 React 集成中心

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `web/src/api/types.ts`
- Create: `web/src/api/integrationApi.ts`
- Create: `web/src/components/IntegrationView.tsx`
- Create: `web/src/components/ReleaseCenterView.tsx`
- Modify: `web/src/components/AppSidebar.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/styles.css`
- Test: `web/src/App.test.tsx`

**Interfaces:**
- Tauri 命令：`read_integration_workspace`、`save_repository`、`validate_repository`、`save_credential`、`delete_credential`、`update_capability`、`import_github_session`、`start_github_device_flow`、`poll_github_device_flow`、`create_github_private_repository`。
- React 新增“仓库与能力”“发布中心”两个可用工作区，并保持阶段 4 页面行为。

- [x] **Step 1: 写前端交互测试**

模拟 IPC，覆盖保存仓库、创建凭据后不显示明文、能力状态更新、GitHub Device Code 展示、私有仓库创建确认和发布中心汇总。

- [x] **Step 2: 运行测试确认失败**

Run: `pnpm --dir web test --run`
Expected: FAIL。

- [x] **Step 3: 实现命令、类型与界面**

Private repo 操作必须有二次确认。Secret 输入使用 `type=password`，提交成功后清空组件状态。发布中心只展示和跳转 Inbox，不新增绕过 MCP 的编码任务完成入口。

- [x] **Step 4: 运行前端与 Tauri 检查**

Run: `pnpm --dir web typecheck`
Run: `pnpm --dir web test --run`
Run: `pnpm --dir web build`
Run: `pnpm --dir apps/desktop tauri build --no-bundle`
Expected: 全部退出码 0。

### Task 7: 文档、门禁与手工验收

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-10-30-coldstart-delivery-roadmap.md`
- Modify: `docs/superpowers/specs/2026-10-30-coldstart-design.md`
- Create: `.superpowers/sdd/2026-10-30-coldstart-05-git-capabilities-release/progress.md`

- [x] **Step 1: 更新中文文档与阶段标识**

README 说明仓库映射、Credential Manager、能力状态、GitHub 两种授权方式、推送失败语义和阶段 5 边界；欢迎页顶栏改为 `PHASE 05 / GIT + CAPABILITIES`。

- [x] **Step 2: 运行全部门禁**

```powershell
. D:\DevTools\coldstart-env.ps1
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo +1.89.0 test --workspace --locked
pnpm --dir web typecheck
pnpm --dir web test --run
pnpm --dir web build
pnpm --dir apps/desktop tauri build --no-bundle
cargo build -p coldstart-mcp --release --locked
```

- [x] **Step 3: 手工验收**

用临时本地裸仓库验证仓库保存、领取建分支、报告提交与自动推送；重启确认持久化。用临时测试凭据验证 Windows Credential Manager 写入/删除且不出现在 SQLite。用本地 HTTP 模拟 GitHub Device Flow 与私有仓库创建；若用户提供 OAuth Client ID，再在桌面 UI 完成一次真实设备授权。真实 GitHub 私有仓库创建必须由用户现场确认，不在自动测试中执行。

- [x] **Step 4: 生成交付摘要**

记录门禁数量、release 二进制大小与 SHA-256、手工验收证据、明确遗留项。任一必需门禁失败时不得宣称阶段完成。

## 计划自检

- 规格覆盖：第 8、9、16、17、28 节由 Task 1-6 实现；第 13、15、19、21-23 节由 Task 4-6 联动；阶段 5 明确不做项没有对应实现任务。
- 占位符检查：每个任务均列出文件、接口、失败测试方向、运行命令和预期结果。
- 类型一致性：`Repository`、`CredentialMetadata`、`CapabilityState`、`GitSyncRecord` 由 Task 1 固定，后续任务使用同名类型；GitHub 与 Git 适配器接口由 Task 2-3 固定，Task 4-6 不重复定义。
