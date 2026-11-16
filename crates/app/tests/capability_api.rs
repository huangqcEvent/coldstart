use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use chrono::Utc;
use coldstart_adapters::{
    CapabilityApiClient, CapabilityApiError, CapabilityApiHttpRequest, CapabilityApiHttpResponse,
    GitAdapter, MemorySecretStore, SecretStore,
};
use coldstart_app::{
    CallCapabilityApiRequest, CapabilityApiQueryParameter, CapabilityApiService,
    IntegrationService, PlanService, ProjectService,
};
use coldstart_core::{
    AcceptanceCriterion, AcceptancePolicy, ArtifactFormat, ArtifactId, ArtifactMetadata,
    BlueprintContent, BlueprintEntityId, BlueprintGeneration, CapabilityApiEndpoint,
    CapabilityApiEndpointId, CapabilityApiEndpointInput, CapabilityApiMethod, CapabilityDescriptor,
    CapabilityObligation, CapabilityState, CapabilityStatus, ClaimResult, CredentialEnvironment,
    CredentialKind, CredentialMetadata, DocumentAnchor, GenerationSource,
    MAX_CAPABILITY_API_REQUEST_BODY_BYTES, ModuleDefinition, PlanContent, PlatformDefinition,
    PlatformKind, ProjectId, Repository, RepositoryAuthMode, RepositorySyncPolicy,
    RepositoryVerificationCommand, SourceReference, TaskKind, TaskSpec, TraceableText, WorkItemId,
};
use coldstart_storage::Database;
use tempfile::TempDir;

const SECRET: &str = "APP_CAPABILITY_SECRET_MARKER";
const SECRET_REF: &str = "coldstart:test:oss";
const ENVIRONMENT: &str = "development";

struct Fixture {
    _directory: TempDir,
    home: PathBuf,
    project_id: ProjectId,
    capability_id: BlueprintEntityId,
    endpoint_id: CapabilityApiEndpointId,
    work_id: WorkItemId,
    lease_token: String,
    secrets: Arc<MemorySecretStore>,
}

impl Fixture {
    fn new(capability_authorized: bool) -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let home = directory.path().join("project");
        let status = ProjectService::new()
            .create_project("能力代理测试", home.clone())
            .expect("project must create");
        let project_id = status.project_id;
        let database_path = home.join(".coldstart").join("state.db");
        let mut database = Database::open(&database_path).expect("database must open");
        let capability_id = BlueprintEntityId::derive(&project_id, "capability", "阿里云 OSS");
        let artifact_id = ArtifactId::new();
        database
            .insert_artifact(
                &ArtifactMetadata {
                    id: artifact_id,
                    project_id,
                    file_name: "requirements.md".to_owned(),
                    format: ArtifactFormat::Markdown,
                    content_hash: "capability-api-content-hash".to_owned(),
                    text_hash: "capability-api-text-hash".to_owned(),
                    byte_size: 64,
                    import_source: "requirements.md".to_owned(),
                    imported_at: Utc::now(),
                },
                &[DocumentAnchor {
                    id: "section_1".to_owned(),
                    label: "文件上传".to_owned(),
                    text: "需要文件上传".to_owned(),
                }],
            )
            .expect("artifact must insert");
        let blueprint = database
            .create_blueprint_revision(
                &project_id,
                blueprint_content(project_id, artifact_id, &capability_id),
                BlueprintGeneration {
                    source: GenerationSource::Manual,
                    model_provider: None,
                    model_name: None,
                    prompt_version: None,
                },
                vec![],
            )
            .expect("blueprint draft must create");
        database
            .publish_blueprint_revision(&project_id, &blueprint.id)
            .expect("blueprint must publish");
        let repository = Repository::new(
            project_id,
            "server".to_owned(),
            "服务端".to_owned(),
            directory.path().join("server"),
            "https://github.com/example/server.git".to_owned(),
            "main".to_owned(),
            vec!["main".to_owned()],
            RepositoryAuthMode::None,
            None,
            RepositorySyncPolicy::Manual,
        )
        .expect("repository must validate");
        database
            .save_repository(&repository)
            .expect("repository must save");
        database
            .replace_repository_verification_commands(
                &project_id,
                "server",
                vec![RepositoryVerificationCommand {
                    name: "test".to_owned(),
                    executable: std::env::current_exe().expect("test executable must exist"),
                    args: vec![],
                    timeout_seconds: 60,
                    position: 0,
                }],
            )
            .expect("verification command must save");
        let plan = database
            .create_plan_revision(
                &project_id,
                blueprint.id,
                PlanContent {
                    tasks: vec![task(project_id, &capability_id, capability_authorized)],
                    warnings: vec![],
                },
                Utc::now(),
            )
            .expect("plan draft must create");
        database
            .publish_plan_revision(&project_id, &plan.id, Utc::now())
            .expect("plan must publish");

