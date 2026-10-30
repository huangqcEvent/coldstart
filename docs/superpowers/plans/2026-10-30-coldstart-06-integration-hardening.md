# ColdStart 阶段 6：集成、重规划与安全加固实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 ColdStart 支持无活动租约时的安全变更重规划，完成跨子系统 MVP 验收，生成可安装到 D 盘的 Windows NSIS 安装包，并收紧桌面安全边界。

**Architecture:** `coldstart-core` 定义计划差异和任务状态继承类型；`coldstart-storage` 在同一 SQLite 事务中替换活动计划、继承状态、标记过期契约并审计；`coldstart-app` 负责强制预览与人工确认；Tauri/React 提供差异确认界面；NSIS 安装包同时携带桌面端和 MCP 二进制。

**Tech Stack:** Rust 1.89、edition 2024、SQLite/rusqlite、Tauri 2、React 19、TypeScript、Vitest、Tauri NSIS、系统 Git。

**Spec:** `docs/superpowers/specs/2026-10-30-coldstart-design.md`，重点阅读第 11-15、19-24、27-29 节。

## Global Constraints

- 文档使用中文；Rust、JSON、TypeScript 标识保留英文。
- 目标平台固定为 Web/管理后台 + App + 小程序；仓库逻辑键固定为 `server`、`admin`、`web`、`app`、`mini_program`。
- Codex 只通过 MCP 主动领取和上报；阶段 6 不增加 MCP 计划发布、确认、回滚或仓库 CRUD 工具。
- Secret 只存在 Windows Credential Manager；SQLite、日志、错误、上下文、Git 和 MCP 响应不得包含明文。
- 有活动租约时不得替换活动计划。预览可读；发布事务必须重新校验。
- 旧计划与旧事件永久保留；禁止删除历史报告或改写已完成事实。
- 发布预览必须由桌面 UI 的明确确认触发。`confirmed=true` 仅表示用户已确认替换活动计划。
- 安装、依赖、缓存、构建产物与验收项目优先放在 D 盘。
- 每个行为先写失败测试，确认失败原因后写最小实现；不得用测试后补替代 TDD。
- 阶段 6 不执行部署、App 打包、商店提审、小程序发布、自动合并或 Release Tag。

## File Structure

- `crates/core/src/plan.rs`：计划差异、任务差异、继承状态和状态机纯逻辑。
- `crates/storage/src/plan_store.rs`：活动计划预览、替换事务、旧契约失效、当前计划查询与写入守卫。
- `crates/storage/src/database.rs`：新增稳定的存储错误语义。
- `crates/storage/tests/replan.rs`：重规划事务、继承、取消、活动和过期租约测试。
- `crates/app/src/plan.rs`：应用层预览、确认规则和发布编排。
- `crates/app/src/error.rs`：人工确认缺失、旧计划失效和契约过期错误码。
- `crates/app/tests/plan.rs`：确认门禁和重规划服务测试。
- `apps/desktop/src-tauri/src/commands.rs`：预览命令与带确认参数的发布命令。
- `web/src/api/types.ts`、`web/src/api/planApi.ts`：差异类型与 IPC 封装。
- `web/src/components/PlanView.tsx`：发布预览、阻塞原因和二次确认对话框。
- `web/src/App.tsx`、`web/src/App.test.tsx`：交互接线、确认流程和真实 workspace 刷新。
- `crates/app/tests/mvp_acceptance.rs`：需求到发布中心的跨子系统闭环与重启恢复。
- `apps/desktop/src-tauri/tauri.conf.json`：NSIS、CSP、资源打包和安装模式。
- `README.md`、规格、路线图和阶段台账：阶段 6 使用说明、安装与已知限制。

---

### Task 1: 计划差异与状态继承领域模型

**Files:**
- Modify: `crates/core/src/plan.rs`
- Test: `crates/core/src/plan.rs`

