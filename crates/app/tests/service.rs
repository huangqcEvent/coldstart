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
fn read_status_resolves_a_unique_wrapped_project_home() {
    let temp = TempDir::new().unwrap();
    let wrapper = temp.path().join("acceptance");
    let project_home = wrapper.join("control");
    let service = ProjectService::new();
    let created = service
        .create_project("Wrapped Project", project_home.clone())
        .expect("project must initialize");

    let status = service
        .read_status(wrapper)
        .expect("the unique nested project must be discovered");

    assert_eq!(status.project_id, created.project_id);
    assert_eq!(status.root_path, std::path::absolute(project_home).unwrap());
}

#[test]
fn read_status_does_not_guess_between_multiple_wrapped_projects() {
    let temp = TempDir::new().unwrap();
    let wrapper = temp.path().join("acceptance");
    let service = ProjectService::new();
    service
        .create_project("Alpha", wrapper.join("alpha"))
        .expect("alpha project must initialize");
    service
        .create_project("Beta", wrapper.join("beta"))
        .expect("beta project must initialize");

    let error = service
        .read_status(wrapper)
        .expect_err("multiple nested projects must require an explicit choice");

    assert!(matches!(error, AppError::ManifestMissing(_)));
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
