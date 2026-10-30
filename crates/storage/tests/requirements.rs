use std::{fs, path::Path};

use chrono::Utc;
use coldstart_core::{
    ArtifactFormat, ArtifactId, ArtifactMetadata, BlueprintContent, BlueprintEntityId,
    BlueprintGeneration, BlueprintStatus, DocumentAnchor, GenerationSource, InfoRequestCategory,
    InfoRequestDraft, InfoRequestStatus, ModuleDefinition, PlatformDefinition, PlatformKind,
    Project, ProjectId, SourceReference, TraceableText,
};
use coldstart_storage::{Database, StorageError};
use rusqlite::{Connection, params};
use tempfile::TempDir;

struct Fixture {
    _directory: TempDir,
    database: Database,
    project: Project,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let project = Project::new("订单系统", directory.path().join("project"))
            .expect("project must be valid");
        let mut database = Database::open(&directory.path().join(".coldstart/state.db"))
            .expect("database must open");
        database
            .initialize_project(&project)
            .expect("project must initialize");
        Self {
            _directory: directory,
            database,
            project,
        }
    }

    fn insert_artifact(&mut self) -> ArtifactId {
        let artifact = sample_artifact(self.project.id);
        let id = artifact.id;
        self.database
            .insert_artifact(&artifact, &sample_anchors())
            .expect("artifact must be inserted");
        id
    }

    fn content(&self, artifact_id: ArtifactId, goal: &str) -> BlueprintContent {
        blueprint_content(self.project.id, artifact_id, goal)
    }
}

fn sample_artifact(project_id: ProjectId) -> ArtifactMetadata {
    ArtifactMetadata {
        id: ArtifactId::new(),
        project_id,
        file_name: "requirements.md".to_owned(),
        format: ArtifactFormat::Markdown,
        content_hash: "content-hash".to_owned(),
        text_hash: "text-hash".to_owned(),
        byte_size: 128,
        import_source: "requirements.md".to_owned(),
        imported_at: Utc::now(),
    }
}

fn sample_anchors() -> Vec<DocumentAnchor> {
    vec![DocumentAnchor {
        id: "section_1".to_owned(),
        label: "业务范围".to_owned(),
        text: "需要 Web、管理后台和小程序，文件上传使用阿里云 OSS。".to_owned(),
    }]
}

fn blueprint_content(
    project_id: ProjectId,
    artifact_id: ArtifactId,
    goal: &str,
) -> BlueprintContent {
    let source = SourceReference {
        artifact_id,
        anchor_id: "section_1".to_owned(),
        quote: Some("需要 Web、管理后台和小程序。".to_owned()),
    };
    let platform_id = BlueprintEntityId::derive(&project_id, "platform", "Web");
    let module_id = BlueprintEntityId::derive(&project_id, "module", "订单模块");
    BlueprintContent {
        product_goal: TraceableText {
            id: BlueprintEntityId::derive(&project_id, "goal", "product"),
            text: goal.to_owned(),
            sources: vec![source.clone()],
        },
        in_scope: Vec::new(),
        out_of_scope: Vec::new(),
        roles: Vec::new(),
        platforms: vec![PlatformDefinition {
            id: platform_id.clone(),
            name: "Web".to_owned(),
            kind: PlatformKind::Web,
            description: "前台网站".to_owned(),
            sources: vec![source.clone()],
        }],
        modules: vec![ModuleDefinition {
            id: module_id,
            name: "订单模块".to_owned(),
            description: "管理订单".to_owned(),
            platforms: vec![platform_id],
            depends_on: Vec::new(),
            sources: vec![source],
        }],
        technologies: Vec::new(),
        capabilities: Vec::new(),
        release_prerequisites: Vec::new(),
        assumptions: Vec::new(),
        risks: Vec::new(),
    }
}

fn generation() -> BlueprintGeneration {
    BlueprintGeneration {
        source: GenerationSource::Model,
        model_provider: Some("OpenAI Compatible".to_owned()),
        model_name: Some("test-model".to_owned()),
        prompt_version: Some("blueprint-v1".to_owned()),
    }
}

fn info_request(question: &str) -> InfoRequestDraft {
    InfoRequestDraft {
        category: InfoRequestCategory::Decision,
        question: question.to_owned(),
        reason: "影响部署和上传实现".to_owned(),
        example: Some("例如华东 1".to_owned()),
        default_value: None,
        impact_scope: "文件上传".to_owned(),
        due_at: None,
        blocking: true,
    }
}

