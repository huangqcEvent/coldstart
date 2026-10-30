# ColdStart 需求与蓝图实施计划

> **供智能体执行者使用：** 必须使用 `superpowers:executing-plans`，按任务逐步执行本计划。使用复选框（`- [ ]`）跟踪步骤。

**目标：** 让用户把 Markdown、TXT、DOCX、PDF 需求文档导入 ColdStart，通过 OpenAI 兼容模型生成可追溯的结构化蓝图，并可人工编辑、发布和在 Inbox 中处理缺失信息。

**架构：** `coldstart-core` 定义需求来源、蓝图、信息请求和稳定 ID；`coldstart-adapters` 负责文档抽取与 OpenAI 兼容模型调用；`coldstart-storage` 用 schema v3 保存不可变蓝图版本、来源锚点和事件；`coldstart-app` 编排导入、生成、编辑、发布和 Inbox；Tauri 只做显式 IPC 适配，React 不接触文件系统、SQLite 或密钥持久化。

**技术栈：** Rust 1.89、2024 edition、Tauri 2、React、TypeScript、Vite、Vitest、pnpm、SQLite、Markdown/TXT/DOCX/PDF 文本提取、OpenAI 兼容 JSON 接口。

**规格：** `docs/superpowers/specs/2026-10-30-coldstart-design.md`

**前置阶段：** `docs/superpowers/plans/2026-10-30-coldstart-02-desktop-shell.md`

> **完成状态（2026-10-30）：已完成并通过全量门禁与真实桌面验收。** 实现范围包括 Markdown/TXT/DOCX/PDF 导入、来源锚点与内容哈希、OpenAI 兼容模型调用、来源真实性校验、结构化蓝图人工编辑、不可变发布、Inbox 回答与版本化项目事实。无来源生成在界面前置禁用，Tauri/Rust 边界仍返回稳定的 `no_requirement_sources` 错误；扫描 PDF 返回 `document_empty`，不实现 OCR。代码已实现但尚未 commit/push，入口与验收记录见 `.superpowers/sdd/2026-10-30-coldstart-03-requirements-blueprint/progress.md`。

## 全局约束

- 在 `D:\coldstart` 的 `main` 分支工作，不新建 worktree；不自动 commit 或 push，除非用户明确要求。
- 文档使用中文；Rust 标识、路径、命令、JSON 键保留英文。
- 使用 Rust stable、最低 Rust 1.89、2024 edition 和 workspace resolver 3。
- 所有 Node/Rust 依赖、缓存和构建产物必须位于 D 盘。
- Rust 负责领域、文件读取、文档抽取、模型 HTTP、SQLite 和凭据边界；React 只负责界面和交互状态。
- 前端不得直接访问文件系统、SQLite、Shell 或模型 API Key 存储；只通过显式 Tauri 命令传值。
- 模型 API Key 只保存在当前 React 组件的内存中，并仅在本次生成调用时通过 IPC 传递；不得写入项目文件、SQLite、事件、日志或错误消息。
- 调用模型前，UI 必须让用户明确确认“当前需求文档将发送到所配置的模型服务”。
- 本阶段只支持 Web、管理后台、服务端、iOS/Android App 和微信等小程序；不扩展其他行业模板。
- 蓝图内容必须是结构化数据，不是自由文本；每个 AI 提取的实质条目必须带来源文档 ID、锚点 ID 和内容哈希对应的导入记录。
- 蓝图的稳定实体 ID 由 `ProjectId + 实体类型 + 规范化名称` 派生；同一蓝图内重复名称必须拒绝。
- 蓝图发布前必须通过结构、来源引用和模块依赖 DAG 校验；失败不得生成可发布版本。
- 原文件最大 20 MiB；单次抽取的锚点文本总量上限 80,000 字符；超出时返回明确错误，不静默截断。
- 支持扫描版 PDF 的“无文本”错误，不在本阶段实现 OCR。
- 本阶段不实现计划编译、MCP、Git/GitHub、第三方能力连接、发布自动化和安装包。
- Rust 门禁：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked` 必须通过。
- 前端门禁：`pnpm --dir web typecheck`、`pnpm --dir web test`、`pnpm --dir web build` 必须通过。
- Tauri 门禁：`pnpm --dir apps/desktop tauri build --no-bundle` 必须通过。

## 文件结构

```text
crates/core/src/entity_id.rs
crates/core/src/artifact.rs
crates/core/src/blueprint.rs
crates/core/src/info_request.rs
crates/core/src/source_reference.rs
crates/core/tests/blueprint.rs

crates/adapters/Cargo.toml
crates/adapters/src/lib.rs
crates/adapters/src/document.rs
crates/adapters/src/model.rs
crates/adapters/tests/document.rs
crates/adapters/tests/model.rs

