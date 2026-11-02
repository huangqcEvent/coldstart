use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use coldstart_adapters::{
    GitAdapter, ModelConfig, OpenAiCompatibleReviewModel, ReviewDiff, ReviewModel,
    ReviewModelRequest, ReviewStandardPrompt,
};
use coldstart_core::{ReviewCategory, ReviewSeverity};
use tempfile::TempDir;

struct GitFixture {
    _directory: TempDir,
    work: PathBuf,
}

impl GitFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let work = directory.path().join("work");
        fs::create_dir_all(&work).expect("work directory must create");
        git(&work, &["init", "-b", "main"]);
        git(&work, &["config", "user.name", "ColdStart Test"]);
        git(
            &work,
            &["config", "user.email", "coldstart@example.invalid"],
        );
        Self {
            _directory: directory,
            work,
        }
    }

    fn write(&self, path: &str, content: &str) {
        let path = self.work.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent must create");
        }
        fs::write(path, content).expect("file must write");
    }

    fn write_bytes(&self, path: &str, content: &[u8]) {
        let path = self.work.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent must create");
        }
        fs::write(path, content).expect("file must write");
    }

    fn commit_all(&self, message: &str) -> String {
        git(&self.work, &["add", "."]);
        git(&self.work, &["commit", "-m", message]);
        git_output(&self.work, &["rev-parse", "HEAD"])
    }
}

fn git(path: &Path, args: &[&str]) -> std::process::Output {
    let output = Command::new("git")
        .current_dir(path)
        .args(args)
        .output()
        .expect("git must run");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn git_output(path: &Path, args: &[&str]) -> String {
    String::from_utf8(git(path, args).stdout)
        .expect("git output must be UTF-8")
        .trim()
        .to_owned()
}

#[test]
fn review_diff_uses_fixed_commits_and_excludes_binary_content() {
    let fixture = GitFixture::new();
    fixture.write("src/lib.rs", "pub fn value() -> u32 { 1 }\n");
    let base = fixture.commit_all("base");
    fixture.write("src/lib.rs", "pub fn value() -> u32 { 2 }\n");
    fixture.write_bytes("assets/icon.bin", &[0, 159, 146, 150]);
    let head = fixture.commit_all("head");

    let diff = GitAdapter::new()
        .extract_review_diff(&fixture.work, &base, &head)
        .expect("diff");
    assert!(diff.patch.contains("pub fn value() -> u32 { 2 }"));
    assert!(diff.binary_files.contains(&"assets/icon.bin".to_owned()));
    assert!(!diff.patch.contains("159"));
    assert_eq!(
        diff.files
            .iter()
            .find(|file| file.path == "src/lib.rs")
            .expect("file")
            .status,
        "M"
    );
}

#[test]
fn review_model_sends_redacted_structured_request_and_parses_findings() {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("server");
    let address = format!("http://{}", server.server_addr());
    let handle = std::thread::spawn(move || {
        let mut request = server.recv().expect("request");
        let authorization = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Authorization"))
            .map(|header| header.value.to_string());
        assert_eq!(authorization.as_deref(), Some("Bearer super-secret"));
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body).expect("body");
        assert!(!body.contains("super-secret"));
        assert!(body.contains("安全基线"));
        assert!(body.contains("src/auth.rs"));
        let response_body = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "{\"summary\":\"发现 1 个问题\",\"findings\":[{\"severity\":\"high\",\"category\":\"security\",\"file\":\"src/auth.rs\",\"line\":42,\"message\":\"令牌被写入日志\",\"recommendation\":\"移除日志中的令牌\",\"standard\":\"安全基线\"}]}"
                }
            }]
        });
        request
            .respond(tiny_http::Response::from_string(response_body.to_string()))
            .expect("respond");
    });

    let diff = ReviewDiff {
        base_commit: "0123456789012345678901234567890123456789".to_owned(),
        head_commit: "9876543210987654321098765432109876543210".to_owned(),
        files: vec![coldstart_adapters::ReviewDiffFile {
            path: "src/auth.rs".to_owned(),
            status: "M".to_owned(),
            added: 1,
            deleted: 0,
            binary: false,
        }],
        binary_files: Vec::new(),
        patch: "+ tracing::info!(token = %token, \"login\");\n".to_owned(),
        character_count: 44,
    };
    let request = ReviewModelRequest {
        config: ModelConfig {
            base_url: address,
            model: "reviewer".to_owned(),
            api_key: "super-secret".to_owned(),
        },
        task_title: "修复登录".to_owned(),
        task_goal: "不能记录令牌".to_owned(),
        acceptance_criteria: vec!["日志不得包含令牌".to_owned()],
        standards: vec![ReviewStandardPrompt {
            name: "安全基线".to_owned(),
            scope: "project".to_owned(),
            content: "禁止记录令牌。".to_owned(),
        }],
        verification_summary: Some("1 command passed".to_owned()),
        diff,
    };
    let output = OpenAiCompatibleReviewModel::new()
        .review(&request)
        .expect("review");
    assert_eq!(output.findings.len(), 1);
    assert_eq!(output.findings[0].severity, ReviewSeverity::High);
    assert_eq!(output.findings[0].category, ReviewCategory::Security);
    handle.join().expect("server thread");
}
