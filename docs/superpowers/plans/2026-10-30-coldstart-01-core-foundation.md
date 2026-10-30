# ColdStart 核心基础与 CLI 实施计划

> **供智能体执行者使用：** 必须使用 `superpowers:subagent-driven-development`（推荐）或 `superpowers:executing-plans`，按任务逐步执行本计划。使用复选框（`- [ ]`）跟踪步骤。

**目标：** 构建一个可运行的 Rust CLI，用于创建 ColdStart 项目主目录、在 SQLite 中持久化项目与事件状态，并查询项目状态。

**架构：** Cargo 工作区将纯领域模型（`coldstart-core`）、SQLite 持久化（`coldstart-storage`）和 CLI 编排（`coldstart-cli`）分开。本阶段不包含 Tauri UI、MCP 服务、GitHub 集成或凭据存储；后续计划会复用这些 crate。

**技术栈：** Rust stable、2024 edition、内置 SQLite 的 `rusqlite` 0.41.1、`clap` 4.6.7、`serde` 1.0.229、`serde_json` 1.0.154、`uuid` 1.29.0、`chrono` 0.4.45、`thiserror` 2.0.21、`tempfile` 3.27.0 和 `assert_cmd` 2.2.2。

**规格：** `docs/superpowers/specs/2026-10-30-coldstart-design.md`

> **完成状态（2026-10-30）：已完成并通过最终门禁。** 实际接口相对初始计划有存储层偏差：SQLite schema 已升级到 v2，`projects` 表不持久化 `root_path`，v1→v2 迁移保留项目与事件；`Database::load_project(&self, id: &ProjectId, root_path: PathBuf) -> Result<Option<Project>, StorageError>` 接受调用方传入的当前项目根路径，并结合数据库中的元数据构造 `Project`。下方全部步骤均已执行。

## 全局约束

- 在 `D:\coldstart` 的 `main` 分支上工作。
- 优先支持 Windows，但领域模型和路径代码必须保持可移植。
- 使用 Rust stable 和 `x86_64-pc-windows-msvc` 目标；最低 Rust 版本为 1.89。
- 使用 2024 edition 和 workspace resolver 3。
- 时间戳统一以 UTC RFC 3339 字符串持久化。
- SQLite 启用 WAL 模式、`foreign_keys=ON` 和 5000 毫秒忙碌超时。
- CLI JSON 键使用 camelCase；Rust 字段名使用 snake_case。
- `coldstart init` 不得覆盖已存在的 `coldstart.project.json`。
- 数据库位于 `<project-home>/.coldstart/state.db`。
- 本计划不得加入 Tauri、React、MCP、GitHub OAuth、凭据存储或 Git 自动化。
- `coldstart.project.json` 只保存可迁移元数据，绝不写入项目绝对根路径。
- 每个任务完成前必须运行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings` 和 `cargo test --workspace`。
- 每个已完成任务使用一次约定式提交。

## 计划文件结构

```text
Cargo.toml
Cargo.lock
rust-toolchain.toml
crates/core/Cargo.toml
crates/core/src/lib.rs
crates/core/src/error.rs
crates/core/src/project.rs
crates/core/src/event.rs
crates/core/src/manifest.rs
crates/core/tests/project_model.rs
crates/core/tests/contracts.rs
crates/storage/Cargo.toml
crates/storage/src/lib.rs
crates/storage/src/database.rs
crates/storage/src/migrations.rs
crates/storage/tests/database.rs
crates/cli/Cargo.toml
crates/cli/src/main.rs
crates/cli/src/error.rs
crates/cli/src/project_home.rs
crates/cli/tests/cli.rs
README.md
```

- `crates/core` 负责经过校验的领域值和可序列化契约，不依赖 SQLite。
- `crates/storage` 负责迁移、项目持久化和追加式事件日志。
- `crates/cli` 负责参数解析、项目主目录文件布局和 JSON 输出。
- 后续 Tauri 和 MCP 消费者必须抽取共享应用函数，不能复制 CLI 的文件或 SQL 逻辑。等到第二个消费者出现时再执行抽取。

本计划只交付路线图中的第一个阶段。Tauri、蓝图、计划执行、MCP、Git、GitHub 和凭据管理均留到各自的后续计划中实施。

---

### 任务 1：初始化 Rust 工作区与项目领域模型

**文件：**
- 创建：`Cargo.toml`
- 创建：`rust-toolchain.toml`
- 创建：`crates/core/Cargo.toml`
- 创建：`crates/core/src/lib.rs`
- 创建：`crates/core/src/error.rs`
- 创建：`crates/core/src/project.rs`
- 测试：`crates/core/tests/project_model.rs`

**接口：**
- 使用：Rust 标准库。
- 产出：`coldstart_core::{Project, ProjectId, ProjectError}`。
- 产出：`Project::new(name: impl Into<String>, root_path: PathBuf) -> Result<Project, ProjectError>`。
- 产出：`ProjectId: Clone + Copy + Display + FromStr + Serialize + Deserialize`。

- [x] **步骤 1：检查并安装 Windows Rust 前置依赖**

运行：

```powershell
Get-Command rustc, cargo -ErrorAction SilentlyContinue
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path -LiteralPath $vswhere) { & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath }
```

预期：`rustc` 和 `cargo` 可解析，且 `vswhere` 输出 Visual Studio 安装路径。当前机器上两项检查均失败。

如果任一检查缺失，请在管理员 PowerShell 中安装：

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-package-agreements --accept-source-agreements --override "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements
```

打开新的 PowerShell 窗口并运行：

```powershell
rustup default stable-x86_64-pc-windows-msvc
rustc --version
cargo --version
```

预期：Rust 为 1.89.0 或更高版本，当前目标为 `x86_64-pc-windows-msvc`。

- [x] **步骤 2：创建工作区和可编译的 core crate 骨架**

创建 `Cargo.toml`：

