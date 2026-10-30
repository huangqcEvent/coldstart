mod support;

use chrono::{Duration, Utc};
use coldstart_core::{ClaimResult, WorkItemStatus};
use coldstart_storage::StorageError;
use support::{PlanFixture, agent_task, report_for};

#[test]
fn accepts_a_complete_report_once_and_unlocks_the_next_task() {
    let mut fixture = PlanFixture::new(vec![
        agent_task("foundation:one", &[]),
        agent_task("slice:next", &["foundation:one"]),
    ]);
    let claimed = fixture
        .database
        .claim_work(
            &fixture.project_id,
            "codex",
            Some("server"),
            "claim-1",
            Utc::now(),
            Duration::minutes(30),
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("first work must be claimed");
    };
    let report = report_for(&fixture.task("foundation:one"), "report-1");
    let first = fixture
        .database
        .submit_work_report(
            &claimed.work_item.id,
            &claimed.lease_token,
            report.clone(),
            Utc::now(),
            Duration::minutes(30),
        )
        .expect("report must submit");
    let second = fixture
        .database
        .submit_work_report(
            &claimed.work_item.id,
            &claimed.lease_token,
            report,
            Utc::now(),
            Duration::minutes(30),
        )
        .expect("idempotent report must return the original");
    assert_eq!(first.id, second.id);

    let items = fixture
        .database
        .list_work_items(&fixture.project_id)
        .expect("work items must load");
    assert_eq!(
        items
            .iter()
            .find(|item| item.task_key == "foundation:one")
            .unwrap()
            .status,
        WorkItemStatus::Done
    );
    assert_eq!(
        items
            .iter()
            .find(|item| item.task_key == "slice:next")
            .unwrap()
            .status,
        WorkItemStatus::Ready
    );
}

#[test]
fn rejects_changed_files_outside_the_contract_paths() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let claimed = fixture
        .database
        .claim_work(
            &fixture.project_id,
            "codex",
            Some("server"),
            "claim-path",
            Utc::now(),
            Duration::minutes(30),
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("work must be claimed");
    };
    let mut report = report_for(&fixture.task("foundation:demo"), "report-path");
    report.changed_files = vec!["C:/outside.txt".to_owned()];
    let error = fixture
        .database
        .submit_work_report(
            &claimed.work_item.id,
            &claimed.lease_token,
            report,
            Utc::now(),
            Duration::minutes(30),
        )
        .expect_err("outside path must fail");
    assert!(matches!(error, StorageError::ReportRejected(_)));
    assert!(error.to_string().contains("unsafe changed file path"));

    let item = fixture
        .database
        .load_work_item(&claimed.work_item.id)
        .expect("work item must load")
        .expect("work item must exist");
    assert_eq!(item.status, WorkItemStatus::InProgress);
}

#[test]
fn completes_a_human_task_and_unlocks_its_dependent() {
    let mut fixture = PlanFixture::new(vec![
        support::human_task("prep:account"),
        agent_task("foundation:demo", &["prep:account"]),
    ]);
    let human = fixture
        .database
        .list_work_items(&fixture.project_id)
        .expect("work items must load")
        .into_iter()
        .find(|item| item.task_key == "prep:account")
        .expect("human task must exist");
    fixture
        .database
        .complete_human_work(
            &fixture.project_id,
            &human.id,
            "账号已准备",
            true,
            Utc::now(),
        )
        .expect("human task must complete");
    let items = fixture
        .database
        .list_work_items(&fixture.project_id)
        .expect("work items must load");
    assert_eq!(
        items
            .iter()
            .find(|item| item.task_key == "foundation:demo")
            .unwrap()
            .status,
        WorkItemStatus::Ready
    );
}
