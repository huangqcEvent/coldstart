mod support;

use std::sync::{Arc, Barrier};

use chrono::{Duration, Utc};
use coldstart_core::{ClaimResult, WorkItemStatus};
use coldstart_storage::Database;
use support::{PlanFixture, agent_task};

#[test]
fn only_one_of_two_concurrent_database_handles_can_claim_the_same_work() {
    let fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for index in 0..2 {
        let path = fixture.database_path.clone();
        let project_id = fixture.project_id;
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let mut database = Database::open(&path).expect("database must open");
            barrier.wait();
            database
                .claim_work(
                    &project_id,
                    "codex",
                    Some("server"),
                    &format!("request-{index}"),
                    Utc::now(),
                    Duration::minutes(30),
                )
                .expect("claim call must complete")
        }));
    }
    barrier.wait();
    let successes = handles
        .into_iter()
        .map(|handle| handle.join().expect("claim thread must join"))
        .filter(|result| matches!(result, ClaimResult::Claimed(_)))
        .count();
    assert_eq!(successes, 1);
}

#[test]
fn expired_lease_returns_work_to_ready_without_completing_it() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let claimed_at = Utc::now() - Duration::minutes(31);
    let claimed = fixture
        .database
        .claim_work(
            &fixture.project_id,
            "codex",
            Some("server"),
            "claim-expired",
            claimed_at,
            Duration::minutes(30),
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("work must be claimed");
    };
    let expired = fixture
        .database
        .expire_leases(&fixture.project_id, Utc::now())
        .expect("lease sweep must pass");
    assert_eq!(expired, 1);
    let item = fixture
        .database
        .load_work_item(&claimed.work_item.id)
        .expect("work item must load")
        .expect("work item must exist");
    assert_eq!(item.status, WorkItemStatus::Ready);
}

#[test]
fn heartbeat_rejects_an_invalid_token_and_extends_a_valid_lease() {
    let mut fixture = PlanFixture::new(vec![agent_task("foundation:demo", &[])]);
    let claimed = fixture
        .database
        .claim_work(
            &fixture.project_id,
            "codex",
            Some("server"),
            "claim-heartbeat",
            Utc::now(),
            Duration::minutes(30),
        )
        .expect("claim must call");
    let ClaimResult::Claimed(claimed) = claimed else {
        panic!("work must be claimed");
    };
    let invalid = fixture
        .database
        .heartbeat_work(
            &claimed.work_item.id,
            "not-the-token",
            Utc::now(),
            Duration::minutes(30),
        )
        .expect_err("invalid token must fail");
    assert!(matches!(
        invalid,
        coldstart_storage::StorageError::LeaseInvalid(_)
    ));

    let heartbeat_at = Utc::now() + Duration::minutes(5);
    let lease = fixture
        .database
        .heartbeat_work(
            &claimed.work_item.id,
            &claimed.lease_token,
            heartbeat_at,
            Duration::minutes(30),
        )
        .expect("heartbeat must pass");
    assert_eq!(lease.last_heartbeat_at, heartbeat_at);
    assert_eq!(lease.expires_at, heartbeat_at + Duration::minutes(30));
    let item = fixture
        .database
        .load_work_item(&claimed.work_item.id)
        .expect("work item must load")
        .expect("work item must exist");
    assert_eq!(item.status, WorkItemStatus::InProgress);
}