```toml
[workspace]
members = ["crates/core"]
resolver = "3"

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.89"

[workspace.dependencies]
chrono = { version = "0.4.45", features = ["serde"] }
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.154"
thiserror = "2.0.21"
uuid = { version = "1.29.0", features = ["v4", "serde"] }
rusqlite = { version = "0.41.1", features = ["bundled"] }
clap = { version = "4.6.7", features = ["derive"] }
tempfile = "3.27.0"
assert_cmd = "2.2.2"
```

创建 `rust-toolchain.toml`：

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

创建 `crates/core/Cargo.toml`：

```toml
[package]
name = "coldstart-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
chrono.workspace = true
serde.workspace = true
thiserror.workspace = true
uuid.workspace = true
```

创建 `crates/core/src/lib.rs`：

```rust
// 领域类型将在步骤 3 中通过失败测试引入。
```

运行：

```powershell
cargo check --workspace
```

预期：通过。

- [x] **步骤 3：编写失败的领域测试**

创建 `crates/core/tests/project_model.rs`：

```rust
use std::path::PathBuf;

use chrono::Utc;
use coldstart_core::{Project, ProjectError};

#[test]
fn creates_a_project_with_a_trimmed_name_and_generated_id() {
    let root = std::env::temp_dir().join("coldstart-domain-test");
    let before = Utc::now();

    let project = Project::new("  Demo Project  ", root.clone()).expect("project must be valid");

    assert_eq!(project.name, "Demo Project");
    assert_eq!(project.root_path, root);
    assert!(!project.id.to_string().is_empty());
    assert!(project.created_at >= before);
}

#[test]
fn generates_distinct_project_ids() {
    let root = std::env::temp_dir().join("coldstart-domain-test");
    let first = Project::new("First", root.clone()).expect("project must be valid");
    let second = Project::new("Second", root).expect("project must be valid");

    assert_ne!(first.id, second.id);
}

#[test]
fn rejects_an_empty_project_name() {
    let root = std::env::temp_dir().join("coldstart-domain-test");
    let error = Project::new("   ", root).expect_err("empty name must fail");

    assert_eq!(error, ProjectError::EmptyName);
}

#[test]
fn rejects_a_project_name_longer_than_80_characters() {
    let root = std::env::temp_dir().join("coldstart-domain-test");
    let name = "a".repeat(81);
    let error = Project::new(name, root).expect_err("81-character name must fail");

    assert_eq!(error, ProjectError::NameTooLong);
}

#[test]
fn accepts_a_project_name_of_exactly_80_characters() {
    let root: PathBuf = std::env::temp_dir().join("coldstart-domain-test");
    let name = "a".repeat(80);
    let project = Project::new(name, root).expect("80-character name must be valid");

    assert_eq!(project.name.len(), 80);
}
```

- [x] **步骤 4：运行测试并确认缺少 API 的失败**

运行：

```powershell
cargo test -p coldstart-core --test project_model
```

预期：失败，错误为 `unresolved import coldstart_core::Project`。

- [x] **步骤 5：实现最小项目模型**

创建 `crates/core/src/error.rs`：

```rust
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProjectError {
    #[error("project name cannot be empty")]
    EmptyName,
    #[error("project name must be at most 80 characters")]
    NameTooLong,
}
```

创建 `crates/core/src/project.rs`：

```rust
use std::{fmt, path::PathBuf, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ProjectError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(Uuid);

impl ProjectId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for ProjectId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub root_path: PathBuf,
    pub created_at: DateTime<Utc>,
}

impl Project {
    pub fn new(name: impl Into<String>, root_path: PathBuf) -> Result<Self, ProjectError> {
        let name = name.into().trim().to_owned();

        if name.is_empty() {
            return Err(ProjectError::EmptyName);
        }

        if name.chars().count() > 80 {
            return Err(ProjectError::NameTooLong);
        }

        Ok(Self {
            id: ProjectId::new(),
            name,
            root_path,
            created_at: Utc::now(),
        })
    }
}
```

将 `crates/core/src/lib.rs` 替换为：

```rust
mod error;
mod project;

pub use error::ProjectError;
pub use project::{Project, ProjectId};
```

- [x] **步骤 6：运行领域测试和质量检查**

运行：

```powershell
cargo test -p coldstart-core --test project_model
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

预期：所有测试通过，Clippy 无警告。

- [x] **步骤 7：提交领域基础**

```powershell
git add Cargo.toml Cargo.lock rust-toolchain.toml crates/core
git commit -m "feat: 添加 ColdStart 项目领域模型"
```

---

### 任务 2：定义事件与可迁移清单契约

**文件：**
- 修改：`crates/core/Cargo.toml`
- 创建：`crates/core/src/event.rs`
- 创建：`crates/core/src/manifest.rs`
- 修改：`crates/core/src/lib.rs`
- 测试：`crates/core/tests/contracts.rs`

**接口：**
- 使用：任务 1 的 `coldstart_core::{Project, ProjectId}`。
- 产出：`DomainEvent`、`DomainEventKind` 和 `DomainEvent::project_initialized(project: &Project)`。
- 产出：`ProjectManifest`、`ManifestError` 和 `PROJECT_MANIFEST_SCHEMA_VERSION`。
- 产出：`ProjectManifest::from_project(project: &Project) -> ProjectManifest`。
- 产出：`ProjectManifest::validate(&self) -> Result<(), ManifestError>`。
- 产出：`ProjectManifest::to_project(&self, root_path: PathBuf) -> Result<Project, ManifestError>`。

清单是项目身份文档，不是机器相关的状态文件。它有意省略 `Project::root_path`，调用方根据 `coldstart.project.json` 所在目录推导项目根路径。等平台目标、仓库映射和模板版本这些领域类型实现后，再通过后续清单 schema 修订加入。

- [x] **步骤 1：添加序列化依赖并编写失败的契约测试**

在 `crates/core/Cargo.toml` 的 `[dependencies]` 下添加 `serde_json`：

```toml
serde_json.workspace = true
```

创建 `crates/core/tests/contracts.rs`：

```rust
use std::path::PathBuf;

