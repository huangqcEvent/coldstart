use std::{
    fs,
    path::Path,
    sync::{Arc, Barrier},
    thread,
};

use coldstart_core::{DomainEvent, Project, ProjectId};
use coldstart_storage::{Database, StorageError};
use rusqlite::{Connection, params};
use tempfile::TempDir;

fn sample_project(dir: &TempDir) -> Project {
    Project::new("Demo Project", dir.path().join("project-home")).expect("project must be valid")
}

fn open_database(dir: &TempDir) -> Database {
    Database::open(&dir.path().join(".coldstart").join("state.db")).expect("database must open")
}

fn open_error(path: &Path) -> StorageError {
    match Database::open(path) {
        Ok(_) => panic!("database open must fail for an unsupported schema"),
        Err(error) => error,
    }
}

#[test]
fn load_project_returns_current_root_after_project_home_moves() {
    let dir = TempDir::new().expect("temp directory must exist");
    let original_home = dir.path().join("original");
    let moved_home = dir.path().join("moved");
    let original_root = std::path::absolute(&original_home).expect("root must become absolute");
    let current_root = std::path::absolute(&moved_home).expect("root must become absolute");
    let project = Project::new("Moved Project", original_root).expect("project must be valid");

    let mut database = Database::open(&original_home.join(".coldstart").join("state.db"))
        .expect("database must open");
    database
        .initialize_project(&project)
        .expect("project must initialize");
    drop(database);

    fs::rename(&original_home, &moved_home).expect("project home must move");
    let database = Database::open(&moved_home.join(".coldstart").join("state.db"))
        .expect("moved database must open");
    let loaded = database
        .load_project(&project.id, current_root.clone())
        .expect("project must load")
        .expect("project must exist");

    assert_eq!(
        loaded.root_path, current_root,
        "load_project returned the stale root path persisted before the move"
    );
}

#[test]
fn creates_schema_v7_without_a_root_path_column() {
    let dir = TempDir::new().expect("temp directory must exist");
    let database = open_database(&dir);
    drop(database);

    let connection = Connection::open(dir.path().join(".coldstart").join("state.db"))
        .expect("database must open directly");
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("schema version must be readable");
    let mut statement = connection
        .prepare("PRAGMA table_info(projects)")
        .expect("project table metadata must be readable");
    let columns: Vec<String> = statement
        .query_map([], |row| row.get(1))
        .expect("project columns must be readable")
        .collect::<Result<_, _>>()
        .expect("project columns must be valid");

    assert_eq!(version, 7);
    assert_eq!(columns, ["id", "name", "created_at"]);
}

#[test]
fn migrates_v1_projects_and_events_without_data_loss() {
    let dir = TempDir::new().expect("temp directory must exist");
    let project = Project::new("Migrated Project", dir.path().join("project-home"))
        .expect("project must be valid");
    let database_path = dir.path().join(".coldstart").join("state.db");
    create_v1_database(&database_path, &project);

    let database = Database::open(&database_path).expect("v1 database must migrate");
    assert_eq!(database.event_count(&project.id).unwrap(), 2);
    let loaded = database
        .load_project(&project.id, project.root_path.clone())
        .expect("migrated project must load")
        .expect("migrated project must exist");
    assert_eq!(loaded.name, project.name);
    assert_eq!(loaded.created_at, project.created_at);
    drop(database);

    let connection = Connection::open(&database_path).expect("database must open directly");
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("schema version must be readable");
    let root_path_columns: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('projects') WHERE name = 'root_path'",
            [],
            |row| row.get(0),
        )
        .expect("project columns must be readable");
    let mut statement = connection
        .prepare("SELECT event_id FROM events ORDER BY seq")
        .expect("events must be readable");
    let event_ids: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .expect("event ids must be readable")
        .collect::<Result<_, _>>()
        .expect("event ids must be valid");

    assert_eq!(version, 7);
    assert_eq!(root_path_columns, 0);
    assert_eq!(event_ids, ["event-one", "event-two"]);
}

#[test]
fn persists_project_and_initial_event_across_reopen() {
    let dir = TempDir::new().expect("temp directory must exist");
    let project = sample_project(&dir);

    {
        let mut database = open_database(&dir);
        database
            .initialize_project(&project)
            .expect("project must initialize");

        assert_eq!(database.event_count(&project.id).unwrap(), 1);
        assert_eq!(
            database
                .load_project(&project.id, project.root_path.clone())
                .unwrap(),
            Some(project.clone())
        );
    }

    let database = open_database(&dir);
    assert_eq!(
        database
            .load_project(&project.id, project.root_path.clone())
            .unwrap(),
        Some(project.clone())
    );
    assert_eq!(database.event_count(&project.id).unwrap(), 1);
}

