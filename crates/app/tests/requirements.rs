use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use coldstart_adapters::{
    BlueprintModel, DocumentError, ModelConfig, ModelError, ModelExtractionRequest,
};
use coldstart_app::{AppError, MAX_SOURCE_CONTEXT_CHARACTERS, ProjectService, RequirementsService};
use coldstart_core::{
    ArtifactId, BlueprintProposal, BlueprintRevisionId, InfoRequestCategory, InfoRequestId,
    PlatformKind, ProposedCapability, ProposedInfoRequest, ProposedModule, ProposedPlatform,
    ProposedReleasePrerequisite, ProposedScopeItem, ProposedSourceReference, ProposedTechnology,
};
use coldstart_storage::StorageError;
use tempfile::TempDir;

const MARKDOWN: &str = r#"# 订单系统

需要 Web、管理后台和微信小程序，文件上传使用阿里云 OSS。

## 发布要求

小程序需要提审，App 需要准备证书。
"#;
const PNG_BYTES: &[u8] = b"\x89PNG\r\n\x1a\ncoldstart-image";

#[derive(Clone)]
enum FakeResponse {
    Proposal {
        anchor_override: Option<String>,
        include_question: bool,
        module_count: usize,
    },
    Error,
}

struct FakeModel {
    response: FakeResponse,
    calls: Arc<AtomicUsize>,
    last_request: Arc<Mutex<Option<ModelExtractionRequest>>>,
}

impl FakeModel {
    fn proposal() -> Self {
        Self::with_response(FakeResponse::Proposal {
            anchor_override: None,
            include_question: true,
            module_count: 2,
        })
    }

    fn with_anchor(anchor: &str) -> Self {
        Self::with_response(FakeResponse::Proposal {
            anchor_override: Some(anchor.to_owned()),
            include_question: false,
            module_count: 1,
        })
    }

    fn failing() -> Self {
        Self::with_response(FakeResponse::Error)
    }

    fn with_response(response: FakeResponse) -> Self {
        Self {
            response,
            calls: Arc::new(AtomicUsize::new(0)),
            last_request: Arc::new(Mutex::new(None)),
        }
    }
}

impl BlueprintModel for FakeModel {
    fn propose_blueprint(
        &self,
        request: &ModelExtractionRequest,
    ) -> Result<BlueprintProposal, ModelError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.last_request.lock().expect("request lock must work") = Some(request.clone());
        match &self.response {
            FakeResponse::Error => Err(ModelError::InvalidResponse("模拟模型错误".to_owned())),
            FakeResponse::Proposal {
                anchor_override,
                include_question,
                module_count,
            } => Ok(fake_proposal(
                request,
                anchor_override.as_deref(),
                *include_question,
                *module_count,
            )),
        }
    }
}

fn fake_proposal(
    request: &ModelExtractionRequest,
    anchor_override: Option<&str>,
    include_question: bool,
    module_count: usize,
) -> BlueprintProposal {
    let source = request.sources.first().expect("fake model needs a source");
    let anchor_id = anchor_override
        .map(str::to_owned)
        .unwrap_or_else(|| source.anchors[0].anchor_id.clone());
    let reference = ProposedSourceReference {
        artifact_id: source.artifact_id.clone(),
        anchor_id,
        quote: Some("需要 Web、管理后台和微信小程序。".to_owned()),
    };
    let module_names = (0..module_count)
        .map(|index| format!("订单模块{}", index + 1))
        .collect::<Vec<_>>();
    let modules = module_names
        .iter()
        .enumerate()
        .map(|(index, name)| ProposedModule {
            name: name.clone(),
            description: "处理订单".to_owned(),
            platforms: vec!["Web".to_owned()],
            depends_on: if index > 0 {
                vec![module_names[index - 1].clone()]
            } else {
                Vec::new()
            },
            sources: vec![reference.clone()],
        })
        .collect();
    BlueprintProposal {
        product_goal: "构建订单系统".to_owned(),
        product_goal_sources: vec![reference.clone()],
        in_scope: vec![ProposedScopeItem {
            text: "订单管理".to_owned(),
            sources: vec![reference.clone()],
        }],
        out_of_scope: Vec::new(),
        roles: Vec::new(),
        platforms: vec![ProposedPlatform {
            name: "Web".to_owned(),
            kind: PlatformKind::Web,
            description: "前台网站".to_owned(),
            sources: vec![reference.clone()],
        }],
        modules,
        technologies: vec![ProposedTechnology {
            category: "后端".to_owned(),
            name: "Rust".to_owned(),
            reason: "可靠性高".to_owned(),
            sources: vec![reference.clone()],
        }],
        capabilities: vec![ProposedCapability {
            name: "文件上传".to_owned(),
            provider: "阿里云 OSS".to_owned(),
            description: "保存用户文件".to_owned(),
            sources: vec![reference.clone()],
        }],
        release_prerequisites: vec![ProposedReleasePrerequisite {
            name: "小程序提审".to_owned(),
            category: "小程序".to_owned(),
            description: "提交微信审核".to_owned(),
            lead_time_days: Some(3),
            blocking: true,
            sources: vec![reference.clone()],
        }],
        assumptions: Vec::new(),
        risks: Vec::new(),
        open_questions: if include_question {
            vec![ProposedInfoRequest {
                category: InfoRequestCategory::Decision,
                question: "OSS 使用哪个地域？".to_owned(),
                reason: "影响部署配置".to_owned(),
                example: Some("华东 1".to_owned()),
                default_value: None,
                impact_scope: "文件上传".to_owned(),
                due_at: None,
                blocking: true,
            }]
        } else {
            Vec::new()
        },
    }
}

