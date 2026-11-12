use chrono::Utc;
use coldstart_core::{
    BlueprintRevisionId, DomainEvent, DomainEventKind, LeaseId, PlanRevisionId, Project, ProjectId,
    WorkItemId,
};
use coldstart_storage::Database;
use tempfile::TempDir;

fn open_database(directory: &TempDir) -> Database {
    Database::open(&directory.path().join("state.db")).expect("database must open")
}

fn sample_project(directory: &TempDir, name: &str) -> Project {
    Project::new(name, directory.path().join("project-home"))
        .expect("project fixture must be valid")
}

fn append(database: &mut Database, project_id: ProjectId, kind: DomainEventKind) -> DomainEvent {
    let event = DomainEvent::new(project_id, kind);
    database.append_event(&event).expect("event must append");
    event
}

#[test]
fn lists_events_newest_first_with_a_stable_cursor() {
    let directory = TempDir::new().expect("temporary directory must exist");
    let project = sample_project(&directory, "Event Pagination");
    let mut database = open_database(&directory);
    database
        .initialize_project(&project)
        .expect("project must initialize");

    let published = append(
        &mut database,
        project.id,
        DomainEventKind::BlueprintPublished {
            revision_id: BlueprintRevisionId::new(),
            revision_number: 2,
        },
    );
    let heartbeat = append(
        &mut database,
        project.id,
        DomainEventKind::WorkHeartbeat {
            work_item_id: WorkItemId::new(),
            lease_id: LeaseId::new(),
            expires_at: Utc::now(),
        },
    );
    let plan = append(
        &mut database,
        project.id,
        DomainEventKind::PlanPublished {
            revision_id: PlanRevisionId::new(),
            revision_number: 1,
            task_count: 3,
        },
    );

    let first_page = database
        .list_events(&project.id, None, 2, true)
        .expect("first page must load");

    assert_eq!(first_page.events.len(), 2);
    assert_eq!(first_page.events[0].event.event_id, plan.event_id);
    assert_eq!(first_page.events[1].event.event_id, heartbeat.event_id);
    assert_eq!(first_page.events[1].event.kind, heartbeat.kind);
    let cursor = first_page
        .next_cursor
        .expect("first page must expose a cursor");
    assert_eq!(cursor, first_page.events[1].seq);

    let second_page = database
        .list_events(&project.id, Some(cursor), 2, true)
        .expect("second page must load");

    assert_eq!(second_page.events.len(), 2);
    assert_eq!(second_page.events[0].event.event_id, published.event_id);
    assert_eq!(
        second_page.events[1].event.kind,
        project_initialized_kind(&project)
    );
    assert_eq!(second_page.next_cursor, None);
}

fn project_initialized_kind(project: &Project) -> DomainEventKind {
    DomainEventKind::ProjectInitialized {
        name: project.name.clone(),
    }
}

#[test]
fn event_pages_hide_heartbeats_and_never_cross_projects() {
    let directory = TempDir::new().expect("temporary directory must exist");
    let project = sample_project(&directory, "Primary Project");
    let other = sample_project(&directory, "Other Project");
    let mut database = open_database(&directory);
    database
        .initialize_project(&project)
        .expect("primary project must initialize");
    database
        .initialize_project(&other)
        .expect("other project must initialize");

    let work_item_id = WorkItemId::new();
    let heartbeat = append(
        &mut database,
        project.id,
        DomainEventKind::WorkHeartbeat {
            work_item_id,
            lease_id: LeaseId::new(),
            expires_at: Utc::now(),
        },
    );
    append(
        &mut database,
        other.id,
        DomainEventKind::PlanPublished {
            revision_id: PlanRevisionId::new(),
            revision_number: 7,
            task_count: 9,
        },
    );

    let primary = database
        .list_events(&project.id, None, 100, false)
        .expect("primary events must load");
    let with_heartbeats = database
        .list_events(&project.id, None, 100, true)
        .expect("events with heartbeats must load");
    let other_page = database
        .list_events(&other.id, None, 100, false)
        .expect("other events must load");

    assert_eq!(primary.events.len(), 1);
    assert_eq!(
        primary.events[0].event.kind,
        project_initialized_kind(&project)
    );
    assert_eq!(with_heartbeats.events.len(), 2);
    assert_eq!(with_heartbeats.events[0].event.kind, heartbeat.kind);
    assert_eq!(other_page.events.len(), 2);
    assert!(
        other_page
            .events
            .iter()
            .all(|stored| stored.event.project_id == other.id)
    );
}

#[test]
fn counts_events_by_type_for_one_project_only() {
    let directory = TempDir::new().expect("temporary directory must exist");
    let project = sample_project(&directory, "Counted Project");
    let other = sample_project(&directory, "Other Project");
    let mut database = open_database(&directory);
    database
        .initialize_project(&project)
        .expect("project must initialize");
    database
        .initialize_project(&other)
        .expect("other project must initialize");
    append(
        &mut database,
        project.id,
        DomainEventKind::WorkHeartbeat {
            work_item_id: WorkItemId::new(),
            lease_id: LeaseId::new(),
            expires_at: Utc::now(),
        },
    );
    append(
        &mut database,
        project.id,
        DomainEventKind::PlanPublished {
            revision_id: PlanRevisionId::new(),
            revision_number: 1,
            task_count: 2,
        },
    );

    let counts = database
        .event_type_counts(&project.id)
        .expect("counts must load");

    assert_eq!(
        count_for(&counts, "project_initialized"),
        1,
        "counts must include the initialization event"
    );
    assert_eq!(count_for(&counts, "plan_published"), 1);
    assert_eq!(count_for(&counts, "work_heartbeat"), 1);
    assert_eq!(
        counts.iter().map(|entry| entry.count).sum::<u64>(),
        3,
        "other project events must not leak into counts"
    );
}

fn count_for(counts: &[coldstart_storage::EventTypeCount], event_type: &str) -> u64 {
    counts
        .iter()
        .find(|entry| entry.event_type == event_type)
        .map(|entry| entry.count)
        .unwrap_or_default()
}