use chrono::Utc;
use coldstart_core::{
    DomainEvent, DomainEventKind, ManifestError, Project, ProjectManifest,
    PROJECT_MANIFEST_SCHEMA_VERSION,
};

fn sample_project() -> Project {
    Project::new("Demo Project", PathBuf::from("D:\\work\\demo")).expect("project must be valid")
}

#[test]
fn manifest_omits_the_machine_specific_root_path() {
    let manifest = ProjectManifest::from_project(&sample_project());
    let json = serde_json::to_value(&manifest).expect("manifest must serialize");

    assert_eq!(
        json["schemaVersion"].as_u64(),
        Some(u64::from(PROJECT_MANIFEST_SCHEMA_VERSION))
    );
    assert_eq!(json["name"], "Demo Project");
    assert!(json["projectId"].is_string());
    assert!(json.get("rootPath").is_none());
}

#[test]
fn manifest_round_trips_without_losing_project_identity() {
    let manifest = ProjectManifest::from_project(&sample_project());
    let encoded = serde_json::to_string(&manifest).expect("manifest must serialize");
    let decoded: ProjectManifest =
        serde_json::from_str(&encoded).expect("manifest must deserialize");

    assert_eq!(decoded, manifest);
}

#[test]
fn restored_project_uses_the_manifest_directory_as_its_root() {
    let manifest = ProjectManifest::from_project(&sample_project());
    let moved_root = PathBuf::from("E:\\moved\\demo");
    let restored = manifest
        .to_project(moved_root.clone())
        .expect("manifest must produce a project");

    assert_eq!(restored.id, manifest.project_id);
    assert_eq!(restored.name, manifest.name);
    assert_eq!(restored.created_at, manifest.created_at);
    assert_eq!(restored.root_path, moved_root);
}

#[test]
fn rejects_an_unknown_manifest_schema() {
    let mut manifest = ProjectManifest::from_project(&sample_project());
    manifest.schema_version += 1;

    assert_eq!(
        manifest.validate(),
        Err(ManifestError::UnsupportedSchema {
            found: PROJECT_MANIFEST_SCHEMA_VERSION + 1,
            supported: PROJECT_MANIFEST_SCHEMA_VERSION,
        })
    );
}

#[test]
fn rejects_blank_and_oversized_manifest_names() {
    let mut manifest = ProjectManifest::from_project(&sample_project());
    manifest.name = "   ".to_owned();
    assert_eq!(manifest.validate(), Err(ManifestError::EmptyName));

    manifest.name = "a".repeat(81);
    assert_eq!(manifest.validate(), Err(ManifestError::NameTooLong));
}

#[test]
fn project_initialized_event_has_a_stable_serialized_type() {
    let project = sample_project();
    let event = DomainEvent::project_initialized(&project);

    assert_eq!(event.project_id, project.id);
    assert_eq!(
        event.kind,
        DomainEventKind::ProjectInitialized {
            name: "Demo Project".to_owned()
        }
    );
    assert_eq!(event.kind.type_name(), "project_initialized");

    let json = serde_json::to_value(&event).expect("event must serialize");
    assert_eq!(json["kind"]["type"], "project_initialized");
    assert_eq!(json["kind"]["data"]["name"], "Demo Project");
}

#[test]
fn event_identity_and_time_are_generated_per_event() {
    let project = sample_project();
    let before = Utc::now();
    let first = DomainEvent::project_initialized(&project);
    let second = DomainEvent::project_initialized(&project);

    assert_ne!(first.event_id, second.event_id);
    assert!(first.occurred_at >= before);
    assert!(first.occurred_at <= Utc::now());
}
```

- [x] **步骤 2：运行契约测试并确认缺少 API 的失败**

运行：

```powershell
cargo test -p coldstart-core --test contracts
```

预期：失败，因为 `coldstart_core::DomainEvent` 和清单相关导出尚不存在。

- [x] **步骤 3：实现事件和清单契约**

创建 `crates/core/src/event.rs`：

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Project, ProjectId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainEvent {
    pub event_id: Uuid,
    pub project_id: ProjectId,
    pub occurred_at: DateTime<Utc>,
    pub kind: DomainEventKind,
}

impl DomainEvent {
    pub fn project_initialized(project: &Project) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            project_id: project.id,
            occurred_at: Utc::now(),
            kind: DomainEventKind::ProjectInitialized {
                name: project.name.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum DomainEventKind {
    ProjectInitialized { name: String },
}

impl DomainEventKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::ProjectInitialized { .. } => "project_initialized",
        }
    }
}
```

创建 `crates/core/src/manifest.rs`：

```rust
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Project, ProjectId};

pub const PROJECT_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectManifest {
    pub schema_version: u32,
    pub project_id: ProjectId,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

impl ProjectManifest {
    pub fn from_project(project: &Project) -> Self {
        Self {
            schema_version: PROJECT_MANIFEST_SCHEMA_VERSION,
            project_id: project.id,
            name: project.name.clone(),
            created_at: project.created_at,
        }
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != PROJECT_MANIFEST_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchema {
                found: self.schema_version,
                supported: PROJECT_MANIFEST_SCHEMA_VERSION,
            });
        }

        let name = self.name.trim();
        if name.is_empty() {
            return Err(ManifestError::EmptyName);
        }
        if name.chars().count() > 80 {
            return Err(ManifestError::NameTooLong);
        }

        Ok(())
    }

    pub fn to_project(&self, root_path: PathBuf) -> Result<Project, ManifestError> {
        self.validate()?;
        Ok(Project {
            id: self.project_id,
            name: self.name.trim().to_owned(),
            root_path,
            created_at: self.created_at,
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("unsupported project manifest schema {found}; supported schema is {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("project name cannot be empty")]
    EmptyName,
    #[error("project name must be at most 80 characters")]
    NameTooLong,
}
```