struct Fixture {
    directory: TempDir,
    home: PathBuf,
    project_id: coldstart_core::ProjectId,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let home = directory.path().join("project");
        let status = ProjectService::new()
            .create_project("订单系统", home.clone())
            .expect("project must initialize");
        Self {
            directory,
            home,
            project_id: status.project_id,
        }
    }

    fn write(&self, name: &str, content: &str) -> PathBuf {
        let path = self.directory.path().join(name);
        fs::write(&path, content).expect("fixture must be written");
        path
    }

    fn write_bytes(&self, name: &str, content: &[u8]) -> PathBuf {
        let path = self.directory.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture directory must be created");
        }
        fs::write(&path, content).expect("fixture must be written");
        path
    }

    fn artifact_directory(&self, artifact_id: ArtifactId) -> PathBuf {
        self.home
            .join(".coldstart")
            .join("artifacts")
            .join(artifact_id.to_string())
    }
}

fn model_config() -> ModelConfig {
    ModelConfig {
        base_url: "https://example.invalid/v1".to_owned(),
        model: "test-model".to_owned(),
        api_key: "test-secret".to_owned(),
    }
}

#[test]
fn imports_document_and_copies_immutable_artifacts() {
    let fixture = Fixture::new();
    let source = fixture.write("requirements.md", MARKDOWN);
    let service = RequirementsService::new(FakeModel::proposal());

    let workspace = service
        .import_document(fixture.home.clone(), source.clone())
        .expect("document must import");

    assert_eq!(workspace.sources.len(), 1);
    assert_eq!(workspace.status.event_count, 2);
    assert!(workspace.draft.is_none());
    assert!(workspace.published.is_none());
    assert!(workspace.inbox.is_empty());
    let artifact = &workspace.sources[0];
    let artifact_directory = fixture.artifact_directory(artifact.id);
    assert_eq!(
        fs::read(artifact_directory.join("source.md")).unwrap(),
        fs::read(source).unwrap()
    );
    assert!(artifact_directory.join("extracted.txt").exists());
    assert!(artifact_directory.join("anchors.json").exists());
    let anchors: Vec<coldstart_core::DocumentAnchor> =
        serde_json::from_slice(&fs::read(artifact_directory.join("anchors.json")).unwrap())
            .unwrap();
    assert_eq!(anchors[0].id, "section_1");
}

