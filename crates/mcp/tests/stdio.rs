use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::Arc,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use coldstart_adapters::{GitAdapter, MemorySecretStore};
use coldstart_app::{
    IntegrationService, PlanService, ProjectService, RepositoryDraft, SaveCredentialInput,
};
use coldstart_core::{
    ArtifactFormat, ArtifactId, ArtifactImage, ArtifactMetadata, BlueprintContent,
    BlueprintEntityId, BlueprintGeneration, CapabilityApiEndpointId, CapabilityObligation,
    CapabilityStatus, CredentialEnvironment, CredentialKind, DocumentAnchor, GenerationSource,
    ModuleDefinition, PlatformDefinition, PlatformKind, ProjectId, RepositoryAuthMode,
    RepositorySyncPolicy, RepositoryVerificationCommand, SourceReference, TraceableText,
    WorkItemStatus,
};
use coldstart_storage::Database;
use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

struct Fixture {
    _directory: TempDir,
    home: PathBuf,
    project_id: ProjectId,
    work: PathBuf,
    artifact_id: ArtifactId,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let home = directory.path().join("project");
        let remote = directory.path().join("remote.git");
        let work = directory.path().join("web-work");
        initialize_git_repository(&remote, &work);
        let status = ProjectService::new()
            .create_project("MCP 集成项目", home.clone())
            .expect("project must create");
        let mut database =
            Database::open(&home.join(".coldstart").join("state.db")).expect("database must open");
        let artifact_id = seed_image_artifact(&mut database, &home, status.project_id);
        let blueprint = database
            .create_blueprint_revision(
                &status.project_id,
                blueprint_content(status.project_id, artifact_id),
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
            .publish_blueprint_revision(&status.project_id, &blueprint.id)
            .expect("blueprint must publish");

        let integrations = IntegrationService::with_dependencies(
            Arc::new(MemorySecretStore::default()),
            GitAdapter::new(),
            None,
        );
        integrations
            .save_repository(
                home.clone(),
                RepositoryDraft {
                    key: "web".to_owned(),
                    name: "Web 前台".to_owned(),
                    local_path: work.clone(),
                    remote_url: remote.to_string_lossy().into_owned(),
                    default_branch: "main".to_owned(),
                    protected_branches: vec!["main".to_owned()],
                    auth_mode: RepositoryAuthMode::None,
                    credential_id: None,
                    sync_policy: RepositorySyncPolicy::FetchBeforeClaim,
                },
            )
            .expect("repository must save");
        integrations
            .save_verification_commands(
                home.clone(),
                "web",
                vec![verification_command(
                    "where-cmd",
                    vec!["cmd.exe".to_owned()],
                )],
            )
            .expect("verification command must save");

        let service = PlanService::new();
        let draft = service
            .generate_draft(home.clone())
            .expect("plan draft must generate")
            .draft
            .expect("plan draft must exist");
        service
            .publish(home.clone(), draft.id)
            .expect("plan must publish");

        Self {
            _directory: directory,
            home,
            project_id: status.project_id,
            work,
            artifact_id,
        }
    }

    fn configure_verification(&self, commands: Vec<RepositoryVerificationCommand>) {
        IntegrationService::with_dependencies(
            Arc::new(MemorySecretStore::default()),
            GitAdapter::new(),
            None,
        )
        .save_verification_commands(self.home.clone(), "web", commands)
        .expect("verification commands must save");
    }

    fn seed_configured_capability(&self) -> (String, String) {
        let secrets = Arc::new(MemorySecretStore::default());
        let service = IntegrationService::with_dependencies(secrets, GitAdapter::new(), None);
        let workspace = service
            .save_credential(
                self.home.clone(),
                SaveCredentialInput {
                    name: "阿里云 OSS".to_owned(),
                    provider: "aliyun".to_owned(),
                    kind: CredentialKind::ApiKey,
                    environment: CredentialEnvironment::Development,
                    secret: "MCP_CAPABILITY_SECRET_MARKER".to_owned(),
                },
            )
            .expect("credential must save");
        let credential = workspace.credentials[0].clone();
        let capability_id = BlueprintEntityId::derive(&self.project_id, "capability", "阿里云 OSS");
        service
            .update_capability(
                self.home.clone(),
                capability_id.clone(),
                "development".to_owned(),
                CapabilityStatus::Configured,
                Some(credential.id),
                Some("MCP 测试能力".to_owned()),
            )
            .expect("capability must update");
        (capability_id.to_string(), credential.secret_ref)
    }
}

fn verification_command(name: &str, args: Vec<String>) -> RepositoryVerificationCommand {
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    RepositoryVerificationCommand {
        name: name.to_owned(),
        executable: system_root.join("System32").join("where.exe"),
        args,
        timeout_seconds: 10,
        position: 0,
    }
}