将 `crates/core/src/lib.rs` 替换为：

```rust
mod error;
mod event;
mod manifest;
mod project;

pub use error::ProjectError;
pub use event::{DomainEvent, DomainEventKind};
pub use manifest::{ManifestError, ProjectManifest, PROJECT_MANIFEST_SCHEMA_VERSION};
pub use project::{Project, ProjectId};
```

- [x] **步骤 4：运行契约测试和全部质量门禁**

运行：

```powershell
cargo test -p coldstart-core --test contracts
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

预期：通过。

- [x] **步骤 5：提交可迁移契约**

```powershell
git add crates/core
git commit -m "feat: 添加可迁移项目清单与领域事件"
```

---

### 任务 3：使用 SQLite 持久化项目和事件日志

**文件：**
- 修改：`Cargo.toml`
- 创建：`crates/storage/Cargo.toml`
- 创建：`crates/storage/src/lib.rs`
- 创建：`crates/storage/src/database.rs`
- 创建：`crates/storage/src/migrations.rs`
- 测试：`crates/storage/tests/database.rs`

**接口：**
- 使用：`coldstart_core::{DomainEvent, Project, ProjectId}`。
- 产出：`coldstart_storage::{Database, StorageError}`。
- 产出：`Database::open(path: &Path) -> Result<Database, StorageError>`。
- 产出：`Database::initialize_project(&mut self, project: &Project) -> Result<(), StorageError>`。
- 产出：`Database::load_project(&self, id: &ProjectId) -> Result<Option<Project>, StorageError>`。
- 产出：`Database::append_event(&mut self, event: &DomainEvent) -> Result<(), StorageError>`。
- 产出：`Database::event_count(&self, project_id: &ProjectId) -> Result<u64, StorageError>`。

- [x] **步骤 1：添加可编译的 storage crate 和失败的集成测试**

将根目录 `Cargo.toml` 的 workspace 成员改为：

```toml
[workspace]
members = ["crates/core", "crates/storage"]
resolver = "3"
```

创建 `crates/storage/Cargo.toml`：

```toml
[package]
name = "coldstart-storage"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
chrono.workspace = true
coldstart-core = { path = "../core" }
rusqlite.workspace = true
serde_json.workspace = true
thiserror.workspace = true
uuid.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

创建 `crates/storage/src/lib.rs`，暂时只声明步骤 3 将要实现的模块：

```rust
mod database;
mod migrations;

pub use database::{Database, StorageError};
```

创建临时编译占位文件 `crates/storage/src/database.rs`，步骤 3 会替换它：

```rust
// 先用失败的测试确定公开 API，再实现。
```

创建临时编译占位文件 `crates/storage/src/migrations.rs`：

```rust
// schema 迁移将在失败测试之后添加。
```

创建 `crates/storage/tests/database.rs`：

```rust
use coldstart_core::{DomainEvent, Project, ProjectId};
use coldstart_storage::{Database, StorageError};
use tempfile::TempDir;

fn sample_project(dir: &TempDir) -> Project {
    Project::new("Demo Project", dir.path().join("project-home"))
        .expect("project must be valid")
}

fn open_database(dir: &TempDir) -> Database {
    Database::open(&dir.path().join(".coldstart").join("state.db"))
        .expect("database must open")
}

#[test]
fn persists_project_and_initial_event_across_reopen() {
    let dir = TempDir::new().expect("temp directory must exist");
    let project = sample_project(&dir);

    {
        let mut database = open_database(&dir);
        database
            .initialize_project(&project)
            .expect("project must initialize");

        assert_eq!(database.event_count(&project.id).unwrap(), 1);
        assert_eq!(
            database.load_project(&project.id).unwrap(),
            Some(project.clone())
        );
    }

    let database = open_database(&dir);
    assert_eq!(
        database.load_project(&project.id).unwrap(),
        Some(project.clone())
    );
    assert_eq!(database.event_count(&project.id).unwrap(), 1);
}

#[test]
fn rejects_a_duplicate_project_without_adding_an_event() {
    let dir = TempDir::new().expect("temp directory must exist");
    let project = sample_project(&dir);
    let mut database = open_database(&dir);
    database.initialize_project(&project).unwrap();

    let error = database
        .initialize_project(&project)
        .expect_err("duplicate project must fail");

    assert!(matches!(
        error,
        StorageError::ProjectAlreadyExists(id) if id == project.id
    ));
    assert_eq!(database.event_count(&project.id).unwrap(), 1);
}

#[test]
fn appends_events_and_rejects_a_duplicate_event_id() {
    let dir = TempDir::new().expect("temp directory must exist");
    let project = sample_project(&dir);
    let mut database = open_database(&dir);
    database.initialize_project(&project).unwrap();
    let event = DomainEvent::project_initialized(&project);

    database.append_event(&event).unwrap();
    assert_eq!(database.event_count(&project.id).unwrap(), 2);

    let error = database
        .append_event(&event)
        .expect_err("duplicate event must fail");
    assert!(matches!(
        error,
        StorageError::EventAlreadyExists(id) if id == event.event_id
    ));
    assert_eq!(database.event_count(&project.id).unwrap(), 2);
}

#[test]
fn reports_unknown_projects_without_writing_orphan_events() {
    let dir = TempDir::new().expect("temp directory must exist");
    let registered = sample_project(&dir);
    let unregistered = Project::new("Other", dir.path().join("other")).unwrap();
    let mut database = open_database(&dir);
    database.initialize_project(&registered).unwrap();
    let event = DomainEvent::project_initialized(&unregistered);

    let error = database
        .append_event(&event)
        .expect_err("unknown project must fail");

    assert!(matches!(
        error,
        StorageError::ProjectNotFound(id) if id == unregistered.id
    ));
    assert_eq!(database.event_count(&unregistered.id).unwrap(), 0);
    assert_eq!(database.load_project(&ProjectId::new()).unwrap(), None);
}

#[test]
fn creates_missing_database_parent_directories() {
    let dir = TempDir::new().expect("temp directory must exist");
    let root = dir.path().join("deep").join("project");
    let database_path = root.join(".coldstart").join("state.db");

    let database = Database::open(&database_path);

    assert!(database.is_ok(), "database must create its parent directories");
    assert!(database_path.exists());
}
```