#[test]
fn imports_requirement_images_into_the_immutable_artifact_directory() {
    let fixture = Fixture::new();
    fixture.write_bytes("images/order.png", PNG_BYTES);
    let source = fixture.write(
        "visual-requirements.md",
        "# 订单原型\n\n![订单列表](images/order.png)\n\n## 发布\n\n小程序提审。\n",
    );
    let service = RequirementsService::new(FakeModel::proposal());

    let workspace = service
        .import_document(fixture.home.clone(), source)
        .expect("document must import");

    let artifact = &workspace.sources[0];
    let images = workspace
        .source_images
        .get(&artifact.id.to_string())
        .expect("artifact images must be indexed");
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].anchor_id, "section_1");
    assert_eq!(images[0].file_name, "order.png");
    assert_eq!(images[0].mime_type, "image/png");
    assert_eq!(images[0].relative_path, "images/image_1.png");
    assert_eq!(
        fs::read(
            fixture
                .artifact_directory(artifact.id)
                .join(&images[0].relative_path)
        )
        .unwrap(),
        PNG_BYTES
    );
    assert!(
        fixture
            .artifact_directory(artifact.id)
            .join("images.json")
            .exists()
    );
}

#[test]
fn reads_an_imported_requirement_image_only_after_hash_verification() {
    let fixture = Fixture::new();
    fixture.write_bytes("images/order.png", PNG_BYTES);
    let source = fixture.write(
        "visual-requirements.md",
        "# 订单原型\n\n![订单列表](images/order.png)\n",
    );
    let service = RequirementsService::new(FakeModel::proposal());
    let workspace = service
        .import_document(fixture.home.clone(), source)
        .expect("document must import");
    let artifact = &workspace.sources[0];
    let image = &workspace.source_images[&artifact.id.to_string()][0];

    let content =
        coldstart_app::read_requirement_image(fixture.home.clone(), artifact.id, &image.id)
            .expect("image must load");
    assert_eq!(content.bytes, PNG_BYTES);
    assert_eq!(content.anchor_label, "订单原型");
    assert_eq!(content.source_file_name, "visual-requirements.md");

    fs::write(
        fixture
            .artifact_directory(artifact.id)
            .join(&image.relative_path),
        b"tampered",
    )
    .expect("stored image must be replaced");
    let error = coldstart_app::read_requirement_image(fixture.home, artifact.id, &image.id)
        .expect_err("modified image must be rejected");
    assert_eq!(error.code(), "requirement_image_invalid");
}

#[test]
fn importing_the_same_content_creates_a_new_immutable_artifact() {
    let fixture = Fixture::new();
    let source = fixture.write("requirements.md", MARKDOWN);
    let service = RequirementsService::new(FakeModel::proposal());
    let first = service
        .import_document(fixture.home.clone(), source.clone())
        .unwrap();
    let second = service
        .import_document(fixture.home.clone(), source)
        .unwrap();

    assert_eq!(second.sources.len(), 2);
    assert_ne!(first.sources[0].id, second.sources[1].id);
    assert!(
        fixture
            .artifact_directory(first.sources[0].id)
            .join("source.md")
            .exists()
    );
    assert!(
        fixture
            .artifact_directory(second.sources[1].id)
            .join("source.md")
            .exists()
    );
    assert_eq!(second.status.event_count, 3);
}

#[test]
fn unsupported_document_import_rolls_back_without_an_event_or_directory() {
    let fixture = Fixture::new();
    let source = fixture.write("requirements.rtf", MARKDOWN);
    let service = RequirementsService::new(FakeModel::proposal());

    let error = service
        .import_document(fixture.home.clone(), source)
        .expect_err("unsupported document must fail");
    assert_eq!(error.code(), "unsupported_document");
    let workspace = service.load_workspace(fixture.home.clone()).unwrap();
    assert!(workspace.sources.is_empty());
    assert_eq!(workspace.status.event_count, 1);
    let artifact_root = fixture.home.join(".coldstart").join("artifacts");
    assert_eq!(fs::read_dir(artifact_root).unwrap().count(), 0);
    assert_eq!(fixture.project_id, workspace.status.project_id);
}

