mod support;

use chrono::{Duration, Utc};
use coldstart_core::{
    ClaimResult, ContractState, DependencyKind, GitSyncRecord, GitSyncStatus, PlanStatus,
    TaskDependency, WorkItemStatus,
};
use coldstart_storage::StorageError;
use rusqlite::Connection;
use support::{PlanFixture, agent_task, human_task, report_for};

#[test]
fn initial_plan_preview_lists_every_task_as_added() {
    let mut fixture = PlanFixture::draft(vec![agent_task("foundation:demo", &[])]);

    let preview = fixture
        .database
        .preview_plan_revision(&fixture.project_id, &fixture.plan_id, Utc::now())
        .expect("initial preview must load");

    assert_eq!(preview.base_revision_id, None);
    assert_eq!(preview.added.len(), 1);
    assert!(preview.changed.is_empty());
    assert!(preview.unchanged.is_empty());
    assert!(preview.removed.is_empty());
    assert_eq!(preview.carried_completed, 0);
    assert_eq!(preview.reopened, 0);
    assert_eq!(preview.active_lease_count, 0);
    assert!(preview.publishable);
    assert!(preview.blockers.is_empty());
}

#[test]
fn replacing_a_plan_inherits_done_reopens_changed_and_cancels_removed() {
    let now = Utc::now();
    let mut done = agent_task("done-stable", &[]);
    done.priority = 300;
    let mut changed = agent_task("changed", &[]);
    changed.priority = 200;
    let mut removed = agent_task("removed", &[]);
    removed.priority = 100;
    {
        let mut fixture = PlanFixture::new(vec![done.clone(), changed.clone(), removed.clone()]);
        let claimed = fixture
            .database
            .claim_work(
                &fixture.project_id,
                "codex",
                Some("server"),
                "complete-done-stable",
                now,
                Duration::minutes(30),
            )
            .expect("claim must call");
        let ClaimResult::Claimed(claimed) = claimed else {
            panic!("done-stable must be claimed");
        };
        assert_eq!(claimed.work_item.task_key, "done-stable");
        let task = fixture.task("done-stable");
        fixture
            .database
            .submit_work_report(
                &claimed.work_item.id,
                &claimed.lease_token,
                report_for(&task, "complete-stable"),
                now + Duration::minutes(1),
                Duration::minutes(30),
            )
            .expect("done-stable report must be accepted");
        let old_item = fixture
            .database
            .load_work_item(&claimed.work_item.id)
            .expect("work item query must pass")
            .expect("work item must exist");
        let repository = fixture
            .database
            .repository_by_key(&fixture.project_id, "server")
            .expect("repository query must work")
            .expect("fixture repository must exist");
        fixture
            .database
            .record_git_sync(&GitSyncRecord::new(
                fixture.project_id,
                old_item.id,
                repository.id,
                "codex/done-stable".to_owned(),
                "0123456789abcdef0123456789abcdef01234567".to_owned(),
                GitSyncStatus::Pushed,
                None,
            ))
            .expect("pushed branch must persist");
        let old_item = fixture
            .database
            .mark_work_integrated(
                &fixture.project_id,
                &old_item.id,
                "PR #12",
                now + Duration::minutes(1),
            )
            .expect("completed task must integrate");

        let mut changed_new = changed.clone();
        changed_new.goal = "实现已经变化的新契约".to_owned();
        changed_new.dependencies = vec![TaskDependency {
            task_key: "done-stable".to_owned(),
            kind: DependencyKind::Blocks,
        }];
        let candidate =
            fixture.create_plan(vec![done.clone(), changed_new, agent_task("added", &[])]);
        let preview = fixture
            .database
            .preview_plan_revision(
                &fixture.project_id,
                &candidate.id,
                now + Duration::minutes(2),
            )
            .expect("replan preview must load");
        assert_eq!(preview.added.len(), 1);
        assert_eq!(preview.changed.len(), 1);
        assert_eq!(preview.unchanged.len(), 1);
        assert_eq!(preview.removed.len(), 1);
        assert_eq!(preview.carried_completed, 1);
        assert_eq!(preview.reopened, 1);
        assert_eq!(preview.cancelled, 1);
        assert_eq!(preview.changed[0].task_key, "changed");
        assert_eq!(preview.added[0].task_key, "added");
        assert!(preview.publishable);

        fixture
            .database
            .publish_plan_revision(
                &fixture.project_id,
                &candidate.id,
                now + Duration::minutes(2),
            )
            .expect("replacement plan must publish");
        let published = fixture
            .database
            .latest_plan(&fixture.project_id, PlanStatus::Published)
            .expect("plan query must pass")
            .expect("published plan must exist");
        assert_eq!(published.id, candidate.id);
        let plans = fixture
            .database
            .list_plans(&fixture.project_id)
            .expect("plans must load");
        assert_eq!(
            plans
                .iter()
                .filter(|plan| plan.status == PlanStatus::Published)
                .count(),
            1
        );
        assert_eq!(
            plans
                .iter()
                .find(|plan| plan.id == fixture.plan_id)
                .expect("old plan must remain")
                .status,
            PlanStatus::Superseded
        );

        let current = fixture
            .database
            .list_work_items_for_plan(&fixture.project_id, &candidate.id)
            .expect("current work items must load");
        let current_done = current
            .iter()
            .find(|item| item.task_key == "done-stable")
            .expect("stable task must exist");
        assert_eq!(current_done.status, WorkItemStatus::Done);
        assert_eq!(current_done.completed_at, old_item.completed_at);
        assert_eq!(current_done.integrated_at, old_item.integrated_at);
        assert_eq!(
            current_done.integration_reference.as_deref(),
            Some("PR #12")
        );
        assert_eq!(
            current
                .iter()
                .find(|item| item.task_key == "changed")
                .expect("changed task must exist")
                .status,
            WorkItemStatus::Ready
        );
        assert!(
            current.iter().all(|item| item.task_key != "removed"),
            "removed tasks must not appear in the active plan"
        );

        let old_items = fixture
            .database
            .list_work_items_for_plan(&fixture.project_id, &fixture.plan_id)
            .expect("old work items must load");
        let old_changed = old_items
            .iter()
            .find(|item| item.task_key == "changed")
            .expect("old changed task must remain");
        assert_eq!(old_changed.contract_state, ContractState::Stale);
        let old_removed = old_items
            .iter()
            .find(|item| item.task_key == "removed")
            .expect("old removed task must remain for audit");
        assert_eq!(old_removed.status, WorkItemStatus::Cancelled);
        assert_eq!(old_removed.contract_state, ContractState::Stale);
    }
}

