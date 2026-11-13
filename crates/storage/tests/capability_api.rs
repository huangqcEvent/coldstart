mod support;

use chrono::{Duration, Utc};
use coldstart_core::{
    BlueprintEntityId, CapabilityApiEndpoint, CapabilityApiEndpointInput, CapabilityApiMethod,
    CapabilityDescriptor, CapabilityState, CapabilityStatus, ClaimResult, CredentialEnvironment,
    CredentialKind, CredentialMetadata, WorkItemStatus,
};
use coldstart_storage::{Database, StorageError};
use rusqlite::Connection;
use support::{PlanFixture, agent_task};

fn capability_state(fixture: &PlanFixture, status: CapabilityStatus) -> CapabilityState {
    CapabilityState::new(
        CapabilityDescriptor {
            project_id: fixture.project_id,
            capability_id: BlueprintEntityId::derive(
                &fixture.project_id,
                "capability",
                "阿里云 OSS",
            ),
            name: "阿里云 OSS".to_owned(),
            provider: "aliyun_oss".to_owned(),
            environment: "development".to_owned(),
        },
        status,
        None,
        None,
    )
    .expect("capability state must validate")
}

fn credential(fixture: &PlanFixture) -> CredentialMetadata {
    CredentialMetadata::new(
        fixture.project_id,
        "OSS 开发凭据".to_owned(),
        "aliyun_oss".to_owned(),
        CredentialKind::ApiKey,
        CredentialEnvironment::Development,
        "coldstart:test:oss".to_owned(),
    )
    .expect("credential must validate")
}

#[test]
fn migrates_schema_nine_to_ten_without_losing_project_data() {
    let fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let database_path = fixture.database_path.clone();
    let project_id = fixture.project_id;
    let events_before = fixture
        .database
        .event_count(&project_id)
        .expect("event count must load");
    let work_items_before = fixture
        .database
        .list_work_items(&project_id)
        .expect("work items must load");
    drop(fixture.database);

    let connection = Connection::open(&database_path).expect("database must open directly");
    connection
        .execute_batch(
            "DROP TABLE capability_api_endpoints;
             PRAGMA user_version = 9;",
        )
        .expect("schema nine must be restored");
    drop(connection);

    let database = Database::open(&database_path).expect("schema ten migration must run");
    assert_eq!(database.schema_version().expect("schema version"), 10);
    assert_eq!(
        database
            .event_count(&project_id)
            .expect("event count must reload"),
        events_before
    );
    assert_eq!(
        database
            .list_work_items(&project_id)
            .expect("work items must reload"),
        work_items_before
    );
}