fn git(path: &Path, args: &[&str]) -> std::process::Output {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .expect("git must start");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn initialize_git_repository(remote: &Path, work: &Path) {
    git(
        work.parent().expect("work parent must exist"),
        &[
            "init",
            "--bare",
            remote.to_str().expect("remote path must be UTF-8"),
        ],
    );
    fs::create_dir_all(work).expect("work directory must create");
    git(work, &["init", "-b", "main"]);
    git(work, &["config", "user.name", "ColdStart MCP Test"]);
    git(
        work,
        &["config", "user.email", "coldstart-mcp@example.invalid"],
    );
    fs::write(work.join("README.md"), "initial\n").expect("initial file must write");
    git(work, &["add", "README.md"]);
    git(work, &["commit", "-m", "initial"]);
    git(
        work,
        &[
            "remote",
            "add",
            "origin",
            remote.to_str().expect("remote path must be UTF-8"),
        ],
    );
    git(work, &["push", "--set-upstream", "origin", "main"]);
}

fn commit_file(work: &Path, relative_path: &str, content: &str) -> String {
    let path = work.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("file parent must create");
    }
    fs::write(&path, content).expect("changed file must write");
    git(work, &["add", relative_path]);
    git(work, &["commit", "-m", "complete MCP task"]);
    String::from_utf8(git(work, &["rev-parse", "HEAD"]).stdout)
        .expect("commit SHA must be UTF-8")
        .trim()
        .to_owned()
}

const TASK_IMAGE_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nmcp-task-image";

fn seed_image_artifact(database: &mut Database, home: &Path, project_id: ProjectId) -> ArtifactId {
    let artifact_id = ArtifactId::new();
    let artifact = ArtifactMetadata {
        id: artifact_id,
        project_id,
        file_name: "requirements.docx".to_owned(),
        format: ArtifactFormat::Docx,
        content_hash: "mcp-artifact-content-hash".to_owned(),
        text_hash: "mcp-artifact-text-hash".to_owned(),
        byte_size: 1024,
        import_source: "requirements.docx".to_owned(),
        imported_at: chrono::Utc::now(),
    };
    let anchors = vec![
        DocumentAnchor {
            id: "section_1".to_owned(),
            label: "订单原型".to_owned(),
            text: "订单列表原型".to_owned(),
        },
        DocumentAnchor {
            id: "section_2".to_owned(),
            label: "发布说明".to_owned(),
            text: "小程序提审说明".to_owned(),
        },
    ];
    let images = vec![
        ArtifactImage {
            id: "image_1".to_owned(),
            anchor_id: "section_1".to_owned(),
            file_name: "orders.png".to_owned(),
            mime_type: "image/png".to_owned(),
            byte_size: TASK_IMAGE_BYTES.len() as u64,
            sha256: format!("{:x}", Sha256::digest(TASK_IMAGE_BYTES)),
            relative_path: "images/image_1.png".to_owned(),
        },
        ArtifactImage {
            id: "image_2".to_owned(),
            anchor_id: "section_2".to_owned(),
            file_name: "release.png".to_owned(),
            mime_type: "image/png".to_owned(),
            byte_size: 10,
            sha256: format!("{:x}", Sha256::digest(b"unrelated!")),
            relative_path: "images/image_2.png".to_owned(),
        },
    ];
    database
        .insert_artifact_with_images(&artifact, &anchors, &images)
        .expect("image artifact must insert");
    let images_directory = home
        .join(".coldstart")
        .join("artifacts")
        .join(artifact_id.to_string())
        .join("images");
    fs::create_dir_all(&images_directory).expect("image directory must create");
    fs::write(images_directory.join("image_1.png"), TASK_IMAGE_BYTES)
        .expect("task image must write");
    fs::write(images_directory.join("image_2.png"), b"unrelated!").expect("other image must write");
    artifact_id
}

fn blueprint_content(project_id: ProjectId, artifact_id: ArtifactId) -> BlueprintContent {
    let source = SourceReference {
        artifact_id,
        anchor_id: "section_1".to_owned(),
        quote: Some("需要 Web 订单系统".to_owned()),
    };
    let platform_id = BlueprintEntityId::derive(&project_id, "platform", "Web");
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
            id: platform_id.clone(),
            name: "Web".to_owned(),
            kind: PlatformKind::Web,
            description: "订单前台".to_owned(),
            sources: vec![source.clone()],
        }],
        modules: vec![ModuleDefinition {
            id: BlueprintEntityId::derive(&project_id, "module", "Orders"),
            name: "订单".to_owned(),
            description: "创建和查询订单".to_owned(),
            platforms: vec![platform_id],
            depends_on: vec![],
            sources: vec![source],
        }],
        technologies: vec![],
        capabilities: vec![CapabilityObligation {
            id: BlueprintEntityId::derive(&project_id, "capability", "阿里云 OSS"),
            name: "阿里云 OSS".to_owned(),
            provider: "aliyun_oss".to_owned(),
            description: "上传订单附件".to_owned(),
            sources: vec![SourceReference {
                artifact_id,
                anchor_id: "section_1".to_owned(),
                quote: Some("需要文件上传".to_owned()),
            }],
        }],
        release_prerequisites: vec![],
        assumptions: vec![],
        risks: vec![],
    }
}

