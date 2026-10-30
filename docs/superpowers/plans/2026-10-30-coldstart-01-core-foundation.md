# ColdStart Core Foundation and CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a runnable Rust CLI that creates a ColdStart Project Home, persists project and event state in SQLite, and reports project status.

**Architecture:** A Cargo workspace separates the pure domain model (`coldstart-core`), SQLite persistence (`coldstart-storage`), and CLI orchestration (`coldstart-cli`). This slice has no Tauri UI, MCP server, GitHub integration, or secret storage; later plans reuse these crates.

**Tech Stack:** Rust stable with edition 2024, `rusqlite` 0.41.1 with bundled SQLite, `clap` 4.6.7, `serde` 1.0.229, `serde_json` 1.0.154, `uuid` 1.29.0, `chrono` 0.4.45, `thiserror` 2.0.21, `tempfile` 3.27.0, and `assert_cmd` 2.2.2.

**Spec:** `docs/superpowers/specs/2026-10-30-coldstart-design.md`

## Global Constraints

- Work from `D:\coldstart` on branch `main`.
- Target Windows first, but keep domain and path code portable.
- Use Rust stable with `x86_64-pc-windows-msvc`; minimum Rust version is 1.85.
- Use edition 2024 and workspace resolver 3.
- Persist timestamps as UTC RFC 3339 strings.
- Use SQLite WAL mode, `foreign_keys=ON`, and a 5000 ms busy timeout.
- CLI JSON keys use camelCase; Rust field names use snake_case.
- `coldstart init` refuses to overwrite an existing `coldstart.project.json`.
- The database lives at `<project-home>/.coldstart/state.db`.
- Do not add Tauri, React, MCP, GitHub OAuth, secret storage, or Git automation in this plan.
- Store only portable metadata in `coldstart.project.json`; never write an absolute project root into it.
- After every task, run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` before committing.
- Use one conventional commit per completed task.

## Planned File Structure

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

- `crates/core` owns validated domain values and serializable contracts. It has no SQLite dependency.
- `crates/storage` owns migrations, project persistence, and the append-only event log.
- `crates/cli` owns argument parsing, Project Home file layout, and JSON output.
- Later Tauri and MCP consumers must extract shared application functions instead of duplicating CLI file or SQL logic. That extraction is intentionally deferred until the second consumer exists.

This plan produces only the first roadmap slice. Tauri, blueprints, plan execution, MCP, Git, GitHub, and credentials remain out of scope until their corresponding plans.

---

### Task 1: Bootstrap the Rust Workspace and Project Domain Model

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `crates/core/Cargo.toml`
- Create: `crates/core/src/lib.rs`
- Create: `crates/core/src/error.rs`
- Create: `crates/core/src/project.rs`
- Test: `crates/core/tests/project_model.rs`

**Interfaces:**
- Consumes: Rust standard library.
- Produces: `coldstart_core::{Project, ProjectId, ProjectError}`.
- Produces: `Project::new(name: impl Into<String>, root_path: PathBuf) -> Result<Project, ProjectError>`.
- Produces: `ProjectId: Clone + Copy + Display + FromStr + Serialize + Deserialize`.

- [ ] **Step 1: Verify and install the Windows Rust prerequisites**

Run:

```powershell
Get-Command rustc, cargo -ErrorAction SilentlyContinue
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path -LiteralPath $vswhere) { & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath }
```

Expected: `rustc` and `cargo` resolve, and `vswhere` prints a Visual Studio installation path. Both checks currently fail on this machine.

If either check is missing, install from an elevated PowerShell:

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-package-agreements --accept-source-agreements --override "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements
```

Open a new PowerShell window and run:

```powershell
rustup default stable-x86_64-pc-windows-msvc
rustc --version
cargo --version
```

Expected: Rust is 1.85.0 or newer and the active target is `x86_64-pc-windows-msvc`.