#[test]
fn generated_blueprint_contains_sources_and_creates_inbox_requests() {
    let fixture = Fixture::new();
    let source = fixture.write("requirements.md", MARKDOWN);
    let model = FakeModel::proposal();
    let calls = Arc::clone(&model.calls);
    let last_request = Arc::clone(&model.last_request);
    let service = RequirementsService::new(model);
    let imported = service
        .import_document(fixture.home.clone(), source)
        .unwrap();
    let artifact_ids = imported
        .sources
        .iter()
        .map(|source| source.id)
        .collect::<Vec<_>>();

    let workspace = service
        .generate_blueprint(fixture.home.clone(), model_config(), artifact_ids.clone())
        .expect("blueprint must generate");

    let draft = workspace.draft.expect("draft must exist");
    assert_eq!(draft.content.modules.len(), 2);
    assert!(!draft.content.modules[0].sources.is_empty());
    assert_eq!(draft.content.capabilities[0].provider, "阿里云 OSS");
    assert_eq!(draft.content.release_prerequisites[0].name, "小程序提审");
    assert_eq!(workspace.inbox.len(), 1);
    assert_eq!(workspace.status.event_count, 4);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let captured = last_request
        .lock()
        .unwrap()
        .clone()
        .expect("model request must be captured");
    assert_eq!(captured.project_name, "订单系统");
    assert_eq!(captured.config.api_key, "test-secret");
    assert_eq!(captured.sources[0].anchors[0].anchor_id, "section_1");
    assert_eq!(draft.source_artifact_ids, artifact_ids);
}

#[test]
fn unknown_source_anchor_rejects_model_output_without_writing_a_revision() {
    let fixture = Fixture::new();
    let source = fixture.write("requirements.md", MARKDOWN);
    let service = RequirementsService::new(FakeModel::with_anchor("missing_anchor"));
    let imported = service
        .import_document(fixture.home.clone(), source)
        .unwrap();
    let artifact_ids = imported
        .sources
        .iter()
        .map(|source| source.id)
        .collect::<Vec<_>>();

    let error = service
        .generate_blueprint(fixture.home.clone(), model_config(), artifact_ids)
        .expect_err("unknown anchor must fail");
    assert_eq!(error.code(), "invalid_source_reference");
    let workspace = service.load_workspace(fixture.home).unwrap();
    assert!(workspace.draft.is_none());
    assert!(workspace.inbox.is_empty());
    assert_eq!(workspace.status.event_count, 2);
}

#[test]
fn oversized_source_context_is_rejected_before_calling_the_model() {
    let fixture = Fixture::new();
    let content = format!(
        "# 大文档\n\n{}\n",
        "需".repeat(MAX_SOURCE_CONTEXT_CHARACTERS + 1)
    );
    let source = fixture.write("large.md", &content);
    let model = FakeModel::proposal();
    let calls = Arc::clone(&model.calls);
    let service = RequirementsService::new(model);
    let imported = service
        .import_document(fixture.home.clone(), source)
        .unwrap();
    let artifact_ids = imported
        .sources
        .iter()
        .map(|source| source.id)
        .collect::<Vec<_>>();

    let error = service
        .generate_blueprint(fixture.home.clone(), model_config(), artifact_ids)
        .expect_err("oversized context must fail");
    assert_eq!(error.code(), "source_context_too_large");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        service
            .load_workspace(fixture.home)
            .unwrap()
            .status
            .event_count,
        2
    );
}