struct McpStdioClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpStdioClient {
    fn spawn(project_home: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_coldstart-mcp"))
            .args(["--project-home", project_home.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("MCP server must start");
        let stdin = child.stdin.take().expect("stdin must be piped");
        let stdout = child.stdout.take().expect("stdout must be piped");
        Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 0,
        }
    }

    fn initialize(&mut self) -> Value {
        let response = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "coldstart-stdio-test", "version": "1.0.0" }
            }),
        );
        assert!(
            response.get("result").is_some(),
            "initialize failed: {response}"
        );
        self.notify("notifications/initialized", json!({}));
        response
    }

    fn list_tools(&mut self) -> Vec<Value> {
        let response = self.request("tools/list", json!({}));
        response["result"]["tools"]
            .as_array()
            .expect("tools must be an array")
            .clone()
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        assert!(response.get("error").is_none(), "tool failed: {response}");
        response["result"].clone()
    }

    fn call_tool_error(&mut self, name: &str, arguments: Value) -> String {
        let response = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        let error = response
            .get("error")
            .unwrap_or_else(|| panic!("missing error: {response}"));
        error["message"]
            .as_str()
            .expect("error message must be a string")
            .to_owned()
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));

        loop {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .expect("stdout must be readable");
            assert!(read > 0, "MCP server closed stdout before response {id}");
            if line.trim().is_empty() {
                continue;
            }
            let message: Value = serde_json::from_str(line.trim()).unwrap_or_else(|error| {
                panic!("stdout contained a non-protocol frame: {error}: {line:?}")
            });
            if message.get("id") == Some(&json!(id)) {
                return message;
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.write_json(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    fn write_json(&mut self, value: &Value) {
        serde_json::to_writer(&mut self.stdin, value).expect("message must serialize");
        self.stdin.write_all(b"\n").expect("newline must write");
        self.stdin.flush().expect("stdin must flush");
    }
}

impl Drop for McpStdioClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn structured(result: &Value) -> &Value {
    result
        .get("structuredContent")
        .unwrap_or_else(|| panic!("missing structured content: {result}"))
}

fn tool_error(result: &Value) -> &Value {
    assert_eq!(
        result.get("isError").and_then(Value::as_bool),
        Some(true),
        "expected a structured tool error: {result}"
    );
    structured(result)
}

fn claim_one(client: &mut McpStdioClient, request_id: &str) -> Value {
    let claim = client.call_tool(
        "claim_next_work",
        json!({
            "agent_profile": "codex",
            "repository": "web",
            "request_id": request_id
        }),
    );
    let claimed = structured(&claim)["claimed"].clone();
    assert!(!claimed.is_null(), "expected claimed work: {claim}");
    claimed
}

fn task_for(fixture: &Fixture, work_id: &str) -> coldstart_core::TaskSpec {
    PlanService::new()
        .load_workspace(fixture.home.clone())
        .expect("workspace must load")
        .work_items
        .into_iter()
        .find(|item| item.work_item.id.to_string() == work_id)
        .expect("claimed task must exist")
        .task
}

fn completed_report(
    task: &coldstart_core::TaskSpec,
    context: &Value,
    work_id: &str,
    lease_token: &str,
    commit_sha: &str,
    idempotency_key: &str,
) -> Value {
    let allowed_root = task
        .allowed_paths
        .first()
        .cloned()
        .unwrap_or_else(|| ".".to_owned());
    let changed_file = if allowed_root == "." {
        "src/main.rs".to_owned()
    } else {
        format!("{allowed_root}/src/main.rs")
    };
    json!({
        "work_id": work_id,
        "lease_token": lease_token,
        "report": {
            "status": "completed",
            "summary": "通过真实 MCP stdio 完成实现",
            "branch": task.work_branch,
            "commitSha": commit_sha,
            "changedFiles": [changed_file],
            "acceptanceResults": task.acceptance_criteria.iter().map(|criterion| json!({
                "criterionId": criterion.id,
                "passed": true,
                "note": "已完成并通过测试",
                "evidenceRefs": ["mcp-stdio-test"]
            })).collect::<Vec<_>>(),
            "commandsRun": [],
            "evidenceRefs": ["mcp-stdio-test"],
            "knownIssues": [],
            "followUpItems": [],
            "contractHash": context["contractHash"],
            "idempotencyKey": idempotency_key
        }
    })
}

fn changed_file_for(task: &coldstart_core::TaskSpec) -> String {
    let allowed_root = task
        .allowed_paths
        .first()
        .cloned()
        .unwrap_or_else(|| ".".to_owned());
    if allowed_root == "." {
        "src/main.rs".to_owned()
    } else {
        format!("{allowed_root}/src/main.rs")
    }
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn scan_text_columns(database_path: &Path) -> Vec<(String, String, String)> {
    let connection = Connection::open(database_path).expect("SQLite database must open");
    let mut tables_statement = connection
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .expect("table query must prepare");
    let tables = tables_statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("table query must run")
        .collect::<Result<Vec<_>, _>>()
        .expect("table names must load");
    drop(tables_statement);

    let mut values = Vec::new();
    for table in tables {
        let quoted_table = quote_identifier(&table);
        let mut columns_statement = connection
            .prepare(&format!("PRAGMA table_info({quoted_table})"))
            .expect("column query must prepare");
        let columns = columns_statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            })
            .expect("column query must run")
            .collect::<Result<Vec<_>, _>>()
            .expect("column names must load");
        drop(columns_statement);

        for (column, column_type) in columns {
            if !column_type.to_ascii_uppercase().contains("TEXT") {
                continue;
            }
            let quoted_column = quote_identifier(&column);
            let query = format!(
                "SELECT CAST({quoted_column} AS TEXT) FROM {quoted_table} WHERE {quoted_column} IS NOT NULL"
            );
            let mut value_statement = connection
                .prepare(&query)
                .expect("column value query must prepare");
            let column_values = value_statement
                .query_map([], |row| row.get::<_, String>(0))
                .expect("column value query must run")
                .collect::<Result<Vec<_>, _>>()
                .expect("column values must load");
            for value in column_values {
                values.push((table.clone(), column.clone(), value));
            }
        }
    }
    values
}

#[test]
fn stdio_server_lists_all_work_tools_and_rejects_an_unknown_tool() {
    let fixture = Fixture::new();
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();

    let mut names = client
        .list_tools()
        .into_iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec![
            "call_capability_api",
            "claim_next_work",
            "get_capability_status",
            "get_project_brief",
            "get_project_memory",
            "get_work_context",
            "get_work_handoff",
            "heartbeat_work",
            "list_task_images",
            "read_task_image",
            "read_task_reference",
            "release_work",
            "report_work_blocker",
            "submit_work_report",
        ]
    );

    let error = client.call_tool_error("delete_repository", json!({}));
    assert!(
        error.contains("tool not found"),
        "unexpected error: {error}"
    );
}