crates/storage/src/migrations.rs
crates/storage/src/database.rs
crates/storage/tests/requirements.rs

crates/app/src/context.rs
crates/app/src/requirements.rs
crates/app/src/workspace.rs
crates/app/src/error.rs
crates/app/tests/requirements.rs

apps/desktop/src-tauri/src/commands.rs
apps/desktop/src-tauri/src/lib.rs

web/src/api/types.ts
web/src/api/requirementsApi.ts
web/src/components/AppSidebar.tsx
web/src/components/ProjectOverview.tsx
web/src/components/RequirementsView.tsx
web/src/components/BlueprintView.tsx
web/src/components/BlueprintEditor.tsx
web/src/components/InboxView.tsx
web/src/App.tsx
web/src/styles.css
web/src/App.test.tsx

README.md
docs/superpowers/plans/2026-10-30-coldstart-delivery-roadmap.md
```

- `crates/core` 不依赖 Tauri、SQLite、HTTP、PDF 解析器或 React。
- `crates/adapters` 不依赖 SQLite 和 React；只依赖 core、文档/网络/序列化库。
- `crates/app` 是唯一编排文件、数据库和模型适配器的地方。
- `apps/desktop` 每个命令只做参数转换、`spawn_blocking` 和错误序列化。
- `web` 不拼接项目路径，不读取原文件，不保存 API Key。

---

### Task 1: 蓝图、来源与 Inbox 领域模型

**Files:**
- Create: `crates/core/src/entity_id.rs`
- Create: `crates/core/src/source_reference.rs`
- Create: `crates/core/src/artifact.rs`
- Create: `crates/core/src/blueprint.rs`
- Create: `crates/core/src/info_request.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/Cargo.toml`
- Modify: `Cargo.toml`
- Test: `crates/core/tests/blueprint.rs`

**Interfaces:**
- Produces: `ArtifactId`, `BlueprintRevisionId`, `InfoRequestId`, `ProjectFactId`, `BlueprintEntityId`，均实现 `Serialize`/`Deserialize`。
- Produces: `ArtifactFormat`, `DocumentAnchor`, `ArtifactMetadata`。
- Produces: `SourceReference`、`BlueprintContent`、`BlueprintRevision` 及所有蓝图实体。
- Produces: `BlueprintProposal` 及 `Proposed*` DTO；`BlueprintContent::from_proposal(project_id, proposal)`。
- Produces: `validate_blueprint_structure(&BlueprintContent) -> Result<(), BlueprintValidationError>`。
- Produces: `InfoRequestCategory`、`InfoRequestStatus`、`InfoRequestDraft`、`InfoRequest`、`ProjectFact`。
- Produces: `BlueprintValidationError`、`ProposalConversionError` 及稳定错误文本。

- [x] **Step 1: 写失败测试，锁定稳定 ID、来源和工作区引用**

```rust
#[test]
fn derives_stable_ids_from_project_and_normalized_name() {
    let project_id = ProjectId::new();
    let first = BlueprintEntityId::derive(&project_id, "module", "订单 管理");
    let second = BlueprintEntityId::derive(&project_id, "module", " 订单   管理 ");
    let other_project = BlueprintEntityId::derive(&ProjectId::new(), "module", "订单 管理");

    assert_eq!(first, second);
    assert_ne!(first, other_project);
    assert!(first.as_str().starts_with("module_"));
}

#[test]
fn rejects_module_dependency_cycles() {
    let content = blueprint_with_modules(vec![
        ("订单", vec!["库存"]),
        ("库存", vec!["订单"]),
    ]);
    let error = validate_blueprint_structure(&content).expect_err("cycle must be rejected");
    assert!(matches!(error, BlueprintValidationError::ModuleCycle { .. }));
}
```

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-core --test blueprint`
Expected: 编译失败，`BlueprintEntityId` 和目标类型尚不存在。

- [x] **Step 3: 实现 UUID 实体 ID 与稳定蓝图 ID**

```rust
macro_rules! uuid_entity_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self { Self(Uuid::new_v4()) }
        }
    };
}

impl BlueprintEntityId {
    pub fn derive(project_id: &ProjectId, kind: &str, name: &str) -> Self {
        let normalized = normalize_entity_name(name);
        let value = format!("{project_id}:{kind}:{normalized}");
        let id = Uuid::new_v5(&BLUEPRINT_NAMESPACE, value.as_bytes());
        Self(format!("{kind}_{}", id.simple()))
    }
}
```

`normalize_entity_name` 合并 Unicode 空白并转小写；实现 `Display`、`FromStr`、`AsRef<str>`，空 ID 解析失败。