        let credential = CredentialMetadata::new(
            project_id,
            "OSS 开发凭据".to_owned(),
            "aliyun_oss".to_owned(),
            CredentialKind::ApiKey,
            CredentialEnvironment::Development,
            SECRET_REF.to_owned(),
        )
        .expect("credential must validate");
        database
            .save_credential(&credential)
            .expect("credential must save");
        let capability = CapabilityState::new(
            CapabilityDescriptor {
                project_id,
                capability_id: capability_id.clone(),
                name: "阿里云 OSS".to_owned(),
                provider: "aliyun_oss".to_owned(),
                environment: ENVIRONMENT.to_owned(),
            },
            CapabilityStatus::Configured,
            Some(credential.id),
            None,
        )
        .expect("capability state must validate");
        database
            .save_capability_state(&capability)
            .expect("capability state must save");
        let endpoint = CapabilityApiEndpoint::new(
            project_id,
            CapabilityApiEndpointInput {
                capability_id: capability_id.clone(),
                environment: ENVIRONMENT.to_owned(),
                name: "附件 API".to_owned(),
                base_url: "https://api.example.com".to_owned(),
                path_prefix: "/v1".to_owned(),
                allowed_methods: vec![CapabilityApiMethod::Get, CapabilityApiMethod::Post],
                credential_id: Some(credential.id),
                auth_header: Some("Authorization".to_owned()),
                auth_scheme: Some("Bearer".to_owned()),
            },
        )
        .expect("endpoint must validate");
        database
            .save_capability_api_endpoint(&endpoint)
            .expect("endpoint must save");

        let claim_result = database
            .claim_work(
                &project_id,
                "codex",
                Some("server"),
                "capability-api-claim",
                Utc::now(),
                chrono::Duration::minutes(30),
            )
            .expect("work must claim");
        let claimed = match claim_result {
            ClaimResult::Claimed(claimed) => claimed,
            other => panic!("expected claimed work: {other:?}"),
        };
        let secrets = Arc::new(MemorySecretStore::default());
        secrets.set(SECRET_REF, SECRET).expect("secret must store");
        Self {
            _directory: directory,
            home,
            project_id,
            capability_id,
            endpoint_id: endpoint.id,
            work_id: claimed.work_item.id,
            lease_token: claimed.lease_token,
            secrets,
        }
    }

    fn request(&self, method: CapabilityApiMethod, path: &str) -> CallCapabilityApiRequest {
        CallCapabilityApiRequest {
            capability_id: self.capability_id.clone(),
            environment: ENVIRONMENT.to_owned(),
            endpoint_id: self.endpoint_id,
            work_id: self.work_id,
            lease_token: self.lease_token.clone(),
            method,
            path: path.to_owned(),
            query: vec![],
            body: None,
            content_type: None,
        }
    }

    fn database(&self) -> Database {
        Database::open(&self.home.join(".coldstart").join("state.db")).expect("database must open")
    }
}

#[derive(Default)]
struct FakeCapabilityApiClient {
    requests: Mutex<Vec<CapabilityApiHttpRequest>>,
    response: Mutex<Option<Result<CapabilityApiHttpResponse, CapabilityApiError>>>,
}

impl FakeCapabilityApiClient {
    fn success(status: u16, content_type: Option<&str>, body: &str) -> Self {
        Self {
            requests: Mutex::new(vec![]),
            response: Mutex::new(Some(Ok(CapabilityApiHttpResponse {
                status,
                content_type: content_type.map(str::to_owned),
                body: body.as_bytes().to_vec(),
                truncated: false,
            }))),
        }
    }

