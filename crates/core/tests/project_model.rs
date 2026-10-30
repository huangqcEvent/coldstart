use std::path::PathBuf;

use chrono::Utc;
use coldstart_core::{Project, ProjectError};

#[test]
fn creates_a_project_with_a_trimmed_name_and_generated_id() {
    let root = std::env::temp_dir().join("coldstart-domain-test");
    let before = Utc::now();

    let project = Project::new("  Demo Project  ", root.clone()).expect("project must be valid");

    assert_eq!(project.name, "Demo Project");
    assert_eq!(project.root_path, root);
    assert!(!project.id.to_string().is_empty());
    assert!(project.created_at >= before);
}

#[test]
fn generates_distinct_project_ids() {
    let root = std::env::temp_dir().join("coldstart-domain-test");
    let first = Project::new("First", root.clone()).expect("project must be valid");
    let second = Project::new("Second", root).expect("project must be valid");

    assert_ne!(first.id, second.id);
}

#[test]
fn rejects_an_empty_project_name() {
    let root = std::env::temp_dir().join("coldstart-domain-test");
    let error = Project::new("   ", root).expect_err("empty name must fail");

    assert_eq!(error, ProjectError::EmptyName);
}

#[test]
fn rejects_a_project_name_longer_than_80_characters() {
    let root = std::env::temp_dir().join("coldstart-domain-test");
    let name = "a".repeat(81);
    let error = Project::new(name, root).expect_err("81-character name must fail");

    assert_eq!(error, ProjectError::NameTooLong);
}

#[test]
fn accepts_a_project_name_of_exactly_80_characters() {
    let root: PathBuf = std::env::temp_dir().join("coldstart-domain-test");
    let name = "a".repeat(80);
    let project = Project::new(name, root).expect("80-character name must be valid");

    assert_eq!(project.name.len(), 80);
}
