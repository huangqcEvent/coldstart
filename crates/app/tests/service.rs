use std::fs;

use coldstart_app::{AppError, ProjectService};
use tempfile::TempDir;

#[test]
fn create_project_returns_the_initialized_status() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("demo");
    let service = ProjectService::new();

    let status = service
        .create_project("Demo Project", home.clone())
        .expect("project must initialize");

    assert_eq!(status.name, "Demo Project");
    assert_eq!(status.event_count, 1);
    assert_eq!(
        status.root_path,
        std::path::absolute(&home).expect("path must be absolute")
    );
    assert_eq!(service.read_status(home).expect("status must load"), status);
}

#[test]
fn read_status_uses_the_current_project_home_after_a_move() {
    let temp = TempDir::new().unwrap();
    let original = temp.path().join("original");
    let moved = temp.path().join("moved");
    let service = ProjectService::new();
    let created = service
        .create_project("Moved Project", original.clone())
        .expect("project must initialize");

    fs::rename(&original, &moved).expect("project must move");
    let status = service
        .read_status(moved.clone())
        .expect("status must load");

    assert_eq!(status.project_id, created.project_id);
    assert_eq!(status.root_path, std::path::absolute(moved).unwrap());
}

#[test]
fn create_project_reports_a_missing_manifest_with_a_stable_code() {
    let temp = TempDir::new().unwrap();
    let error = ProjectService::new()
        .read_status(temp.path().to_path_buf())
        .expect_err("empty directory must fail");

    assert!(matches!(error, AppError::ManifestMissing(_)));
    assert_eq!(error.code(), "manifest_missing");
}