#[test]
fn generation_without_sources_fails_before_calling_the_model() {
    let fixture = Fixture::new();
    let model = FakeModel::proposal();
    let calls = Arc::clone(&model.calls);
    let service = RequirementsService::new(model);

    let error = service
        .generate_blueprint(fixture.home, model_config(), Vec::new())
        .expect_err("missing sources must fail");
    assert_eq!(error.code(), "no_requirement_sources");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn saves_publishes_and_resolves_without_losing_the_workspace() {
    let fixture = Fixture::new();
    let source = fixture.write("requirements.md", MARKDOWN);
    let service = RequirementsService::new(FakeModel::proposal());
    let imported = service
        .import_document(fixture.home.clone(), source)
        .unwrap();
    let artifact_ids = imported
        .sources
        .iter()
        .map(|source| source.id)
        .collect::<Vec<_>>();
    let generated = service
        .generate_blueprint(fixture.home.clone(), model_config(), artifact_ids)
        .unwrap();
    let first_draft = generated.draft.expect("draft must exist");
    let mut content = first_draft.content.clone();
    content.product_goal.text = "更新后的订单系统目标".to_owned();

    let saved = service
        .save_blueprint(fixture.home.clone(), content)
        .expect("manual draft must save");
    let manual_draft = saved.draft.expect("manual draft must be latest");
    assert_ne!(manual_draft.id, first_draft.id);
    assert_eq!(manual_draft.revision_number, 2);
    assert_eq!(
        manual_draft.generation.source,
        coldstart_core::GenerationSource::Manual
    );
    assert_eq!(
        manual_draft.content.product_goal.text,
        "更新后的订单系统目标"
    );
    assert_eq!(saved.inbox.len(), 1);

    let blocked = service
        .publish_blueprint(fixture.home.clone(), manual_draft.id)
        .expect_err("open blocking information must stop publication");
    assert_eq!(blocked.code(), "unresolved_blocking_info_requests");
    let request_id = saved.inbox[0].id;
    let resolved = service
        .resolve_info_request(fixture.home.clone(), request_id, " 华东 1 ".to_owned())
        .expect("answer must resolve");
    assert_eq!(
        resolved.inbox[0].status,
        coldstart_core::InfoRequestStatus::Answered
    );
    assert_eq!(resolved.inbox[0].answer.as_deref(), Some("华东 1"));
    assert_eq!(resolved.facts.len(), 1);
    assert_eq!(resolved.facts[0].value, "华东 1");
    assert_eq!(resolved.facts[0].version, 1);

    let regenerated = service
        .generate_blueprint(
            fixture.home.clone(),
            model_config(),
            saved.sources.iter().map(|source| source.id).collect(),
        )
        .expect("answered facts must feed the next blueprint");
    let regenerated_draft = regenerated.draft.expect("new draft must exist");
    let captured = service
        .load_workspace(fixture.home.clone())
        .expect("workspace must load");
    assert_eq!(captured.facts.len(), 1);
    assert_eq!(
        regenerated_draft.source_artifact_ids,
        vec![saved.sources[0].id]
    );

    let published = service
        .publish_blueprint(fixture.home.clone(), regenerated_draft.id)
        .expect("draft must publish");
    let published_revision = published
        .published
        .clone()
        .expect("published blueprint must exist");
    assert_eq!(published_revision.id, regenerated_draft.id);
    assert!(published_revision.published_at.is_some());
    assert!(published.draft.is_none());
    assert_eq!(published.sources.len(), 1);

    let already_published = service
        .publish_blueprint(fixture.home.clone(), regenerated_draft.id)
        .expect_err("published revision must not publish twice");
    assert_eq!(already_published.code(), "blueprint_not_draft");
    let already_answered = service
        .resolve_info_request(fixture.home, request_id, "华南 1".to_owned())
        .expect_err("answered request must not answer twice");
    assert_eq!(already_answered.code(), "info_request_not_open");
}

#[test]
fn dismisses_an_obsolete_blocking_question_and_allows_publication() {
    let fixture = Fixture::new();
    let source = fixture.write("requirements.md", MARKDOWN);
    let service = RequirementsService::new(FakeModel::proposal());
    let imported = service
        .import_document(fixture.home.clone(), source)
        .unwrap();
    let generated = service
        .generate_blueprint(
            fixture.home.clone(),
            model_config(),
            imported.sources.iter().map(|source| source.id).collect(),
        )
        .unwrap();
    let draft = generated.draft.expect("draft must exist");
    let request_id = generated.inbox[0].id;

    let blocked = service
        .publish_blueprint(fixture.home.clone(), draft.id)
        .expect_err("open blocking information must stop publication");
    assert_eq!(blocked.code(), "unresolved_blocking_info_requests");

    let empty_reason = service
        .dismiss_info_request(fixture.home.clone(), request_id, "  ".to_owned())
        .expect_err("empty dismissal reason must fail");
    assert_eq!(empty_reason.code(), "info_request_dismiss_reason_empty");

    let dismissed = service
        .dismiss_info_request(
            fixture.home.clone(),
            request_id,
            " 该问题已被新的库存方案取代 ".to_owned(),
        )
        .expect("obsolete question must close");
    assert_eq!(
        dismissed.inbox[0].status,
        coldstart_core::InfoRequestStatus::Dismissed
    );
    assert_eq!(
        dismissed.inbox[0].answer.as_deref(),
        Some("该问题已被新的库存方案取代")
    );
    assert!(dismissed.facts.is_empty());

    let published = service
        .publish_blueprint(fixture.home.clone(), draft.id)
        .expect("dismissed question must not block publication");
    assert_eq!(
        published.published.expect("published blueprint").id,
        draft.id
    );

    let already_closed = service
        .dismiss_info_request(fixture.home, request_id, "重复关闭".to_owned())
        .expect_err("dismissed request must not close twice");
    assert_eq!(already_closed.code(), "info_request_not_open");
}

#[test]
fn model_failure_does_not_write_a_blueprint_revision() {
    let fixture = Fixture::new();
    let source = fixture.write("requirements.md", MARKDOWN);
    let service = RequirementsService::new(FakeModel::failing());
    let imported = service
        .import_document(fixture.home.clone(), source)
        .unwrap();
    let artifact_ids = imported
        .sources
        .iter()
        .map(|source| source.id)
        .collect::<Vec<_>>();

    let error = service
        .generate_blueprint(fixture.home.clone(), model_config(), artifact_ids)
        .expect_err("model failure must stop generation");
    assert_eq!(error.code(), "model_response_invalid");
    let workspace = service.load_workspace(fixture.home).unwrap();
    assert!(workspace.draft.is_none());
    assert_eq!(workspace.status.event_count, 2);
}

#[test]
fn missing_blueprint_or_info_request_has_a_stable_error_code() {
    let fixture = Fixture::new();
    let service = RequirementsService::new(FakeModel::proposal());
    let revision_id = BlueprintRevisionId::new();
    let request_id = InfoRequestId::new();

    let error = service
        .publish_blueprint(fixture.home.clone(), revision_id)
        .expect_err("missing revision must fail");
    assert_eq!(error.code(), "blueprint_not_found");
    let error = service
        .resolve_info_request(fixture.home.clone(), request_id, "答案".to_owned())
        .expect_err("missing request must fail");
    assert_eq!(error.code(), "info_request_not_found");
    let error = service
        .resolve_info_request(fixture.home, request_id, "  ".to_owned())
        .expect_err("empty answer must fail");
    assert_eq!(error.code(), "info_request_answer_empty");
}

#[test]
fn app_error_codes_are_stable_for_expected_failures() {
    let revision_id = BlueprintRevisionId::new();
    let request_id = InfoRequestId::new();
    let cases = vec![
        (
            AppError::Document(DocumentError::UnsupportedFormat("rtf".to_owned())),
            "unsupported_document",
        ),
        (
            AppError::Document(DocumentError::NoExtractableText),
            "document_empty",
        ),
        (
            AppError::Document(DocumentError::TooLarge { limit: 20 }),
            "document_too_large",
        ),
        (
            AppError::Model(ModelError::InvalidResponse("bad".to_owned())),
            "model_response_invalid",
        ),
        (
            AppError::Model(ModelError::Request("offline".to_owned())),
            "model_request_failed",
        ),
        (
            AppError::InvalidBlueprint("bad".to_owned()),
            "invalid_blueprint",
        ),
        (
            AppError::InvalidSourceReference("missing".to_owned()),
            "invalid_source_reference",
        ),
        (
            AppError::SourceContextTooLarge {
                actual: 81,
                limit: 80,
            },
            "source_context_too_large",
        ),
        (AppError::NoRequirementSources, "no_requirement_sources"),
        (
            AppError::BlueprintNotFound(revision_id),
            "blueprint_not_found",
        ),
        (
            AppError::BlueprintNotDraft(revision_id),
            "blueprint_not_draft",
        ),
        (
            AppError::InfoRequestNotFound(request_id),
            "info_request_not_found",
        ),
        (
            AppError::InfoRequestNotOpen(request_id),
            "info_request_not_open",
        ),
        (
            AppError::InfoRequestAnswerEmpty,
            "info_request_answer_empty",
        ),
        (
            AppError::Storage(StorageError::BlueprintNotFound(revision_id)),
            "blueprint_not_found",
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(error.code(), expected, "unexpected code for {error}");
    }
}

#[test]
fn project_context_refuses_a_database_with_mismatched_metadata() {
    let fixture = Fixture::new();
    let service = RequirementsService::new(FakeModel::proposal());
    let manifest_path = fixture.home.join("coldstart.project.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["name"] = serde_json::Value::String("被篡改的名称".to_owned());
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let error = service
        .load_workspace(fixture.home)
        .expect_err("manifest mismatch must fail");
    assert_eq!(error.code(), "project_metadata_mismatch");
}
