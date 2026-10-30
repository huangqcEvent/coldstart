mod support;

use chrono::Utc;
use coldstart_core::BlueprintStatus;
use coldstart_storage::Database;
use rusqlite::Connection;
use support::{PlanFixture, agent_task};

#[test]
fn migrates_v3_database_to_v6_and_preserves_the_published_blueprint() {
    let fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let database_path = fixture.database_path.clone();
    let project_id = fixture.project_id;
    drop(fixture.database);

    let connection = Connection::open(&database_path).expect("database must open directly");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP TABLE project_settings;
             DROP TABLE repository_verification_commands;
             DROP TABLE git_syncs;
             DROP TABLE capability_states;
             DROP TABLE repositories;
             DROP TABLE credentials;
             DROP TABLE work_blockers;
             DROP TABLE work_reports;
             DROP TABLE work_runs;
             DROP TABLE leases;
             DROP TABLE work_items;
             DROP TABLE task_specs;
             DROP TABLE plan_revisions;
             PRAGMA user_version = 3;",
        )
        .expect("v3 state must be restored");
    drop(connection);

    let database = Database::open(&database_path).expect("v5 migration must run");
    let published = database
        .latest_blueprint(&project_id, BlueprintStatus::Published)
        .expect("blueprint query must work")
        .expect("published blueprint must survive");
    assert_eq!(published.project_id, project_id);

    let version: i64 = Connection::open(&database_path)
        .expect("read connection must open")
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("schema version must be readable");
    assert_eq!(version, 6);
}

#[test]
fn published_plan_and_work_items_survive_reopen() {
    let fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let work_items = fixture
        .database
        .list_work_items(&fixture.project_id)
        .expect("work items must load");
    assert_eq!(work_items.len(), 1);
    assert_eq!(work_items[0].status, coldstart_core::WorkItemStatus::Ready);
    let database_path = fixture.database_path.clone();
    let project_id = fixture.project_id;
    drop(fixture.database);

    let reopened = Database::open(&database_path).expect("database must reopen");
    assert_eq!(
        reopened
            .list_work_items(&project_id)
            .expect("work items must reload"),
        work_items
    );
    assert!(
        reopened
            .latest_plan(&project_id, coldstart_core::PlanStatus::Published)
            .expect("plan query must work")
            .is_some()
    );
    let _ = Utc::now();
}
