mod support;

use chrono::{Duration, Utc};
use coldstart_core::{ClaimResult, DomainEventKind, PullRequestRecord, PullRequestState};
use support::{PlanFixture, agent_task, report_for};

#[test]
fn records_a_pull_request_and_its_state_changes_with_project_audit_events() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation-demo", &[])]);
    assert_eq!(fixture.database.schema_version().expect("schema"), 8);
    let claimed = fixture
        .database
        .claim_work(
            &fixture.project_id,
            "codex",
            Some("server"),
            "claim-pr",
            Utc::now(),
            Duration::minutes(30),
        )
        .expect("claim must run");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("work must be claimed");
    };
    let task = fixture.task("foundation-demo");
    let report = fixture
        .database
        .submit_work_report(
            &claimed.work_item.id,
            &claimed.lease_token,
            report_for(&task, "report-pr"),
            Utc::now(),
            Duration::minutes(30),
        )
        .expect("report must submit");
    let repository = fixture
        .database
        .list_repositories(&fixture.project_id)
        .expect("repositories must load")
        .into_iter()
        .find(|repository| repository.key == "server")
        .expect("server repository must exist");
    let branch = task.work_branch.clone().expect("task branch must exist");

    let mut pull_request = PullRequestRecord::new(
        fixture.project_id,
        claimed.work_item.id,
        report.id,
        repository.id,
        17,
        "https://github.com/octocat/demo/pull/17".to_owned(),
        "[foundation-demo] 实现任务".to_owned(),
        branch,
        "main".to_owned(),
        PullRequestState::Open,
        true,
    )
    .expect("pull request must be valid");
    fixture
        .database
        .save_pull_request(&pull_request)
        .expect("pull request must save");

    let stored = fixture
        .database
        .load_pull_request(&fixture.project_id, &pull_request.id)
        .expect("pull request must load")
        .expect("pull request must exist");
    assert_eq!(stored, pull_request);

    assert!(
        pull_request
            .update_state(PullRequestState::Merged, false)
            .expect("state must validate")
    );
    fixture
        .database
        .save_pull_request(&pull_request)
        .expect("state update must save");
    let stored = fixture
        .database
        .list_pull_requests(&fixture.project_id)
        .expect("pull requests must list");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].state, PullRequestState::Merged);

    let events = fixture
        .database
        .list_events(&fixture.project_id, None, 100, true)
        .expect("events must load");
    assert!(events.events.iter().any(|event| matches!(
        &event.event.kind,
        DomainEventKind::PullRequestOpened { number: 17, .. }
    )));
    assert!(events.events.iter().any(|event| matches!(
        &event.event.kind,
        DomainEventKind::PullRequestStateChanged {
            number: 17,
            state,
            ..
        } if state == "merged"
    )));
}
