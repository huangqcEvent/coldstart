//! 多仓库交付总览聚合。
//!
//! 直接读取本地已保存的计划、任务、验证、Git 同步、PR 和 Review 快照，按仓库
//! 汇总风险与下一步建议；不发起网络请求、不写事件，也不改变外部状态。

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use chrono::{DateTime, Utc};
use coldstart_core::{
    GitSyncRecord, GitSyncStatus, ProjectId, PullRequestChecksState, PullRequestInsights,
    PullRequestMergeability, PullRequestRecord, PullRequestReviewState, PullRequestState,
    REPOSITORY_KEYS, RepositoryId, ReviewStatus, TaskKind, WorkItemStatus, WorkReportId,
};
use serde::{Deserialize, Serialize};

use crate::{AppError, context::OpenProject, plan_workspace::build_plan_workspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryRepositoryState {
    Attention,
    Active,
    Ready,
    Complete,
    Idle,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryTaskCounts {
    pub total: usize,
    pub done: usize,
    pub ready: usize,
    pub active: usize,
    pub blocked: usize,
    pub failed: usize,
    pub submitted: usize,
    pub reopened: usize,
    pub cancelled: usize,
    pub draft: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryGitSyncView {
    pub status: GitSyncStatus,
    pub branch: String,
    pub commit_sha: String,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryChecksView {
    pub state: PullRequestChecksState,
    pub total: usize,
    pub failed: usize,
    pub pending: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryPullRequestView {
    pub number: u64,
    pub title: String,
    pub html_url: String,
    pub state: PullRequestState,
    pub draft: bool,
    pub checks: Option<DeliveryChecksView>,
    pub mergeability: Option<PullRequestMergeability>,
    pub conflict_path_count: usize,
    pub changes_requested_review_count: usize,
    pub review_status: Option<ReviewStatus>,
    pub synced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDeliveryView {
    pub repository_key: String,
    pub repository_name: String,
    pub remote_url: String,
    pub default_branch: String,
    pub state: DeliveryRepositoryState,
    pub tasks: DeliveryTaskCounts,
    pub verification_command_count: usize,
    pub latest_git_sync: Option<DeliveryGitSyncView>,
    pub pull_request: Option<DeliveryPullRequestView>,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryOverviewView {
    pub project_id: ProjectId,
    pub plan_revision_number: Option<u64>,
    pub repositories: Vec<RepositoryDeliveryView>,
    pub unassigned_agent_task_count: usize,
}

#[derive(Debug, Clone)]
struct DeliveryRepositorySource {
    id: RepositoryId,
    key: String,
    name: String,
    remote_url: String,
    default_branch: String,
    verification_command_count: usize,
}

#[derive(Debug, Clone)]
struct DeliveryTaskSource {
    repository_key: Option<String>,
    kind: TaskKind,
    status: WorkItemStatus,
}

#[derive(Debug, Clone)]
struct SelectedDeliveryPullRequest {
    record: PullRequestRecord,
    insights: Option<PullRequestInsights>,
    review_status: Option<ReviewStatus>,
}

#[derive(Clone)]
pub struct DeliveryOverviewService;

impl DeliveryOverviewService {
    pub fn new() -> Self {
        Self
    }

    pub fn load(&self, project_home: PathBuf) -> Result<DeliveryOverviewView, AppError> {
        let project = OpenProject::open(project_home)?;
        let plan = build_plan_workspace(&project)?;
        let repositories = project.database.list_repositories(&project.project.id)?;
        let repositories = repositories
            .into_iter()
            .map(|repository| {
                Ok(DeliveryRepositorySource {
                    id: repository.id,
                    verification_command_count: project
                        .database
                        .list_repository_verification_commands(
                            &project.project.id,
                            &repository.key,
                        )?
                        .len(),
                    key: repository.key,
                    name: repository.name,
                    remote_url: repository.remote_url,
                    default_branch: repository.default_branch,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;

        let tasks = plan
            .work_items
            .iter()
            .map(|view| DeliveryTaskSource {
                repository_key: view.task.repository_key.clone(),
                kind: view.task.kind,
                status: view.work_item.status,
            })
            .collect::<Vec<_>>();
        let git_syncs = project.database.list_git_syncs(&project.project.id)?;
        let pull_requests = project.database.list_pull_requests(&project.project.id)?;
        let selected_pull_requests = select_pull_requests(&pull_requests);
        let report_reviews = plan
            .reports
            .iter()
            .map(|report| (report.id, report.review_status))
            .collect::<BTreeMap<WorkReportId, ReviewStatus>>();

        let mut selected = Vec::with_capacity(selected_pull_requests.len());
        for record in selected_pull_requests.into_values() {
            let insights = project
                .database
                .load_pull_request_insights(&project.project.id, &record.id)?;
            let review_status = report_reviews
                .get(&record.report_id)
                .copied()
                .filter(|status| *status != ReviewStatus::NotRequired);
            selected.push(SelectedDeliveryPullRequest {
                record,
                insights,
                review_status,
            });
        }

        Ok(aggregate_delivery_overview(
            project.project.id,
            plan.published.as_ref().map(|plan| plan.revision_number),
            repositories,
            tasks,
            git_syncs,
            selected,
        ))
    }
}

impl Default for DeliveryOverviewService {
    fn default() -> Self {
        Self::new()
    }
}

fn select_pull_requests(
    records: &[PullRequestRecord],
) -> BTreeMap<RepositoryId, PullRequestRecord> {
    let mut latest = BTreeMap::new();
    let mut latest_open = BTreeMap::new();
    for record in records {
        let replace = latest
            .get(&record.repository_id)
            .is_none_or(|current: &PullRequestRecord| record.created_at > current.created_at);
        if replace {
            latest.insert(record.repository_id, record.clone());
        }
        if record.state == PullRequestState::Open {
            let replace = latest_open
                .get(&record.repository_id)
                .is_none_or(|current: &PullRequestRecord| record.created_at > current.created_at);
            if replace {
                latest_open.insert(record.repository_id, record.clone());
            }
        }
    }
    latest.extend(latest_open);
    latest
}

fn aggregate_delivery_overview(
    project_id: ProjectId,
    plan_revision_number: Option<u64>,
    repositories: Vec<DeliveryRepositorySource>,
    tasks: Vec<DeliveryTaskSource>,
    git_syncs: Vec<GitSyncRecord>,
    pull_requests: Vec<SelectedDeliveryPullRequest>,
) -> DeliveryOverviewView {
    let configured_keys = repositories
        .iter()
        .map(|repository| repository.key.as_str())
        .collect::<BTreeSet<_>>();
    let mut counts_by_repository = repositories
        .iter()
        .map(|repository| (repository.key.clone(), DeliveryTaskCounts::default()))
        .collect::<BTreeMap<_, _>>();
    let mut unassigned_agent_task_count = 0usize;
    for task in tasks {
        let assigned_key = task
            .repository_key
            .as_deref()
            .filter(|key| configured_keys.contains(key));
        if let Some(key) = assigned_key {
            if let Some(counts) = counts_by_repository.get_mut(key) {
                count_task(counts, task.status);
            }
        } else if task.kind == TaskKind::AgentCoding && task.status != WorkItemStatus::Cancelled {
            unassigned_agent_task_count += 1;
        }
    }

    let mut latest_git_syncs = BTreeMap::<RepositoryId, &GitSyncRecord>::new();
    for sync in &git_syncs {
        let replace = latest_git_syncs
            .get(&sync.repository_id)
            .is_none_or(|current| sync.created_at > current.created_at);
        if replace {
            latest_git_syncs.insert(sync.repository_id, sync);
        }
    }
    let pull_requests = pull_requests
        .into_iter()
        .map(|pull_request| (pull_request.record.repository_id, pull_request))
        .collect::<BTreeMap<_, _>>();

    let repositories_by_key = repositories
        .into_iter()
        .map(|repository| (repository.key.clone(), repository))
        .collect::<BTreeMap<_, _>>();
    let mut views = Vec::with_capacity(REPOSITORY_KEYS.len());
    for repository_key in REPOSITORY_KEYS {
        let Some(repository) = repositories_by_key.get(repository_key) else {
            continue;
        };
        let tasks = counts_by_repository
            .remove(repository_key)
            .unwrap_or_default();
        let latest_git_sync =
            latest_git_syncs
                .get(&repository.id)
                .map(|sync| DeliveryGitSyncView {
                    status: sync.status,
                    branch: sync.branch.clone(),
                    commit_sha: sync.commit_sha.clone(),
                    error_message: sync.error_message.clone(),
                    created_at: sync.created_at,
                });
        let pull_request = pull_requests.get(&repository.id).map(pull_request_view);
        let state = repository_state(
            &tasks,
            repository.verification_command_count,
            latest_git_sync.as_ref(),
            pull_request.as_ref(),
        );
        let next_action = next_action(
            plan_revision_number,
            &tasks,
            repository.verification_command_count,
            latest_git_sync.as_ref(),
            pull_request.as_ref(),
        );
        views.push(RepositoryDeliveryView {
            repository_key: repository.key.clone(),
            repository_name: repository.name.clone(),
            remote_url: repository.remote_url.clone(),
            default_branch: repository.default_branch.clone(),
            state,
            tasks,
            verification_command_count: repository.verification_command_count,
            latest_git_sync,
            pull_request,
            next_action,
        });
    }

    DeliveryOverviewView {
        project_id,
        plan_revision_number,
        repositories: views,
        unassigned_agent_task_count,
    }
}

fn count_task(counts: &mut DeliveryTaskCounts, status: WorkItemStatus) {
    if status == WorkItemStatus::Cancelled {
        counts.cancelled += 1;
        return;
    }
    counts.total += 1;
    match status {
        WorkItemStatus::Draft => counts.draft += 1,
        WorkItemStatus::Ready => counts.ready += 1,
        WorkItemStatus::Claimed | WorkItemStatus::InProgress => counts.active += 1,
        WorkItemStatus::Submitted => counts.submitted += 1,
        WorkItemStatus::Done => counts.done += 1,
        WorkItemStatus::Blocked => counts.blocked += 1,
        WorkItemStatus::Failed => counts.failed += 1,
        WorkItemStatus::Reopened => counts.reopened += 1,
        WorkItemStatus::Cancelled => {}
    }
}

fn pull_request_view(pull_request: &SelectedDeliveryPullRequest) -> DeliveryPullRequestView {
    let checks = pull_request.insights.as_ref().map(|insights| {
        let summary = insights.check_summary();
        DeliveryChecksView {
            state: insights.checks_state(),
            total: summary.total,
            failed: summary.failed,
            pending: summary.pending,
        }
    });
    let conflict_path_count = pull_request
        .insights
        .as_ref()
        .map(|insights| insights.conflict_details.paths.len())
        .unwrap_or_default();
    let changes_requested_review_count = pull_request
        .insights
        .as_ref()
        .map(|insights| {
            insights
                .reviews
                .iter()
                .filter(|review| review.state == PullRequestReviewState::ChangesRequested)
                .count()
        })
        .unwrap_or_default();
    DeliveryPullRequestView {
        number: pull_request.record.number,
        title: pull_request.record.title.clone(),
        html_url: pull_request.record.html_url.clone(),
        state: pull_request.record.state,
        draft: pull_request.record.draft,
        checks,
        mergeability: pull_request
            .insights
            .as_ref()
            .map(|insights| insights.mergeability),
        conflict_path_count,
        changes_requested_review_count,
        review_status: pull_request.review_status,
        synced_at: pull_request
            .insights
            .as_ref()
            .map(|insights| insights.synced_at),
    }
}

fn repository_state(
    tasks: &DeliveryTaskCounts,
    verification_command_count: usize,
    latest_git_sync: Option<&DeliveryGitSyncView>,
    pull_request: Option<&DeliveryPullRequestView>,
) -> DeliveryRepositoryState {
    let open_pull_request =
        pull_request.filter(|pull_request| pull_request.state == PullRequestState::Open);
    let review_requires_attention = open_pull_request.is_some_and(|pull_request| {
        matches!(
            pull_request.review_status,
            Some(ReviewStatus::Blocked | ReviewStatus::Failed | ReviewStatus::Interrupted)
        ) || pull_request.changes_requested_review_count > 0
    });
    let pull_request_requires_attention = open_pull_request.is_some_and(|pull_request| {
        pull_request.mergeability == Some(PullRequestMergeability::Conflicted)
            || pull_request.conflict_path_count > 0
            || pull_request
                .checks
                .as_ref()
                .is_some_and(|checks| checks.state == PullRequestChecksState::Failure)
    });
    let review_or_pull_request_attention =
        review_requires_attention || pull_request_requires_attention;
    let task_attention = tasks.blocked > 0
        || tasks.failed > 0
        || tasks.reopened > 0
        || (tasks.total > 0 && tasks.done < tasks.total && verification_command_count == 0);
    if task_attention
        || latest_git_sync.is_some_and(|sync| sync.status == GitSyncStatus::Failed)
        || review_or_pull_request_attention
    {
        return DeliveryRepositoryState::Attention;
    }
    if tasks.active > 0 || tasks.submitted > 0 || open_pull_request.is_some() {
        return DeliveryRepositoryState::Active;
    }
    if tasks.ready > 0 || tasks.reopened > 0 || tasks.draft > 0 {
        return DeliveryRepositoryState::Ready;
    }
    if tasks.total > 0 && tasks.done == tasks.total {
        return DeliveryRepositoryState::Complete;
    }
    DeliveryRepositoryState::Idle
}

fn next_action(
    plan_revision_number: Option<u64>,
    tasks: &DeliveryTaskCounts,
    verification_command_count: usize,
    latest_git_sync: Option<&DeliveryGitSyncView>,
    pull_request: Option<&DeliveryPullRequestView>,
) -> String {
    let blocked_or_failed = tasks.blocked + tasks.failed;
    if blocked_or_failed > 0 {
        return format!("先处理 {blocked_or_failed} 个阻塞或失败任务");
    }
    if tasks.reopened > 0 {
        return format!("复查 {} 个已重开任务，再更新计划", tasks.reopened);
    }
    if latest_git_sync.is_some_and(|sync| sync.status == GitSyncStatus::Failed) {
        return "排查最近一次任务分支推送失败".to_owned();
    }
    if let Some(pull_request) = pull_request
        && pull_request.state == PullRequestState::Open
    {
        if pull_request.mergeability == Some(PullRequestMergeability::Conflicted)
            || pull_request.conflict_path_count > 0
        {
            return if pull_request.conflict_path_count > 0 {
                format!(
                    "先人工处理 PR #{} 的 {} 个冲突文件",
                    pull_request.number, pull_request.conflict_path_count
                )
            } else {
                format!("先人工核对 PR #{} 的合并冲突", pull_request.number)
            };
        }
        if pull_request
            .checks
            .as_ref()
            .is_some_and(|checks| checks.state == PullRequestChecksState::Failure)
        {
            let failed = pull_request
                .checks
                .as_ref()
                .map(|checks| checks.failed)
                .unwrap_or_default();
            return format!("检查 PR #{} 的 {failed} 个失败 Checks", pull_request.number);
        }
        if matches!(
            pull_request.review_status,
            Some(ReviewStatus::Blocked | ReviewStatus::Failed | ReviewStatus::Interrupted)
        ) {
            return format!("处理 PR #{} 的 AI Review 阻断", pull_request.number);
        }
        if pull_request.changes_requested_review_count > 0 {
            return format!(
                "处理 PR #{} 中 {} 条要求修改的 Review",
                pull_request.number, pull_request.changes_requested_review_count
            );
        }
        if pull_request
            .checks
            .as_ref()
            .is_some_and(|checks| checks.state == PullRequestChecksState::Pending)
        {
            return format!("等待 PR #{} 的 Checks 完成", pull_request.number);
        }
        if pull_request.mergeability == Some(PullRequestMergeability::Checking) {
            return format!(
                "等待 GitHub 完成 PR #{} 的合并状态计算",
                pull_request.number
            );
        }
        if pull_request.checks.is_none() || pull_request.mergeability.is_none() {
            return format!("在 AI 评审页刷新 PR #{} 的验收状态", pull_request.number);
        }
        if matches!(
            pull_request.mergeability,
            Some(
                PullRequestMergeability::Blocked
                    | PullRequestMergeability::Behind
                    | PullRequestMergeability::Unstable
            )
        ) {
            return format!(
                "人工核对 PR #{} 的合并阻塞、落后或稳定性问题",
                pull_request.number
            );
        }
        return format!(
            "前往 GitHub 人工评审 PR #{} 并决定是否合并",
            pull_request.number
        );
    }

    if tasks.total > 0 && verification_command_count == 0 {
        return "先为该仓库配置本地验证命令".to_owned();
    }
    if tasks.ready > 0 {
        return format!("Codex 可主动领取 {} 个任务", tasks.ready);
    }
    if tasks.active > 0 {
        return format!("监控 {} 个执行中任务", tasks.active);
    }
    if tasks.submitted > 0 {
        return format!("核对 {} 个待验收任务", tasks.submitted);
    }
    if tasks.total == 0 {
        return if plan_revision_number.is_some() {
            "当前计划暂无该仓库任务".to_owned()
        } else {
            "发布开发计划后查看仓库任务".to_owned()
        };
    }
    if tasks.done == tasks.total {
        return "仓库任务已完成，等待下一轮需求".to_owned();
    }
    "继续推进当前计划".to_owned()
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use coldstart_core::{
        PullRequestCheck, PullRequestCheckConclusion, PullRequestCheckStatus,
        PullRequestConflictDetails, PullRequestReview, PullRequestReviewState, WorkItemId,
        WorkReportId,
    };

    use super::*;

    fn repository(
        _project_id: ProjectId,
        key: &str,
        verification_command_count: usize,
    ) -> DeliveryRepositorySource {
        DeliveryRepositorySource {
            id: RepositoryId::new(),
            key: key.to_owned(),
            name: format!("{key} 仓库"),
            remote_url: format!("https://example.invalid/{key}.git"),
            default_branch: "main".to_owned(),
            verification_command_count,
        }
    }

    fn task(
        repository_key: Option<&str>,
        kind: TaskKind,
        status: WorkItemStatus,
    ) -> DeliveryTaskSource {
        DeliveryTaskSource {
            repository_key: repository_key.map(str::to_owned),
            kind,
            status,
        }
    }

    fn pull_request(
        project_id: ProjectId,
        repository_id: RepositoryId,
        number: u64,
        state: PullRequestState,
        created_at: DateTime<Utc>,
    ) -> PullRequestRecord {
        let mut record = PullRequestRecord::new(
            project_id,
            WorkItemId::new(),
            WorkReportId::new(),
            repository_id,
            number,
            format!("https://github.com/example/demo/pull/{number}"),
            format!("测试 PR {number}"),
            format!("codex/test-{number}"),
            "main".to_owned(),
            state,
            false,
        )
        .expect("test pull request must validate");
        record.created_at = created_at;
        record
    }

    fn failed_checks(
        pull_request_id: coldstart_core::PullRequestId,
        synced_at: DateTime<Utc>,
    ) -> PullRequestInsights {
        PullRequestInsights::new(
            pull_request_id,
            vec![PullRequestCheck {
                name: "unit tests".to_owned(),
                status: PullRequestCheckStatus::Completed,
                conclusion: Some(PullRequestCheckConclusion::Failure),
                details_url: Some("https://github.com/example/demo/actions/runs/1".to_owned()),
                app_name: Some("GitHub Actions".to_owned()),
            }],
            vec![PullRequestReview {
                author: "reviewer".to_owned(),
                state: PullRequestReviewState::ChangesRequested,
                body: "请补充失败分支测试。".to_owned(),
                html_url: Some(
                    "https://github.com/example/demo/pull/1#pullrequestreview-1".to_owned(),
                ),
            }],
            vec![],
            synced_at,
        )
        .expect("test insights must validate")
        .with_mergeability(PullRequestMergeability::Unstable)
    }

    #[test]
    fn aggregates_counts_risks_and_repository_order_without_losing_unassigned_tasks() {
        let project_id = ProjectId::new();
        let mut server = repository(project_id, "server", 1);
        let web = repository(project_id, "web", 0);
        server.name = "服务端".to_owned();
        let now = Utc::now();
        let server_pr = pull_request(project_id, server.id, 11, PullRequestState::Open, now);
        let conflicted = PullRequestInsights::new(server_pr.id, vec![], vec![], vec![], now)
            .expect("test insights must validate")
            .with_mergeability(PullRequestMergeability::Conflicted)
            .with_conflict_details(PullRequestConflictDetails::detected(
                vec!["src/order.rs".to_owned(), "src/payment.rs".to_owned()],
                false,
            ))
            .expect("conflict details must validate");
        let overview = aggregate_delivery_overview(
            project_id,
            Some(3),
            vec![web.clone(), server.clone()],
            vec![
                task(Some("server"), TaskKind::AgentCoding, WorkItemStatus::Ready),
                task(
                    Some("server"),
                    TaskKind::AgentCoding,
                    WorkItemStatus::Blocked,
                ),
                task(Some("server"), TaskKind::AgentCoding, WorkItemStatus::Done),
                task(
                    Some("web"),
                    TaskKind::AgentCoding,
                    WorkItemStatus::InProgress,
                ),
                task(
                    Some("web"),
                    TaskKind::AgentCoding,
                    WorkItemStatus::Submitted,
                ),
                task(None, TaskKind::AgentCoding, WorkItemStatus::Ready),
                task(None, TaskKind::HumanExternal, WorkItemStatus::Ready),
            ],
            vec![GitSyncRecord::new(
                project_id,
                WorkItemId::new(),
                server.id,
                "codex/server-task".to_owned(),
                "a".repeat(40),
                GitSyncStatus::Failed,
                Some("remote rejected".to_owned()),
            )],
            vec![SelectedDeliveryPullRequest {
                record: server_pr.clone(),
                insights: Some(conflicted),
                review_status: Some(ReviewStatus::Blocked),
            }],
        );

        assert_eq!(overview.plan_revision_number, Some(3));
        assert_eq!(overview.unassigned_agent_task_count, 1);
        assert_eq!(overview.repositories.len(), 2);
        assert_eq!(overview.repositories[0].repository_key, "server");
        assert_eq!(overview.repositories[1].repository_key, "web");
        let server = &overview.repositories[0];
        assert_eq!(server.tasks.total, 3);
        assert_eq!(server.tasks.ready, 1);
        assert_eq!(server.tasks.blocked, 1);
        assert_eq!(server.tasks.done, 1);
        assert_eq!(
            server.state,
            DeliveryRepositoryState::Attention,
            "blocked work must dominate a conflicted PR"
        );
        assert!(server.next_action.contains("阻塞或失败"));
        assert_eq!(
            server
                .pull_request
                .as_ref()
                .expect("pull request must exist")
                .conflict_path_count,
            2
        );
        assert_eq!(
            server
                .latest_git_sync
                .as_ref()
                .expect("sync must exist")
                .status,
            GitSyncStatus::Failed
        );
        let web = &overview.repositories[1];
        assert_eq!(web.tasks.active, 1);
        assert_eq!(web.tasks.submitted, 1);
        assert_eq!(web.state, DeliveryRepositoryState::Attention);
        assert_eq!(web.next_action, "先为该仓库配置本地验证命令");
    }

    #[test]
    fn selects_the_latest_open_pull_request_before_a_newer_closed_one() {
        let project_id = ProjectId::new();
        let repository_id = RepositoryId::new();
        let now = Utc::now();
        let open = pull_request(
            project_id,
            repository_id,
            20,
            PullRequestState::Open,
            now - Duration::hours(1),
        );
        let merged = pull_request(project_id, repository_id, 21, PullRequestState::Merged, now);
        let selected = select_pull_requests(&[merged, open.clone()]);

        assert_eq!(
            selected
                .get(&repository_id)
                .expect("open PR must win")
                .number,
            20
        );
        assert_eq!(selected[&repository_id], open);
    }

    #[test]
    fn falls_back_to_the_latest_closed_pull_request_when_none_are_open() {
        let project_id = ProjectId::new();
        let repository_id = RepositoryId::new();
        let now = Utc::now();
        let closed = pull_request(
            project_id,
            repository_id,
            30,
            PullRequestState::Closed,
            now - Duration::hours(2),
        );
        let merged = pull_request(
            project_id,
            repository_id,
            31,
            PullRequestState::Merged,
            now - Duration::hours(1),
        );
        let selected = select_pull_requests(&[closed, merged]);

        assert_eq!(selected[&repository_id].number, 31);
    }

    #[test]
    fn surface_checks_review_and_missing_snapshot_as_distinct_next_actions() {
        let project_id = ProjectId::new();
        let repository = repository(project_id, "web", 1);
        let now = Utc::now();
        let record = pull_request(project_id, repository.id, 41, PullRequestState::Open, now);
        let checks_failed = failed_checks(record.id, now);
        let overview = aggregate_delivery_overview(
            project_id,
            Some(1),
            vec![repository.clone()],
            vec![task(
                Some("web"),
                TaskKind::AgentCoding,
                WorkItemStatus::Done,
            )],
            vec![],
            vec![SelectedDeliveryPullRequest {
                record: record.clone(),
                insights: Some(checks_failed),
                review_status: None,
            }],
        );
        let view = &overview.repositories[0];
        assert_eq!(view.state, DeliveryRepositoryState::Attention);
        assert_eq!(view.next_action, "检查 PR #41 的 1 个失败 Checks");
        assert_eq!(
            view.pull_request
                .as_ref()
                .and_then(|pull_request| pull_request.checks.as_ref())
                .expect("checks must exist")
                .state,
            PullRequestChecksState::Failure
        );
        assert_eq!(
            view.pull_request
                .as_ref()
                .expect("pull request must exist")
                .changes_requested_review_count,
            1
        );

        let unsynced = aggregate_delivery_overview(
            project_id,
            Some(1),
            vec![repository],
            vec![task(
                Some("web"),
                TaskKind::AgentCoding,
                WorkItemStatus::Done,
            )],
            vec![],
            vec![SelectedDeliveryPullRequest {
                record,
                insights: None,
                review_status: None,
            }],
        );
        assert_eq!(
            unsynced.repositories[0].next_action,
            "在 AI 评审页刷新 PR #41 的验收状态"
        );
        assert!(
            unsynced.repositories[0]
                .pull_request
                .as_ref()
                .unwrap()
                .checks
                .is_none()
        );
    }

    #[test]
    fn reports_empty_and_completed_repository_states_truthfully() {
        let project_id = ProjectId::new();
        let empty = repository(project_id, "app", 0);
        let completed = repository(project_id, "mini_program", 1);
        let overview = aggregate_delivery_overview(
            project_id,
            Some(2),
            vec![empty, completed.clone()],
            vec![
                task(
                    Some("mini_program"),
                    TaskKind::AgentCoding,
                    WorkItemStatus::Done,
                ),
                task(
                    Some("mini_program"),
                    TaskKind::AgentCoding,
                    WorkItemStatus::Cancelled,
                ),
            ],
            vec![],
            vec![],
        );

        let empty = &overview.repositories[0];
        assert_eq!(empty.repository_key, "app");
        assert_eq!(empty.state, DeliveryRepositoryState::Idle);
        assert_eq!(empty.next_action, "当前计划暂无该仓库任务");

        let completed = &overview.repositories[1];
        assert_eq!(completed.repository_key, "mini_program");
        assert_eq!(completed.tasks.total, 1);
        assert_eq!(completed.tasks.done, 1);
        assert_eq!(completed.tasks.cancelled, 1);
        assert_eq!(completed.state, DeliveryRepositoryState::Complete);
        assert_eq!(completed.next_action, "仓库任务已完成，等待下一轮需求");
    }
}