#[test]
fn migrates_v2_to_v3_without_losing_projects_or_events() {
    let directory = tempfile::tempdir().expect("temporary directory must exist");
    let project =
        Project::new("迁移项目", directory.path().join("project")).expect("project must be valid");
    let database_path = directory.path().join(".coldstart/state.db");
    create_v2_database(&database_path, &project);

    let database = Database::open(&database_path).expect("v2 must migrate");
    assert_eq!(database.event_count(&project.id).unwrap(), 1);
    assert!(database.list_artifacts(&project.id).unwrap().is_empty());
    assert!(database.list_info_requests(&project.id).unwrap().is_empty());
    assert_eq!(
        database
            .load_project(&project.id, project.root_path.clone())
            .unwrap()
            .expect("project must survive")
            .name,
        project.name
    );
    drop(database);

    let connection = Connection::open(database_path).expect("database must open directly");
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("schema version must be readable");
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN
             ('artifacts', 'blueprint_revisions', 'info_requests', 'project_facts')",
            [],
            |row| row.get(0),
        )
        .expect("tables must be readable");
    assert_eq!(version, 3);
    assert_eq!(table_count, 4);
}

#[test]
fn persists_artifacts_and_anchors_with_an_import_event() {
    let mut fixture = Fixture::new();
    let artifact = sample_artifact(fixture.project.id);
    let artifact_id = artifact.id;
    fixture
        .database
        .insert_artifact(&artifact, &sample_anchors())
        .expect("artifact must be inserted");

    let artifacts = fixture
        .database
        .list_artifacts(&fixture.project.id)
        .expect("artifacts must load");
    assert_eq!(artifacts, vec![artifact]);
    assert_eq!(
        fixture
            .database
            .load_artifact_anchors(&artifact_id)
            .expect("anchors must load"),
        sample_anchors()
    );
    assert_eq!(
        fixture.database.event_count(&fixture.project.id).unwrap(),
        2
    );
}

#[test]
fn repeated_open_question_does_not_create_duplicate_info_requests() {
    let mut fixture = Fixture::new();
    let artifact_id = fixture.insert_artifact();
    let question = "OSS 使用哪个地域？";
    let first = fixture
        .database
        .create_blueprint_revision(
            &fixture.project.id,
            fixture.content(artifact_id, "第一版订单系统"),
            generation(),
            vec![info_request(question)],
        )
        .expect("first revision must be created");
    let second = fixture
        .database
        .create_blueprint_revision(
            &fixture.project.id,
            fixture.content(artifact_id, "第二版订单系统"),
            generation(),
            vec![info_request(question)],
        )
        .expect("second revision must be created");

    assert_eq!(first.revision_number, 1);
    assert_eq!(second.revision_number, 2);
    assert_eq!(first.status, BlueprintStatus::Draft);
    assert_eq!(first.source_artifact_ids, vec![artifact_id]);
    assert!(!first.content_hash.is_empty());
    let requests = fixture
        .database
        .list_info_requests(&fixture.project.id)
        .expect("info requests must load");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].question, question);
    assert_eq!(requests[0].status, InfoRequestStatus::Open);
    assert_eq!(requests[0].blueprint_revision_id, first.id);
    let latest = fixture
        .database
        .latest_blueprint(&fixture.project.id, BlueprintStatus::Draft)
        .expect("latest draft must load")
        .expect("latest draft must exist");
    assert_eq!(latest.id, second.id);
}

#[test]
fn publishing_supersedes_the_previous_published_blueprint() {
    let mut fixture = Fixture::new();
    let artifact_id = fixture.insert_artifact();
    let first = fixture
        .database
        .create_blueprint_revision(
            &fixture.project.id,
            fixture.content(artifact_id, "第一版订单系统"),
            generation(),
            Vec::new(),
        )
        .unwrap();
    let published_first = fixture
        .database
        .publish_blueprint_revision(&fixture.project.id, &first.id)
        .expect("first revision must publish");
    let second = fixture
        .database
        .create_blueprint_revision(
            &fixture.project.id,
            fixture.content(artifact_id, "第二版订单系统"),
            generation(),
            Vec::new(),
        )
        .unwrap();
    let published_second = fixture
        .database
        .publish_blueprint_revision(&fixture.project.id, &second.id)
        .expect("second revision must publish");

    assert_eq!(published_first.status, BlueprintStatus::Published);
    assert!(published_first.published_at.is_some());
    assert_eq!(published_second.status, BlueprintStatus::Published);
    let first_after = fixture.database.load_blueprint(&first.id).unwrap().unwrap();
    assert_eq!(first_after.status, BlueprintStatus::Superseded);
    let latest = fixture
        .database
        .latest_blueprint(&fixture.project.id, BlueprintStatus::Published)
        .unwrap()
        .unwrap();
    assert_eq!(latest.id, second.id);
    let error = fixture
        .database
        .publish_blueprint_revision(&fixture.project.id, &first.id)
        .expect_err("superseded revision must not publish again");
    assert!(matches!(error, StorageError::BlueprintNotDraft(_)));
}