- [x] **Step 4: 实现蓝图内容、模型提案和转换**

蓝图内容固定包含：

```rust
pub struct BlueprintContent {
    pub product_goal: TraceableText,
    pub in_scope: Vec<TraceableText>,
    pub out_of_scope: Vec<TraceableText>,
    pub roles: Vec<RoleDefinition>,
    pub platforms: Vec<PlatformDefinition>,
    pub modules: Vec<ModuleDefinition>,
    pub technologies: Vec<TechnologyChoice>,
    pub capabilities: Vec<CapabilityObligation>,
    pub release_prerequisites: Vec<ReleasePrerequisite>,
    pub assumptions: Vec<Assumption>,
    pub risks: Vec<Risk>,
}

pub struct BlueprintProposal {
    pub product_goal: String,
    pub product_goal_sources: Vec<ProposedSourceReference>,
    pub in_scope: Vec<ProposedScopeItem>,
    pub out_of_scope: Vec<ProposedScopeItem>,
    pub roles: Vec<ProposedRole>,
    pub platforms: Vec<ProposedPlatform>,
    pub modules: Vec<ProposedModule>,
    pub technologies: Vec<ProposedTechnology>,
    pub capabilities: Vec<ProposedCapability>,
    pub release_prerequisites: Vec<ProposedReleasePrerequisite>,
    pub assumptions: Vec<ProposedNote>,
    pub risks: Vec<ProposedRisk>,
    pub open_questions: Vec<ProposedInfoRequest>,
}
```

`BlueprintProposal` 的每个结构、字段和枚举都派生 `JsonSchema`、`Serialize`、`Deserialize`，并设置 `deny_unknown_fields`。模块提案引用平台和依赖时使用名称；转换后变成稳定 ID。未知依赖、重名和非法来源 ID 均返回 `ProposalConversionError`。

- [x] **Step 5: 实现结构校验**

校验至少包含：

```rust
pub enum BlueprintValidationError {
    EmptyText { field: String },
    EmptyCollection { field: String },
    DuplicateId(BlueprintEntityId),
    DuplicateName { kind: String, name: String },
    UnknownModuleDependency { module: String, dependency: String },
    ModuleDependsOnItself(String),
    ModuleCycle { modules: Vec<String> },
    TextTooLong { field: String, max: usize },
}
```

产品目标、至少一个平台和至少一个模块为必需；名称最长 120 字符，说明最长 4,000 字符，集合只接受非空项。模块依赖必须存在、不得自依赖，并通过深度优先搜索拒绝环。

- [x] **Step 6: 实现信息请求与项目事实**

```rust
pub enum InfoRequestCategory {
    Information,
    Decision,
    Authorization,
    Approval,
    ExternalOperation,
}

pub struct InfoRequestDraft {
    pub category: InfoRequestCategory,
    pub question: String,
    pub reason: String,
    pub example: Option<String>,
    pub default_value: Option<String>,
    pub impact_scope: String,
    pub due_at: Option<DateTime<Utc>>,
    pub blocking: bool,
}
```

`InfoRequest` 包含 `id`、`project_id`、`blueprint_revision_id`、`status`、`answer`、`resolved_at`、`created_at` 和规范化 `fingerprint`。`ProjectFact` 包含稳定 `fact_key`、问题、答案、版本和来源请求 ID。

- [x] **Step 7: 运行核心测试与门禁**

Run:

```powershell
cargo test -p coldstart-core
cargo clippy -p coldstart-core --all-targets --locked -- -D warnings
```

Expected: 全部通过，无 Clippy 警告。

---

### Task 2: 文档抽取与 OpenAI 兼容模型适配器

**Files:**
- Create: `crates/adapters/Cargo.toml`
- Create: `crates/adapters/src/lib.rs`
- Create: `crates/adapters/src/document.rs`
- Create: `crates/adapters/src/model.rs`
- Create: `crates/adapters/tests/document.rs`
- Create: `crates/adapters/tests/model.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `extract_document(path: &Path) -> Result<ExtractedDocument, DocumentError>`。
- Produces: `ExtractedDocument { format, text, anchors }`，锚点 ID 在单文档内稳定。
- Produces: `BlueprintModel` trait：

```rust
pub trait BlueprintModel: Send + Sync {
    fn propose_blueprint(
        &self,
        request: &ModelExtractionRequest,
    ) -> Result<BlueprintProposal, ModelError>;
}
```

- Produces: `OpenAiCompatibleModel`、`ModelConfig`、`ModelExtractionRequest`、`ModelSource`。
- Consumes: Task 1 的 `BlueprintProposal` 和 `DocumentFormat`。

- [x] **Step 1: 写失败测试，锁定四种格式和锚点**

```rust
#[test]
fn extracts_markdown_sections_with_heading_anchors() {
    let doc = extract_fixture("requirements.md");
    assert_eq!(doc.format, DocumentFormat::Markdown);
    assert_eq!(doc.anchors[0].id, "section_1");
    assert!(doc.anchors[0].text.contains("订单"));
}