#[test]
fn stdio_server_exposes_a_safe_project_brief_and_memory_page() {
    let fixture = Fixture::new();
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();

    let brief = client.call_tool("get_project_brief", json!({}));
    let brief = structured(&brief);
    assert_eq!(brief["project"]["name"].as_str(), Some("MCP 集成项目"));
    assert_eq!(brief["plan"]["revisionNumber"].as_u64(), Some(1));
    assert_eq!(brief["repositories"][0]["key"].as_str(), Some("web"));
    assert!(!brief["currentWork"].as_array().unwrap().is_empty());
    assert!(!brief["recentMemory"].as_array().unwrap().is_empty());
    let serialized = serde_json::to_string(brief).expect("brief must serialize");
    assert!(!serialized.contains(&fixture.home.to_string_lossy().into_owned()));
    assert!(!serialized.contains(&fixture.work.to_string_lossy().into_owned()));
    assert!(!serialized.contains("remote.git"));
    assert!(!serialized.contains("credentialRef"));

    let memory = client.call_tool(
        "get_project_memory",
        json!({ "limit": 1, "include_heartbeats": false }),
    );
    let memory = structured(&memory);
    assert_eq!(memory["items"].as_array().unwrap().len(), 1);
    assert_eq!(memory["items"][0]["isHeartbeat"].as_bool(), Some(false));
    assert!(memory["totalCount"].as_u64().unwrap() >= 1);
}

#[test]
fn stdio_server_returns_authorized_task_images_without_local_paths() {
    let fixture = Fixture::new();
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();
    let claimed = claim_one(&mut client, "stdio-task-image-claim");
    let work_id = claimed["workItem"]["id"].as_str().unwrap().to_owned();
    let lease_token = claimed["leaseToken"].as_str().unwrap().to_owned();

    let listed = client.call_tool(
        "list_task_images",
        json!({ "work_id": work_id, "lease_token": lease_token }),
    );
    let listed = structured(&listed);
    let images = listed["images"]
        .as_array()
        .expect("task image list must be an array");
    assert_eq!(images.len(), 1);
    assert_eq!(images[0]["artifactId"], fixture.artifact_id.to_string());
    assert_eq!(images[0]["imageId"], "image_1");
    assert_eq!(images[0]["anchorId"], "section_1");
    assert_eq!(images[0]["anchorLabel"], "订单原型");
    assert!(
        !serde_json::to_string(&listed)
            .expect("image list must serialize")
            .contains(&fixture.home.to_string_lossy().into_owned())
    );

    let read = client.call_tool(
        "read_task_image",
        json!({
            "work_id": work_id,
            "lease_token": lease_token,
            "artifact_id": fixture.artifact_id,
            "image_id": "image_1"
        }),
    );
    let image = structured(&read);
    assert_eq!(image["image"]["imageId"], "image_1");
    let image_block = read["content"]
        .as_array()
        .expect("tool content must be an array")
        .iter()
        .find(|block| block["type"] == "image")
        .expect("tool result must include an image block");
    assert_eq!(image_block["mimeType"], "image/png");
    assert_eq!(
        image_block["data"].as_str(),
        Some(BASE64_STANDARD.encode(TASK_IMAGE_BYTES).as_str())
    );
    let serialized = serde_json::to_string(&read).expect("image result must serialize");
    assert!(!serialized.contains(&fixture.home.to_string_lossy().into_owned()));
    assert!(!serialized.contains("images/image_1.png"));
    assert!(!serialized.contains(&lease_token));

    let unauthorized = client.call_tool(
        "read_task_image",
        json!({
            "work_id": work_id,
            "lease_token": lease_token,
            "artifact_id": fixture.artifact_id,
            "image_id": "image_2"
        }),
    );
    assert_eq!(
        tool_error(&unauthorized)["code"].as_str(),
        Some("requirement_image_not_found")
    );

    let invalid_lease = client.call_tool(
        "list_task_images",
        json!({ "work_id": work_id, "lease_token": "invalid-lease" }),
    );
    assert_eq!(
        tool_error(&invalid_lease)["code"].as_str(),
        Some("lease_invalid")
    );
}