#[test]
fn answering_an_info_request_creates_a_project_fact_with_version() {
    let mut fixture = Fixture::new();
    let artifact_id = fixture.insert_artifact();
    let revision = fixture
        .database
        .create_blueprint_revision(
            &fixture.project.id,
            fixture.content(artifact_id, "订单系统"),
            generation(),
            vec![info_request("OSS 使用哪个地域？")],
        )
        .unwrap();
    let request = fixture
        .database
        .list_info_requests(&fixture.project.id)
        .unwrap()
        .remove(0);

    let empty_error = fixture
        .database
        .resolve_info_request(&fixture.project.id, &request.id, "   ")
        .expect_err("empty answer must fail");
    assert!(matches!(empty_error, StorageError::EmptyInfoRequestAnswer));

    let fact = fixture
        .database
        .resolve_info_request(&fixture.project.id, &request.id, "  华东 1  ")
        .expect("answer must resolve");
    assert_eq!(fact.value, "华东 1");
    assert_eq!(fact.version, 1);
    assert_eq!(fact.source_info_request_id, request.id);
    assert_eq!(fact.project_id, fixture.project.id);

    let requests = fixture
        .database
        .list_info_requests(&fixture.project.id)
        .unwrap();
    assert_eq!(requests[0].status, InfoRequestStatus::Answered);
    assert_eq!(requests[0].answer.as_deref(), Some("华东 1"));
    assert!(requests[0].resolved_at.is_some());
    assert_eq!(
        fixture
            .database
            .list_project_facts(&fixture.project.id)
            .unwrap(),
        vec![fact]
    );
    let repeated = fixture
        .database
        .resolve_info_request(&fixture.project.id, &request.id, "华南 1")
        .expect_err("answered request must not resolve twice");
    assert!(matches!(repeated, StorageError::InfoRequestNotOpen(_)));
    assert_eq!(
        fixture
            .database
            .latest_blueprint(&fixture.project.id, BlueprintStatus::Draft)
            .unwrap()
            .unwrap()
            .id,
        revision.id
    );
}

#[test]
fn rejects_blueprints_that_fail_domain_validation() {
    let mut fixture = Fixture::new();
    let artifact_id = fixture.insert_artifact();
    let mut content = fixture.content(artifact_id, "订单系统");
    content.platforms.clear();
    let error = fixture
        .database
        .create_blueprint_revision(&fixture.project.id, content, generation(), Vec::new())
        .expect_err("invalid blueprint must fail");
    assert!(matches!(error, StorageError::InvalidBlueprint(_)));
    assert!(
        fixture
            .database
            .latest_blueprint(&fixture.project.id, BlueprintStatus::Draft)
            .unwrap()
            .is_none()
    );
}

fn create_v2_database(path: &Path, project: &Project) {
    fs::create_dir_all(path.parent().expect("database path must have a parent"))
        .expect("database parent must be created");
    let connection = Connection::open(path).expect("database must open directly");
    connection
        .execute_batch(
            "
            CREATE TABLE projects (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
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
        )
        .expect("v2 schema must be created");
    connection
        .execute(
            "INSERT INTO projects (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![
                project.id.to_string(),
                project.name,
                project.created_at.to_rfc3339()
            ],
        )
        .expect("v2 project must be inserted");
    connection
        .execute(
            "INSERT INTO events (event_id, project_id, occurred_at, event_type, payload_json)
             VALUES (?1, ?2, ?3, 'project_initialized', ?4)",
            params![
                "v2-event",
                project.id.to_string(),
                project.created_at.to_rfc3339(),
                r#"{"type":"project_initialized","data":{"name":"迁移项目"}}"#
            ],
        )
        .expect("v2 event must be inserted");
    connection
        .pragma_update(None, "user_version", 2)
        .expect("v2 version must be set");
}

#[test]
fn rejects_unknown_project_and_request_identifiers() {
    let mut fixture = Fixture::new();
    let artifact = sample_artifact(ProjectId::new());
    let error = fixture
        .database
        .insert_artifact(&artifact, &sample_anchors())
        .expect_err("foreign project must fail");
    assert!(matches!(error, StorageError::ProjectNotFound(_)));

    let request_id = coldstart_core::InfoRequestId::new();
    let error = fixture
        .database
        .resolve_info_request(&fixture.project.id, &request_id, "答案")
        .expect_err("unknown request must fail");
    assert!(matches!(error, StorageError::InfoRequestNotFound(_)));
}