#[test]
fn rejects_a_duplicate_project_without_adding_an_event() {
    let dir = TempDir::new().expect("temp directory must exist");
    let project = sample_project(&dir);
    let mut database = open_database(&dir);
    database.initialize_project(&project).unwrap();

    let error = database
        .initialize_project(&project)
        .expect_err("duplicate project must fail");

    assert!(matches!(
        error,
        StorageError::ProjectAlreadyExists(id) if id == project.id
    ));
    assert_eq!(database.event_count(&project.id).unwrap(), 1);
}

#[test]
fn appends_events_and_rejects_a_duplicate_event_id() {
    let dir = TempDir::new().expect("temp directory must exist");
    let project = sample_project(&dir);
    let mut database = open_database(&dir);
    database.initialize_project(&project).unwrap();
    let event = DomainEvent::project_initialized(&project);

    database.append_event(&event).unwrap();
    assert_eq!(database.event_count(&project.id).unwrap(), 2);

    let error = database
        .append_event(&event)
        .expect_err("duplicate event must fail");
    assert!(matches!(
        error,
        StorageError::EventAlreadyExists(id) if id == event.event_id
    ));
    assert_eq!(database.event_count(&project.id).unwrap(), 2);
}

#[test]
fn reports_unknown_projects_without_writing_orphan_events() {
    let dir = TempDir::new().expect("temp directory must exist");
    let registered = sample_project(&dir);
    let unregistered = Project::new("Other", dir.path().join("other")).unwrap();
    let mut database = open_database(&dir);
    database.initialize_project(&registered).unwrap();
    let event = DomainEvent::project_initialized(&unregistered);

    let error = database
        .append_event(&event)
        .expect_err("unknown project must fail");

    assert!(matches!(
        error,
        StorageError::ProjectNotFound(id) if id == unregistered.id
    ));
    assert_eq!(database.event_count(&unregistered.id).unwrap(), 0);
    assert_eq!(
        database
            .load_project(&unregistered.id, unregistered.root_path.clone())
            .unwrap(),
        None
    );
}

#[test]
fn creates_missing_database_parent_directories() {
    let dir = TempDir::new().expect("temp directory must exist");
    let root = dir.path().join("deep").join("project");
    let database_path = root.join(".coldstart").join("state.db");

    let database = Database::open(&database_path);

    assert!(
        database.is_ok(),
        "database must create its parent directories"
    );
    assert!(database_path.exists());
}

fn create_v1_database(path: &Path, project: &Project) {
    fs::create_dir_all(path.parent().expect("database path must have a parent"))
        .expect("database parent must be created");
    let connection = Connection::open(path).expect("database must open directly");
    connection
        .execute_batch(
            "
            CREATE TABLE projects (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                root_path TEXT NOT NULL,
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
        .expect("v1 schema must be created");
    connection
        .execute(
            "INSERT INTO projects (id, name, root_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                project.id.to_string(),
                project.name,
                project.root_path.to_string_lossy(),
                project.created_at.to_rfc3339(),
            ],
        )
        .expect("v1 project must be inserted");
    for event_id in ["event-one", "event-two"] {
        connection
            .execute(
                "INSERT INTO events (event_id, project_id, occurred_at, event_type, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    event_id,
                    project.id.to_string(),
                    project.created_at.to_rfc3339(),
                    "project_initialized",
                    r#"{"type":"project_initialized","data":{"name":"Migrated Project"}}"#,
                ],
            )
            .expect("v1 event must be inserted");
    }
    connection
        .pragma_update(None, "user_version", 1)
        .expect("v1 schema version must be set");
}

#[test]
fn concurrent_first_open_succeeds_and_database_remains_usable() {
    let dir = TempDir::new().expect("temp directory must exist");
    let database_path = dir.path().join(".coldstart").join("state.db");
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let database_path = database_path.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let database = Database::open(&database_path).map_err(|error| error.to_string());
                barrier.wait();
                database.and_then(|database| {
                    database
                        .event_count(&ProjectId::new())
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                })
            })
        })
        .collect();

    for handle in handles {
        handle
            .join()
            .expect("open thread must not panic")
            .expect("concurrent database open must succeed");
    }

    let project = sample_project(&dir);
    let mut database = Database::open(&database_path).expect("database must remain usable");
    database.initialize_project(&project).unwrap();
    assert_eq!(
        database
            .load_project(&project.id, project.root_path.clone())
            .unwrap(),
        Some(project.clone())
    );
    assert_eq!(database.event_count(&project.id).unwrap(), 1);
}

#[test]
fn rejects_negative_and_future_schema_versions() {
    for version in [-1, 8] {
        let dir = TempDir::new().expect("temp directory must exist");
        let database_path = dir.path().join("state.db");
        let connection = Connection::open(&database_path).expect("database must open");
        connection
            .pragma_update(None, "user_version", version)
            .expect("schema version must be set");
        drop(connection);

        let error = open_error(&database_path);
        assert!(matches!(
            error,
            StorageError::UnsupportedSchema { found, supported }
                if found == version && supported == 7
        ));
    }
}
