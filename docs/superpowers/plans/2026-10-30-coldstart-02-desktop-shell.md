# ColdStart 桌面壳实施计划

> **供智能体执行者使用：** 必须使用 `superpowers:subagent-driven-development`（推荐）或 `superpowers:executing-plans`，按任务逐步执行本计划。使用复选框（`- [ ]`）跟踪步骤。

**目标：** 交付一个可运行的 Tauri 2 桌面窗口，用于创建、打开并查看 ColdStart 项目状态，同时把 CLI 的项目用例抽取为共享 Rust 应用层。

**架构：** 新增 `coldstart-app` 承载项目用例与 DTO；CLI 和 Tauri 都是薄适配器。React/TypeScript 只负责界面，通过显式 Tauri 命令调用 Rust，不直接访问文件系统或 SQLite。

**技术栈：** Rust 1.89、2024 edition、Tauri 2、React、TypeScript、Vite、Vitest、pnpm、SQLite。

**规格：** `docs/superpowers/specs/2026-10-30-coldstart-design.md`

**前置阶段：** `docs/superpowers/plans/2026-10-30-coldstart-01-core-foundation.md`

## 全局约束

- 在 `D:\coldstart` 的 `main` 分支上工作；不在本阶段 push。
- 文档使用中文；Rust 标识、路径、命令、JSON 键保留英文。
- 使用 Rust stable、最低 Rust 1.89、2024 edition 和 workspace resolver 3。
- Tauri 2 负责窗口与原生命令边界；React/TypeScript 只负责界面和交互状态。
- CLI、Tauri 和后续 MCP 必须复用 `coldstart-app`，不得复制项目文件或 SQLite 逻辑。
- 前端不得直接访问文件系统、SQLite、Shell 或凭据；不得启用任意 Shell 权限。
- 本阶段只实现项目创建、打开、状态查看与刷新；不实现需求导入、蓝图、计划引擎、MCP、Git、GitHub、凭据或发布中心。
- 不生成安装包、签名或自动更新；生产验证使用 `tauri build --no-bundle`。
- 所有 Node/Rust 依赖和构建产物保持在 D 盘当前工作区内。
- Rust 代码必须通过 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --locked`。
- 前端代码必须通过 `pnpm --dir web typecheck`、`pnpm --dir web test`、`pnpm --dir web build`。
- 每个任务结束使用一次约定式提交，并写独立 SDD 报告。

## 文件结构

```text
Cargo.toml
package.json
pnpm-workspace.yaml
crates/app/Cargo.toml
crates/app/src/lib.rs
crates/app/src/error.rs
crates/app/src/project_home.rs
crates/app/src/service.rs
crates/app/src/status.rs
crates/app/tests/service.rs
crates/cli/Cargo.toml
crates/cli/src/main.rs
crates/cli/tests/cli.rs
web/package.json
web/index.html
web/vite.config.ts
web/tsconfig.json
web/src/main.tsx
web/src/App.tsx
web/src/api/projectApi.ts
web/src/api/types.ts
web/src/components/CreateProjectDialog.tsx
web/src/components/ErrorBanner.tsx
web/src/components/ProjectStatusView.tsx
web/src/components/Sidebar.tsx
web/src/components/WelcomeView.tsx
web/src/styles.css
web/src/test/setup.ts
web/src/App.test.tsx
apps/desktop/package.json
apps/desktop/app-icon.svg
apps/desktop/src-tauri/Cargo.toml
apps/desktop/src-tauri/build.rs
apps/desktop/src-tauri/tauri.conf.json
apps/desktop/src-tauri/capabilities/default.json
apps/desktop/src-tauri/src/main.rs
apps/desktop/src-tauri/src/lib.rs
apps/desktop/src-tauri/src/commands.rs
apps/desktop/src-tauri/src/error.rs
README.md
```

- `crates/app` 只依赖 core/storage 和通用库，不依赖 Tauri、clap 或 React。
- `crates/cli` 只保留参数解析和文本/JSON 输出。
- `web` 不包含任何 SQL 或路径拼接；目录选择通过 Tauri dialog。
- `apps/desktop/src-tauri` 只暴露两个项目命令。

---

### 任务 1：抽取共享项目应用层

**文件：**
- 创建：`crates/app/Cargo.toml`
- 创建：`crates/app/src/lib.rs`
- 创建：`crates/app/src/error.rs`
- 创建：`crates/app/src/project_home.rs`
- 创建：`crates/app/src/service.rs`
- 创建：`crates/app/src/status.rs`
- 创建：`crates/app/tests/service.rs`
- 修改：`Cargo.toml`
- 修改：`crates/cli/Cargo.toml`
- 修改：`crates/cli/src/main.rs`
- 删除：`crates/cli/src/error.rs`
- 删除：`crates/cli/src/project_home.rs`
- 保留并验证：`crates/cli/tests/cli.rs`

**接口：**
- 使用：`coldstart_core::{Project, ProjectManifest, ProjectId}`。
- 使用：`coldstart_storage::{Database, StorageError}`。
- 产出：`coldstart_app::{AppError, ProjectService, ProjectStatus}`。
- 产出：`ProjectService::create_project(&self, name: &str, project_home: PathBuf) -> Result<ProjectStatus, AppError>`。
- 产出：`ProjectService::read_status(&self, project_home: PathBuf) -> Result<ProjectStatus, AppError>`。

- [ ] **步骤 1：先写应用层失败测试**

在 `crates/app/tests/service.rs` 写以下契约测试：

```rust
use std::fs;