- [x] **步骤 2：运行存储测试并确认缺少 API 的失败**

运行：

```powershell
cargo test -p coldstart-storage --test database
```

预期：失败，因为 `Database` 和 `StorageError` 尚未实现。

- [x] **步骤 3：实现迁移和事务持久化**

将 `crates/storage/src/database.rs` 替换为：

```rust
use std::{fs, path::Path, time::Duration};

use chrono::{DateTime, Utc};
use coldstart_core::{DomainEvent, Project, ProjectId};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use thiserror::Error;
use uuid::Uuid;

use crate::migrations;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid project id in storage: {0}")]
    InvalidProjectId(#[from] uuid::Error),
    #[error("invalid timestamp in storage: {0}")]
    InvalidTimestamp(#[from] chrono::ParseError),
    #[error("project {0} already exists")]
    ProjectAlreadyExists(ProjectId),
    #[error("project {0} does not exist")]
    ProjectNotFound(ProjectId),
    #[error("event {0} already exists")]
    EventAlreadyExists(Uuid),
    #[error("database schema version {found} is newer than supported version {supported}")]
    UnsupportedSchema { found: i64, supported: i64 },
    #[error("project root path is not valid UTF-8")]
    NonUtf8ProjectRoot,
    #[error("SQLite did not enable WAL mode; active journal mode is {0}")]
    WalUnavailable(String),
}

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }

        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_millis(5_000))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let journal_mode: String =
            connection.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        if !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(StorageError::WalUnavailable(journal_mode));
        }
        migrations::migrate(&mut connection)?;

        Ok(Self { connection })
    }

    pub fn initialize_project(&mut self, project: &Project) -> Result<(), StorageError> {
        let root_path = project
            .root_path
            .to_str()
            .ok_or(StorageError::NonUtf8ProjectRoot)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if project_exists(&transaction, &project.id)? {
            return Err(StorageError::ProjectAlreadyExists(project.id));
        }

        transaction.execute(
            "INSERT INTO projects (id, name, root_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                project.id.to_string(),
                project.name,
                root_path,
                project.created_at.to_rfc3339(),
            ],
        )?;
        insert_event(&transaction, &DomainEvent::project_initialized(project))?;
        transaction.commit()?;
        Ok(())
    }

    pub fn load_project(&self, id: &ProjectId) -> Result<Option<Project>, StorageError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, name, root_path, created_at FROM projects WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;

        let Some((stored_id, name, root_path, created_at)) = row else {
            return Ok(None);
        };
        let created_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&created_at)?
            .with_timezone(&Utc);

        Ok(Some(Project {
            id: stored_id.parse()?,
            name,
            root_path: root_path.into(),
            created_at,
        }))
    }

    pub fn append_event(&mut self, event: &DomainEvent) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if !project_exists(&transaction, &event.project_id)? {
            return Err(StorageError::ProjectNotFound(event.project_id));
        }
        if event_exists(&transaction, event.event_id)? {
            return Err(StorageError::EventAlreadyExists(event.event_id));
        }

        insert_event(&transaction, event)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn event_count(&self, project_id: &ProjectId) -> Result<u64, StorageError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM events WHERE project_id = ?1",
            [project_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }
}

fn project_exists(transaction: &Transaction<'_>, id: &ProjectId) -> Result<bool, StorageError> {
    let exists = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
        [id.to_string()],
        |row| row.get(0),
    )?;
    Ok(exists)
}

fn event_exists(transaction: &Transaction<'_>, id: Uuid) -> Result<bool, StorageError> {
    let exists = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE event_id = ?1)",
        [id.to_string()],
        |row| row.get(0),
    )?;
    Ok(exists)
}

fn insert_event(transaction: &Transaction<'_>, event: &DomainEvent) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO events (event_id, project_id, occurred_at, event_type, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            event.event_id.to_string(),
            event.project_id.to_string(),
            event.occurred_at.to_rfc3339(),
            event.kind.type_name(),
            serde_json::to_string(&event.kind)?,
        ],
    )?;
    Ok(())
}
```

创建 `crates/storage/src/migrations.rs`：

```rust
use rusqlite::Connection;

use crate::StorageError;

const SCHEMA_VERSION: i64 = 1;

pub(crate) fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
    let current: i64 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;

    if current > SCHEMA_VERSION {
        return Err(StorageError::UnsupportedSchema {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }

    if current == 0 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "
            CREATE TABLE projects (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                root_path TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL UNIQUE,
                project_id TEXT NOT NULL,
                occurred_at TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE INDEX events_by_project ON events(project_id, seq);
            ",
        )?;
        transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        transaction.commit()?;
    }

    Ok(())
}
```

- [x] **步骤 4：运行存储测试和全部质量门禁**

运行：

