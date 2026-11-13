use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};

use coldstart_adapters::{GitAdapter, GitHubClient, MemorySecretStore, SecretStore};
use coldstart_app::{
    GitHubProjectService, IntegrationService, ProjectService, RepositoryDraft, SaveCredentialInput,
};
use coldstart_core::{
    CredentialEnvironment, CredentialKind, RepositoryAuthMode, RepositorySyncPolicy,
};
use tempfile::TempDir;
use tiny_http::{Header, Response, Server, StatusCode};

const TOKEN: &str = "gho_project_overview_sensitive_token";

struct CapturedRequest {
    method: String,
    url: String,
    authorization: Option<String>,
}

struct MockServer {
    base_url: String,
    receiver: mpsc::Receiver<CapturedRequest>,
    handle: JoinHandle<()>,
}

impl MockServer {
    fn start(body: &str) -> Self {
        let server = Server::http("127.0.0.1:0").expect("mock server must start");
        let port = server
            .server_addr()
            .to_ip()
            .expect("mock server must use TCP")
            .port();
        let (sender, receiver) = mpsc::channel();
        let body = body.to_owned();
        let handle = thread::spawn(move || {
            let request = server.recv().expect("mock server must receive a request");
            let authorization = request
                .headers()
                .iter()
                .find(|header| {
                    header
                        .field
                        .to_string()
                        .eq_ignore_ascii_case("authorization")
                })
                .map(|header| header.value.as_str().to_owned());
            sender
                .send(CapturedRequest {
                    method: request.method().to_string(),
                    url: request.url().to_owned(),
                    authorization,
                })
                .expect("captured request must be sent");
            let mut response = Response::from_string(body).with_status_code(StatusCode(200));
            response.add_header(
                Header::from_bytes("Content-Type", "application/json")
                    .expect("response header must be valid"),
            );
            request.respond(response).expect("response must be written");
        });
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            receiver,
            handle,
        }
    }

    fn finish(self) -> CapturedRequest {
        let captured = self
            .receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("mock server must capture one request");
        self.handle.join().expect("mock server must stop cleanly");
        captured
    }
}

struct Fixture {
    _directory: TempDir,
    home: PathBuf,
    work: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let home = directory.path().join("project-home");
        ProjectService::new()
            .create_project("GitHub Projects 只读概览测试", home.clone())
            .expect("project must create");
        let work = directory.path().join("work");
        fs::create_dir_all(&work).expect("work directory must create");
        git(&work, &["init", "-b", "main"]);
        git(&work, &["config", "user.name", "ColdStart Test"]);
        git(
            &work,
            &["config", "user.email", "coldstart@example.invalid"],
        );
        fs::write(work.join("README.md"), "initial\n").expect("initial file must write");
        git(&work, &["add", "README.md"]);
        git(&work, &["commit", "-m", "initial"]);
        git(
            &work,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/octocat/demo.git",
            ],
        );
        Self {
            _directory: directory,
            home,
            work,
        }
    }

    fn project_service(&self, base_url: &str) -> GitHubProjectService {
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecretStore::default());
        let integration =
            IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);
        let workspace = integration
            .save_credential(
                self.home.clone(),
                SaveCredentialInput {
                    name: "GitHub Projects Test".to_owned(),
                    provider: "github".to_owned(),
                    kind: CredentialKind::OAuthToken,
                    environment: CredentialEnvironment::Shared,
                    secret: TOKEN.to_owned(),
                },
            )
            .expect("credential must save");
        let credential_id = workspace.credentials[0].id;
        integration
            .save_repository(
                self.home.clone(),
                RepositoryDraft {
                    key: "web".to_owned(),
                    name: "Web 前台".to_owned(),
                    local_path: self.work.clone(),
                    remote_url: "https://github.com/octocat/demo.git".to_owned(),
                    default_branch: "main".to_owned(),
                    protected_branches: vec!["main".to_owned()],
                    auth_mode: RepositoryAuthMode::Credential,
                    credential_id: Some(credential_id),
                    sync_policy: RepositorySyncPolicy::Manual,
                },
            )
            .expect("repository must save");
        let github =
            GitHubClient::with_endpoints(base_url, base_url).expect("GitHub client must create");
        GitHubProjectService::with_dependencies(secrets, Some(github))
    }
}

#[test]
fn reads_projects_without_writing_project_state_or_secrets() {
    let fixture = Fixture::new();
    let body = serde_json::json!({
        "data": {
            "repository": {
                "projectsV2": {
                    "pageInfo": { "hasNextPage": false },
                    "nodes": [{
                        "id": "PVT_kwDO_project_1",
                        "number": 3,
                        "title": "冷启动开发计划",
                        "url": "https://github.com/orgs/octocat/projects/3",
                        "closed": false,
                        "updatedAt": "2026-11-13T12:00:00Z",
                        "items": {
                            "totalCount": 1,
                            "pageInfo": { "hasNextPage": false },
                            "nodes": [{
                                "id": "PVTI_kwDO_item_1",
                                "updatedAt": "2026-11-13T11:30:00Z",
                                "fieldValues": {
                                    "nodes": [{
                                        "__typename": "ProjectV2ItemFieldSingleSelectValue",
                                        "name": "进行中",
                                        "field": { "name": "Status" }
                                    }]
                                },
                                "content": {
                                    "__typename": "Issue",
                                    "number": 7,
                                    "title": "新增导出",
                                    "state": "OPEN",
                                    "url": "https://github.com/octocat/demo/issues/7",
                                    "updatedAt": "2026-11-13T11:00:00Z"
                                }
                            }]
                        }
                    }]
                }
            }
        }
    })
    .to_string();
    let server = MockServer::start(&body);
    let service = fixture.project_service(&server.base_url);
    let status_before = ProjectService::new()
        .read_status(fixture.home.clone())
        .expect("status must load");
    let overview = service
        .list_projects(fixture.home.clone(), "web")
        .expect("projects must load");

    assert_eq!(overview.repository_key, "web");
    assert_eq!(overview.repository_label, "octocat/demo");
    assert_eq!(overview.projects.len(), 1);
    let project = &overview.projects[0];
    assert_eq!(project.title, "冷启动开发计划");
    assert_eq!(project.items[0].kind, "issue");
    assert_eq!(project.items[0].state.as_deref(), Some("open"));
    assert_eq!(project.items[0].fields[0].value, "进行中");
    let captured = server.finish();
    assert_eq!(captured.method, "POST");
    assert_eq!(captured.url, "/graphql");
    assert_eq!(
        captured.authorization.as_deref(),
        Some("Bearer gho_project_overview_sensitive_token")
    );

    let status_after = ProjectService::new()
        .read_status(fixture.home.clone())
        .expect("status must load after projects");
    assert_eq!(status_before.event_count, status_after.event_count);
    assert!(!directory_contains(
        &fixture.home.join(".coldstart"),
        TOKEN.as_bytes()
    ));
}

fn git(directory: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("git command must execute");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn directory_contains(directory: &Path, needle: &[u8]) -> bool {
    let Ok(entries) = fs::read_dir(directory) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if directory_contains(&path, needle) {
                return true;
            }
            continue;
        }
        if fs::read(&path)
            .map(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}
