mod support;

use std::path::PathBuf;

use coldstart_core::{
    BlueprintEntityId, CapabilityDescriptor, CapabilityState, CapabilityStatus,
    CredentialEnvironment, CredentialKind, CredentialMetadata, GitSyncRecord, GitSyncStatus,
    Repository, RepositoryAuthMode, RepositorySyncPolicy,
};
use coldstart_storage::{Database, StorageError};
use rusqlite::Connection;
use support::PlanFixture;

fn repository(
    project_id: coldstart_core::ProjectId,
    key: &str,
    credential_id: Option<coldstart_core::CredentialId>,
) -> Repository {
    Repository::new(
        project_id,
        key.to_owned(),
        format!("{key} 仓库"),
        PathBuf::from(format!("D:/coldstart-test/repos/{key}")),
        format!("https://github.com/acme/{key}.git"),
        "main".to_owned(),
        vec!["main".to_owned()],
        if credential_id.is_some() {
            RepositoryAuthMode::Credential
        } else {
            RepositoryAuthMode::None
        },
        credential_id,
        RepositorySyncPolicy::FetchBeforeClaim,
    )
    .expect("repository must be valid")
}

fn credential(project_id: coldstart_core::ProjectId) -> CredentialMetadata {
    CredentialMetadata::new(
        project_id,
        "GitHub 集成".to_owned(),
        "github".to_owned(),
        CredentialKind::OAuthToken,
        CredentialEnvironment::Shared,
        "coldstart:test:github".to_owned(),
    )
    .expect("credential must be valid")
}

#[test]
fn migrates_v4_to_current_and_persists_integration_state() {
    let fixture = PlanFixture::new(vec![support::agent_task("foundation:demo", &[])]);
    let database_path = fixture.database_path.clone();
    let project_id = fixture.project_id;
    let events_before = fixture
        .database
        .event_count(&project_id)
        .expect("event count must load");
    drop(fixture.database);

    let connection = Connection::open(&database_path).expect("database must open directly");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP INDEX work_items_by_integration;
             ALTER TABLE work_items DROP COLUMN integrated_at;
             ALTER TABLE work_items DROP COLUMN integration_reference;
             DROP INDEX work_reports_one_running_verification;
             DROP INDEX work_reports_one_running_review;
             ALTER TABLE work_runs DROP COLUMN review_snapshot_json;
             ALTER TABLE work_runs DROP COLUMN review_snapshot_hash;
             ALTER TABLE work_reports DROP COLUMN review_status;
             ALTER TABLE work_reports DROP COLUMN review_json;
             ALTER TABLE work_reports DROP COLUMN review_started_at;
             ALTER TABLE work_reports DROP COLUMN review_finished_at;
             ALTER TABLE work_reports DROP COLUMN review_waiver_json;
             ALTER TABLE work_runs DROP COLUMN verification_snapshot_json;
             ALTER TABLE work_runs DROP COLUMN verification_snapshot_hash;
             ALTER TABLE work_reports DROP COLUMN verification_status;
             ALTER TABLE work_reports DROP COLUMN verification_json;
             ALTER TABLE work_reports DROP COLUMN verification_started_at;
             ALTER TABLE work_reports DROP COLUMN verification_finished_at;
             DROP TABLE git_syncs;
             DROP TABLE capability_api_endpoints;
             DROP TABLE review_standard_revisions;
             DROP TABLE review_standards;
             DROP TABLE review_configurations;
             DROP TABLE pull_request_insights;
             DROP TABLE pull_requests;
             DROP TABLE code_investigations;
             DROP TABLE repository_verification_commands;
             DROP TABLE capability_states;
             DROP TABLE repositories;
             DROP TABLE credentials;
             DROP TABLE project_settings;
             ALTER TABLE artifacts DROP COLUMN images_json;
             PRAGMA user_version = 4;",
        )
        .expect("v4 state must be restored");
    drop(connection);

    let mut database = Database::open(&database_path).expect("v4 migration must run");
    assert_eq!(
        database.schema_version().expect("schema version must load"),
        13
    );

    let credential = credential(project_id);
    database
        .save_credential(&credential)
        .expect("credential must save");
    let repository = repository(project_id, "admin", Some(credential.id));
    database
        .save_repository(&repository)
        .expect("repository must save");
    assert_eq!(
        database
            .repository_by_key(&project_id, "admin")
            .expect("repository query must work"),
        Some(repository.clone())
    );
    assert_eq!(
        database.event_count(&project_id).unwrap(),
        events_before + 2
    );

    drop(database);
    let database = Database::open(&database_path).expect("database must reopen");
    assert_eq!(
        database
            .repository_by_key(&project_id, "admin")
            .expect("repository query must work"),
        Some(repository)
    );
}

