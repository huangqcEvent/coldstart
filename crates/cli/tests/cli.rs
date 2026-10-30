use std::fs;
use std::path::Path;
use std::process::Output;

use assert_cmd::Command;
use coldstart_core::ProjectId;
use coldstart_storage::Database;
use rusqlite::Connection;
use tempfile::TempDir;

fn run_init(home: &Path) -> Output {
    run_init_with_name(home, "Demo Project")
}

fn run_init_with_name(home: &Path, name: &str) -> Output {
    Command::cargo_bin("coldstart")
        .expect("binary must build")
        .args(["init", "--name"])
        .arg(name)
        .arg("--path")
        .arg(home)
        .output()
        .expect("command must run")
}

#[test]
fn init_creates_a_portable_project_home() {
    let temp = TempDir::new().expect("temp directory must exist");
    let home = temp.path().join("demo");

    let output = run_init(&home);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(home.join("coldstart.project.json").is_file());
    assert!(home.join(".coldstart").join("state.db").is_file());
    for relative in [
        "docs/requirements",
        "docs/architecture",
        "docs/decisions",
        "docs/release",
        ".coldstart/artifacts",
        ".coldstart/snapshots",
    ] {
        assert!(home.join(relative).is_dir(), "missing {relative}");
    }

    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(home.join("coldstart.project.json")).expect("manifest must be readable"),
    )
    .expect("manifest must be valid JSON");
    assert_eq!(manifest["name"], "Demo Project");
    assert_eq!(manifest["schemaVersion"].as_u64(), Some(1));
    assert!(manifest.get("rootPath").is_none());
}

#[test]
fn init_refuses_to_overwrite_an_existing_project_manifest() {
    let temp = TempDir::new().expect("temp directory must exist");
    let home = temp.path().join("demo");
    let first = run_init(&home);
    assert!(first.status.success());
    let manifest_path = home.join("coldstart.project.json");
    let before = fs::read(&manifest_path).expect("manifest must be readable");

    let second = run_init(&home);

    assert!(!second.status.success());
    let stderr = String::from_utf8_lossy(&second.stderr).to_lowercase();
    assert!(stderr.contains("already initialized"), "stderr: {stderr}");
    let after = fs::read(&manifest_path).expect("manifest must still exist");
    assert_eq!(after, before);
}

#[test]
fn invalid_names_do_not_create_or_modify_the_project_home() {
    let temp = TempDir::new().expect("temp directory must exist");

    for (index, name) in ["".to_owned(), "x".repeat(81)].into_iter().enumerate() {
        let home = temp.path().join(format!("invalid-{index}"));
        let output = run_init_with_name(&home, &name);

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        assert!(stderr.contains("invalid project name"), "stderr: {stderr}");
        assert!(!home.exists(), "invalid name created {}", home.display());
    }

    let home = temp.path().join("existing");
    fs::create_dir(&home).expect("existing project home must be created");
    fs::write(home.join("sentinel.txt"), b"keep").expect("sentinel must be written");

    let output = run_init_with_name(&home, "");

    assert!(!output.status.success());
    assert_eq!(
        fs::read(home.join("sentinel.txt")).expect("sentinel must remain readable"),
        b"keep"
    );
    assert_eq!(
        fs::read_dir(&home)
            .expect("project home must remain readable")
            .count(),
        1,
        "invalid name modified the existing project home"
    );
}

#[test]
fn manifest_project_id_matches_database_project_and_initial_event() {
    let temp = TempDir::new().expect("temp directory must exist");
    let home = temp.path().join("demo");
    let output = run_init(&home);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(home.join("coldstart.project.json")).expect("manifest must be readable"),
    )
    .expect("manifest must be valid JSON");
    let project_id_text = manifest["projectId"]
        .as_str()
        .expect("manifest must contain a project id");
    let project_id: ProjectId = project_id_text.parse().expect("project id must be valid");
    let database_path = home.join(".coldstart").join("state.db");
    let database = Database::open(&database_path).expect("database must open");
    let stored_project = database
        .load_project(&project_id)
        .expect("project must be readable")
        .expect("project must exist");
    assert_eq!(stored_project.id, project_id);

    let connection = Connection::open(&database_path).expect("database must open directly");
    let event_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .expect("initial event count must be readable");
    let (event_project_id, event_type): (String, String) = connection
        .query_row(
            "SELECT project_id, event_type FROM events ORDER BY seq LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("initial event must be readable");
    assert_eq!(event_count, 1);
    assert_eq!(event_project_id, project_id_text);
    assert_eq!(event_type, "project_initialized");
}