**Interfaces:**
- Adds `PlanTaskDiff { task_key, title, previous_status, next_status, contract_changed }`。
- Adds `PlanPublishPreview { base_revision_id, base_revision_number, candidate_revision_id, candidate_revision_number, added, changed, unchanged, removed, carried_completed, reopened, cancelled, active_lease_count, publishable, blockers }`。
- Adds `inherited_work_status(previous: WorkItemStatus, contract_changed: bool) -> WorkItemStatus`。

- [x] **Step 1: 写状态继承失败测试**

在 `crates/core/src/plan.rs` 的测试模块添加：

```rust
#[test]
fn replanning_inherits_only_safe_task_state() {
    use WorkItemStatus::*;
    assert_eq!(inherited_work_status(Done, false), Done);
    assert_eq!(inherited_work_status(Ready, false), Ready);
    assert_eq!(inherited_work_status(Cancelled, false), Cancelled);
    assert_eq!(inherited_work_status(Done, true), Reopened);
    assert_eq!(inherited_work_status(Cancelled, true), Reopened);
    assert_eq!(inherited_work_status(InProgress, false), Reopened);
    assert_eq!(inherited_work_status(Submitted, true), Reopened);
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-core replanning_inherits_only_safe_task_state --locked`
Expected: FAIL，原因是 `inherited_work_status` 尚不存在。

- [x] **Step 3: 实现最小状态继承**

规则：

- `Cancelled` 且契约未变：保持 `Cancelled`。
- `Done` 且契约未变：保持 `Done`。
- `Draft`、`Ready`、`Blocked`、`Failed`、`Reopened` 且契约未变：保持原状态。
- `Claimed`、`InProgress`、`Submitted` 为异常遗留状态：无活动租约时转为 `Reopened`。
- 契约变化：除新计划删除场景外统一转为 `Reopened`。

- [x] **Step 4: 增加差异类型序列化测试**

断言 camelCase 字段、`publishable=false` 与 blocker 数组可往返序列化；不允许出现未知字段。

- [x] **Step 5: 运行核心测试**

Run: `cargo test -p coldstart-core --locked`
Expected: PASS。

### Task 2: 存储层事务化重规划

**Files:**
- Modify: `crates/storage/src/database.rs`
- Modify: `crates/storage/src/plan_store.rs`
- Create: `crates/storage/tests/replan.rs`
- Modify: `crates/storage/tests/support/mod.rs`
- Modify: `crates/storage/tests/plan.rs`

**Interfaces:**
- Adds `Database::preview_plan_revision(project_id, plan_revision_id, now) -> PlanPublishPreview`。
- Changes `Database::publish_plan_revision`：无活动租约时允许替换活动计划。
- Adds `Database::list_work_items_for_plan(project_id, plan_revision_id)`。
- Adds `StorageError::{PlanLeaseActive, PlanNoLongerActive, ContractStale}`。
- `PlanPublishPreview` 由存储层按 `task_key`、`TaskSpec::contract_hash()` 和旧工作项构造。

- [x] **Step 1: 写初始发布预览测试**