#[test]
fn rejects_pdf_without_extractable_text() {
    let error = extract_document(Path::new("scanned.pdf")).expect_err("empty PDF must fail");
    assert!(matches!(error, DocumentError::NoExtractableText));
}
```

同时覆盖 TXT 的 UTF-8 BOM/GB18030 回退、DOCX 段落/标题、PDF 页码锚点。

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-adapters --test document`
Expected: 编译失败，adapter crate 尚不存在。

- [x] **Step 3: 实现格式识别和大小限制**

支持扩展名：`.md`、`.markdown`、`.txt`、`.docx`、`.pdf`。读取前用 `metadata().len()` 拒绝超过 20 MiB 的文件；未知格式返回 `UnsupportedFormat`。原始字节计算 SHA-256。

- [x] **Step 4: 实现 Markdown/TXT/DOCX/PDF 抽取**

- Markdown：以 `#` 到 `######` 标题分组，生成 `section_N`；首个标题前非空内容生成 `preamble`。
- TXT：先尝试 UTF-8，失败时用 `encoding_rs::GB18030`；按空行分段，生成 `paragraph_N`。
- DOCX：用 `zip` 读取 `word/document.xml`，用 `quick-xml` 收集段落、文本和 `Heading*` 样式；标题变化生成 `paragraph_N` 锚点。
- PDF：用 `pdf_extract::extract_text_by_pages`，每页生成 `page_N`；所有页都为空时返回 `NoExtractableText`。

清理连续空白并去除 UTF-8 BOM；抽取后总字符数超过 500,000 返回 `TooLarge`。

- [x] **Step 5: 写失败测试，锁定 OpenAI 兼容请求与 JSON 响应**