#[test]
fn repository_keys_are_unique_and_deleting_a_credential_nulls_the_reference() {
    let mut fixture = PlanFixture::new(vec![support::agent_task("foundation:demo", &[])]);
    let project_id = fixture.project_id;
    let credential = credential(project_id);
    fixture
        .database
        .save_credential(&credential)
        .expect("credential must save");
    let repository = repository(project_id, "admin", Some(credential.id));
    fixture
        .database
        .save_repository(&repository)
        .expect("repository must save");

    let mut duplicate = repository.clone();
    duplicate.id = coldstart_core::RepositoryId::new();
    let error = fixture
        .database
        .save_repository(&duplicate)
        .expect_err("duplicate repository key must fail");
    assert!(matches!(
        error,
        StorageError::RepositoryKeyAlreadyExists {
            project_id: found,
            key
        } if found == project_id && key == "admin"
    ));

    assert!(
        fixture
            .database
            .delete_credential(&project_id, &credential.id)
            .expect("credential must delete")
    );
    assert!(
        !fixture
            .database
            .delete_credential(&project_id, &credential.id)
            .expect("second delete must be a no-op")
    );
    let stored = fixture
        .database
        .repository_by_key(&project_id, "server")
        .expect("repository query must work")
        .expect("repository must remain");
    assert_eq!(stored.credential_id, None);
    assert!(
        fixture
            .database
            .credential(&project_id, &credential.id)
            .expect("credential query must work")
            .is_none()
    );
}

#[test]
fn capability_state_upserts_and_project_settings_persist() {
    let mut fixture = PlanFixture::new(vec![support::agent_task("foundation:demo", &[])]);
    let project_id = fixture.project_id;
    let capability_id = BlueprintEntityId::derive(&project_id, "capability", "对象存储");
    let missing = CapabilityState::new(
        CapabilityDescriptor {
            project_id,
            capability_id: capability_id.clone(),
            name: "对象存储".to_owned(),
            provider: "aliyun_oss".to_owned(),
            environment: "development".to_owned(),
        },
        CapabilityStatus::Missing,
        None,
        Some("等待提供 AccessKey".to_owned()),
    )
    .expect("capability must be valid");
    fixture
        .database
        .save_capability_state(&missing)
        .expect("capability must save");
    let configured = CapabilityState::new(
        CapabilityDescriptor {
            project_id,
            capability_id: capability_id.clone(),
            name: "对象存储".to_owned(),
            provider: "aliyun_oss".to_owned(),
            environment: "development".to_owned(),
        },
        CapabilityStatus::Configured,
        None,
        Some("已写入 Credential Manager".to_owned()),
    )
    .expect("capability must be valid");
    fixture
        .database
        .save_capability_state(&configured)
        .expect("capability must update");

    let states = fixture
        .database
        .list_capability_states(&project_id)
        .expect("capability states must load");
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].status, CapabilityStatus::Configured);

    fixture
        .database
        .set_project_setting(&project_id, "release.channel", "internal")
        .expect("setting must save");
    assert_eq!(
        fixture
            .database
            .project_setting(&project_id, "release.channel")
            .expect("setting query must work"),
        Some("internal".to_owned())
    );
    fixture
        .database
        .set_project_setting(&project_id, "release.channel", "beta")
        .expect("setting must update");
    assert_eq!(
        fixture
            .database
            .project_setting(&project_id, "release.channel")
            .expect("setting query must work"),
        Some("beta".to_owned())
    );
}

#[test]
fn records_git_sync_outcomes_for_a_work_item() {
    let mut fixture = PlanFixture::new(vec![support::agent_task("foundation:demo", &[])]);
    let project_id = fixture.project_id;
    let repository = fixture
        .database
        .repository_by_key(&project_id, "server")
        .expect("repository query must work")
        .expect("fixture repository must exist");
    let work_item = fixture
        .database
        .list_work_items(&project_id)
        .expect("work items must load")
        .into_iter()
        .next()
        .expect("work item must exist");
    let pushed = GitSyncRecord::new(
        project_id,
        work_item.id,
        repository.id,
        "codex/foundation-demo".to_owned(),
        "0123456789abcdef0123456789abcdef01234567".to_owned(),
        GitSyncStatus::Pushed,
        None,
    );
    fixture
        .database
        .record_git_sync(&pushed)
        .expect("git sync must save");
    let failed = GitSyncRecord::new(
        project_id,
        work_item.id,
        repository.id,
        "codex/foundation-demo".to_owned(),
        "0123456789abcdef0123456789abcdef01234567".to_owned(),
        GitSyncStatus::Failed,
        Some("remote rejected the update".to_owned()),
    );
    fixture
        .database
        .record_git_sync(&failed)
        .expect("failed git sync must save");

    let syncs = fixture
        .database
        .list_git_syncs(&project_id)
        .expect("git syncs must load");
    assert_eq!(syncs.len(), 2);
    assert!(syncs.iter().any(|sync| sync.id == pushed.id));
    assert!(
        syncs
            .iter()
            .any(|sync| sync.id == failed.id && sync.status == GitSyncStatus::Failed)
    );
}