在 `crates/storage/tests/replan.rs` 创建计划后断言：base 为空、所有任务位于 `added`、`reopened=0`、`cancelled=0`、`publishable=true`。

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-storage --test replan --locked`
Expected: FAIL，原因是预览 API 不存在。

- [x] **Step 3: 写重规划继承测试**

构造旧计划三个任务：

- `done-stable`：相同契约且完成。
- `changed`：旧契约完成，新计划修改目标文本。
- `removed`：新计划删除。

新计划新增 `added`，并让 `changed` 依赖 `done-stable`。领取并完成旧计划任务后发布新计划，断言：

- 旧计划 `Superseded`，新计划 `Published`。
- `done-stable` 在新计划仍为 `Done`，完成时间保留，预览 `carriedCompleted=1`。
- `changed` 为 `Ready`，旧计划对应项 `contract_state=Stale`。
- `removed` 不出现在活动计划，旧工作项未完成状态为 `Cancelled` 且契约为 `Stale`。
- 新增任务存在，预览计数与差异分类准确。
- 新计划是唯一 `published` 计划。

- [x] **Step 4: 写租约与旧工作项拒绝测试**

- 存在活动租约时 `preview.publishable=false`，`publish_plan_revision` 返回 `PlanLeaseActive` 且旧计划仍为 `Published`。
- 租约过期后再次发布成功。
- 用旧工作项调用心跳、报告、阻塞或人工完成返回 `PlanNoLongerActive`。
- `contract_state=Stale` 的工作项即使人为恢复到活动计划表也不得通过授权，返回 `ContractStale`。

- [x] **Step 5: 实现事务**

`publish_plan_revision` 在同一 `IMMEDIATE` 事务中：

1. 校验候选计划为草稿、蓝图已发布且哈希一致。
2. 惰性过期租约并重新读取活动租约；仍有活动租约则返回 `PlanLeaseActive`。
3. 读取最新已发布旧计划、旧任务规格和旧工作项。
4. 生成并验证差异分类。
5. 旧计划标记 `superseded`，写入 `PlanSuperseded` 事件。
6. 候选计划标记 `published`。
7. 为每个新任务插入工作项：新任务 `draft`；未变化任务继承状态；变化任务 `reopened`；仅未变化 `done` 保留 `completed_at`。
8. 标记旧任务：无对应新任务时契约为 `stale`，未完成状态改为 `cancelled`；契约变化时契约为 `stale`。
9. 刷新新计划依赖和里程碑。
10. 任一 SQL 操作失败则回滚，不留下半成品状态。

- [x] **Step 6: 增加活动计划读取守卫**

`authorize_in_tx`、`complete_human_work` 在操作前验证所属计划是当前唯一 `published` 计划且工作项契约是 `current`。错误优先级固定为：工作项不存在 -> 计划失效 -> 契约过期 -> 状态不可操作。

- [x] **Step 7: 当前计划工作项查询**

新增 `list_work_items_for_plan` 并在测试中断言只返回指定活动计划，避免 UI、Inbox 和发布中心混入历史工作项。

- [x] **Step 8: 运行存储测试**

Run: `cargo test -p coldstart-storage --locked`
Expected: PASS。

### Task 3: 应用层预览与确认门禁

**Files:**
- Modify: `crates/app/src/plan.rs`
- Modify: `crates/app/src/error.rs`
- Modify: `crates/app/src/lib.rs`
- Modify: `crates/app/tests/plan.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Adds `PlanService::preview_publish(project_home, revision_id) -> PlanPublishPreview`。
- Adds `PlanService::publish_confirmed(project_home, revision_id, confirmed: bool) -> PlanWorkspace`。
- Keeps `PlanService::publish` 作为首次发布便捷入口，但存在活动计划时必须走 `publish_confirmed`。
- Adds `AppError::ReplanConfirmationRequired`，错误码 `replan_confirmation_required`。
- Adds Tauri commands `preview_plan_publish` 与 `publish_plan(path, revisionId, confirmed)`。

- [x] **Step 1: 写确认门禁失败测试**

在 `crates/app/tests/plan.rs` 中完成首个计划、生成第二个草稿后：