    fn failure(error: CapabilityApiError) -> Self {
        Self {
            requests: Mutex::new(vec![]),
            response: Mutex::new(Some(Err(error))),
        }
    }

    fn requests(&self) -> Vec<CapabilityApiHttpRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

impl CapabilityApiClient for FakeCapabilityApiClient {
    fn execute(
        &self,
        request: &CapabilityApiHttpRequest,
    ) -> Result<CapabilityApiHttpResponse, CapabilityApiError> {
        self.requests
            .lock()
            .expect("requests lock")
            .push(request.clone());
        self.response
            .lock()
            .expect("response lock")
            .take()
            .expect("fake response must be configured")
    }
}

fn blueprint_content(
    project_id: ProjectId,
    artifact_id: ArtifactId,
    capability_id: &BlueprintEntityId,
) -> BlueprintContent {
    let source = SourceReference {
        artifact_id,
        anchor_id: "section_1".to_owned(),
        quote: Some("需要文件上传".to_owned()),
    };
    let platform_id = BlueprintEntityId::derive(&project_id, "platform", "Server");
    BlueprintContent {
        product_goal: TraceableText {
            id: BlueprintEntityId::derive(&project_id, "goal", "product"),
            text: "构建订单系统".to_owned(),
            sources: vec![source.clone()],
        },
        in_scope: vec![],
        out_of_scope: vec![],
        roles: vec![],
        platforms: vec![PlatformDefinition {
            id: platform_id,
            name: "服务端".to_owned(),
            kind: PlatformKind::Server,
            description: "Rust 服务".to_owned(),
            sources: vec![source.clone()],
        }],
        modules: vec![ModuleDefinition {
            id: BlueprintEntityId::derive(&project_id, "module", "Files"),
            name: "文件".to_owned(),
            description: "文件上传".to_owned(),
            platforms: vec![],
            depends_on: vec![],
            sources: vec![source.clone()],
        }],
        technologies: vec![],
        capabilities: vec![CapabilityObligation {
            id: capability_id.clone(),
            name: "阿里云 OSS".to_owned(),
            provider: "aliyun_oss".to_owned(),
            description: "上传附件".to_owned(),
            sources: vec![source],
        }],
        release_prerequisites: vec![],
        assumptions: vec![],
        risks: vec![],
    }
}

fn task(
    project_id: ProjectId,
    capability_id: &BlueprintEntityId,
    capability_authorized: bool,
) -> TaskSpec {
    TaskSpec {
        task_key: "files:upload".to_owned(),
        title: "接入文件上传".to_owned(),
        kind: TaskKind::AgentCoding,
        goal: "实现文件上传".to_owned(),
        scope: vec!["上传接口".to_owned()],
        out_of_scope: vec!["不写入明文密钥".to_owned()],
        module_id: Some(BlueprintEntityId::derive(&project_id, "module", "Files")),
        platform_id: Some(BlueprintEntityId::derive(&project_id, "platform", "Server")),
        repository_key: Some("server".to_owned()),
        base_ref: Some("main".to_owned()),
        work_branch: Some("codex/files-upload".to_owned()),
        allowed_paths: vec!["src".to_owned()],
        context_refs: if capability_authorized {
            vec![capability_id.clone()]
        } else {
            vec![]
        },
        acceptance_criteria: vec![AcceptanceCriterion {
            id: "upload".to_owned(),
            text: "上传成功".to_owned(),
            required_evidence: true,
        }],
        acceptance_policy: AcceptancePolicy::AgentReport,
        evidence_requirements: vec!["测试摘要".to_owned()],
        priority: 100,
        critical_path: true,
        context_budget_tokens: 12_000,
        resource_keys: vec!["server".to_owned()],
        dependencies: vec![],
    }
}

fn service(fixture: &Fixture, client: Arc<FakeCapabilityApiClient>) -> CapabilityApiService {
    CapabilityApiService::with_dependencies(fixture.secrets.clone(), client)
}

#[test]
fn creates_updates_and_deletes_capability_api_endpoints() {
    let fixture = Fixture::new(true);
    let credential_id = fixture
        .database()
        .list_credentials(&fixture.project_id)
        .expect("credentials must load")[0]
        .id;
    let integrations =
        IntegrationService::with_dependencies(fixture.secrets.clone(), GitAdapter::new(), None);
    let input = |name: &str, method: CapabilityApiMethod| CapabilityApiEndpointInput {
        capability_id: fixture.capability_id.clone(),
        environment: ENVIRONMENT.to_owned(),
        name: name.to_owned(),
        base_url: "https://upload.example.com".to_owned(),
        path_prefix: "/v2".to_owned(),
        allowed_methods: vec![method],
        credential_id: Some(credential_id),
        auth_header: Some("X-API-Key".to_owned()),
        auth_scheme: None,
    };

    let workspace = integrations
        .save_capability_api_endpoint(
            fixture.home.clone(),
            None,
            input("上传白名单", CapabilityApiMethod::Get),
        )
        .expect("endpoint must create");
    let created = workspace
        .capability_api_endpoints
        .iter()
        .find(|endpoint| endpoint.name == "上传白名单")
        .expect("created endpoint must appear")
        .clone();
    assert_eq!(created.allowed_methods, vec![CapabilityApiMethod::Get]);

    let workspace = integrations
        .save_capability_api_endpoint(
            fixture.home.clone(),
            Some(created.id),
            input("上传白名单 v2", CapabilityApiMethod::Post),
        )
        .expect("endpoint must update");
    let updated = workspace
        .capability_api_endpoints
        .iter()
        .find(|endpoint| endpoint.id == created.id)
        .expect("updated endpoint must appear");
    assert_eq!(updated.name, "上传白名单 v2");
    assert_eq!(updated.allowed_methods, vec![CapabilityApiMethod::Post]);

    let workspace = integrations
        .delete_capability_api_endpoint(fixture.home.clone(), created.id)
        .expect("endpoint must delete");
    assert!(
        workspace
            .capability_api_endpoints
            .iter()
            .all(|endpoint| endpoint.id != created.id)
    );
}

#[test]
fn work_context_lists_authorized_endpoint_metadata_without_secrets() {
    let fixture = Fixture::new(true);
    let context = PlanService::new()
        .get_context(
            fixture.home.clone(),
            fixture.work_id,
            fixture.lease_token.clone(),
        )
        .expect("task context must load");
    assert!(context.markdown.contains("可调用的能力 API"));
    assert!(context.markdown.contains(&fixture.endpoint_id.to_string()));
    assert!(context.markdown.contains("附件 API"));
    assert!(context.markdown.contains("/v1"));
    assert!(!context.markdown.contains(SECRET));
}

#[test]
fn injects_the_secret_redacts_the_response_and_does_not_write_events() {
    let fixture = Fixture::new(true);
    let events_before = fixture
        .database()
        .event_count(&fixture.project_id)
        .expect("event count must load");
    let client = Arc::new(FakeCapabilityApiClient::success(
        201,
        Some("application/json"),
        &format!("{{\"echo\":\"{SECRET}\"}}"),
    ));
    let mut request = fixture.request(CapabilityApiMethod::Post, "/v1/files");
    request.query = vec![CapabilityApiQueryParameter {
        key: "mode".to_owned(),
        value: "a&b".to_owned(),
    }];
    request.body = Some("{\"name\":\"demo\"}".to_owned());
    request.content_type = Some("application/json".to_owned());

    let result = service(&fixture, client.clone())
        .call(fixture.home.clone(), request)
        .expect("call must succeed");

    assert_eq!(result.status, 201);
    assert_eq!(result.endpoint_id, fixture.endpoint_id);
    assert_eq!(result.capability_id, fixture.capability_id);
    assert_eq!(result.environment, ENVIRONMENT);
    assert_eq!(result.method, CapabilityApiMethod::Post);
    assert_eq!(result.path, "/v1/files");
    assert_eq!(result.content_type.as_deref(), Some("application/json"));
    assert_eq!(result.body, "{\"echo\":\"[REDACTED]\"}");
    assert!(result.redacted);
    assert!(!serde_json::to_string(&result).unwrap().contains(SECRET));

    let requests = client.requests();
    assert_eq!(requests.len(), 1);
    let captured = &requests[0];
    assert_eq!(captured.url, "https://api.example.com/v1/files?mode=a%26b");
    assert_eq!(captured.method, CapabilityApiMethod::Post);
    assert_eq!(
        captured.body.as_deref(),
        Some(b"{\"name\":\"demo\"}".as_slice())
    );
    assert!(
        captured.headers.iter().any(|(name, value)| {
            name == "Authorization" && value == &format!("Bearer {SECRET}")
        })
    );
    assert!(
        captured
            .headers
            .iter()
            .any(|(name, value)| name == "Content-Type" && value == "application/json")
    );
    assert_eq!(
        fixture
            .database()
            .event_count(&fixture.project_id)
            .expect("event count must load"),
        events_before
    );
}

#[test]
fn rejects_invalid_leases_before_transport() {
    let fixture = Fixture::new(true);
    let client = Arc::new(FakeCapabilityApiClient::success(200, None, "ok"));
    let mut request = fixture.request(CapabilityApiMethod::Get, "/v1/files");
    request.lease_token = "invalid-lease".to_owned();

    let error = service(&fixture, client.clone())
        .call(fixture.home.clone(), request)
        .expect_err("invalid lease must fail");
    assert_eq!(error.code(), "lease_invalid");
    assert!(client.requests().is_empty());
}

#[test]
fn rejects_capabilities_not_referenced_by_the_task_contract() {
    let fixture = Fixture::new(false);
    let client = Arc::new(FakeCapabilityApiClient::success(200, None, "ok"));

    let error = service(&fixture, client.clone())
        .call(
            fixture.home.clone(),
            fixture.request(CapabilityApiMethod::Get, "/v1/files"),
        )
        .expect_err("unauthorized capability must fail");
    assert_eq!(error.code(), "capability_api_not_authorized");
    assert!(client.requests().is_empty());
}

#[test]
fn rejects_unavailable_capability_status() {
    let fixture = Fixture::new(true);
    let mut database = fixture.database();
    let mut state = database
        .list_capability_states(&fixture.project_id)
        .expect("capabilities must load")
        .into_iter()
        .find(|state| state.capability_id == fixture.capability_id)
        .expect("capability must exist");
    state.status = CapabilityStatus::Missing;
    state.updated_at = Utc::now();
    database
        .save_capability_state(&state)
        .expect("capability must update");
    let client = Arc::new(FakeCapabilityApiClient::success(200, None, "ok"));

    let error = service(&fixture, client.clone())
        .call(
            fixture.home.clone(),
            fixture.request(CapabilityApiMethod::Get, "/v1/files"),
        )
        .expect_err("unavailable capability must fail");
    assert_eq!(error.code(), "capability_api_unavailable");
    assert!(client.requests().is_empty());
}

#[test]
fn rejects_disallowed_methods_paths_and_endpoints_before_transport() {
    let fixture = Fixture::new(true);
    let client = Arc::new(FakeCapabilityApiClient::success(200, None, "ok"));

    let error = service(&fixture, client.clone())
        .call(
            fixture.home.clone(),
            fixture.request(CapabilityApiMethod::Delete, "/v1/files"),
        )
        .expect_err("method must be rejected");
    assert_eq!(error.code(), "capability_api_method_not_allowed");

    let error = service(&fixture, client.clone())
        .call(
            fixture.home.clone(),
            fixture.request(CapabilityApiMethod::Get, "/outside"),
        )
        .expect_err("path must be rejected");
    assert_eq!(error.code(), "capability_api_path_not_allowed");

    let mut request = fixture.request(CapabilityApiMethod::Get, "/v1/files");
    request.endpoint_id = CapabilityApiEndpointId::new();
    let error = service(&fixture, client.clone())
        .call(fixture.home.clone(), request)
        .expect_err("endpoint must be rejected");
    assert_eq!(error.code(), "capability_api_endpoint_not_found");
    assert!(client.requests().is_empty());
}

#[test]
fn rejects_credentials_from_the_wrong_environment() {
    let fixture = Fixture::new(true);
    let mut database = fixture.database();
    let production_credential = CredentialMetadata::new(
        fixture.project_id,
        "OSS 生产凭据".to_owned(),
        "aliyun_oss".to_owned(),
        CredentialKind::ApiKey,
        CredentialEnvironment::Production,
        "coldstart:test:oss-production".to_owned(),
    )
    .expect("credential must validate");
    database
        .save_credential(&production_credential)
        .expect("credential must save");
    fixture
        .secrets
        .set("coldstart:test:oss-production", SECRET)
        .expect("secret must store");
    let mut endpoint = database
        .capability_api_endpoint(&fixture.project_id, &fixture.endpoint_id)
        .expect("endpoint must load")
        .expect("endpoint must exist");
    endpoint.credential_id = Some(production_credential.id);
    database
        .save_capability_api_endpoint(&endpoint)
        .expect("endpoint must update");
    let client = Arc::new(FakeCapabilityApiClient::success(200, None, "ok"));

    let error = service(&fixture, client.clone())
        .call(
            fixture.home.clone(),
            fixture.request(CapabilityApiMethod::Get, "/v1/files"),
        )
        .expect_err("environment mismatch must fail");
    assert_eq!(error.code(), "capability_api_credential_mismatch");
    assert!(client.requests().is_empty());
}

#[test]
fn rejects_missing_secrets_without_leaking_the_secret_reference() {
    let fixture = Fixture::new(true);
    fixture
        .secrets
        .delete(SECRET_REF)
        .expect("secret must delete");
    let client = Arc::new(FakeCapabilityApiClient::success(200, None, "ok"));

    let error = service(&fixture, client.clone())
        .call(
            fixture.home.clone(),
            fixture.request(CapabilityApiMethod::Get, "/v1/files"),
        )
        .expect_err("missing secret must fail");
    assert_eq!(error.code(), "capability_api_credential_missing");
    assert!(!error.to_string().contains(SECRET_REF));
    assert!(client.requests().is_empty());
}

#[test]
fn rejects_oversized_request_bodies_before_transport() {
    let fixture = Fixture::new(true);
    let client = Arc::new(FakeCapabilityApiClient::success(200, None, "ok"));
    let mut request = fixture.request(CapabilityApiMethod::Post, "/v1/files");
    request.body = Some("x".repeat(MAX_CAPABILITY_API_REQUEST_BODY_BYTES + 1));

    let error = service(&fixture, client.clone())
        .call(fixture.home.clone(), request)
        .expect_err("oversized body must fail");
    assert_eq!(error.code(), "capability_api_request_too_large");
    assert!(client.requests().is_empty());
}

#[test]
fn maps_transport_failures_to_stable_error_codes() {
    for (failure, expected_code) in [
        (CapabilityApiError::Timeout, "capability_api_timeout"),
        (
            CapabilityApiError::RedirectNotAllowed,
            "capability_api_redirect_not_allowed",
        ),
        (
            CapabilityApiError::Request(SECRET.to_owned()),
            "capability_api_request_failed",
        ),
    ] {
        let fixture = Fixture::new(true);
        let client = Arc::new(FakeCapabilityApiClient::failure(failure));
        let error = service(&fixture, client)
            .call(
                fixture.home.clone(),
                fixture.request(CapabilityApiMethod::Get, "/v1/files"),
            )
            .expect_err("transport failure must fail");
        assert_eq!(error.code(), expected_code);
        assert!(!error.to_string().contains(SECRET));
    }
}

#[test]
fn returns_non_success_http_statuses_without_turning_them_into_transport_errors() {
    let fixture = Fixture::new(true);
    let client = Arc::new(FakeCapabilityApiClient::success(
        404,
        Some("application/json"),
        "{\"error\":\"missing\"}",
    ));

    let result = service(&fixture, client)
        .call(
            fixture.home.clone(),
            fixture.request(CapabilityApiMethod::Get, "/v1/missing"),
        )
        .expect("HTTP error response must be returned");
    assert_eq!(result.status, 404);
    assert_eq!(result.body, "{\"error\":\"missing\"}");
}