use coldstart_app::{AppError, ProjectService};
use tempfile::TempDir;

#[test]
fn create_project_returns_the_initialized_status() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("demo");
    let service = ProjectService::new();

    let status = service
        .create_project("Demo Project", home.clone())
        .expect("project must initialize");

    assert_eq!(status.name, "Demo Project");
    assert_eq!(status.event_count, 1);
    assert_eq!(
        status.root_path,
        std::path::absolute(&home).expect("path must be absolute")
    );
    assert_eq!(
        service.read_status(home).expect("status must load"),
        status
    );
}

#[test]
fn read_status_uses_the_current_project_home_after_a_move() {
    let temp = TempDir::new().unwrap();
    let original = temp.path().join("original");
    let moved = temp.path().join("moved");
    let service = ProjectService::new();
    let created = service
        .create_project("Moved Project", original.clone())
        .expect("project must initialize");

    fs::rename(&original, &moved).expect("project must move");
    let status = service.read_status(moved.clone()).expect("status must load");

    assert_eq!(status.project_id, created.project_id);
    assert_eq!(status.root_path, std::path::absolute(moved).unwrap());
}

#[test]
fn create_project_reports_a_missing_manifest_with_a_stable_code() {
    let temp = TempDir::new().unwrap();
    let error = ProjectService::new()
        .read_status(temp.path().to_path_buf())
        .expect_err("empty directory must fail");

    assert!(matches!(error, AppError::ManifestMissing(_)));
    assert_eq!(error.code(), "manifest_missing");
}
```

- [ ] **步骤 2：运行测试并确认 crate 尚不存在**

```powershell
cargo test -p coldstart-app --test service
```

预期失败：workspace 中没有 `coldstart-app`。

- [ ] **步骤 3：创建 app crate 与稳定 DTO**

`crates/app/Cargo.toml`：

```toml
[package]
name = "coldstart-app"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
chrono.workspace = true
coldstart-core = { path = "../core" }
coldstart-storage = { path = "../storage" }
serde.workspace = true
serde_json.workspace = true
tempfile.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

把 `crates/cli` 加入 workspace 后，再把 `crates/app` 加入根 `Cargo.toml` 的 members：

```toml
members = ["crates/core", "crates/storage", "crates/app", "crates/cli"]
```

`crates/app/src/status.rs`：

```rust
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use coldstart_core::ProjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatus {
    pub project_id: ProjectId,
    pub name: String,
    pub root_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub event_count: u64,
}
```

`crates/app/src/lib.rs`：

```rust
mod error;
mod project_home;
mod service;
mod status;

pub use error::AppError;
pub use service::ProjectService;
pub use status::ProjectStatus;
```