- `preview_publish` 返回 base 和差异。
- `publish_confirmed(..., false)` 返回 `replan_confirmation_required` 且计划未变化。
- `publish_confirmed(..., true)` 返回新活动计划。

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-app --test plan --locked`
Expected: FAIL，原因是新方法不存在。

- [x] **Step 3: 实现应用层**

预览只读。确认发布先检查预览，存在 base 且 `confirmed=false` 时拒绝；随后调用存储事务。Tauri 命令继续使用 `spawn_blocking`，无效 UUID 在访问数据库前拒绝。

- [x] **Step 4: 更新 Tauri 命令测试**

覆盖预览结果、确认参数和错误码序列化；响应不得包含 Secret。

- [x] **Step 5: 运行应用和 Tauri 测试**

Run: `cargo test -p coldstart-app --locked`
Run: `cargo test -p coldstart-desktop --locked`
Expected: PASS。

### Task 4: 计划差异确认界面

**Files:**
- Modify: `web/src/api/types.ts`
- Modify: `web/src/api/planApi.ts`
- Modify: `web/src/components/PlanView.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/App.test.tsx`
- Modify: `web/src/styles.css`

**Interfaces:**
- Adds `previewPlanPublish(path, revisionId)` 与 `publishPlan(path, revisionId, confirmed)`。
- `PlanView` 增加 `onPreviewPublish`；发布按钮先请求预览，不直接发布。
- 确认区域使用 `role="dialog"` 和 `aria-modal="true"`，显示差异、阻塞与人工确认说明。

- [x] **Step 1: 写前端预览交互失败测试**

在 `web/src/App.test.tsx` 模拟 IPC：点击“发布计划”只调用 `preview_plan_publish`；预览卡显示新增、契约变化、未变化、删除、继承完成与重新打开数量；存在活动租约时确认按钮禁用并显示原因；点击确认才调用 `publish_plan` 且第三个参数为 `true`。

- [x] **Step 2: 运行测试确认失败**

Run: `pnpm --dir web test --run`
Expected: FAIL，原因是预览 API 与对话框不存在。

- [x] **Step 3: 实现类型和 IPC**

类型字段与 Rust camelCase 序列化一致；IPC 命令使用 `preview_plan_publish` 与带 `confirmed` 的 `publish_plan`。

- [x] **Step 4: 实现预览和确认**

PlanView 保存当前预览、加载状态和错误；用户取消时清除预览；成功后关闭预览并刷新 workspace。预览加载失败显示稳定错误 banner，不退回直接发布。

- [x] **Step 5: 增加可访问性与样式**

确认按钮名称明确为“确认替换活动计划”；显示 `canPublish=false` 的原因；键盘焦点落在对话框标题，关闭后回到发布按钮。

- [x] **Step 6: 运行前端门禁**

Run: `pnpm --dir web typecheck`
Run: `pnpm --dir web test --run`
Run: `pnpm --dir web build`
Expected: 全部退出码 0。

### Task 5: 跨子系统闭环与安全测试

**Files:**
- Create: `crates/app/tests/mvp_acceptance.rs`
- Modify: `crates/app/src/plan_workspace.rs`
- Modify: `crates/app/src/integration.rs`
- Modify: `crates/mcp/tests/stdio.rs`
- Modify: `apps/desktop/src-tauri/tauri.conf.json`

**Interfaces:**
- `PlanWorkspace` 只绑定最新活动计划的 `WorkItemView`，历史项不得进入 Inbox、计划看板或发布中心。
- `mcp_binary_path` 依次查找桌面端同目录、`resources/` 与 PATH 中的 `coldstart-mcp.exe`。
- Tauri 生产 CSP 与开发 CSP 分离；不新增文件系统、Shell、HTTP 或更新插件。

- [x] **Step 1: 写跨子系统验收测试**

测试必须真实执行：

1. 创建临时项目并导入 Markdown 需求源。
2. 人工创建并发布蓝图，然后编译、预览、确认发布计划。
3. 配置临时本地 Git 工作树和裸远程。
4. 通过 `PlanService` 模拟 Codex 领取任务，断言分支创建。
5. 提交真实文件，使用当前任务契约提交报告，断言任务完成且远程分支 SHA 一致。
6. 加载发布中心，断言人工门禁和发布任务来自活动计划。
7. 发布新计划验证状态继承，再重启数据库断言版本、报告、同步和能力状态仍存在。

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-app --test mvp_acceptance --locked`
Expected: FAIL，原因是重规划或当前计划查询尚未完成。

- [x] **Step 3: 修正活动计划投影**

`build_plan_workspace` 与发布中心只使用最新 `published` 计划的 `list_work_items_for_plan`；`draft` 存在时不暴露未发布任务。历史租约和报告可保留在审计 API，不混入行动列表。