- [ ] **Step 2: Create the workspace and a compiling core-crate skeleton**

Create `Cargo.toml`:

```toml
[workspace]
members = ["crates/core"]
resolver = "3"

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.85"

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

Create `rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

Create `crates/core/Cargo.toml`:

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

Create `crates/core/src/lib.rs`:

```rust
// Domain types are added by the failing test in Step 3.
```

Run:

```powershell
cargo check --workspace
```

Expected: PASS.

- [ ] **Step 3: Write the failing project-domain test**

Create `crates/core/tests/project_model.rs`:

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

- [ ] **Step 4: Run the test and confirm the missing-API failure**

Run:

```powershell
cargo test -p coldstart-core --test project_model
```

Expected: FAIL with `unresolved import coldstart_core::Project`.

- [ ] **Step 5: Implement the minimal project model**

Create `crates/core/src/error.rs`:

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

Create `crates/core/src/project.rs`:

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

Replace `crates/core/src/lib.rs` with:

```rust
mod error;
mod project;

pub use error::ProjectError;
pub use project::{Project, ProjectId};
```

- [ ] **Step 6: Run the domain tests and quality checks**

Run:

```powershell
cargo test -p coldstart-core --test project_model
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all tests PASS and Clippy reports no warnings.

- [ ] **Step 7: Commit the domain foundation**

```powershell
git add Cargo.toml Cargo.lock rust-toolchain.toml crates/core
git commit -m "feat: add the ColdStart project domain model"
```

---

### Task 2: Define Event and Portable Manifest Contracts

**Files:**
- Modify: `crates/core/Cargo.toml`
- Create: `crates/core/src/event.rs`
- Create: `crates/core/src/manifest.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/contracts.rs`

**Interfaces:**
- Consumes: `coldstart_core::{Project, ProjectId}` from Task 1.
- Produces: `DomainEvent`, `DomainEventKind`, and `DomainEvent::project_initialized(project: &Project)`.
- Produces: `ProjectManifest`, `ManifestError`, and `PROJECT_MANIFEST_SCHEMA_VERSION`.
- Produces: `ProjectManifest::from_project(project: &Project) -> ProjectManifest`.
- Produces: `ProjectManifest::validate(&self) -> Result<(), ManifestError>`.
- Produces: `ProjectManifest::to_project(&self, root_path: PathBuf) -> Result<Project, ManifestError>`.

The manifest is an identity document, not a machine-local state file. It deliberately omits `Project::root_path`; callers derive that path from the directory containing `coldstart.project.json`. Platform targets, repository mappings, and the template version will be added with a later manifest schema revision when those domain types exist.

- [ ] **Step 1: Add the serialization dependency and failing contract tests**

Add `serde_json` under `[dependencies]` in `crates/core/Cargo.toml`:

```toml
serde_json.workspace = true
```

Create `crates/core/tests/contracts.rs`:

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

- [ ] **Step 2: Run the contract tests and confirm the missing-API failure**

Run:

```powershell
cargo test -p coldstart-core --test contracts
```

Expected: FAIL because `coldstart_core::DomainEvent` and the manifest exports do not exist yet.

- [ ] **Step 3: Implement the event and manifest contracts**

Create `crates/core/src/event.rs`:

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

Create `crates/core/src/manifest.rs`:

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

Replace `crates/core/src/lib.rs` with:

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

- [ ] **Step 4: Run the contract tests and all quality gates**

Run:

```powershell
cargo test -p coldstart-core --test contracts
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 5: Commit the portable contracts**

```powershell
git add crates/core
git commit -m "feat: add portable project manifests and domain events"
```

---