#[test]
fn active_lease_blocks_replanning_until_it_expires() {
    let now = Utc::now();
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let claimed = fixture
        .database
        .claim_work(
            &fixture.project_id,
            "codex",
            Some("server"),
            "block-replan",
            now,
            Duration::minutes(30),
        )
        .expect("claim must call");
    assert!(matches!(claimed, ClaimResult::Claimed(_)));
    let candidate = fixture.create_plan(vec![agent_task("foundation:demo", &[])]);

    let preview = fixture
        .database
        .preview_plan_revision(
            &fixture.project_id,
            &candidate.id,
            now + Duration::minutes(1),
        )
        .expect("preview must load");
    assert_eq!(preview.active_lease_count, 1);
    assert!(!preview.publishable);
    assert!(!preview.blockers.is_empty());
    let error = fixture
        .database
        .publish_plan_revision(
            &fixture.project_id,
            &candidate.id,
            now + Duration::minutes(1),
        )
        .expect_err("active lease must block publish");
    assert!(matches!(error, StorageError::PlanLeaseActive));
    assert_eq!(
        fixture
            .database
            .load_plan(&fixture.plan_id)
            .expect("plan query must pass")
            .expect("old plan must exist")
            .status,
        PlanStatus::Published
    );

    fixture
        .database
        .publish_plan_revision(
            &fixture.project_id,
            &candidate.id,
            now + Duration::minutes(31),
        )
        .expect("expired lease must allow replanning");
}

#[test]
fn superseded_work_cannot_report_or_complete_after_replanning() {
    let now = Utc::now();
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let claimed = fixture
        .database
        .claim_work(
            &fixture.project_id,
            "codex",
            Some("server"),
            "old-report",
            now,
            Duration::minutes(30),
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("work must be claimed");
    };
    fixture
        .database
        .release_work(
            &claimed.work_item.id,
            &claimed.lease_token,
            "replan",
            now + Duration::minutes(1),
            Duration::minutes(30),
        )
        .expect("lease must release");
    let task = fixture.task("foundation:demo");
    let candidate = fixture.create_plan(vec![task.clone()]);
    fixture
        .database
        .publish_plan_revision(
            &fixture.project_id,
            &candidate.id,
            now + Duration::minutes(2),
        )
        .expect("replacement must publish");

    let error = fixture
        .database
        .submit_work_report(
            &claimed.work_item.id,
            &claimed.lease_token,
            report_for(&task, "old-report-submit"),
            now + Duration::minutes(3),
            Duration::minutes(30),
        )
        .expect_err("superseded work must not report");
    assert!(matches!(error, StorageError::PlanNoLongerActive(_)));
}

#[test]
fn superseded_human_work_cannot_complete_after_replanning() {
    let now = Utc::now();
    let mut fixture = PlanFixture::new(vec![human_task("release:submit")]);
    let old_item = fixture
        .database
        .list_work_items(&fixture.project_id)
        .expect("work items must load")
        .into_iter()
        .next()
        .expect("human task must exist");
    let candidate = fixture.create_plan(vec![human_task("release:submit")]);
    fixture
        .database
        .publish_plan_revision(
            &fixture.project_id,
            &candidate.id,
            now + Duration::minutes(1),
        )
        .expect("replacement must publish");

    let error = fixture
        .database
        .complete_human_work(
            &fixture.project_id,
            &old_item.id,
            "old completion",
            true,
            now + Duration::minutes(2),
        )
        .expect_err("superseded human work must not complete");
    assert!(matches!(error, StorageError::PlanNoLongerActive(_)));
}

#[test]
fn a_stale_contract_cannot_heartbeat_even_with_a_live_lease() {
    let now = Utc::now();
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let claimed = fixture
        .database
        .claim_work(
            &fixture.project_id,
            "codex",
            Some("server"),
            "stale-contract",
            now,
            Duration::minutes(30),
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("work must be claimed");
    };
    let connection = Connection::open(&fixture.database_path).expect("tamper connection must open");
    connection
        .execute(
            "UPDATE work_items SET contract_state = 'stale' WHERE id = ?1",
            [claimed.work_item.id.to_string()],
        )
        .expect("contract state must update");
    drop(connection);

    let error = fixture
        .database
        .heartbeat_work(
            &claimed.work_item.id,
            &claimed.lease_token,
            now + Duration::minutes(1),
            Duration::minutes(30),
        )
        .expect_err("stale contract must not heartbeat");
    assert!(matches!(error, StorageError::ContractStale(_)));
}