#[test]
fn persists_updates_and_deletes_capability_api_endpoints() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let state = capability_state(&fixture, CapabilityStatus::Configured);
    fixture
        .database
        .save_capability_state(&state)
        .expect("capability state must save");
    let credential = credential(&fixture);
    fixture
        .database
        .save_credential(&credential)
        .expect("credential must save");

    let mut endpoint = CapabilityApiEndpoint::new(
        fixture.project_id,
        CapabilityApiEndpointInput {
            capability_id: state.capability_id.clone(),
            environment: "development".to_owned(),
            name: "附件上传".to_owned(),
            base_url: "https://oss-api.example.com".to_owned(),
            path_prefix: "/attachments".to_owned(),
            allowed_methods: vec![CapabilityApiMethod::Get, CapabilityApiMethod::Post],
            credential_id: Some(credential.id),
            auth_header: Some("Authorization".to_owned()),
            auth_scheme: Some("Bearer".to_owned()),
        },
    )
    .expect("endpoint must validate");
    fixture
        .database
        .save_capability_api_endpoint(&endpoint)
        .expect("endpoint must save");

    let listed = fixture
        .database
        .list_capability_api_endpoints(&fixture.project_id)
        .expect("endpoints must list");
    assert_eq!(listed, [endpoint.clone()]);
    assert_eq!(
        fixture
            .database
            .capability_api_endpoint(&fixture.project_id, &endpoint.id)
            .expect("endpoint must load"),
        Some(endpoint.clone())
    );

    endpoint
        .update_from(CapabilityApiEndpointInput {
            capability_id: state.capability_id.clone(),
            environment: "development".to_owned(),
            name: "附件上传（新）".to_owned(),
            base_url: "https://upload.example.com".to_owned(),
            path_prefix: "/files".to_owned(),
            allowed_methods: vec![CapabilityApiMethod::Post],
            credential_id: Some(credential.id),
            auth_header: Some("X-API-Key".to_owned()),
            auth_scheme: None,
        })
        .expect("endpoint update must validate");
    fixture
        .database
        .save_capability_api_endpoint(&endpoint)
        .expect("endpoint update must save");
    assert_eq!(
        fixture
            .database
            .capability_api_endpoint(&fixture.project_id, &endpoint.id)
            .expect("endpoint must reload"),
        Some(endpoint.clone())
    );

    let duplicate = CapabilityApiEndpoint::new(
        fixture.project_id,
        CapabilityApiEndpointInput {
            capability_id: state.capability_id,
            environment: "development".to_owned(),
            name: "附件上传（新）".to_owned(),
            base_url: "https://other.example.com".to_owned(),
            path_prefix: "/".to_owned(),
            allowed_methods: vec![CapabilityApiMethod::Get],
            credential_id: None,
            auth_header: None,
            auth_scheme: None,
        },
    )
    .expect("duplicate draft must validate");
    assert!(matches!(
        fixture
            .database
            .save_capability_api_endpoint(&duplicate)
            .expect_err("duplicate name must fail"),
        StorageError::CapabilityApiEndpointNameAlreadyExists { .. }
    ));

    fixture
        .database
        .delete_credential(&fixture.project_id, &credential.id)
        .expect("credential must delete");
    let detached = fixture
        .database
        .capability_api_endpoint(&fixture.project_id, &endpoint.id)
        .expect("endpoint must reload")
        .expect("endpoint must remain");
    assert_eq!(detached.credential_id, None);

    assert!(
        fixture
            .database
            .delete_capability_api_endpoint(&fixture.project_id, &endpoint.id)
            .expect("endpoint must delete")
    );
    assert_eq!(
        fixture
            .database
            .list_capability_api_endpoints(&fixture.project_id)
            .expect("endpoints must list"),
        []
    );
}

#[test]
fn readonly_authorization_does_not_progress_or_write_events() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let claimed = fixture
        .database
        .claim_work(
            &fixture.project_id,
            "codex",
            Some("server"),
            "readonly-claim",
            Utc::now(),
            Duration::minutes(30),
        )
        .expect("work must claim");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("expected claimed work");
    };
    let events_before = fixture
        .database
        .event_count(&fixture.project_id)
        .expect("event count must load");
    let runs_before = fixture
        .database
        .list_work_runs(&fixture.project_id)
        .expect("runs must load");

    let authorization = fixture
        .database
        .authorize_work_readonly(&claimed.work_item.id, &claimed.lease_token, Utc::now())
        .expect("readonly authorization must succeed");

    assert_eq!(authorization.work_item.status, WorkItemStatus::Claimed);
    assert_eq!(authorization.run.status, claimed.run.status);
    assert_eq!(
        fixture
            .database
            .event_count(&fixture.project_id)
            .expect("event count must load"),
        events_before
    );
    assert_eq!(
        fixture
            .database
            .list_work_runs(&fixture.project_id)
            .expect("runs must load"),
        runs_before
    );

    assert!(matches!(
        fixture
            .database
            .authorize_work_readonly(&claimed.work_item.id, "wrong-token", Utc::now())
            .expect_err("wrong token must fail"),
        StorageError::LeaseInvalid(_)
    ));
    assert!(matches!(
        fixture
            .database
            .authorize_work_readonly(
                &claimed.work_item.id,
                &claimed.lease_token,
                Utc::now() + Duration::hours(1)
            )
            .expect_err("expired lease must fail"),
        StorageError::LeaseExpired(_)
    ));
}