### Task 3: Persist Projects and Event Logs in SQLite

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/storage/Cargo.toml`
- Create: `crates/storage/src/lib.rs`
- Create: `crates/storage/src/database.rs`
- Create: `crates/storage/src/migrations.rs`
- Test: `crates/storage/tests/database.rs`

**Interfaces:**
- Consumes: `coldstart_core::{DomainEvent, Project, ProjectId}`.
- Produces: `coldstart_storage::{Database, StorageError}`.
- Produces: `Database::open(path: &Path) -> Result<Database, StorageError>`.
- Produces: `Database::initialize_project(&mut self, project: &Project) -> Result<(), StorageError>`.
- Produces: `Database::load_project(&self, id: &ProjectId) -> Result<Option<Project>, StorageError>`.
- Produces: `Database::append_event(&mut self, event: &DomainEvent) -> Result<(), StorageError>`.
- Produces: `Database::event_count(&self, project_id: &ProjectId) -> Result<u64, StorageError>`.

- [ ] **Step 1: Add a compiling storage crate and the failing integration tests**

Change the workspace members in the root `Cargo.toml` to:

```toml
[workspace]
members = ["crates/core", "crates/storage"]
resolver = "3"
```

Create `crates/storage/Cargo.toml`:

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

Create `crates/storage/src/lib.rs` with only the modules that will be implemented in Step 3:

```rust
mod database;
mod migrations;

pub use database::{Database, StorageError};
```

Create `crates/storage/src/database.rs` as a temporary compile stub; it will be replaced in Step 3:

```rust
// The failing tests define the public API before implementation.
```

Create `crates/storage/src/migrations.rs` as a temporary compile stub:

```rust
// The schema migration is added after the failing tests.
```

Create `crates/storage/tests/database.rs`:

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

- [ ] **Step 2: Run the storage tests and confirm the missing-API failure**

Run:

```powershell
cargo test -p coldstart-storage --test database
```

Expected: FAIL because `Database` and `StorageError` have no implementation.

- [ ] **Step 3: Implement migrations and transactional persistence**

Replace `crates/storage/src/database.rs` with:

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

Create `crates/storage/src/migrations.rs`:

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

- [ ] **Step 4: Run the storage tests and all quality gates**

Run:

```powershell
cargo test -p coldstart-storage --test database
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: PASS. The parent-directory test also proves `Database::open` creates `<project-home>/.coldstart/` when missing.

- [ ] **Step 5: Commit SQLite persistence**

```powershell
git add Cargo.toml Cargo.lock crates/storage
git commit -m "feat: persist projects and events in SQLite"
```

---

### Task 4: Create the Project Home with `coldstart init`

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/cli/Cargo.toml`
- Create: `crates/cli/src/main.rs`
- Create: `crates/cli/src/error.rs`
- Create: `crates/cli/src/project_home.rs`
- Test: `crates/cli/tests/cli.rs`

**Interfaces:**
- Consumes: `coldstart_core::{Project, ProjectManifest, ProjectError}`.
- Consumes: `coldstart_storage::{Database, StorageError}`.
- Produces: `coldstart init --name <NAME> [--path <PROJECT_HOME>]`.
- Produces: `ProjectHome::initialize(&self, name: &str) -> Result<Project, CliError>`.
- Produces: Project Home layout `coldstart.project.json`, `docs/{requirements,architecture,decisions,release}/`, and `.coldstart/{state.db,artifacts,snapshots}/`.

- [ ] **Step 1: Add the CLI crate and failing initialization tests**

Change the workspace members in the root `Cargo.toml` to:

```toml
[workspace]
members = ["crates/core", "crates/storage", "crates/cli"]
resolver = "3"
```

Create `crates/cli/Cargo.toml`:

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

Create `crates/cli/src/main.rs` as a temporary stub:

```rust
fn main() {
    eprintln!("coldstart init is not implemented yet");
    std::process::exit(1);
}
```

Create `crates/cli/tests/cli.rs`:

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

- [ ] **Step 2: Run the CLI tests and confirm the init failure**

Run:

```powershell
cargo test -p coldstart-cli --test cli
```

Expected: FAIL because the stub exits with status 1 and does not create a Project Home.

- [ ] **Step 3: Implement Project Home creation and the init command**

Create `crates/cli/src/error.rs`:

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

Create `crates/cli/src/project_home.rs`:

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

Replace `crates/cli/src/main.rs` with:

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

- [ ] **Step 4: Run CLI tests and all quality gates**

Run:

```powershell
cargo test -p coldstart-cli --test cli
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: PASS. If the database cannot initialize after the manifest is written, the implementation removes the manifest and any database/WAL/SHM files it just created.