```powershell
cargo test -p coldstart-storage --test database
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

预期：通过。父目录测试同时证明 `Database::open` 会在目录缺失时创建 `<project-home>/.coldstart/`。

- [x] **步骤 5：提交 SQLite 持久化**

```powershell
git add Cargo.toml Cargo.lock crates/storage
git commit -m "feat: 使用 SQLite 持久化项目与事件"
```

---

### 任务 4：使用 `coldstart init` 创建项目主目录

**文件：**
- 修改：`Cargo.toml`
- 创建：`crates/cli/Cargo.toml`
- 创建：`crates/cli/src/main.rs`
- 创建：`crates/cli/src/error.rs`
- 创建：`crates/cli/src/project_home.rs`
- 测试：`crates/cli/tests/cli.rs`

**接口：**
- 使用：`coldstart_core::{Project, ProjectManifest, ProjectError}`。
- 使用：`coldstart_storage::{Database, StorageError}`。
- 产出：`coldstart init --name <NAME> [--path <PROJECT_HOME>]`。
- 产出：`ProjectHome::initialize(&self, name: &str) -> Result<Project, CliError>`。
- 产出：项目主目录布局 `coldstart.project.json`、`docs/{requirements,architecture,decisions,release}/` 和 `.coldstart/{state.db,artifacts,snapshots}/`。

- [x] **步骤 1：添加 CLI crate 和失败的初始化测试**

将根目录 `Cargo.toml` 的 workspace 成员改为：

```toml
[workspace]
members = ["crates/core", "crates/storage", "crates/cli"]
resolver = "3"
```

创建 `crates/cli/Cargo.toml`：

```toml
[package]
name = "coldstart-cli"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "coldstart"
path = "src/main.rs"

[dependencies]
clap.workspace = true
coldstart-core = { path = "../core" }
coldstart-storage = { path = "../storage" }
serde_json.workspace = true
thiserror.workspace = true

[dev-dependencies]
assert_cmd.workspace = true
tempfile.workspace = true
```

创建临时占位文件 `crates/cli/src/main.rs`：

```rust
fn main() {
    eprintln!("coldstart init is not implemented yet");
    std::process::exit(1);
}
```

创建 `crates/cli/tests/cli.rs`：

```rust
use std::fs;
use std::path::Path;
use std::process::Output;

use assert_cmd::Command;
use tempfile::TempDir;

fn run_init(home: &Path) -> Output {
    Command::cargo_bin("coldstart")
        .expect("binary must build")
        .args(["init", "--name", "Demo Project", "--path"])
        .arg(home)
        .output()
        .expect("command must run")
}

#[test]
fn init_creates_a_portable_project_home() {
    let temp = TempDir::new().expect("temp directory must exist");
    let home = temp.path().join("demo");

    let output = run_init(&home);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(home.join("coldstart.project.json").is_file());
    assert!(home.join(".coldstart").join("state.db").is_file());
    for relative in [
        "docs/requirements",
        "docs/architecture",
        "docs/decisions",
        "docs/release",
        ".coldstart/artifacts",
        ".coldstart/snapshots",
    ] {
        assert!(home.join(relative).is_dir(), "missing {relative}");
    }

    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(home.join("coldstart.project.json")).expect("manifest must be readable"),
    )
    .expect("manifest must be valid JSON");
    assert_eq!(manifest["name"], "Demo Project");
    assert_eq!(manifest["schemaVersion"].as_u64(), Some(1));
    assert!(manifest.get("rootPath").is_none());
}

#[test]
fn init_refuses_to_overwrite_an_existing_project_manifest() {
    let temp = TempDir::new().expect("temp directory must exist");
    let home = temp.path().join("demo");
    let first = run_init(&home);
    assert!(first.status.success());
    let manifest_path = home.join("coldstart.project.json");
    let before = fs::read(&manifest_path).expect("manifest must be readable");

    let second = run_init(&home);

    assert!(!second.status.success());
    let stderr = String::from_utf8_lossy(&second.stderr).to_lowercase();
    assert!(stderr.contains("already initialized"), "stderr: {stderr}");
    let after = fs::read(&manifest_path).expect("manifest must still exist");
    assert_eq!(after, before);
}
```

- [x] **步骤 2：运行 CLI 测试并确认初始化失败**

运行：

```powershell
cargo test -p coldstart-cli --test cli
```

预期：失败，因为占位程序以状态码 1 退出，且不会创建项目主目录。

- [x] **步骤 3：实现项目主目录创建和 init 命令**

创建 `crates/cli/src/error.rs`：

```rust
use std::{io, path::PathBuf};

use coldstart_core::ProjectError;
use coldstart_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("project home path is not a directory: {0}")]
    NotADirectory(PathBuf),
    #[error("project is already initialized: {0}")]
    AlreadyInitialized(PathBuf),
    #[error("state database exists without a project manifest: {0}")]
    StateAlreadyExists(PathBuf),
    #[error("invalid project name: {0}")]
    InvalidProject(#[from] ProjectError),
    #[error("JSON failure: {0}")]
    Json(#[from] serde_json::Error),
    #[error("storage failure: {0}")]
    Storage(#[from] StorageError),
    #[error("filesystem failure: {0}")]
    Io(#[from] io::Error),
}
```

创建 `crates/cli/src/project_home.rs`：

```rust
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use coldstart_core::{Project, ProjectManifest};
use coldstart_storage::Database;

use crate::CliError;

const MANIFEST_FILE: &str = "coldstart.project.json";
const STATE_DIRECTORY: &str = ".coldstart";
const DATABASE_FILE: &str = "state.db";
const PROJECT_DIRECTORIES: [&str; 8] = [
    "docs/requirements",
    "docs/architecture",
    "docs/decisions",
    "docs/release",
    ".coldstart/artifacts",
    ".coldstart/snapshots",
    ".coldstart",
    "docs",
];

pub struct ProjectHome {
    root: PathBuf,
}

impl ProjectHome {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn initialize(&self, name: &str) -> Result<Project, CliError> {
        if self.root.exists() && !self.root.is_dir() {
            return Err(CliError::NotADirectory(self.root.clone()));
        }
        fs::create_dir_all(&self.root)?;

        let manifest_path = self.manifest_path();
        if manifest_path.exists() {
            return Err(CliError::AlreadyInitialized(self.root.clone()));
        }

        let database_path = self.database_path();
        if database_path.exists() {
            return Err(CliError::StateAlreadyExists(database_path));
        }

        for relative in PROJECT_DIRECTORIES {
            fs::create_dir_all(self.root.join(relative))?;
        }

        let project = Project::new(name, std::path::absolute(&self.root)?)?;
        let manifest = ProjectManifest::from_project(&project);
        let mut encoded = serde_json::to_vec_pretty(&manifest)?;
        encoded.push(b'\n');
        let mut manifest_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&manifest_path)?;
        manifest_file.write_all(&encoded)?;
        manifest_file.sync_all()?;
        drop(manifest_file);

        let database_result: Result<(), CliError> = (|| {
            let mut database = Database::open(&database_path)?;
            database.initialize_project(&project)?;
            Ok(())
        })();
        if let Err(error) = database_result {
            let _ = fs::remove_file(&manifest_path);
            remove_database_files(&database_path);
            return Err(error);
        }

        Ok(project)
    }

    fn manifest_path(&self) -> PathBuf {
        self.root.join(MANIFEST_FILE)
    }

    fn database_path(&self) -> PathBuf {
        self.root.join(STATE_DIRECTORY).join(DATABASE_FILE)
    }
}

fn remove_database_files(database_path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut candidate: OsString = database_path.as_os_str().to_owned();
        candidate.push(suffix);
        let _ = fs::remove_file(PathBuf::from(candidate));
    }
}
```

将 `crates/cli/src/main.rs` 替换为：

```rust
mod error;
mod project_home;

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use project_home::ProjectHome;

use crate::error::CliError;

#[derive(Debug, Parser)]
#[command(name = "coldstart", version, about = "ColdStart project control plane")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
}