- [x] **Step 4: 增加安全断言**

断言应用错误、计划预览、MCP 响应、SQLite 全 TEXT 列和报告回放不包含测试 Secret 或租约令牌；路径越界仍被拒绝。

- [x] **Step 5: 收紧 Tauri 配置**

生产 `csp` 移除开发服务器来源，设置独立 `devCsp` 允许 `http://127.0.0.1:5173` 与 WebSocket；启用 `freezePrototype`；不增加额外插件权限。

- [x] **Step 6: 运行相关测试**

Run: `cargo test -p coldstart-app --locked`
Run: `cargo test -p coldstart-mcp --locked`
Expected: PASS。

### Task 6: NSIS 安装包、文档与最终验收

**Files:**
- Modify: `apps/desktop/src-tauri/tauri.conf.json`
- Modify: `apps/desktop/package.json`
- Modify: `apps/desktop/src-tauri/src/plan_workspace.rs`（如 MCP 查找仍需调整）
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-10-30-coldstart-delivery-roadmap.md`
- Create: `.superpowers/sdd/2026-10-30-coldstart-06-integration-hardening/progress.md`
- Create: `.superpowers/sdd/2026-10-30-coldstart-06-integration-hardening/acceptance-summary.json`

**Interfaces:**
- `bundle.active=true`，`targets=["nsis"]`，NSIS `installMode="currentUser"`。
- 构建脚本先构建 `coldstart-mcp --release`，再构建 Tauri NSIS；安装包包含 MCP 二进制。
- `apps/desktop/package.json` 增加明确的 `build:installer` 脚本。

- [x] **Step 1: 检查并固定打包依赖位置**

确认 NSIS 下载缓存与安装包输出位于 D 盘；若工具缺失，只安装必要组件并记录路径，不安装到 C 盘用户缓存。

- [x] **Step 2: 配置安装包**

启用 NSIS 当前用户安装；配置简体中文；将 MCP 二进制放入安装资源并在运行时解析；不启用签名、自动更新或开机启动。

- [x] **Step 3: 构建并验证安装包**

Run: `pnpm --dir apps/desktop tauri build`
Expected: 生成 `ColdStart_0.1.0_x64-setup.exe`，退出码 0。

使用临时安装目录执行静默安装：

```powershell
& '<installer>' /S /D=D:\DevTools\ColdStart
```

断言桌面 EXE、`coldstart-mcp.exe`、Web 资源和卸载器存在；启动安装版并读取项目；执行卸载并断言项目目录未被删除。

- [x] **Step 4: 更新中文文档**

README 说明阶段 6 能力、重规划确认规则、安装包位置、D 盘安装方式、未签名限制、MCP 安装版定位和仍不执行发布自动化。

- [x] **Step 5: 运行全部自动化门禁**

```powershell
. D:\DevTools\coldstart-env.ps1
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

- [x] **Step 6: 手工 MVP 验收**

使用桌面 UI 完成创建/打开项目、需求导入、蓝图发布、计划预览与确认、Codex MCP 领取和报告、Git 推送、发布中心、重规划、重启恢复。记录截图、日志、安装路径、卸载结果、二进制大小与 SHA-256。

- [x] **Step 7: 生成阶段台账**

仅在所有必需门禁通过后标记完成；列出门禁数量、失败修复、验收证据、安装包哈希和明确遗留项。

## 计划自检

- 规格覆盖：第 20、21、22、23、24、29.1 由 Task 1-5 实现；第 29.2 由 Task 5-6 实现；第 29.3、29.4 由 Task 5-6 实现。
- 安全边界：MCP 无计划发布与仓库 CRUD；旧工作项和 `stale` 契约不能推进新计划；Secret 不进入任何响应或持久化文本。
- 类型一致性：`PlanPublishPreview` 由 Task 1 固定；存储、应用、Tauri、React 全程使用同名 camelCase 字段。
- 阶段边界：未加入部署、提审、自动更新、代码签名、多人协作或新平台。