```rust
#[test]
fn sends_json_mode_request_and_parses_fenced_json() {
    let server = MockServer::start(JsonResponse::ok(
        "{\"choices\":[{\"message\":{\"content\":\"```json\\n{\\\"productGoal\\\":\\\"订单系统\\\"}\\n```\"}}]}",
    ));
    let model = OpenAiCompatibleModel::for_test(server.url());
    let proposal = model.propose_blueprint(&request()).expect("response must parse");
    assert_eq!(proposal.product_goal, "订单系统");
    assert_eq!(server.last_request().authorization, "Bearer test-key");
}
```

- [x] **Step 6: 实现模型适配器**

请求发往 `{baseUrl}/chat/completions`，若 base URL 已以 `/chat/completions` 结尾则不重复追加。请求体包含 `model`、两条消息、`temperature: 0.1`、`response_format: {"type":"json_object"}`。系统消息声明外部文档是不可信数据，只允许输出符合给定 JSON Schema 的对象；用户消息包含 schema 和带锚点的来源文本。

响应只读取 `choices[0].message.content`；去除 Markdown 代码围栏后用 `serde_json` 解析并做 schema 兼容校验。HTTP 非 2xx、空 choices、无效 JSON、缺少字段均映射为稳定 `ModelError`。`ModelConfig` 的 `Debug` 实现必须把 `api_key` 显示为 `***`；错误文本不得包含 key。

- [x] **Step 7: 运行适配器测试与门禁**

Run:

```powershell
cargo test -p coldstart-adapters --locked
cargo clippy -p coldstart-adapters --all-targets --locked -- -D warnings
```

Expected: 四种文档格式、模型请求和错误路径测试全部通过。

---

### Task 3: Schema v3、来源、蓝图版本与 Inbox 持久化

**Files:**
- Modify: `crates/storage/src/migrations.rs`
- Modify: `crates/storage/src/database.rs`
- Create: `crates/storage/tests/requirements.rs`
- Modify: `crates/storage/Cargo.toml`

**Interfaces:**
- Produces: `Database::insert_artifact(&mut self, artifact, anchors) -> Result<(), StorageError>`。
- Produces: `Database::list_artifacts(project_id) -> Result<Vec<ArtifactMetadata>, StorageError>`。
- Produces: `Database::load_artifact_anchors(artifact_id) -> Result<Vec<DocumentAnchor>, StorageError>`。
- Produces: `Database::create_blueprint_revision(&mut self, project_id, content, generation, info_requests) -> Result<BlueprintRevision, StorageError>`。
- Produces: `Database::latest_blueprint(project_id, status) -> Result<Option<BlueprintRevision>, StorageError>`。
- Produces: `Database::publish_blueprint_revision(&mut self, project_id, revision_id) -> Result<BlueprintRevision, StorageError>`。
- Produces: `Database::list_info_requests(project_id) -> Result<Vec<InfoRequest>, StorageError>`。
- Produces: `Database::resolve_info_request(&mut self, project_id, request_id, answer) -> Result<ProjectFact, StorageError>`。

- [x] **Step 1: 写失败测试，锁定 schema v3 与迁移**

```rust
#[test]
fn migrates_v2_to_v3_without_losing_projects_or_events() {
    let fixture = create_v2_database();
    let database = Database::open(fixture.path()).expect("migration must succeed");
    assert_eq!(database.event_count(&fixture.project_id).unwrap(), 1);
    assert!(database.list_artifacts(&fixture.project_id).unwrap().is_empty());
}
```

另外测试 v0、v1、v2 升级，schema v3 不允许回退，未知未来版本仍被拒绝。

- [x] **Step 2: 运行测试确认失败**

Run: `cargo test -p coldstart-storage --test requirements`
Expected: 编译失败，新表和仓库方法尚不存在。

- [x] **Step 3: 增加 schema v3 表**

```sql
CREATE TABLE artifacts (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  file_name TEXT NOT NULL,
  format TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  text_hash TEXT NOT NULL,
  byte_size INTEGER NOT NULL,
  import_source TEXT NOT NULL,
  anchors_json TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE blueprint_revisions (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  revision_number INTEGER NOT NULL,
  status TEXT NOT NULL,
  content_json TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  source_artifact_ids_json TEXT NOT NULL,
  generation_source TEXT NOT NULL,
  model_provider TEXT,
  model_name TEXT,
  prompt_version TEXT,
  created_at TEXT NOT NULL,
  published_at TEXT,
  UNIQUE(project_id, revision_number),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE info_requests (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  blueprint_revision_id TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  category TEXT NOT NULL,
  question TEXT NOT NULL,
  reason TEXT NOT NULL,
  example TEXT,
  default_value TEXT,
  impact_scope TEXT NOT NULL,
  due_at TEXT,
  blocking INTEGER NOT NULL,
  status TEXT NOT NULL,
  answer TEXT,
  resolved_at TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(project_id, fingerprint),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (blueprint_revision_id) REFERENCES blueprint_revisions(id) ON DELETE CASCADE
);

CREATE TABLE project_facts (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  fact_key TEXT NOT NULL,
  category TEXT NOT NULL,
  question TEXT NOT NULL,
  value TEXT NOT NULL,
  version INTEGER NOT NULL,
  source_info_request_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(project_id, fact_key, version),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);
```

添加项目/状态索引。迁移顺序必须为 `0 -> v2 -> v3`、`1 -> v2 -> v3`、`2 -> v3`；最终 `user_version=3`。

- [x] **Step 4: 写失败测试，锁定不可变版本、Inbox 去重和事实版本**

```rust
#[test]
fn publishing_supersedes_the_previous_published_blueprint() {
    let (mut database, project_id) = initialized_project();
    let first = create_blueprint(&mut database, &project_id, "v1");
    database.publish_blueprint_revision(&project_id, &first.id).unwrap();
    let second = create_blueprint(&mut database, &project_id, "v2");
    database.publish_blueprint_revision(&project_id, &second.id).unwrap();

    let first = database.load_blueprint(&first.id).unwrap().unwrap();
    assert_eq!(first.status, BlueprintStatus::Superseded);
}

#[test]
fn repeated_open_question_does_not_create_duplicate_info_requests() {
    // 两次蓝图生成使用相同问题，仍只有一条 info_request。
}
```

- [x] **Step 5: 实现事务化持久化与事件**

每次写入都在同一个 `TransactionBehavior::Immediate` 事务内同时写投影表和事件：

- `artifact_imported`
- `blueprint_draft_created`
- `blueprint_published`
- `info_request_created`
- `info_request_resolved`

创建蓝图时在事务内查询 `MAX(revision_number)` 并加一；发布时校验目标属于本项目且为 `draft`，把旧 `published` 标记 `superseded`。信息请求按 `pending:category:normalized_question` 的 SHA-256 指纹去重。回答 Inbox 时在同一事务创建下一个版本的 `ProjectFact` 并标记请求 `answered`。

- [x] **Step 6: 运行存储测试与门禁**

Run:

```powershell
cargo test -p coldstart-storage --locked
cargo clippy -p coldstart-storage --all-targets --locked -- -D warnings
```

Expected: v1/v2 迁移、项目/事件守恒、蓝图不可变和 Inbox 事务测试全部通过。

---

### Task 4: 需求工作区应用服务

**Files:**
- Create: `crates/app/src/context.rs`
- Create: `crates/app/src/requirements.rs`
- Create: `crates/app/src/workspace.rs`
- Modify: `crates/app/src/error.rs`
- Modify: `crates/app/src/lib.rs`
- Modify: `crates/app/src/service.rs`
- Modify: `crates/app/Cargo.toml`
- Test: `crates/app/tests/requirements.rs`

**Interfaces:**
- Produces: `ProjectWorkspace { status, sources, draft, published, inbox }`。
- Produces: `RequirementsService<M: BlueprintModel>`：

```rust
pub fn load_workspace(&self, project_home: PathBuf) -> Result<ProjectWorkspace, AppError>;
pub fn import_document(&self, project_home: PathBuf, file_path: PathBuf) -> Result<ProjectWorkspace, AppError>;
pub fn generate_blueprint(&self, project_home: PathBuf, config: ModelConfig) -> Result<ProjectWorkspace, AppError>;
pub fn save_blueprint(&self, project_home: PathBuf, content: BlueprintContent) -> Result<ProjectWorkspace, AppError>;
pub fn publish_blueprint(&self, project_home: PathBuf, revision_id: BlueprintRevisionId) -> Result<ProjectWorkspace, AppError>;
pub fn resolve_info_request(&self, project_home: PathBuf, request_id: InfoRequestId, answer: String) -> Result<ProjectWorkspace, AppError>;
```

- Consumes: Task 1 的领域模型、Task 2 的 `BlueprintModel`/`extract_document`、Task 3 的存储方法。

- [x] **Step 1: 写失败测试，锁定导入文件布局和重复导入**

```rust
#[test]
fn imports_document_and_copies_immutable_artifacts() {
    let fixture = project_fixture();
    let source = fixture.write_requirements_markdown();
    let service = RequirementsService::new(FakeModel::unused());

    let workspace = service.import_document(fixture.home(), source).unwrap();

    assert_eq!(workspace.sources.len(), 1);
    assert!(fixture.artifact_path(workspace.sources[0].id).join("source.md").exists());
    assert!(fixture.artifact_path(workspace.sources[0].id).join("extracted.txt").exists());
    assert_eq!(workspace.status.event_count, 2);
}
```

同一内容再次导入创建新的 ArtifactRevision，不覆盖已有目录；导入失败时清理新目录且不增加事件。

- [x] **Step 2: 实现统一项目上下文**

把 `ProjectService::read_status` 的清单/数据库/元数据校验抽取到 `OpenProject::open(project_home)`；其他应用服务复用该上下文，避免重复路径和数据库规则。

- [x] **Step 3: 实现文档导入**

流程为：校验项目 -> `extract_document` -> 生成 Artifact ID -> 写 `.coldstart/artifacts/<id>/source.<ext>`、`extracted.txt`、`anchors.json` -> 数据库事务写元数据和事件 -> 返回工作区。任意数据库失败删除本次创建目录；清理失败加入错误信息。

- [x] **Step 4: 写失败测试，锁定模型生成、来源校验和上下文上限**

```rust
#[test]
fn generated_blueprint_contains_sources_and_creates_inbox_requests() {
    let workspace = service.generate_blueprint(home, model_config()).unwrap();
    let draft = workspace.draft.expect("draft must exist");
    assert_eq!(draft.content.modules.len(), 2);
    assert!(!draft.content.modules[0].sources.is_empty());
    assert_eq!(workspace.inbox.len(), 1);
}

#[test]
fn unknown_source_anchor_rejects_model_output() {
    let error = service.generate_blueprint(home, model_config())
        .expect_err("hallucinated anchor must fail");
    assert_eq!(error.code(), "invalid_source_reference");
}
```

- [x] **Step 5: 实现模型生成工作区**

读取所有来源锚点并构建 `ModelExtractionRequest`；总锚点文本超过 80,000 字符返回 `source_context_too_large`。调用模型后：

1. `BlueprintContent::from_proposal` 生成稳定 ID。
2. 校验产品目标、平台、模块、依赖 DAG。
3. 校验每个来源引用确实存在，且引用条目属于已导入 Artifact。
4. 将 `openQuestions` 转为 Inbox 草稿。
5. 在同一数据库事务创建新草稿版本、去重 Inbox 请求和事件。

manifest 中的来源引用部分必须至少有一条；生成失败不写入任何新版本。

- [x] **Step 6: 实现人工保存、发布与 Inbox 回答**

- `save_blueprint` 接受 UI 编辑后的完整内容，验证结构及来源引用后创建新的 `generation_source=manual` 草稿；不创建新问题。
- `publish_blueprint` 只允许发布 `draft`；发布前重新校验，并自动把旧发布版置为 `superseded`。
- `resolve_info_request` 要求答案非空；记录 ProjectFact 并将请求改为 `answered`。
- 所有方法返回完整 `ProjectWorkspace`，其中状态来自同一个 `OpenProject` 数据库连接。

- [x] **Step 7: 实现 `AppError` 稳定码并测试**

新增至少以下代码：`unsupported_document`、`document_empty`、`document_too_large`、`source_context_too_large`、`model_request_failed`、`model_response_invalid`、`invalid_blueprint`、`invalid_source_reference`、`blueprint_not_found`、`info_request_not_found`、`info_request_not_open`、`no_requirement_sources`。表驱动测试锁定每个变体的 `code()`。

- [x] **Step 8: 运行应用测试与门禁**

Run:

```powershell
cargo test -p coldstart-app --locked
cargo clippy -p coldstart-app --all-targets --locked -- -D warnings
```

Expected: 导入、生成、编辑、发布、Inbox 和失败回滚测试全部通过。

---

### Task 5: Tauri IPC 适配

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`

**Interfaces:**
- Produces commands:
  - `read_requirements_workspace(path)`
  - `import_requirement_document(path, filePath)`
  - `generate_blueprint(path, config)`
  - `save_blueprint_draft(path, content)`
  - `publish_blueprint(path, revisionId)`
  - `resolve_info_request(path, infoRequestId, answer)`
- All long-running/blocking work uses `tauri::async_runtime::spawn_blocking`.
- No capability beyond existing `core:default` and `dialog:allow-open`。

- [x] **Step 1: 写失败测试，锁定命令同步包装和参数转换**

```rust
#[test]
fn read_requirements_workspace_sync_reports_manifest_missing() {
    let temp = TempDir::new().unwrap();
    let error = read_requirements_workspace_sync(path_string(temp.path()))
        .expect_err("empty directory must fail");
    assert_eq!(error.code, "manifest_missing");
}
```

- [x] **Step 2: 实现薄命令与错误映射**

每个 `_sync` 函数构造 `RequirementsService<OpenAiCompatibleModel>`，并只做 `PathBuf`/ID 转换。`generate_blueprint` 的 `ModelConfig` 从命令参数反序列化；不打印参数，不把错误序列化为 `Debug`。异步包装只把 join error 映射为 `task_join`。

- [x] **Step 3: 更新 invoke handler 并验证能力清单**

只注册 2 个已有项目命令和 6 个新需求命令。`capabilities/default.json` 保持：

```json
{
  "permissions": ["core:default", "dialog:allow-open"]
}
```

- [x] **Step 4: 运行 Tauri Rust 测试**

Run: `cargo test -p coldstart-desktop --locked`
Expected: 命令包装、错误序列化和 handler 构建测试通过。

---

### Task 6: 需求、蓝图与 Inbox 界面

**Files:**
- Modify: `web/src/api/types.ts`
- Create: `web/src/api/requirementsApi.ts`
- Create: `web/src/components/AppSidebar.tsx`
- Create: `web/src/components/ProjectOverview.tsx`
- Create: `web/src/components/RequirementsView.tsx`
- Create: `web/src/components/BlueprintView.tsx`
- Create: `web/src/components/BlueprintEditor.tsx`
- Create: `web/src/components/InboxView.tsx`
- Modify: `web/src/App.tsx`
- Modify: `web/src/styles.css`
- Modify: `web/src/App.test.tsx`
- Delete: `web/src/components/Sidebar.tsx`
- Delete: `web/src/components/ProjectStatusView.tsx`

**Interfaces:**
- Consumes 六个新 Tauri 命令和 `readProjectStatus`；所有项目路径来自后端 DTO。
- Produces 单页工作区导航：项目总览、需求导入、蓝图编排、人工 Inbox；计划、MCP、发布仍禁用。

- [x] **Step 1: 写失败测试，锁定导航和项目工作区加载**

```tsx
it('enables requirement, blueprint and inbox workspaces after opening a project', async () => {
  vi.mocked(projectApi.readProjectWorkspace).mockResolvedValue(workspace);
  render(<App />);
  await user.click(screen.getByRole('button', { name: '打开项目' }));

  expect(screen.getByRole('button', { name: '需求导入' })).toBeEnabled();
  expect(screen.getByRole('button', { name: '蓝图编排' })).toBeEnabled();
  expect(screen.getByRole('button', { name: '人工 Inbox' })).toBeEnabled();
});
```

- [x] **Step 2: 扩展 TypeScript DTO 与 API**

DTO 字段与 Rust `camelCase` 完全一致。`pickRequirementFile()` 使用 `open` 过滤器：

```ts
open({
  multiple: false,
  directory: false,
  filters: [{ name: '需求文档', extensions: ['md', 'markdown', 'txt', 'docx', 'pdf'] }],
});
```

API 只封装 `invoke` 与文件选择，不包含业务逻辑。

- [x] **Step 3: 重构 App 状态和导航**

加载项目时并行读取状态和需求工作区；任一步失败保留最后一次完整快照并显示可关闭错误。创建项目后同样加载工作区。导入、生成、保存、发布、回答 Inbox 都通过统一的 action runner 更新工作区，并在成功后清除旧错误。

- [x] **Step 4: 实现项目总览**

展示项目名称、路径、事件数，以及四个流程指标：来源文档数、草稿状态、发布版本、待处理 Inbox。提供“进入需求导入”按钮；刷新按钮同时刷新状态和工作区。

- [x] **Step 5: 实现需求导入页**

包含来源文档列表、导入按钮、模型配置（Base URL、模型名、API Key）、外部发送确认复选框和“生成蓝图草稿”按钮。API Key 输入使用 `type=password` 且 `autoComplete="off"`，不进入 localStorage。未勾选确认时生成按钮禁用。

- [x] **Step 6: 实现蓝图编辑与发布**

草稿编辑覆盖产品目标、范围、角色、平台、模块、技术栈、三方能力、发布前置条件、假设和风险；列表支持新增、删除和修改。模块的平台和依赖通过实体 ID 选择。来源引用只读显示为“文档名 · 锚点名”。保存调用 `save_blueprint_draft`，发布前显示校验错误。无草稿时允许创建空白人工草稿。

- [x] **Step 7: 实现 Inbox**

按类别显示待处理请求、阻塞标记、影响范围、原因、示例、默认值和最晚时间；创建回答文本框与提交按钮。已回答项折叠显示最终答案。空状态说明“当前没有需要人工补充的信息”。

- [x] **Step 8: 写行为测试**

至少覆盖：

- 导入文档后来源列表和事件数更新。
- 未确认外部发送时生成按钮禁用；确认并提交后草稿出现。
- 蓝图编辑保存后 API 收到修改内容。
- 发布草稿后状态变为已发布。
- 回答 Inbox 后请求从待处理区消失并显示答案。
- 后端错误可关闭且旧工作区仍可操作。

- [x] **Step 9: 运行前端门禁**

Run:

```powershell
pnpm --dir web typecheck
pnpm --dir web test
pnpm --dir web build
```

Expected: TypeScript 零错误，全部 Vitest 通过，生产构建成功。

---

### Task 7: 文档、桌面构建与人工验收

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-10-30-coldstart-delivery-roadmap.md`
- Create: `.superpowers/sdd/2026-10-30-coldstart-03-requirements-blueprint/progress.md`

- [x] **Step 1: 更新用户文档**

README 增加：第三阶段能力、支持格式、模型配置与 API Key 不持久化说明、D 盘环境脚本、桌面构建命令、蓝图发布与 Inbox 流程。路线图把阶段 2 标为已完成，阶段 3 链接到本计划。

- [x] **Step 2: 运行全量门禁**

```powershell
. 'D:\DevTools\coldstart-env.ps1'
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo +1.89.0 test --workspace --locked
pnpm --dir web typecheck
pnpm --dir web test
pnpm --dir web build
pnpm --dir apps/desktop tauri build --no-bundle
```

- [x] **Step 3: 启动本地 OpenAI 兼容模拟服务并做桌面人工验收**

验收步骤：

1. 新建项目，导入含 Web、管理后台、小程序、模块、OSS、提审等内容的 Markdown。
2. 配置指向本地模拟服务的模型端点，确认发送提示，生成蓝图。
3. 检查平台矩阵、模块依赖、技术栈、OSS 义务、提审前置条件、来源引用和 Inbox 问题。
4. 人工修改一个模块名称和技术栈，保存新草稿并发布。
5. 在 Inbox 回答问题，确认请求消失且出现项目事实。
6. 关闭重开桌面应用，确认来源、草稿、发布版本和 Inbox 仍在。
7. 验证模型服务未配置、无效 JSON、错误锚点、扫描 PDF 等错误有稳定提示。

截图保存到 `.superpowers/sdd/2026-10-30-coldstart-03-requirements-blueprint/manual/`。

- [x] **Step 4: 记录 Ledger 与执行裁定**

记录每个任务验证结果、最终 SHA、二进制路径/大小/SHA-256、人工验收、剩余 Minor 和阶段完成状态。若代码未提交，明确写“工作区变更尚未 commit/push”。

- [x] **Step 5: 最终交付检查**

确认没有明文 API Key、没有 `.coldstart`/`target`/`node_modules` 进入 Git，能力清单未增加文件系统或 Shell 权限，临时项目与本地模拟服务已清理。