#[test]
fn stdio_server_returns_handoff_only_with_a_valid_lease() {
    let fixture = Fixture::new();
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();
    let claimed = claim_one(&mut client, "stdio-handoff-claim");
    let work_id = claimed["workItem"]["id"].as_str().unwrap().to_owned();
    let lease_token = claimed["leaseToken"].as_str().unwrap().to_owned();

    client.call_tool(
        "heartbeat_work",
        json!({ "work_id": work_id, "lease_token": lease_token }),
    );
    let handoff = client.call_tool(
        "get_work_handoff",
        json!({ "work_id": work_id, "lease_token": lease_token }),
    );
    let handoff = structured(&handoff);
    assert_eq!(
        handoff["currentWork"]["taskKey"],
        claimed["workItem"]["taskKey"]
    );
    assert!(handoff["relatedWork"].as_array().is_some());
    let serialized = serde_json::to_string(handoff).expect("handoff must serialize");
    assert!(!serialized.contains(&lease_token));
    assert!(!serialized.contains(&fixture.work.to_string_lossy().into_owned()));

    let heartbeats = client.call_tool(
        "get_project_memory",
        json!({ "limit": 100, "include_heartbeats": true }),
    );
    let heartbeats = structured(&heartbeats);
    assert!(
        heartbeats["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["isHeartbeat"].as_bool() == Some(true))
    );

    let invalid = client.call_tool(
        "get_work_handoff",
        json!({ "work_id": work_id, "lease_token": "invalid-lease" }),
    );
    assert_eq!(tool_error(&invalid)["code"].as_str(), Some("lease_invalid"));
}

#[test]
fn stdio_server_returns_capability_status_without_the_secret() {
    let fixture = Fixture::new();
    let (capability_id, credential_ref) = fixture.seed_configured_capability();
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();

    let status = client.call_tool(
        "get_capability_status",
        json!({
            "capability_id": capability_id,
            "environment": "development"
        }),
    );
    let status = structured(&status);
    assert_eq!(status["name"].as_str(), Some("阿里云 OSS"));
    assert_eq!(status["provider"].as_str(), Some("aliyun_oss"));
    assert_eq!(status["environment"].as_str(), Some("development"));
    assert_eq!(status["status"].as_str(), Some("configured"));
    assert_eq!(
        status["credentialRef"].as_str(),
        Some(credential_ref.as_str())
    );
    assert_eq!(status["note"].as_str(), Some("MCP 测试能力"));
    let serialized = serde_json::to_string(&status).expect("status must serialize");
    assert!(!serialized.contains("MCP_CAPABILITY_SECRET_MARKER"));

    let missing = BlueprintEntityId::derive(&fixture.project_id, "capability", "missing");
    let missing = client.call_tool(
        "get_capability_status",
        json!({
            "capability_id": missing.to_string(),
            "environment": "development"
        }),
    );
    assert_eq!(
        tool_error(&missing)["code"].as_str(),
        Some("capability_not_found")
    );
}

#[test]
fn stdio_server_rejects_capability_api_calls_without_authorization() {
    let fixture = Fixture::new();
    let (capability_id, _) = fixture.seed_configured_capability();
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();
    let claimed = claim_one(&mut client, "stdio-capability-api-claim");
    let work_id = claimed["workItem"]["id"].as_str().unwrap().to_owned();
    let lease_token = claimed["leaseToken"].as_str().unwrap().to_owned();
    let endpoint_id = CapabilityApiEndpointId::new().to_string();

    let invalid_lease = client.call_tool(
        "call_capability_api",
        json!({
            "capability_id": capability_id,
            "environment": "development",
            "endpoint_id": endpoint_id,
            "work_id": work_id,
            "lease_token": "invalid-lease",
            "method": "GET",
            "path": "/v1/files"
        }),
    );
    assert_eq!(
        tool_error(&invalid_lease)["code"].as_str(),
        Some("lease_invalid")
    );

    let unauthorized = client.call_tool(
        "call_capability_api",
        json!({
            "capability_id": capability_id,
            "environment": "development",
            "endpoint_id": endpoint_id,
            "work_id": work_id,
            "lease_token": lease_token,
            "method": "GET",
            "path": "/v1/files"
        }),
    );
    assert_eq!(
        tool_error(&unauthorized)["code"].as_str(),
        Some("capability_api_not_authorized")
    );
}

#[test]
fn stdio_server_claims_work_returns_context_heartbeats_and_accepts_a_report() {
    let fixture = Fixture::new();
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();

    let claim = client.call_tool(
        "claim_next_work",
        json!({
            "agent_profile": "codex",
            "repository": "web",
            "request_id": "stdio-claim-1"
        }),
    );
    let claimed = structured(&claim)["claimed"].clone();
    assert!(!claimed.is_null(), "expected claimed work: {claim}");
    let work_id = claimed["workItem"]["id"].as_str().unwrap().to_owned();
    let lease_token = claimed["leaseToken"].as_str().unwrap().to_owned();

    let context = client.call_tool(
        "get_work_context",
        json!({ "work_id": work_id, "lease_token": lease_token }),
    );
    let context = structured(&context);
    assert!(context["markdown"].as_str().unwrap().contains("验收标准"));
    assert!(
        context["markdown"]
            .as_str()
            .unwrap()
            .contains("仓库验证门禁")
    );
    assert_eq!(context["taskKey"], claimed["workItem"]["taskKey"]);
    assert_eq!(context["verification"]["repositoryKey"], "web");
    assert_eq!(context["verification"]["commands"][0]["name"], "where-cmd");
    assert_eq!(
        context["verification"]["commands"][0]["executable"]
            .as_str()
            .expect("verification executable must be visible"),
        verification_command("where-cmd", vec!["cmd.exe".to_owned()])
            .executable
            .to_string_lossy()
    );

    let heartbeat = client.call_tool(
        "heartbeat_work",
        json!({ "work_id": work_id, "lease_token": lease_token }),
    );
    assert_eq!(structured(&heartbeat)["status"], "active");

    let workspace = PlanService::new()
        .load_workspace(fixture.home.clone())
        .expect("workspace must load");
    let task = workspace
        .work_items
        .iter()
        .find(|item| item.work_item.id.to_string() == work_id)
        .expect("claimed task must exist")
        .task
        .clone();
    let allowed_root = task
        .allowed_paths
        .first()
        .cloned()
        .unwrap_or_else(|| ".".to_owned());
    let changed_file = if allowed_root == "." {
        "src/main.rs".to_owned()
    } else {
        format!("{allowed_root}/src/main.rs")
    };
    let commit = commit_file(&fixture.work, &changed_file, "pub fn main() {}\n");
    let branch = task
        .work_branch
        .clone()
        .expect("agent task must have a work branch");
    let report = json!({
        "work_id": work_id,
        "lease_token": lease_token,
        "report": {
            "status": "completed",
            "summary": "通过真实 MCP stdio 完成实现",
            "branch": branch,
            "commitSha": commit,
            "changedFiles": [changed_file],
            "acceptanceResults": task.acceptance_criteria.iter().map(|criterion| json!({
                "criterionId": criterion.id,
                "passed": true,
                "note": "已完成并通过测试",
                "evidenceRefs": ["mcp-stdio-test"]
            })).collect::<Vec<_>>(),
            "commandsRun": [],
            "evidenceRefs": ["mcp-stdio-test"],
            "knownIssues": [],
            "followUpItems": [],
            "contractHash": context["contractHash"],
            "idempotencyKey": "stdio-report-1"
        }
    });
    let submitted = client.call_tool("submit_work_report", report);
    let submitted = structured(&submitted);
    assert_eq!(submitted["accepted"], true);
    assert_eq!(submitted["verificationStatus"], "passed");
    assert_eq!(submitted["verification"]["failureKind"], Value::Null);

    let workspace = PlanService::new()
        .load_workspace(fixture.home.clone())
        .expect("workspace must reload");
    assert!(workspace.work_items.iter().any(|item| {
        item.work_item.id.to_string() == work_id && item.work_item.status == WorkItemStatus::Done
    }));
}

#[test]
fn stdio_server_returns_verification_failure_and_interrupted_without_leaking_lease() {
    let fixture = Fixture::new();
    fixture.configure_verification(vec![verification_command(
        "missing-command",
        vec!["coldstart-command-that-does-not-exist".to_owned()],
    )]);
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();
    let claimed = claim_one(&mut client, "stdio-verification-failure-claim");
    let work_id = claimed["workItem"]["id"].as_str().unwrap().to_owned();
    let lease_token = claimed["leaseToken"].as_str().unwrap().to_owned();
    let context = structured(&client.call_tool(
        "get_work_context",
        json!({ "work_id": work_id, "lease_token": lease_token }),
    ))
    .clone();
    let task = task_for(&fixture, &work_id);
    let changed_file = changed_file_for(&task);
    let commit = commit_file(&fixture.work, &changed_file, "pub fn main() {}\n");
    let report = completed_report(
        &task,
        &context,
        &work_id,
        &lease_token,
        &commit,
        "stdio-verification-failure-report",
    );

    let failed = client.call_tool("submit_work_report", report.clone());
    let error = tool_error(&failed);
    assert_eq!(error["code"], "verification_failed");
    assert_eq!(error["details"]["failureKind"], "non_zero_exit");
    assert_eq!(error["details"]["command"], "missing-command");
    assert_eq!(error["details"]["exitCode"], 1);
    assert!(error["details"]["durationMs"].as_u64().is_some());
    assert!(
        error["details"]["outputSummary"]
            .as_str()
            .expect("output summary must be a string")
            .len()
            <= 64 * 1024
    );
    let serialized = serde_json::to_string(&failed).expect("tool failure must serialize");
    assert!(!serialized.contains(&lease_token));

    let workspace = PlanService::new()
        .load_workspace(fixture.home.clone())
        .expect("workspace must load");
    assert!(workspace.work_items.iter().any(|item| {
        item.work_item.id.to_string() == work_id
            && item.work_item.status == WorkItemStatus::InProgress
    }));

    let connection = Connection::open(fixture.home.join(".coldstart").join("state.db"))
        .expect("database must open");
    let updated = connection
        .execute(
            "UPDATE work_reports SET verification_status = 'interrupted'
             WHERE work_item_id = ?1 AND verification_status = 'failed'",
            [&work_id],
        )
        .expect("verification report must update");
    assert_eq!(updated, 1);
    drop(connection);

    let interrupted = client.call_tool("submit_work_report", report);
    let error = tool_error(&interrupted);
    assert_eq!(error["code"], "verification_interrupted");
    assert_eq!(error["details"]["failureKind"], "non_zero_exit");
    let serialized = serde_json::to_string(&interrupted).expect("tool failure must serialize");
    assert!(!serialized.contains(&lease_token));
}

#[test]
fn stdio_server_returns_a_timeout_error_with_bounded_details() {
    let fixture = Fixture::new();
    let powershell = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    fixture.configure_verification(vec![RepositoryVerificationCommand {
        name: "timeout-command".to_owned(),
        executable: powershell,
        args: vec![
            "-NoProfile".to_owned(),
            "-NonInteractive".to_owned(),
            "-Command".to_owned(),
            "Start-Sleep -Seconds 12".to_owned(),
        ],
        timeout_seconds: 10,
        position: 0,
    }]);
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();
    let claimed = claim_one(&mut client, "stdio-verification-timeout-claim");
    let work_id = claimed["workItem"]["id"].as_str().unwrap().to_owned();
    let lease_token = claimed["leaseToken"].as_str().unwrap().to_owned();
    let context = structured(&client.call_tool(
        "get_work_context",
        json!({ "work_id": work_id, "lease_token": lease_token }),
    ))
    .clone();
    let task = task_for(&fixture, &work_id);
    let changed_file = changed_file_for(&task);
    let commit = commit_file(&fixture.work, &changed_file, "pub fn main() {}\n");
    let report = completed_report(
        &task,
        &context,
        &work_id,
        &lease_token,
        &commit,
        "stdio-verification-timeout-report",
    );

    let timed_out = client.call_tool("submit_work_report", report);
    let error = tool_error(&timed_out);
    assert_eq!(error["code"], "verification_timeout");
    assert_eq!(error["details"]["failureKind"], "timeout");
    assert_eq!(error["details"]["command"], "timeout-command");
    assert!(
        error["details"]["outputSummary"]
            .as_str()
            .expect("timeout summary must be a string")
            .chars()
            .count()
            <= 64 * 1024
    );
    let serialized = serde_json::to_string(&timed_out).expect("timeout must serialize");
    assert!(!serialized.contains(&lease_token));
}

#[test]
fn stdio_server_reads_only_whitelisted_task_references() {
    let fixture = Fixture::new();
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();
    let claimed = claim_one(&mut client, "stdio-reference-claim");
    let work_id = claimed["workItem"]["id"].as_str().unwrap().to_owned();
    let lease_token = claimed["leaseToken"].as_str().unwrap().to_owned();

    let workspace = PlanService::new()
        .load_workspace(fixture.home)
        .expect("workspace must load");
    let task = workspace
        .work_items
        .iter()
        .find(|item| item.work_item.id.to_string() == work_id)
        .expect("claimed task must exist")
        .task
        .clone();
    let reference_id = task
        .context_refs
        .first()
        .expect("generated web task must expose a whitelisted reference")
        .as_str()
        .to_owned();

    let reference = client.call_tool(
        "read_task_reference",
        json!({
            "work_id": work_id,
            "lease_token": lease_token,
            "reference_id": reference_id
        }),
    );
    let reference = structured(&reference);
    assert_eq!(
        reference["referenceId"].as_str(),
        Some(reference_id.as_str())
    );
    assert!(!reference["title"].as_str().unwrap().is_empty());
    assert!(!reference["content"].as_str().unwrap().is_empty());
    assert_eq!(reference["contentHash"].as_str().unwrap().len(), 64);

    let denied = client.call_tool(
        "read_task_reference",
        json!({
            "work_id": work_id,
            "lease_token": lease_token,
            "reference_id": "module:not-whitelisted"
        }),
    );
    let error = tool_error(&denied);
    assert_eq!(error["code"].as_str(), Some("reference_not_found"));
}

#[test]
fn stdio_server_reports_a_blocker_and_requires_human_input() {
    let fixture = Fixture::new();
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();
    let claimed = claim_one(&mut client, "stdio-blocker-claim");
    let work_id = claimed["workItem"]["id"].as_str().unwrap().to_owned();
    let lease_token = claimed["leaseToken"].as_str().unwrap().to_owned();

    let blocker = client.call_tool(
        "report_work_blocker",
        json!({
            "work_id": work_id,
            "lease_token": lease_token,
            "reason": "缺少小程序 AppID",
            "needed_input": ["提供 AppID 的账号归属与可用环境"],
            "proposal": "先完成 Web 切片，小程序配置由人工补齐"
        }),
    );
    let blocker = structured(&blocker);
    assert_eq!(blocker["reason"].as_str(), Some("缺少小程序 AppID"));
    assert_eq!(blocker["neededInput"].as_array().unwrap().len(), 1);
    assert!(blocker["proposal"].as_str().unwrap().contains("Web"));

    let workspace = PlanService::new()
        .load_workspace(fixture.home)
        .expect("workspace must load");
    let view = workspace
        .work_items
        .iter()
        .find(|item| item.work_item.id.to_string() == work_id)
        .expect("blocked task must exist");
    assert_eq!(view.work_item.status, WorkItemStatus::Blocked);
    assert!(view.active_lease.is_none());
    assert!(workspace.blockers.iter().any(|blocker| {
        blocker.work_item_id.to_string() == work_id && blocker.reason == "缺少小程序 AppID"
    }));

    let old_context = client.call_tool(
        "get_work_context",
        json!({ "work_id": work_id, "lease_token": lease_token }),
    );
    let error = tool_error(&old_context);
    assert_eq!(error["code"].as_str(), Some("work_not_claimable"));
}

#[test]
fn stdio_server_releases_a_lease_and_allows_a_fresh_claim() {
    let fixture = Fixture::new();
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();
    let claimed = claim_one(&mut client, "stdio-release-claim");
    let work_id = claimed["workItem"]["id"].as_str().unwrap().to_owned();
    let lease_token = claimed["leaseToken"].as_str().unwrap().to_owned();

    let released = client.call_tool(
        "release_work",
        json!({
            "work_id": work_id,
            "lease_token": lease_token,
            "reason": "需要切换到更合适的执行代理"
        }),
    );
    assert_eq!(structured(&released)["status"].as_str(), Some("ready"));

    let workspace = PlanService::new()
        .load_workspace(fixture.home)
        .expect("workspace must load");
    let view = workspace
        .work_items
        .iter()
        .find(|item| item.work_item.id.to_string() == work_id)
        .expect("released task must exist");
    assert_eq!(view.work_item.status, WorkItemStatus::Ready);
    assert!(view.active_lease.is_none());
    let branch = view
        .task
        .work_branch
        .clone()
        .expect("released agent task must have a work branch");
    git(&fixture.work, &["switch", "main"]);
    git(&fixture.work, &["branch", "--delete", &branch]);

    let reclaimed = claim_one(&mut client, "stdio-release-reclaim");
    assert_eq!(reclaimed["workItem"]["id"].as_str(), Some(work_id.as_str()));
    assert_ne!(
        reclaimed["leaseToken"].as_str(),
        Some(lease_token.as_str()),
        "a fresh claim must issue a new lease token"
    );
}

#[test]
fn malformed_work_ids_return_a_structured_invalid_id_error() {
    let fixture = Fixture::new();
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();

    let response = client.call_tool(
        "heartbeat_work",
        json!({ "work_id": "not-a-uuid", "lease_token": "unused" }),
    );
    let error = tool_error(&response);
    assert_eq!(error["code"].as_str(), Some("invalid_id"));
    assert_eq!(error["details"]["field"].as_str(), Some("work_id"));
}

#[test]
fn lease_tokens_are_never_persisted_in_sqlite_text_columns() {
    let fixture = Fixture::new();
    let mut client = McpStdioClient::spawn(&fixture.home);
    client.initialize();
    let claimed = claim_one(&mut client, "stdio-token-scan");
    let work_id = claimed["workItem"]["id"].as_str().unwrap().to_owned();
    let lease_token = claimed["leaseToken"].as_str().unwrap().to_owned();
    drop(client);

    let expected_hash = format!("{:x}", Sha256::digest(lease_token.as_bytes()));
    let database_path = fixture.home.join(".coldstart").join("state.db");
    let values = scan_text_columns(&database_path);
    assert!(
        values.len() > 20,
        "the scan must cover the persisted work tables"
    );
    let leaks = values
        .iter()
        .filter(|(_, _, value)| value.contains(&lease_token))
        .collect::<Vec<_>>();
    assert!(
        leaks.is_empty(),
        "raw lease token was persisted in SQLite: {leaks:?}"
    );
    let hash_values = values
        .iter()
        .filter(|(_, _, value)| value == &expected_hash)
        .collect::<Vec<_>>();
    assert_eq!(
        hash_values.len(),
        1,
        "the token hash must be persisted exactly once"
    );

    let connection = Connection::open(&database_path).expect("SQLite database must open");
    let stored_hash = connection
        .query_row(
            "SELECT token_hash FROM leases WHERE work_item_id = ?1",
            [&work_id],
            |row| row.get::<_, String>(0),
        )
        .expect("stored token hash must load");
    assert_eq!(stored_hash, expected_hash);
}