- [ ] **步骤 4：迁移 ProjectHome 并实现 ProjectService**

把当前 `crates/cli/src/project_home.rs` 的完整实现迁入 `crates/app/src/project_home.rs`，保留：

- 名称先于文件系统校验。
- 临时清单、`sync_all`、`persist_noclobber`。
- 初始化前存在性检查。
- 本次拥有文件的回滚与 `RollbackFailed` 残留路径。
- `PROJECT_DIRECTORIES` 和 `.coldstart/state.db` 布局。

把 `CliError` 改名为 `AppError`，原错误消息保持不变，并增加：

```rust
impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotADirectory(_) => "not_a_directory",
            Self::AlreadyInitialized(_) => "already_initialized",
            Self::StateAlreadyExists(_) => "state_already_exists",
            Self::ManifestMissing(_) => "manifest_missing",
            Self::DatabaseMissing(_) => "database_missing",
            Self::InvalidManifest(_) => "invalid_manifest",
            Self::InvalidProject(_) => "invalid_project",
            Self::ProjectMissingFromDatabase(_) => "project_missing_from_database",
            Self::ProjectMetadataMismatch(_) => "project_metadata_mismatch",
            Self::NonUtf8Path(_) => "non_utf8_path",
            Self::Json(_) => "json",
            Self::Storage(_) => "storage",
            Self::Io(_) => "io",
            Self::RollbackFailed { .. } => "rollback_failed",
        }
    }
}
```

`ProjectService` 必须复用原 CLI 状态逻辑：

1. `absolute_root()`。
2. `load_manifest()`。
3. `manifest.to_project(root)`。
4. 数据库存在性检查且绝不重建。
5. `Database::open`、`load_project(id, root)`、事件计数、名称/时间一致性校验。
6. 构造 `ProjectStatus`。

`create_project` 调用 `ProjectHome::initialize` 后，再调用 `read_status` 返回完整状态。

- [ ] **步骤 5：把 CLI 改成薄适配器**

`crates/cli/Cargo.toml` 删除对 `coldstart-core`/`coldstart-storage` 的直接依赖，改为：

```toml
[dependencies]
clap.workspace = true
coldstart-app = { path = "../app" }
serde_json.workspace = true
thiserror.workspace = true
```

删除 `crates/cli/src/error.rs` 和 `crates/cli/src/project_home.rs`。`main.rs` 只保留 Clap 和输出逻辑，错误类型直接使用 `coldstart_app::AppError`：

```rust
fn run(cli: Cli) -> Result<(), AppError> {
    match cli.command {
        Command::Init { name, path } => {
            let status = ProjectService::new().create_project(&name, path)?;
            println!(
                "Initialized project \"{}\" at {}",
                status.name,
                status.root_path.display()
            );
        }
        Command::Status { path, json } => print_status(path, json)?,
    }
    Ok(())
}
```

`status --json` 直接序列化 `ProjectStatus`，必须继续输出五个 camelCase 键。文本输出保持原字段顺序。现有 CLI 集成测试不得改写期望值。

- [ ] **步骤 6：运行定向测试和全量门禁**

