use std::path::PathBuf;

use chrono::Utc;
use coldstart_core::{
    DomainEvent, DomainEventKind, ManifestError, PROJECT_MANIFEST_SCHEMA_VERSION, Project,
    ProjectManifest,
};

fn sample_project() -> Project {
    Project::new("Demo Project", PathBuf::from("D:\\work\\demo")).expect("project must be valid")
}

#[test]
fn manifest_omits_the_machine_specific_root_path() {
    let manifest = ProjectManifest::from_project(&sample_project());
    let json = serde_json::to_value(&manifest).expect("manifest must serialize");

    assert_eq!(
        json["schemaVersion"].as_u64(),
        Some(u64::from(PROJECT_MANIFEST_SCHEMA_VERSION))
    );
    assert_eq!(json["name"], "Demo Project");
    assert!(json["projectId"].is_string());
    assert!(json.get("rootPath").is_none());
}

#[test]
fn manifest_round_trips_without_losing_project_identity() {
    let manifest = ProjectManifest::from_project(&sample_project());
    let encoded = serde_json::to_string(&manifest).expect("manifest must serialize");
    let decoded: ProjectManifest =
        serde_json::from_str(&encoded).expect("manifest must deserialize");

    assert_eq!(decoded, manifest);
}

#[test]
fn restored_project_uses_the_manifest_directory_as_its_root() {
    let manifest = ProjectManifest::from_project(&sample_project());
    let moved_root = PathBuf::from("E:\\moved\\demo");
    let restored = manifest
        .to_project(moved_root.clone())
        .expect("manifest must produce a project");

    assert_eq!(restored.id, manifest.project_id);
    assert_eq!(restored.name, manifest.name);
    assert_eq!(restored.created_at, manifest.created_at);
    assert_eq!(restored.root_path, moved_root);
}

#[test]
fn rejects_an_unknown_manifest_schema() {
    let mut manifest = ProjectManifest::from_project(&sample_project());
    manifest.schema_version += 1;

    assert_eq!(
        manifest.validate(),
        Err(ManifestError::UnsupportedSchema {
            found: PROJECT_MANIFEST_SCHEMA_VERSION + 1,
            supported: PROJECT_MANIFEST_SCHEMA_VERSION,
        })
    );
}

#[test]
fn rejects_blank_and_oversized_manifest_names() {
    let mut manifest = ProjectManifest::from_project(&sample_project());
    manifest.name = "   ".to_owned();
    assert_eq!(manifest.validate(), Err(ManifestError::EmptyName));

    manifest.name = "a".repeat(81);
    assert_eq!(manifest.validate(), Err(ManifestError::NameTooLong));
}

#[test]
fn project_initialized_event_has_a_stable_serialized_type() {
    let project = sample_project();
    let event = DomainEvent::project_initialized(&project);

    assert_eq!(event.project_id, project.id);
    assert_eq!(
        event.kind,
        DomainEventKind::ProjectInitialized {
            name: "Demo Project".to_owned()
        }
    );
    assert_eq!(event.kind.type_name(), "project_initialized");

    let json = serde_json::to_value(&event).expect("event must serialize");
    assert_eq!(json["kind"]["type"], "project_initialized");
    assert_eq!(json["kind"]["data"]["name"], "Demo Project");
}

#[test]
fn event_identity_and_time_are_generated_per_event() {
    let project = sample_project();
    let before = Utc::now();
    let first = DomainEvent::project_initialized(&project);
    let second = DomainEvent::project_initialized(&project);

    assert_ne!(first.event_id, second.event_id);
    assert!(first.occurred_at >= before);
    assert!(first.occurred_at <= Utc::now());
}
