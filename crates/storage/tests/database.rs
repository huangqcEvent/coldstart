use std::{
    path::Path,
    sync::{Arc, Barrier},
    thread,
};

use coldstart_core::{DomainEvent, Project};
use coldstart_storage::{Database, StorageError};
use rusqlite::Connection;
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
            database.load_project(&project.id).unwrap(),
            Some(project.clone())
        );
    }

    let database = open_database(&dir);
    assert_eq!(
        database.load_project(&project.id).unwrap(),
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
    assert_eq!(database.load_project(&unregistered.id).unwrap(), None);
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
                let result = Database::open(&database_path)
                    .map(|_| ())
                    .map_err(|error| error.to_string());
                barrier.wait();
                result
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
        database.load_project(&project.id).unwrap(),
        Some(project.clone())
    );
    assert_eq!(database.event_count(&project.id).unwrap(), 1);
}

#[test]
fn rejects_negative_and_future_schema_versions() {
    for version in [-1, 2] {
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
                if found == version && supported == 1
        ));
    }
}
