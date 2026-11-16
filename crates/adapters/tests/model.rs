use std::{
    sync::mpsc,
    thread::{self, JoinHandle},
};

use coldstart_adapters::{
    BlueprintModel, ModelAnchor, ModelConfig, ModelError, ModelExtractionRequest, ModelSource,
    OpenAiCompatibleModel,
};
use coldstart_core::{ArtifactFormat, BlueprintProposal};
use tiny_http::{Header, Response, Server, StatusCode};

const API_KEY: &str = "test-secret-key";

struct CapturedRequest {
    authorization: Option<String>,
    body: String,
}

struct MockServer {
    base_url: String,
    receiver: mpsc::Receiver<CapturedRequest>,
    handle: JoinHandle<()>,
}

impl MockServer {
    fn start(status: u16, response_body: &str) -> Self {
        let server = Server::http("127.0.0.1:0").expect("mock server must start");
        let port = server
            .server_addr()
            .to_ip()
            .expect("mock server must use TCP")
            .port();
        let (sender, receiver) = mpsc::channel();
        let response_body = response_body.to_owned();
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
            let mut body = String::new();
            request
                .as_reader()
                .read_to_string(&mut body)
                .expect("request body must be readable");
            sender
                .send(CapturedRequest {
                    authorization,
                    body,
                })
                .expect("captured request must be sent");
            let content_type = Header::from_bytes("Content-Type", "application/json")
                .expect("header must be valid");
            request
                .respond(
                    Response::from_string(response_body)
                        .with_status_code(StatusCode(status))
                        .with_header(content_type),
                )
                .expect("response must be written");
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

fn model_request(base_url: &str) -> ModelExtractionRequest {
    ModelExtractionRequest {
        config: ModelConfig {
            base_url: base_url.to_owned(),
            model: "test-model".to_owned(),
            api_key: API_KEY.to_owned(),
        },
        project_name: "订单系统".to_owned(),
        sources: vec![ModelSource {
            artifact_id: "artifact-1".to_owned(),
            file_name: "requirements.md".to_owned(),
            format: ArtifactFormat::Markdown,
            content_hash: "abc123".to_owned(),
            anchors: vec![ModelAnchor {
                anchor_id: "section_1".to_owned(),
                label: "业务范围".to_owned(),
                text: "需要 Web、管理后台和小程序。".to_owned(),
            }],
        }],
        project_facts: Vec::new(),
        code_evidence: Vec::new(),
    }
}

fn completion(content: &str) -> String {
    serde_json::json!({
        "choices": [{ "message": { "content": content } }]
    })
    .to_string()
}

#[test]
fn sends_json_mode_request_and_parses_fenced_json() {
    let server = MockServer::start(
        200,
        &completion("```json\n{\"productGoal\":\"订单系统\"}\n```"),
    );
    let model = OpenAiCompatibleModel::new();
    let proposal = model
        .propose_blueprint(&model_request(&server.base_url))
        .expect("fenced JSON must parse");

    assert_eq!(proposal.product_goal, "订单系统");
    let captured = server.finish();
    assert_eq!(
        captured.authorization.as_deref(),
        Some("Bearer test-secret-key")
    );
    let body: serde_json::Value = serde_json::from_str(&captured.body).expect("body must be JSON");
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["temperature"], 0.1);
    assert_eq!(body["response_format"]["type"], "json_object");
    let prompt = body["messages"][1]["content"]
        .as_str()
        .expect("prompt must be a string");
    assert!(prompt.contains("<anchor id=\"section_1\""));
    assert!(prompt.contains("\"BlueprintProposal\""));
}

#[test]
fn sends_without_authorization_when_api_key_is_empty() {
    let server = MockServer::start(200, &completion("{\"productGoal\":\"订单系统\"}"));
    let mut request = model_request(&server.base_url);
    request.config.api_key.clear();
    OpenAiCompatibleModel::new()
        .propose_blueprint(&request)
        .expect("response must parse");

    let captured = server.finish();
    assert!(captured.authorization.is_none());
}

#[test]
fn maps_non_success_responses_to_a_stable_error() {
    let server = MockServer::start(401, "unauthorized");
    let error = OpenAiCompatibleModel::new()
        .propose_blueprint(&model_request(&server.base_url))
        .expect_err("401 must fail");

    assert!(matches!(error, ModelError::HttpStatus { status: 401, .. }));
    assert_eq!(error.code(), "model_request_failed");
    server.finish();
}

#[test]
fn rejects_empty_choices_and_invalid_response_json() {
    let server = MockServer::start(200, "{\"choices\":[]}");
    let error = OpenAiCompatibleModel::new()
        .propose_blueprint(&model_request(&server.base_url))
        .expect_err("empty choices must fail");
    assert!(matches!(error, ModelError::EmptyChoices));
    assert_eq!(error.code(), "model_response_invalid");
    server.finish();

    let server = MockServer::start(200, &completion("这不是 JSON"));
    let error = OpenAiCompatibleModel::new()
        .propose_blueprint(&model_request(&server.base_url))
        .expect_err("invalid model JSON must fail");
    assert!(matches!(error, ModelError::InvalidResponse(_)));
    assert_eq!(error.code(), "model_response_invalid");
    server.finish();
}

#[test]
fn rejects_invalid_configuration_and_missing_proposal_fields() {
    let mut request = model_request("ftp://invalid");
    let error = OpenAiCompatibleModel::new()
        .propose_blueprint(&request)
        .expect_err("non-HTTP URL must fail");
    assert_eq!(error.code(), "model_config_invalid");

    request.config.base_url = "http://127.0.0.1:1".to_owned();
    request.config.model.clear();
    let error = OpenAiCompatibleModel::new()
        .propose_blueprint(&request)
        .expect_err("empty model must fail");
    assert_eq!(error.code(), "model_config_invalid");

    let server = MockServer::start(200, &completion("{\"unexpected\":true}"));
    let error = OpenAiCompatibleModel::new()
        .propose_blueprint(&model_request(&server.base_url))
        .expect_err("missing productGoal must fail");
    assert!(matches!(error, ModelError::InvalidResponse(_)));
    server.finish();
}

#[test]
fn never_exposes_the_api_key_in_debug_or_error_text() {
    let config = ModelConfig {
        base_url: "https://example.invalid/v1".to_owned(),
        model: "test-model".to_owned(),
        api_key: API_KEY.to_owned(),
    };
    let debug = format!("{config:?}");
    assert!(!debug.contains(API_KEY));
    assert!(debug.contains("***"));

    let server = MockServer::start(500, &format!("the key {API_KEY} was rejected"));
    let error = OpenAiCompatibleModel::new()
        .propose_blueprint(&model_request(&server.base_url))
        .expect_err("500 must fail");
    let message = error.to_string();
    assert!(
        !message.contains(API_KEY),
        "error leaked API key: {message}"
    );
    server.finish();
}

#[test]
fn appends_chat_completions_only_once() {
    let config = ModelConfig {
        base_url: "https://example.invalid/v1/chat/completions/".to_owned(),
        model: "test-model".to_owned(),
        api_key: String::new(),
    };
    assert_eq!(
        config.endpoint(),
        "https://example.invalid/v1/chat/completions"
    );
}

#[test]
fn proposal_type_is_constructible_from_minimal_json() {
    let proposal: BlueprintProposal = serde_json::from_str("{\"productGoal\":\"订单系统\"}")
        .expect("minimal proposal must deserialize");
    assert_eq!(proposal.product_goal, "订单系统");
}
