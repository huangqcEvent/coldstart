use std::{
    sync::mpsc,
    thread::{self, JoinHandle},
};

use coldstart_adapters::{
    CreateDraftPullRequestRequest, CreatePrivateRepositoryRequest, DeviceFlowPoll, GitHubClient,
    GitHubError, GitHubRepositoryCoordinates, GitHubToken,
};
use tiny_http::{Header, Response, Server, StatusCode};

const TOKEN: &str = "gho_sensitive_test_token";

struct CapturedRequest {
    method: String,
    url: String,
    authorization: Option<String>,
    body: String,
}

struct MockResponse {
    status: u16,
    headers: Vec<(&'static str, &'static str)>,
    body: String,
}

struct MockServer {
    base_url: String,
    receiver: mpsc::Receiver<CapturedRequest>,
    handle: JoinHandle<()>,
}

impl MockServer {
    fn start(status: u16, headers: Vec<(&'static str, &'static str)>, body: &str) -> Self {
        Self::start_response(MockResponse {
            status,
            headers,
            body: body.to_owned(),
        })
    }

    fn start_response(response: MockResponse) -> Self {
        let MockResponse {
            status,
            headers,
            body,
        } = response;
        let server = Server::http("127.0.0.1:0").expect("mock server must start");
        let port = server
            .server_addr()
            .to_ip()
            .expect("mock server must use TCP")
            .port();
        let (sender, receiver) = mpsc::channel();
        let handle = thread::spawn(move || {
            let mut request = server.recv().expect("mock server must receive a request");
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
            let method = request.method().to_string();
            let url = request.url().to_owned();
            let mut captured_request_body = String::new();
            request
                .as_reader()
                .read_to_string(&mut captured_request_body)
                .expect("request body must be readable");
            sender
                .send(CapturedRequest {
                    method,
                    url,
                    authorization,
                    body: captured_request_body,
                })
                .expect("captured request must be sent");
            let mut response = Response::from_string(body).with_status_code(StatusCode(status));
            response.add_header(
                Header::from_bytes("Content-Type", "application/json")
                    .expect("response header must be valid"),
            );
            for (name, value) in headers {
                response.add_header(
                    Header::from_bytes(name, value).expect("response header must be valid"),
                );
            }
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

fn client(base_url: &str) -> GitHubClient {
    GitHubClient::with_endpoints(base_url, base_url).expect("client must be valid")
}

#[test]
fn starts_device_flow_without_leaking_the_device_code_in_debug() {
    let device_code = "device-code-sensitive";
    let body = serde_json::json!({
        "device_code": device_code,
        "user_code": "ABCD-EFGH",
        "verification_uri": "https://github.com/login/device",
        "expires_in": 900,
        "interval": 5
    })
    .to_string();
    let server = MockServer::start(200, vec![], &body);
    let flow = client(&server.base_url)
        .start_device_flow("client-id", &["repo"])
        .expect("device flow must start");

    assert_eq!(flow.user_code, "ABCD-EFGH");
    assert_eq!(flow.verification_uri, "https://github.com/login/device");
    assert_eq!(flow.expires_in, 900);
    assert_eq!(flow.interval, 5);
    assert_eq!(flow.device_code(), device_code);
    assert!(!format!("{flow:?}").contains(device_code));

    let captured = server.finish();
    assert_eq!(captured.method, "POST");
    assert_eq!(captured.url, "/login/device/code");
    let request: serde_json::Value =
        serde_json::from_str(&captured.body).expect("request body must be JSON");
    assert_eq!(request["client_id"], "client-id");
    assert_eq!(request["scope"], "repo");
}

#[test]
fn polls_device_flow_and_maps_pending_and_authorized_states() {
    let pending_body = serde_json::json!({ "error": "authorization_pending" }).to_string();
    let server = MockServer::start(200, vec![], &pending_body);
    let poll = client(&server.base_url)
        .poll_device_flow("client-id", "device-code")
        .expect("pending poll must succeed");
    assert!(matches!(poll, DeviceFlowPoll::Pending { .. }));
    let captured = server.finish();
    assert_eq!(captured.url, "/login/oauth/access_token");

    let access_body = serde_json::json!({
        "access_token": TOKEN,
        "token_type": "bearer",
        "scope": "repo,read:org"
    })
    .to_string();
    let server = MockServer::start(200, vec![], &access_body);
    let poll = client(&server.base_url)
        .poll_device_flow("client-id", "device-code")
        .expect("authorized poll must succeed");
    let DeviceFlowPoll::Authorized {
        token,
        token_type,
        scopes,
    } = poll
    else {
        panic!("device flow must be authorized");
    };
    assert_eq!(token.expose_secret(), TOKEN);
    assert_eq!(token_type, "bearer");
    assert_eq!(scopes, vec!["repo".to_owned(), "read:org".to_owned()]);
    assert!(!format!("{token:?}").contains(TOKEN));
    server.finish();
}

#[test]
fn reads_authenticated_user_and_scopes() {
    let body = serde_json::json!({
        "login": "octocat",
        "name": "The Octocat",
        "avatar_url": "https://avatars.example/octocat.png"
    })
    .to_string();
    let server = MockServer::start(200, vec![("x-oauth-scopes", "repo, read:org")], &body);
    let token = GitHubToken::new(TOKEN).expect("token must be valid");
    let connection = client(&server.base_url)
        .authenticated_user(&token)
        .expect("user must load");

    assert_eq!(connection.login, "octocat");
    assert_eq!(connection.name.as_deref(), Some("The Octocat"));
    assert_eq!(
        connection.scopes,
        vec!["repo".to_owned(), "read:org".to_owned()]
    );
    assert!(!format!("{connection:?}").contains(TOKEN));
    let captured = server.finish();
    assert_eq!(captured.method, "GET");
    assert_eq!(captured.url, "/user");
    assert_eq!(
        captured.authorization.as_deref(),
        Some("Bearer gho_sensitive_test_token")
    );
}

#[test]
fn creates_a_private_repository_with_the_explicit_request() {
    let body = serde_json::json!({
        "id": 42,
        "name": "coldstart-demo",
        "full_name": "octocat/coldstart-demo",
        "private": true,
        "clone_url": "https://github.com/octocat/coldstart-demo.git",
        "ssh_url": "git@github.com:octocat/coldstart-demo.git",
        "html_url": "https://github.com/octocat/coldstart-demo",
        "default_branch": "main"
    })
    .to_string();
    let server = MockServer::start(201, vec![], &body);
    let token = GitHubToken::new(TOKEN).expect("token must be valid");
    let repository = client(&server.base_url)
        .create_private_repository(
            &token,
            &CreatePrivateRepositoryRequest {
                name: "coldstart-demo".to_owned(),
                description: Some("ColdStart 私有仓库".to_owned()),
            },
        )
        .expect("private repository must create");

    assert_eq!(repository.full_name, "octocat/coldstart-demo");
    assert!(repository.private);
    assert_eq!(repository.default_branch, "main");
    let captured = server.finish();
    assert_eq!(captured.method, "POST");
    assert_eq!(captured.url, "/user/repos");
    let request: serde_json::Value =
        serde_json::from_str(&captured.body).expect("request body must be JSON");
    assert_eq!(request["name"], "coldstart-demo");
    assert_eq!(request["private"], true);
    assert_eq!(request["auto_init"], false);
}

#[test]
fn redacts_tokens_from_http_errors_and_rejects_invalid_tokens() {
    assert!(matches!(
        GitHubToken::new("  "),
        Err(GitHubError::InvalidToken)
    ));
    let body = format!("rejected token {TOKEN}");
    let server = MockServer::start(401, vec![], &body);
    let token = GitHubToken::new(TOKEN).expect("token must be valid");
    let error = client(&server.base_url)
        .authenticated_user(&token)
        .expect_err("401 must fail");
    let message = error.to_string();
    assert!(!message.contains(TOKEN), "error leaked token: {message}");
    assert!(format!("{error:?}").contains("***"));
    server.finish();
}

#[test]
fn parses_supported_github_remotes_only() {
    for remote in [
        "https://github.com/octocat/demo.git",
        "https://github.com/octocat/demo",
        "git@github.com:octocat/demo.git",
        "ssh://git@github.com/octocat/demo.git",
    ] {
        let coordinates = GitHubRepositoryCoordinates::from_remote_url(remote)
            .expect("supported remote must parse");
        assert_eq!(coordinates.owner, "octocat");
        assert_eq!(coordinates.repository, "demo");
    }
    for remote in [
        "https://gitlab.com/octocat/demo.git",
        "https://github.com/octocat/demo/extra",
        "file:///D:/repos/demo.git",
        "https://user@github.com/octocat/demo.git",
    ] {
        assert!(
            GitHubRepositoryCoordinates::from_remote_url(remote).is_err(),
            "remote must be rejected: {remote}"
        );
    }
}

#[test]
fn creates_a_draft_pull_request_with_an_explicit_branch_boundary() {
    let body = serde_json::json!({
        "number": 7,
        "html_url": "https://github.com/octocat/demo/pull/7",
        "state": "open",
        "draft": true,
        "merged": false,
        "title": "[task] 完成订单",
        "head": { "ref": "codex/task-one" },
        "base": { "ref": "main" }
    })
    .to_string();
    let server = MockServer::start(201, vec![], &body);
    let token = GitHubToken::new(TOKEN).expect("token must be valid");
    let pull_request = client(&server.base_url)
        .create_draft_pull_request(
            &token,
            &CreateDraftPullRequestRequest {
                owner: "octocat".to_owned(),
                repository: "demo".to_owned(),
                title: "[task] 完成订单".to_owned(),
                body: "## ColdStart 交付\n".to_owned(),
                head: "codex/task-one".to_owned(),
                base: "main".to_owned(),
            },
        )
        .expect("draft pull request must create");

    assert_eq!(pull_request.number, 7);
    assert!(pull_request.draft);
    assert_eq!(pull_request.head.reference, "codex/task-one");
    assert_eq!(pull_request.base.reference, "main");
    let captured = server.finish();
    assert_eq!(captured.method, "POST");
    assert_eq!(captured.url, "/repos/octocat/demo/pulls");
    assert_eq!(
        captured.authorization.as_deref(),
        Some("Bearer gho_sensitive_test_token")
    );
    let request: serde_json::Value =
        serde_json::from_str(&captured.body).expect("request body must be JSON");
    assert_eq!(request["draft"], true);
    assert_eq!(request["head"], "codex/task-one");
    assert_eq!(request["base"], "main");
}

#[test]
fn refreshes_pull_request_state_and_rejects_non_https_urls() {
    let body = serde_json::json!({
        "number": 7,
        "html_url": "https://github.com/octocat/demo/pull/7",
        "state": "closed",
        "draft": false,
        "merged": true,
        "title": "[task] 完成订单",
        "head": { "ref": "codex/task-one" },
        "base": { "ref": "main" }
    })
    .to_string();
    let server = MockServer::start(200, vec![], &body);
    let token = GitHubToken::new(TOKEN).expect("token must be valid");
    let pull_request = client(&server.base_url)
        .pull_request(&token, "octocat", "demo", 7)
        .expect("pull request must load");
    assert!(pull_request.merged);
    assert!(!pull_request.draft);
    let captured = server.finish();
    assert_eq!(captured.method, "GET");
    assert_eq!(captured.url, "/repos/octocat/demo/pulls/7");

    let insecure = serde_json::json!({
        "number": 7,
        "html_url": "http://example.invalid/pull/7",
        "state": "open",
        "draft": true,
        "merged": false,
        "title": "demo",
        "head": { "ref": "codex/task-one" },
        "base": { "ref": "main" }
    })
    .to_string();
    let server = MockServer::start(200, vec![], &insecure);
    let error = client(&server.base_url)
        .pull_request(&token, "octocat", "demo", 7)
        .expect_err("non-HTTPS URL must be rejected");
    assert!(matches!(error, GitHubError::InvalidResponse(_)));
    server.finish();
}