- [ ] **Step 5: Commit Project Home initialization**

```powershell
git add Cargo.toml Cargo.lock crates/cli
git commit -m "feat: initialize ColdStart project homes"
```

---

### Task 5: Report Project Status and Lock the CLI Flow End to End

**Files:**
- Modify: `crates/cli/src/error.rs`
- Modify: `crates/cli/src/project_home.rs`
- Modify: `crates/cli/src/main.rs`
- Modify: `crates/cli/tests/cli.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: `ProjectHome::initialize` and `ProjectManifest` from Task 4.
- Consumes: `Database::{open, load_project, event_count}` from Task 3.
- Produces: `coldstart status [--path <PROJECT_HOME>] [--json]`.
- Produces JSON keys `projectId`, `name`, `rootPath`, `createdAt`, and `eventCount`.

- [ ] **Step 1: Add failing status and end-to-end tests**

Add these imports to `crates/cli/tests/cli.rs`:

```rust
use std::path::PathBuf;
```

Append these helpers and tests to `crates/cli/tests/cli.rs`:

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

- [ ] **Step 2: Run the status tests and confirm the missing-subcommand failure**

Run:

```powershell
cargo test -p coldstart-cli --test cli
```

Expected: FAIL because Clap reports an unrecognized `status` subcommand.

- [ ] **Step 3: Implement manifest loading and status output**

Replace `crates/cli/src/error.rs` with:

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

Add these methods inside `impl ProjectHome`:

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

Remove the now-duplicated private `database_path` method from the previous task. Keep `manifest_path` private.

Replace `crates/cli/src/main.rs` with:

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

- [ ] **Step 4: Document the working CLI**

Replace the `## 当前状态` section in `README.md` with:

````markdown
## 当前状态

第一阶段 Rust 核心与 CLI 已实现：可以创建可迁移的 Project Home，在本地 SQLite 中保存项目和追加式事件，并查询项目状态。Tauri 桌面界面、蓝图、计划引擎、MCP、Git 与凭据管理仍在后续阶段。

```powershell
cargo run -p coldstart-cli -- init --name "Demo Project" --path D:\work\demo
cargo run -p coldstart-cli -- status --path D:\work\demo --json
```

`coldstart.project.json` 不保存机器绝对路径；移动到其他目录后，项目根路径由该文件所在目录重新推导。
````

- [ ] **Step 5: Run the full end-to-end quality gate**

Run:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all CLI tests PASS, including init, overwrite refusal, JSON status, missing-manifest failure, and missing-database safety.

- [ ] **Step 6: Commit status reporting and documentation**

```powershell
git add crates/cli README.md
git commit -m "feat: report ColdStart project status"
```

## Phase Acceptance Trace

| Phase requirement | Covered by |
| --- | --- |
| Rust workspace with pure domain, storage, and CLI boundaries | Tasks 1-4 |
| Portable project identity and append-only event contracts | Task 2 |
| SQLite WAL, foreign keys, busy timeout, migration, and event persistence | Task 3 |
| Project Home layout and refusal to overwrite | Task 4 |
| Machine-independent `coldstart.project.json` | Tasks 2, 4, and 5 |
| `coldstart init` and `coldstart status --json` | Tasks 4-5 |
| End-to-end validation of the first roadmap slice | Task 5 |

Deferred to later plans by design: Tauri shell, React UI, document import, blueprint generation, plan DAG, MCP, leases, Git/GitHub, credentials, and release gates.