fn main() -> ExitCode {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Init { name, path } => {
            let home = ProjectHome::at(path);
            let project = home.initialize(&name)?;
            println!(
                "Initialized project \"{}\" at {}",
                project.name,
                project.root_path.display()
            );
        }
    }
    Ok(())
}
```

- [x] **步骤 4：运行 CLI 测试和全部质量门禁**

运行：

```powershell
cargo test -p coldstart-cli --test cli
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

预期：通过。如果写入清单后数据库初始化失败，实现会删除清单以及本次刚创建的数据库、WAL 和 SHM 文件。

- [x] **步骤 5：提交项目主目录初始化**

```powershell
git add Cargo.toml Cargo.lock crates/cli
git commit -m "feat: 初始化 ColdStart 项目主目录"
```

---

### 任务 5：输出项目状态并完成 CLI 端到端闭环

**文件：**
- 修改：`crates/cli/src/error.rs`
- 修改：`crates/cli/src/project_home.rs`
- 修改：`crates/cli/src/main.rs`
- 修改：`crates/cli/tests/cli.rs`
- 修改：`README.md`

**接口：**
- 使用：任务 4 的 `ProjectHome::initialize` 和 `ProjectManifest`。
- 使用：任务 3 的 `Database::{open, load_project, event_count}`。
- 产出：`coldstart status [--path <PROJECT_HOME>] [--json]`。
- 产出 JSON 键 `projectId`、`name`、`rootPath`、`createdAt` 和 `eventCount`。

- [x] **步骤 1：添加失败的状态查询与端到端测试**

在 `crates/cli/tests/cli.rs` 中添加以下导入：

```rust
use std::path::PathBuf;
```

在 `crates/cli/tests/cli.rs` 末尾追加以下辅助函数和测试：

```rust
fn run_status(home: &Path) -> Output {
    Command::cargo_bin("coldstart")
        .expect("binary must build")
        .args(["status", "--json", "--path"])
        .arg(home)
        .output()
        .expect("command must run")
}

#[test]
fn status_reports_the_initialized_project_as_camel_case_json() {
    let temp = TempDir::new().expect("temp directory must exist");
    let home = temp.path().join("demo");
    let initialized = run_init(&home);
    assert!(initialized.status.success());
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(home.join("coldstart.project.json")).expect("manifest must be readable"),
    )
    .expect("manifest must be valid JSON");

    let output = run_status(&home);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("status must be valid JSON");
    let expected_root: PathBuf = std::path::absolute(&home).expect("path must be absolute");
    assert_eq!(status["projectId"], manifest["projectId"]);
    assert_eq!(status["name"], "Demo Project");
    assert_eq!(status["rootPath"].as_str(), expected_root.to_str());
    assert_eq!(status["createdAt"], manifest["createdAt"]);
    assert_eq!(status["eventCount"].as_u64(), Some(1));
}

#[test]
fn status_derives_the_root_after_the_project_home_moves() {
    let temp = TempDir::new().expect("temp directory must exist");
    let original_home = temp.path().join("original");
    let moved_home = temp.path().join("moved");
    assert!(run_init(&original_home).status.success());
    fs::rename(&original_home, &moved_home).expect("project home must move");

    let output = run_status(&moved_home);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("status must be valid JSON");
    let expected_root = std::path::absolute(&moved_home).expect("path must be absolute");
    assert_eq!(status["rootPath"].as_str(), expected_root.to_str());
}

#[test]
fn status_fails_for_an_empty_directory() {
    let temp = TempDir::new().expect("temp directory must exist");

    let output = run_status(temp.path());

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    assert!(stderr.contains("manifest is missing"), "stderr: {stderr}");
}

#[test]
fn status_does_not_recreate_a_missing_database() {
    let temp = TempDir::new().expect("temp directory must exist");
    let home = temp.path().join("demo");
    assert!(run_init(&home).status.success());
    let database_path = home.join(".coldstart").join("state.db");
    fs::remove_file(&database_path).expect("database must be removable");

    let output = run_status(&home);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    assert!(stderr.contains("database is missing"), "stderr: {stderr}");
    assert!(!database_path.exists());
}
```

- [x] **步骤 2：运行状态测试并确认缺少子命令的失败**

运行：

```powershell
cargo test -p coldstart-cli --test cli
```

预期：失败，因为 Clap 报告无法识别 `status` 子命令。

- [x] **步骤 3：实现清单加载和状态输出**

将 `crates/cli/src/error.rs` 替换为：