```powershell
cargo test -p coldstart-app
cargo test -p coldstart-cli --test cli
cargo check --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

预期：CLI 9 个集成测试通过；新 app 契约测试通过；全 workspace 无回归。

- [ ] **步骤 7：提交**

```powershell
git add Cargo.toml Cargo.lock crates/app crates/cli
git commit -m "refactor: 抽取共享项目应用服务"
```

---

### 任务 2：实现 React 项目界面

**文件：**
- 创建：`package.json`
- 创建：`pnpm-workspace.yaml`
- 创建：`web/package.json`
- 创建：`web/index.html`
- 创建：`web/vite.config.ts`
- 创建：`web/tsconfig.json`
- 创建：`web/src/main.tsx`
- 创建：`web/src/App.tsx`
- 创建：`web/src/api/projectApi.ts`
- 创建：`web/src/api/types.ts`
- 创建：`web/src/components/CreateProjectDialog.tsx`
- 创建：`web/src/components/ErrorBanner.tsx`
- 创建：`web/src/components/ProjectStatusView.tsx`
- 创建：`web/src/components/Sidebar.tsx`
- 创建：`web/src/components/WelcomeView.tsx`
- 创建：`web/src/styles.css`
- 创建：`web/src/test/setup.ts`
- 创建：`web/src/App.test.tsx`
- 创建：`pnpm-lock.yaml`

**接口：**
- 消费：任务 1 的 `ProjectStatus` 契约。
- 产出：`projectApi.createProject(name, path)`、`projectApi.readProjectStatus(path)`、`projectApi.pickProjectHome()`。
- 产出：可独立用 mock API 测试和构建的 React 页面。

- [ ] **步骤 1：创建 pnpm workspace 和前端骨架**

根 `package.json`：

```json
{
  "name": "coldstart",
  "private": true,
  "packageManager": "pnpm@11.25.0",
  "scripts": {
    "web:dev": "pnpm --dir web dev",
    "web:build": "pnpm --dir web build",
    "web:test": "pnpm --dir web test",
    "web:typecheck": "pnpm --dir web typecheck"
  }
}
```

`pnpm-workspace.yaml`：

```yaml
packages:
  - web
  - apps/desktop
```

在 `web` 目录执行：

```powershell
pnpm init
pnpm add react react-dom @tauri-apps/api @tauri-apps/plugin-dialog
pnpm add -D vite @vitejs/plugin-react typescript vitest @testing-library/react @testing-library/jest-dom @testing-library/user-event jsdom @types/react @types/react-dom @types/node
```

`web/package.json` scripts 必须为：

```json
{
  "dev": "vite",
  "build": "tsc --noEmit && vite build",
  "typecheck": "tsc --noEmit",
  "test": "vitest run"
}
```

`vite.config.ts` 使用 `127.0.0.1:5173`、`strictPort: true`，并配置 jsdom 与 `src/test/setup.ts`。

- [ ] **步骤 2：先写前端失败测试**

`web/src/App.test.tsx` 至少覆盖：

1. 首屏显示“冷启动”和“新建项目/打开项目”。
2. 新建项目：填写名称、选择目录、调用 `createProject`，显示项目状态。
3. 打开项目：选择目录、调用 `readProjectStatus`，显示项目状态。
4. 后端错误：显示错误信息和错误码。
5. 刷新：再次调用 `readProjectStatus` 并更新时间/事件数。

测试通过 `vi.mock('./api/projectApi')` 隔离 Tauri IPC；不得依赖真实窗口。

- [ ] **步骤 3：实现 API 边界和 DTO**

`web/src/api/types.ts`：

```ts
export interface ProjectStatus {
  projectId: string;
  name: string;
  rootPath: string;
  createdAt: string;
  eventCount: number;
}

export interface CommandError {
  code: string;
  message: string;
}
```

`projectApi.ts` 只调用 Tauri：

```ts
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

export async function createProject(name: string, path: string) {
  return invoke<ProjectStatus>('create_project', { name, path });
}

export async function readProjectStatus(path: string) {
  return invoke<ProjectStatus>('read_project_status', { path });
}

export async function pickProjectHome(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: '选择 ColdStart 项目主目录',
  });
  return typeof selected === 'string' ? selected : null;
}
```

所有 rejected promise 由 `App.tsx` 统一规范化为 `CommandError`；未知值回退到 `{ code: 'unknown', message: String(error) }`。

- [ ] **步骤 4：实现单窗口界面**

布局要求：

- 固定左侧导航，品牌为“COLDSTART / 冷启动”，当前阶段只启用“项目总览”；其余导航显示为后续阶段禁用项。
- 欢迎页包含一句产品定位、一个“新建项目”主按钮和一个“打开项目”次按钮。
- 新建弹窗包含项目名称输入、项目主目录选择、创建中状态、取消和提交。
- 项目状态页展示名称、项目 ID、根路径、创建时间、事件数和刷新按钮。
- 错误以可关闭的横幅展示，包含错误码和消息。
- 颜色使用冷蓝/冰青点缀，背景深色，确保文本对比度；不依赖远程字体或图片。
- 所有按钮有明确 `type`、键盘可操作；弹窗使用 `role="dialog"`、`aria-modal="true"` 和可见标签。

`App.tsx` 状态至少包含：

```ts
type AppState =
  | { kind: 'welcome' }
  | { kind: 'loading'; label: string }
  | { kind: 'project'; status: ProjectStatus }
  | { kind: 'error'; error: CommandError; previous: ProjectStatus | null };
