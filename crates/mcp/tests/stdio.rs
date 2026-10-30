use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use coldstart_app::{PlanService, ProjectService};
use coldstart_core::{
    ArtifactId, BlueprintContent, BlueprintEntityId, BlueprintGeneration, GenerationSource,
    ModuleDefinition, PlatformDefinition, PlatformKind, ProjectId, SourceReference, TraceableText,
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
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let home = directory.path().join("project");
        let status = ProjectService::new()
            .create_project("MCP 集成项目", home.clone())
            .expect("project must create");
        let mut database =
            Database::open(&home.join(".coldstart").join("state.db")).expect("database must open");
        let blueprint = database
            .create_blueprint_revision(
                &status.project_id,
                blueprint_content(status.project_id),
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
        }
    }
}

fn blueprint_content(project_id: ProjectId) -> BlueprintContent {
    let source = SourceReference {
        artifact_id: ArtifactId::new(),
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
        capabilities: vec![],
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
            "claim_next_work",
            "get_work_context",
            "heartbeat_work",
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
    assert_eq!(context["taskKey"], claimed["workItem"]["taskKey"]);

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
    let report = json!({
        "work_id": work_id,
        "lease_token": lease_token,
        "report": {
            "status": "completed",
            "summary": "通过真实 MCP stdio 完成实现",
            "branch": task.work_branch,
            "commitSha": null,
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
    assert_eq!(structured(&submitted)["accepted"], true);

    let workspace = PlanService::new()
        .load_workspace(fixture.home.clone())
        .expect("workspace must reload");
    assert!(workspace.work_items.iter().any(|item| {
        item.work_item.id.to_string() == work_id && item.work_item.status == WorkItemStatus::Done
    }));
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