```rust
use std::{io, path::PathBuf};

use coldstart_core::{ManifestError, ProjectError, ProjectId};
use coldstart_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("project home path is not a directory: {0}")]
    NotADirectory(PathBuf),
    #[error("project is already initialized: {0}")]
    AlreadyInitialized(PathBuf),
    #[error("state database exists without a project manifest: {0}")]
    StateAlreadyExists(PathBuf),
    #[error("project manifest is missing: {0}")]
    ManifestMissing(PathBuf),
    #[error("project database is missing: {0}")]
    DatabaseMissing(PathBuf),
    #[error("invalid project manifest: {0}")]
    InvalidManifest(#[from] ManifestError),
    #[error("invalid project name: {0}")]
    InvalidProject(#[from] ProjectError),
    #[error("project {0} is missing from the database")]
    ProjectMissingFromDatabase(ProjectId),
    #[error("project manifest and database metadata disagree for project {0}")]
    ProjectMetadataMismatch(ProjectId),
    #[error("project path is not valid UTF-8: {0}")]
    NonUtf8Path(PathBuf),
    #[error("JSON failure: {0}")]
    Json(#[from] serde_json::Error),
    #[error("storage failure: {0}")]
    Storage(#[from] StorageError),
    #[error("filesystem failure: {0}")]
    Io(#[from] io::Error),
}
```

在 `impl ProjectHome` 中添加以下方法：

```rust
    pub fn absolute_root(&self) -> Result<PathBuf, CliError> {
        Ok(std::path::absolute(&self.root)?)
    }

    pub fn database_path(&self) -> PathBuf {
        self.root.join(STATE_DIRECTORY).join(DATABASE_FILE)
    }

    pub fn load_manifest(&self) -> Result<ProjectManifest, CliError> {
        let path = self.manifest_path();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(CliError::ManifestMissing(path));
            }
            Err(error) => return Err(error.into()),
        };
        let manifest: ProjectManifest = serde_json::from_slice(&bytes)?;
        manifest.validate()?;
        Ok(manifest)
    }
```

删除上一任务中现已重复的私有 `database_path` 方法。`manifest_path` 继续保持私有。

将 `crates/cli/src/main.rs` 替换为：

```rust
mod error;
mod project_home;

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use coldstart_storage::Database;
use project_home::ProjectHome;
use serde_json::json;

use crate::error::CliError;

#[derive(Debug, Parser)]
#[command(name = "coldstart", version, about = "ColdStart project control plane")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    Status {
        #[arg(long, default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Init { name, path } => {
            let home = ProjectHome::at(path);
            let project = home.initialize(&name)?;
            println!(
                "Initialized project \"{}\" at {}",
                project.name,
                project.root_path.display()
            );
        }
        Command::Status { path, json } => {
            print_status(ProjectHome::at(path), json)?;
        }
    }
    Ok(())
}

fn print_status(home: ProjectHome, as_json: bool) -> Result<(), CliError> {
    let root = home.absolute_root()?;
    let manifest = home.load_manifest()?;
    let project = manifest.to_project(root)?;
    let database_path = home.database_path();
    if !database_path.exists() {
        return Err(CliError::DatabaseMissing(database_path));
    }

    let database = Database::open(&database_path)?;
    let stored = database
        .load_project(&project.id)?
        .ok_or(CliError::ProjectMissingFromDatabase(project.id))?;
    if stored.name != project.name || stored.created_at != project.created_at {
        return Err(CliError::ProjectMetadataMismatch(project.id));
    }
    let event_count = database.event_count(&project.id)?;
    let root_path = project
        .root_path
        .to_str()
        .ok_or_else(|| CliError::NonUtf8Path(project.root_path.clone()))?;
    let created_at = serde_json::to_value(project.created_at)?;

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "projectId": project.id,
                "name": project.name,
                "rootPath": root_path,
                "createdAt": created_at,
                "eventCount": event_count,
            }))?
        );
    } else {
        println!("Project: {}", project.name);
        println!("ID: {}", project.id);
        println!("Root: {root_path}");
        println!("Created: {}", created_at.as_str().unwrap_or_default());
        println!("Events: {event_count}");
    }

    Ok(())
}
```

- [x] **步骤 4：记录可用的 CLI 用法**

将 `README.md` 中的 `## 当前状态` 小节替换为：

````markdown
## 当前状态

第一阶段 Rust 核心与 CLI 已实现：可以创建可迁移的项目主目录，在本地 SQLite 中保存项目和追加式事件，并查询项目状态。Tauri 桌面界面、蓝图、计划引擎、MCP、Git 与凭据管理仍在后续阶段。

```powershell
cargo run -p coldstart-cli -- init --name "Demo Project" --path D:\work\demo
cargo run -p coldstart-cli -- status --path D:\work\demo --json
```

`coldstart.project.json` 不保存机器绝对路径；移动到其他目录后，项目根路径由该文件所在目录重新推导。
````

- [x] **步骤 5：运行完整的端到端质量门禁**

运行：

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

预期：所有 CLI 测试通过，包括初始化、拒绝覆盖、JSON 状态、清单缺失失败和数据库缺失安全处理。

- [x] **步骤 6：提交状态查询与文档**

```powershell
git add crates/cli README.md
git commit -m "feat: 输出 ColdStart 项目状态"
```

## 阶段验收追踪

| 阶段要求 | 覆盖任务 |
| --- | --- |
| 具有纯领域、存储和 CLI 边界的 Rust 工作区 | 任务 1-4 |
| 可迁移的项目身份与追加式事件契约 | 任务 2 |
| SQLite WAL、外键、忙碌超时、迁移和事件持久化 | 任务 3 |
| 项目主目录布局与拒绝覆盖 | 任务 4 |
| 不依赖机器的 `coldstart.project.json` | 任务 2、4、5 |
| `coldstart init` 和 `coldstart status --json` | 任务 4-5 |
| 第一阶段路线图的端到端验证 | 任务 5 |

后续计划再实现：Tauri 桌面壳、React UI、文档导入、蓝图生成、计划 DAG、MCP、租约、Git/GitHub、凭据和发布门禁。