```

打开或创建时进入 loading；成功进入 project；失败显示 error，同时保留上次成功状态以便继续查看。

- [ ] **步骤 5：运行前端门禁**

```powershell
pnpm install
pnpm --dir web typecheck
pnpm --dir web test
pnpm --dir web build
```

预期：类型检查零错误，所有组件测试通过，`web/dist` 生成。

- [ ] **步骤 6：提交**

```powershell
git add package.json pnpm-workspace.yaml pnpm-lock.yaml web
git commit -m "feat: 添加 ColdStart 项目界面"
```

---

### 任务 3：接入 Tauri 桌面壳并完成端到端闭环

**文件：**
- 创建：`apps/desktop/package.json`
- 创建：`apps/desktop/app-icon.svg`
- 创建：`apps/desktop/src-tauri/Cargo.toml`
- 创建：`apps/desktop/src-tauri/build.rs`
- 创建：`apps/desktop/src-tauri/tauri.conf.json`
- 创建：`apps/desktop/src-tauri/capabilities/default.json`
- 创建：`apps/desktop/src-tauri/src/main.rs`
- 创建：`apps/desktop/src-tauri/src/lib.rs`
- 创建：`apps/desktop/src-tauri/src/commands.rs`
- 创建：`apps/desktop/src-tauri/src/error.rs`
- 生成：`apps/desktop/src-tauri/icons/*`
- 修改：`Cargo.toml`
- 修改：`Cargo.lock`
- 修改：`README.md`
- 修改：`docs/superpowers/plans/2026-10-30-coldstart-delivery-roadmap.md`

**接口：**
- 消费：任务 1 的 `ProjectService`、`ProjectStatus`、`AppError`。
- 消费：任务 2 的 `createProject`、`readProjectStatus`、`pickProjectHome` 前端边界。
- 产出：Tauri 命令 `create_project(name, path)` 与 `read_project_status(path)`。
- 产出：可运行窗口和生产构建 `coldstart-desktop.exe`。

- [ ] **步骤 1：添加桌面包与 Tauri CLI 依赖**

`apps/desktop/package.json`：

```json
{
  "name": "@coldstart/desktop",
  "private": true,
  "scripts": {
    "tauri": "tauri",
    "dev": "tauri dev",
    "build": "tauri build --no-bundle"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2"
  }
}
```

运行 `pnpm install`，确认 CLI 位于工作区 `node_modules`，不全局安装。

- [ ] **步骤 2：先写 Rust 命令失败测试**

在 `commands.rs` 的 `#[cfg(test)]` 模块中先写：

1. `create_project_sync` 创建临时项目并返回 `eventCount=1`。
2. `read_project_status_sync` 读取同一项目。
3. 重复创建返回 `CommandError.code == "already_initialized"`。
4. 空目录读取返回 `manifest_missing`。

先运行：

```powershell
cargo test -p coldstart-desktop
```

预期失败：desktop crate 尚不存在或命令函数未定义。

- [ ] **步骤 3：创建 Tauri Rust crate**

根 `Cargo.toml` members 增加 `apps/desktop/src-tauri`。

`Cargo.toml` 关键内容：

```toml
[package]
name = "coldstart-desktop"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[lib]
name = "coldstart_desktop_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
coldstart-app = { path = "../../../crates/app" }
serde.workspace = true
serde_json.workspace = true
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"

[dev-dependencies]
tempfile.workspace = true
```

`build.rs` 调用 `tauri_build::build()`。

`error.rs`：

```rust
#[derive(Debug, serde::Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl From<coldstart_app::AppError> for CommandError {
    fn from(error: coldstart_app::AppError) -> Self {
        Self {
            code: error.code().to_owned(),
            message: error.to_string(),
        }
    }
}
```

`commands.rs` 同时提供可测试的同步函数和 Tauri async 命令：

```rust
pub fn create_project_sync(name: String, path: String) -> Result<ProjectStatus, CommandError> {
    ProjectService::new()
        .create_project(&name, path.into())
        .map_err(Into::into)
}

#[tauri::command]
pub async fn create_project(name: String, path: String) -> Result<ProjectStatus, CommandError> {
    tauri::async_runtime::spawn_blocking(move || create_project_sync(name, path))
        .await
        .map_err(|error| CommandError {
            code: "task_join".to_owned(),
            message: error.to_string(),
        })?
}
```

`read_project_status_sync`/`read_project_status` 采用同样结构。不得在前端或命令中拼接项目内部路径。

- [ ] **步骤 4：配置 Tauri 窗口、能力和 CSP**

`tauri.conf.json` 使用：

- `productName = "ColdStart"`
- `identifier = "com.coldstart.desktop"`
- `beforeDevCommand = "pnpm --dir ../../web dev"`
- `beforeBuildCommand = "pnpm --dir ../../web build"`
- `devUrl = "http://127.0.0.1:5173"`
- `frontendDist = "../../../web/dist"`
- 窗口 label `main`，标题 `ColdStart`，1200×800，最小 960×640。
- CSP 至少允许 `default-src 'self'`、Tauri IPC、Vite 开发地址和 `'unsafe-inline'` 样式。
- `bundle.active = false`，本阶段不生成安装包。

`capabilities/default.json` 只赋予主窗口：

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "ColdStart main window",
  "windows": ["main"],
  "permissions": ["core:default", "dialog:allow-open"]
}
```

不得加入 shell、fs、process 或任意命令执行权限。

`lib.rs` 初始化 dialog 插件并只注册两个命令：

```rust
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::create_project,
            commands::read_project_status
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ColdStart desktop");
}
```

`main.rs` 设置 Windows release 的 `windows_subsystem = "windows"` 并调用 `run()`。

- [ ] **步骤 5：生成应用图标**

创建简洁 SVG 图标：深蓝底、冰青色冷启动标记，不含文字。运行：

```powershell
pnpm --dir apps/desktop tauri icon app-icon.svg
```

确认生成 `src-tauri/icons` 并在配置中引用。若 CLI 不接受 SVG，使用其生成的 1024×1024 PNG 作为输入；不得跳过图标导致 release 构建失败。

- [ ] **步骤 6：更新文档并完成全量验证**

README 增加：

```powershell
pnpm install
pnpm --dir apps/desktop dev
pnpm --dir apps/desktop build
```

说明：

- 开发窗口依赖 Vite 与 Tauri。
- 生产构建输出 `coldstart-desktop.exe`；本阶段不生成安装包。
- 当前只实现项目创建、打开、状态刷新。
- 路线图第二阶段标记为已实现。

运行：

```powershell
cargo check --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo +1.89.0 check --workspace --locked
pnpm --dir web typecheck
pnpm --dir web test
pnpm --dir web build
pnpm --dir apps/desktop tauri build --no-bundle
```

预期：全部退出码 0；生成桌面可执行文件；无 Tauri 权限越界。

- [ ] **步骤 7：提交**

```powershell
git add Cargo.toml Cargo.lock apps/desktop README.md docs/superpowers/plans/2026-10-30-coldstart-delivery-roadmap.md
git commit -m "feat: 接入 Tauri 桌面应用"
```

---

## 手工验收

控制器在全部任务完成后执行：

1. 启动 `pnpm --dir apps/desktop dev`。
2. 新建一个临时项目，确认窗口显示名称、路径、五个状态字段和 `eventCount=1`。
3. 关闭并重新打开应用，选择同一目录，确认状态恢复。
4. 移动项目目录后重新打开，确认根路径来自新位置。
5. 删除数据库后打开，确认明确报错且数据库未被重建。
6. 截图检查深色主题、文本对比度、按钮状态和错误横幅。
7. 关闭开发进程并清理临时项目；只在验证过的 D 盘临时路径内递归删除。

