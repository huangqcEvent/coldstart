mod support;

use chrono::{Duration, Utc};
use coldstart_core::{
    ClaimResult, DomainEventKind, PullRequestCheck, PullRequestCheckConclusion,
    PullRequestCheckStatus, PullRequestInsights, PullRequestMergeability, PullRequestRecord,
    PullRequestReview, PullRequestReviewComment, PullRequestReviewState, PullRequestState,
};
use support::{PlanFixture, agent_task, report_for};

#[test]
fn records_a_pull_request_and_its_state_changes_with_project_audit_events() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation-demo", &[])]);
    assert_eq!(fixture.database.schema_version().expect("schema"), 9);
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

    let insights = PullRequestInsights::new(
        pull_request.id,
        vec![PullRequestCheck {
            name: "unit tests".to_owned(),
            status: PullRequestCheckStatus::Completed,
            conclusion: Some(PullRequestCheckConclusion::Success),
            details_url: Some("https://github.com/octocat/demo/runs/1".to_owned()),
            app_name: Some("GitHub Actions".to_owned()),
        }],
        vec![PullRequestReview {
            author: "reviewer".to_owned(),
            state: PullRequestReviewState::Approved,
            body: "可以合并。".to_owned(),
            html_url: Some(
                "https://github.com/octocat/demo/pull/17#pullrequestreview-1".to_owned(),
            ),
        }],
        vec![PullRequestReviewComment {
            author: "reviewer".to_owned(),
            path: "src/lib.rs".to_owned(),
            line: Some(12),
            body: "建议补充注释。".to_owned(),
            html_url: Some("https://github.com/octocat/demo/pull/17#discussion_r1".to_owned()),
        }],
        Utc::now(),
    )
    .expect("insights must validate")
    .with_mergeability(PullRequestMergeability::Conflicted);
    assert!(
        fixture
            .database
            .save_pull_request_insights(&insights)
            .expect("insights must save")
    );
    assert_eq!(
        fixture
            .database
            .load_pull_request_insights(&fixture.project_id, &pull_request.id)
            .expect("insights must load"),
        Some(insights.clone())
    );

    let mut unchanged_refresh = insights.clone();
    unchanged_refresh.synced_at += Duration::minutes(1);
    assert!(
        !fixture
            .database
            .save_pull_request_insights(&unchanged_refresh)
            .expect("unchanged refresh must save")
    );
    let events = fixture
        .database
        .list_events(&fixture.project_id, None, 100, true)
        .expect("events must reload");
    assert_eq!(
        events
            .events
            .iter()
            .filter(|event| matches!(
                event.event.kind,
                DomainEventKind::PullRequestInsightsSynced { number: 17, .. }
            ))
            .count(),
        1,
        "an unchanged refresh must not create a duplicate audit event"
    );

    unchanged_refresh.checks[0].conclusion = Some(PullRequestCheckConclusion::Failure);
    assert!(
        fixture
            .database
            .save_pull_request_insights(&unchanged_refresh)
            .expect("changed insights must save")
    );
    let events = fixture
        .database
        .list_events(&fixture.project_id, None, 100, true)
        .expect("events must reload");
    assert!(events.events.iter().any(|event| matches!(
        &event.event.kind,
        DomainEventKind::PullRequestInsightsSynced {
            number: 17,
            checks_state,
            checks_failed: 1,
            ..
        } if checks_state == "failure"
    )));

    let database_path = fixture.database_path.clone();
    drop(fixture.database);
    let connection = rusqlite::Connection::open(database_path).expect("database must reopen");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("foreign keys must enable");
    connection
        .execute(
            "DELETE FROM pull_requests WHERE id = ?1",
            [pull_request.id.to_string()],
        )
        .expect("pull request must delete");
    let remaining: i64 = connection
        .query_row("SELECT COUNT(*) FROM pull_request_insights", [], |row| {
            row.get(0)
        })
        .expect("insight count must load");
    assert_eq!(
        remaining, 0,
        "deleting a PR must cascade its insight snapshot"
    );
}
