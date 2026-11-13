use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};

use coldstart_adapters::{GitAdapter, GitHubClient, MemorySecretStore, SecretStore};
use coldstart_app::{
    AppError, GitHubIssueService, IntegrationService, ProjectService, RepositoryDraft,
    SaveCredentialInput,
};
use coldstart_core::{
    CredentialEnvironment, CredentialKind, RepositoryAuthMode, RepositorySyncPolicy,
};
use tempfile::TempDir;
use tiny_http::{Header, Response, Server, StatusCode};

const TOKEN: &str = "gho_issue_intake_sensitive_token";

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
            .create_project("Issue 需求导入测试", home.clone())
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

    fn issue_service(&self, base_url: &str) -> GitHubIssueService {
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecretStore::default());
        let integration =
            IntegrationService::with_dependencies(secrets.clone(), GitAdapter::new(), None);
        let workspace = integration
            .save_credential(
                self.home.clone(),
                SaveCredentialInput {
                    name: "GitHub Test".to_owned(),
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
        GitHubIssueService::with_dependencies(secrets, Some(github))
    }
}

#[test]
fn lists_open_issues_for_a_mapped_repository() {
    let fixture = Fixture::new();
    let body = serde_json::json!([
        {
            "id": 1,
            "number": 7,
            "title": "历史项目新增导出需求",
            "state": "open",
            "body": "希望支持 CSV 导出。\n\n需要包含筛选条件。",
            "labels": [{ "name": "backend" }, { "name": "enhancement" }],
            "html_url": "https://github.com/octocat/demo/issues/7",
            "comments": 3,
            "updated_at": "2026-11-13T10:00:00Z"
        }
    ])
    .to_string();
    let server = MockServer::start(&body);
    let service = fixture.issue_service(&server.base_url);
    let list = service
        .list_issues(fixture.home.clone(), "web")
        .expect("issues must load");

    assert_eq!(list.repository_key, "web");
    assert_eq!(list.repository_label, "octocat/demo");
    assert_eq!(list.issues.len(), 1);
    assert_eq!(list.issues[0].number, 7);
    assert_eq!(list.issues[0].labels, vec!["backend", "enhancement"]);
    assert_eq!(
        list.issues[0].source_reference,
        "github://octocat/demo/issues/7"
    );
    assert!(list.issues[0].body_preview.contains("CSV 导出"));
    let captured = server.finish();
    assert_eq!(captured.method, "GET");
    assert_eq!(
        captured.url,
        "/repos/octocat/demo/issues?state=open&sort=updated&direction=desc&per_page=100"
    );
    assert_eq!(
        captured.authorization.as_deref(),
        Some("Bearer gho_issue_intake_sensitive_token")
    );
}

#[test]
fn imports_an_issue_as_a_requirement_source_and_rejects_duplicates() {
    let fixture = Fixture::new();
    let body = serde_json::json!({
        "id": 1,
        "number": 7,
        "title": "历史项目新增导出需求",
        "state": "open",
        "body": "希望支持 CSV 导出。\n\n需要包含筛选条件。",
        "labels": [{ "name": "backend" }],
        "html_url": "https://github.com/octocat/demo/issues/7",
        "comments": 3,
        "updated_at": "2026-11-13T10:00:00Z"
    })
    .to_string();
    let server = MockServer::start(&body);
    let service = fixture.issue_service(&server.base_url);
    let workspace = service
        .import_issue(fixture.home.clone(), "web", 7)
        .expect("issue must import");

    assert_eq!(workspace.sources.len(), 1);
    let source = &workspace.sources[0];
    assert_eq!(source.file_name, "github-issue-7.md");
    assert_eq!(source.import_source, "github://octocat/demo/issues/7");
    let anchors = workspace
        .source_anchors
        .get(&source.id.to_string())
        .expect("source anchors must exist");
    assert!(
        anchors
            .iter()
            .any(|anchor| anchor.text.contains("希望支持 CSV 导出"))
    );
    let artifact_source = fixture
        .home
        .join(".coldstart")
        .join("artifacts")
        .join(source.id.to_string())
        .join("source.md");
    let markdown = fs::read_to_string(artifact_source).expect("artifact source must read");
    assert!(markdown.contains("# #7 历史项目新增导出需求"));
    assert!(markdown.contains("评论数：3（评论未导入）"));
    assert!(!directory_contains(
        &fixture.home.join(".coldstart"),
        TOKEN.as_bytes()
    ));

    let error = service
        .import_issue(fixture.home.clone(), "web", 7)
        .expect_err("duplicate issue must fail");
    assert!(matches!(
        error,
        AppError::GitHubIssueAlreadyImported {
            ref repository,
            number: 7,
        } if repository == "octocat/demo"
    ));
    assert_eq!(error.code(), "github_issue_already_imported");
    let captured = server.finish();
    assert_eq!(captured.url, "/repos/octocat/demo/issues/7");
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
