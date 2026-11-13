use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use chrono::Utc;
use coldstart_adapters::{
    CreateDraftPullRequestRequest, GitAdapter, GitHubCheckRun, GitHubClient, GitHubError,
    GitHubPullRequest, GitHubPullRequestReview, GitHubPullRequestReviewComment,
    GitHubRepositoryCoordinates, GitHubToken, SecretStore, WindowsSecretStore,
    sanitize_verification_output,
};
use coldstart_core::{
    CredentialKind, GitSyncStatus, MAX_PULL_REQUEST_CHECK_APP_CHARS,
    MAX_PULL_REQUEST_CHECK_NAME_CHARS, MAX_PULL_REQUEST_CONFLICT_PATHS,
    MAX_PULL_REQUEST_INSIGHT_AUTHOR_CHARS, MAX_PULL_REQUEST_INSIGHT_PATH_CHARS,
    MAX_PULL_REQUEST_INSIGHT_TEXT_CHARS, PullRequestCheck, PullRequestCheckConclusion,
    PullRequestCheckStatus, PullRequestConflictDetails, PullRequestId, PullRequestInsights,
    PullRequestMergeability, PullRequestRecord, PullRequestReview, PullRequestReviewComment,
    PullRequestReviewState, PullRequestState, ReportOutcome, Repository, RepositoryAuthMode,
    ReviewStatus, TaskKind, TaskSpec, WorkItemStatus, WorkReportId,
};

use crate::{AppError, context::OpenProject};

const MAX_PR_BODY_CHARS: usize = 60_000;
const MAX_PR_SUMMARY_CHARS: usize = 1_200;
const MAX_PR_LIST_ITEMS: usize = 10;
const MAX_PR_CHANGED_FILES: usize = 30;
const MAX_PR_LIST_ITEM_CHARS: usize = 300;

pub struct PullRequestService {
    secrets: Arc<dyn SecretStore>,
    github: Option<GitHubClient>,
    git: GitAdapter,
}

impl PullRequestService {
    pub fn new() -> Self {
        Self::with_dependencies(
            Arc::new(WindowsSecretStore::new()),
            GitHubClient::new().ok(),
        )
    }

    pub fn with_dependencies(secrets: Arc<dyn SecretStore>, github: Option<GitHubClient>) -> Self {
        Self {
            secrets,
            github,
            git: GitAdapter::new(),
        }
    }

    pub fn create_draft(
        &self,
        project_home: PathBuf,
        report_id: WorkReportId,
    ) -> Result<PullRequestRecord, AppError> {
        let mut project = OpenProject::open(project_home)?;
        if let Some(existing) = project
            .database
            .list_pull_requests(&project.project.id)?
            .into_iter()
            .find(|record| record.report_id == report_id)
        {
            return Ok(existing);
        }

        let report = project
            .database
            .list_work_reports(&project.project.id)?
            .into_iter()
            .find(|record| record.id == report_id)
            .ok_or(AppError::PullRequestReportNotFound(report_id))?;
        if !report.accepted
            || report.report.status != ReportOutcome::Completed
            || !report.review_status.permits_completion()
        {
            return Err(AppError::PullRequestIneligible(
                "报告必须已通过本地验证与 Review 门禁".to_owned(),
            ));
        }
        let work_item = project
            .database
            .load_work_item(&report.work_item_id)?
            .ok_or(AppError::WorkItemNotFound(report.work_item_id))?;
        if work_item.status != WorkItemStatus::Done {
            return Err(AppError::PullRequestIneligible(
                "任务尚未完成，不能创建 PR".to_owned(),
            ));
        }
        let task = project
            .database
            .task_spec_for_work_item(&report.work_item_id)?
            .ok_or_else(|| AppError::PullRequestIneligible("任务契约不存在".to_owned()))?;
        if task.kind != TaskKind::AgentCoding {
            return Err(AppError::PullRequestIneligible(
                "只有 Agent 编码任务可以创建 PR".to_owned(),
            ));
        }
        let repository_key = task
            .repository_key
            .as_deref()
            .ok_or_else(|| AppError::PullRequestIneligible("任务没有关联仓库".to_owned()))?;
        let repository = project
            .database
            .list_repositories(&project.project.id)?
            .into_iter()
            .find(|repository| repository.key == repository_key)
            .ok_or_else(|| AppError::RepositoryNotFound(repository_key.to_owned()))?;
        let branch = report
            .report
            .branch
            .as_deref()
            .ok_or_else(|| AppError::PullRequestIneligible("报告没有任务分支".to_owned()))?;
        let commit_sha = report
            .report
            .commit_sha
            .as_deref()
            .ok_or_else(|| AppError::PullRequestIneligible("报告没有 Commit SHA".to_owned()))?;
        let pushed = project
            .database
            .list_git_syncs(&project.project.id)?
            .iter()
            .any(|sync| {
                sync.work_item_id == report.work_item_id
                    && sync.repository_id == repository.id
                    && sync.branch == branch
                    && sync.commit_sha == commit_sha
                    && sync.status == GitSyncStatus::Pushed
            });
        if !pushed {
            return Err(AppError::PullRequestIneligible(
                "任务分支尚未成功推送到远程".to_owned(),
            ));
        }
        let coordinates = github_coordinates(&repository)?;
        let token = self.github_token(&project, &repository)?;
        let github = self.github.as_ref().ok_or(AppError::GitHubNotConfigured)?;
        let title = pull_request_title(&task);
        let body = pull_request_body(&task, &report, MAX_PR_BODY_CHARS);
        let remote = github.create_draft_pull_request(
            &token,
            &CreateDraftPullRequestRequest {
                owner: coordinates.owner,
                repository: coordinates.repository,
                title: title.clone(),
                body,
                head: branch.to_owned(),
                base: repository.default_branch.clone(),
            },
        )?;
        if remote.head.reference != branch
            || remote.base.reference != repository.default_branch
            || !remote.draft
        {
            return Err(GitHubError::InvalidResponse(
                "GitHub returned a pull request outside the requested draft branch boundary"
                    .to_owned(),
            )
            .into());
        }
        let state = pull_request_state(&remote)?;
        let record = PullRequestRecord::new(
            project.project.id,
            report.work_item_id,
            report.id,
            repository.id,
            remote.number,
            remote.html_url,
            title,
            branch.to_owned(),
            repository.default_branch.clone(),
            state,
            true,
        )?;
        project.database.save_pull_request(&record)?;
        Ok(record)
    }

    pub fn refresh(
        &self,
        project_home: PathBuf,
        pull_request_id: PullRequestId,
    ) -> Result<PullRequestRecord, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let mut record = project
            .database
            .load_pull_request(&project.project.id, &pull_request_id)?
            .ok_or(AppError::PullRequestNotFound(pull_request_id))?;
        let repository = project
            .database
            .list_repositories(&project.project.id)?
            .into_iter()
            .find(|repository| repository.id == record.repository_id)
            .ok_or_else(|| AppError::RepositoryNotFound(record.repository_id.to_string()))?;
        let coordinates = github_coordinates(&repository)?;
        let token = self.github_token(&project, &repository)?;
        let github = self.github.as_ref().ok_or(AppError::GitHubNotConfigured)?;
        let remote = github.pull_request(
            &token,
            &coordinates.owner,
            &coordinates.repository,
            record.number,
        )?;
        let check_runs = github.pull_request_checks(
            &token,
            &coordinates.owner,
            &coordinates.repository,
            &remote.head.sha,
        )?;
        let reviews = github.pull_request_reviews(
            &token,
            &coordinates.owner,
            &coordinates.repository,
            record.number,
        )?;
        let review_comments = github.pull_request_review_comments(
            &token,
            &coordinates.owner,
            &coordinates.repository,
            record.number,
        )?;
        let mergeability = PullRequestMergeability::from_signals(
            remote.mergeable,
            remote.mergeable_state.as_deref(),
        );
        let state = pull_request_state(&remote)?;
        let conflict_details =
            diagnose_conflict_details(&self.git, &repository, &remote, state, mergeability);
        let insights = pull_request_insights(
            record.id,
            mergeability,
            check_runs,
            reviews,
            review_comments,
        )?
        .with_conflict_details(conflict_details)?;
        if record.update_state(state, remote.draft)? {
            project.database.save_pull_request(&record)?;
        }
        project.database.save_pull_request_insights(&insights)?;
        Ok(record)
    }

    fn github_token(
        &self,
        project: &OpenProject,
        repository: &Repository,
    ) -> Result<GitHubToken, AppError> {
        if repository.auth_mode != RepositoryAuthMode::Credential {
            return Err(AppError::PullRequestIneligible(
                "GitHub PR 需要仓库使用凭据认证".to_owned(),
            ));
        }
        let credential_id = repository
            .credential_id
            .ok_or(AppError::RepositoryCredentialMissing(repository.id))?;
        let credential = project
            .database
            .credential(&project.project.id, &credential_id)?
            .ok_or(AppError::CredentialNotFound(credential_id))?;
        if !credential.provider.trim().eq_ignore_ascii_case("github")
            || !matches!(
                credential.kind,
                CredentialKind::OAuthToken | CredentialKind::PersonalAccessToken
            )
        {
            return Err(AppError::PullRequestIneligible(
                "仓库凭据不是 GitHub OAuth Token 或 Personal Access Token".to_owned(),
            ));
        }
        GitHubToken::new(self.secrets.get(&credential.secret_ref)?).map_err(Into::into)
    }
}

impl Default for PullRequestService {
    fn default() -> Self {
        Self::new()
    }
}

fn github_coordinates(repository: &Repository) -> Result<GitHubRepositoryCoordinates, AppError> {
    GitHubRepositoryCoordinates::from_remote_url(&repository.remote_url)
        .map_err(|error| AppError::PullRequestIneligible(error.to_string()))
}

fn pull_request_state(remote: &GitHubPullRequest) -> Result<PullRequestState, AppError> {
    if remote.merged {
        return Ok(PullRequestState::Merged);
    }
    PullRequestState::parse(&remote.state).ok_or_else(|| {
        GitHubError::InvalidResponse(format!(
            "GitHub returned an unsupported pull request state: {}",
            remote.state
        ))
        .into()
    })
}

fn diagnose_conflict_details(
    git: &GitAdapter,
    repository: &Repository,
    remote: &GitHubPullRequest,
    state: PullRequestState,
    mergeability: PullRequestMergeability,
) -> PullRequestConflictDetails {
    if state != PullRequestState::Open || mergeability != PullRequestMergeability::Conflicted {
        return PullRequestConflictDetails::not_applicable();
    }
    match git.find_merge_conflicts(
        &repository.local_path,
        &remote.base.sha,
        &remote.head.sha,
        MAX_PULL_REQUEST_CONFLICT_PATHS,
    ) {
        Ok(report) if !report.has_conflicts => PullRequestConflictDetails::not_reproduced(),
        Ok(report) => {
            let mut paths = BTreeSet::new();
            for path in report.conflicted_paths {
                if let Some(path) = conflict_path(&path) {
                    paths.insert(path);
                }
            }
            PullRequestConflictDetails::detected(
                paths
                    .into_iter()
                    .take(MAX_PULL_REQUEST_CONFLICT_PATHS)
                    .collect(),
                report.truncated,
            )
        }
        Err(_) => PullRequestConflictDetails::unavailable(),
    }
}

fn conflict_path(value: &str) -> Option<String> {
    let value = sanitize_verification_output(value).trim().to_owned();
    if value.is_empty() {
        None
    } else {
        Some(truncate_chars(&value, MAX_PULL_REQUEST_INSIGHT_PATH_CHARS))
    }
}

fn pull_request_insights(
    pull_request_id: PullRequestId,
    mergeability: PullRequestMergeability,
    check_runs: Vec<GitHubCheckRun>,
    reviews: Vec<GitHubPullRequestReview>,
    review_comments: Vec<GitHubPullRequestReviewComment>,
) -> Result<PullRequestInsights, AppError> {
    let checks = check_runs
        .into_iter()
        .map(|check| PullRequestCheck {
            name: bounded_text(&check.name, MAX_PULL_REQUEST_CHECK_NAME_CHARS),
            status: PullRequestCheckStatus::parse(&check.status),
            conclusion: check
                .conclusion
                .as_deref()
                .map(PullRequestCheckConclusion::parse),
            details_url: safe_optional_url(check.details_url),
            app_name: check
                .app
                .and_then(|app| app.name.or(app.slug))
                .map(|value| bounded_text(&value, MAX_PULL_REQUEST_CHECK_APP_CHARS))
                .filter(|value| !value.is_empty()),
        })
        .collect();
    let reviews = reviews
        .into_iter()
        .map(|review| PullRequestReview {
            author: insight_actor(review.user.map(|user| user.login)),
            state: PullRequestReviewState::parse(&review.state),
            body: bounded_text(
                review.body.as_deref().unwrap_or_default(),
                MAX_PULL_REQUEST_INSIGHT_TEXT_CHARS,
            ),
            html_url: safe_optional_url(review.html_url),
        })
        .collect();
    let review_comments = review_comments
        .into_iter()
        .map(|comment| PullRequestReviewComment {
            author: insight_actor(comment.user.map(|user| user.login)),
            path: insight_path(&comment.path),
            line: comment.line.or(comment.original_line).map(u64::from),
            body: bounded_text(
                comment.body.as_deref().unwrap_or_default(),
                MAX_PULL_REQUEST_INSIGHT_TEXT_CHARS,
            ),
            html_url: safe_optional_url(comment.html_url),
        })
        .collect();
    PullRequestInsights::new(
        pull_request_id,
        checks,
        reviews,
        review_comments,
        Utc::now(),
    )
    .map(|insights| insights.with_mergeability(mergeability))
    .map_err(Into::into)
}

fn insight_actor(value: Option<String>) -> String {
    value
        .as_deref()
        .map(|value| bounded_text(value, MAX_PULL_REQUEST_INSIGHT_AUTHOR_CHARS))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn insight_path(value: &str) -> String {
    let value = sanitize_verification_output(value)
        .replace(['\r', '\n', '\t'], " ")
        .trim()
        .to_owned();
    if value.is_empty() {
        "unknown".to_owned()
    } else {
        truncate_chars(&value, MAX_PULL_REQUEST_INSIGHT_PATH_CHARS)
    }
}

fn safe_optional_url(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn pull_request_title(task: &TaskSpec) -> String {
    bounded_text(&format!("[{}] {}", task.task_key, task.title), 200)
}

fn pull_request_body(
    task: &TaskSpec,
    report: &coldstart_core::WorkReportRecord,
    limit: usize,
) -> String {
    let mut body = String::from("## ColdStart 交付\n\n");
    body.push_str("> 此 PR 由 ColdStart 创建为草稿，未自动合并，也未创建 Release Tag。\n\n");
    body.push_str(&format!(
        "### 任务\n\n- 任务键：`{}`\n",
        safe_inline(&task.task_key)
    ));
    body.push_str(&format!(
        "- 标题：{}\n",
        bounded_text(&task.title, MAX_PR_SUMMARY_CHARS)
    ));
    body.push_str(&format!(
        "- 目标：{}\n\n",
        bounded_text(&task.goal, MAX_PR_SUMMARY_CHARS)
    ));

    body.push_str("### 完成报告\n\n");
    body.push_str(&format!(
        "{}\n\n",
        bounded_text(&report.report.summary, MAX_PR_SUMMARY_CHARS)
    ));
    let acceptance_passed = report
        .report
        .acceptance_results
        .iter()
        .filter(|result| result.passed)
        .count();
    body.push_str(&format!(
        "- 验收项：{acceptance_passed}/{} 通过\n",
        report.report.acceptance_results.len()
    ));
    body.push_str(&format!(
        "- Commit：`{}`\n",
        safe_inline(report.report.commit_sha.as_deref().unwrap_or("unknown"))
    ));
    body.push_str(&format!(
        "- 分支：`{}`\n\n",
        safe_inline(report.report.branch.as_deref().unwrap_or("unknown"))
    ));

    body.push_str("### 门禁\n\n");
    body.push_str(&format!(
        "- 本地验证：{}\n",
        verification_label(report.verification_status)
    ));
    if let Some(verification) = &report.verification {
        for command in verification.commands.iter().take(MAX_PR_LIST_ITEMS) {
            body.push_str(&format!(
                "  - {}：{}\n",
                safe_inline(&command.name),
                command
                    .exit_code
                    .map(|code| format!("退出码 {code}"))
                    .unwrap_or_else(|| "已执行".to_owned())
            ));
        }
    }
    body.push_str(&format!(
        "- AI Review：{}\n",
        review_label(report.review_status)
    ));
    if let Some(review) = &report.review {
        body.push_str(&format!(
            "  - 结论：{}\n",
            bounded_text(&review.summary, MAX_PR_SUMMARY_CHARS)
        ));
        body.push_str(&format!("  - 问题数：{}\n", review.findings.len()));
        body.push_str(&format!("  - 模型：`{}`\n", safe_inline(&review.model)));
    }
    if let Some(waiver) = &report.review_waiver {
        body.push_str(&format!(
            "  - 人工豁免：{}\n",
            bounded_text(&waiver.reason, MAX_PR_SUMMARY_CHARS)
        ));
    }
    body.push('\n');

    append_list_section(
        &mut body,
        "变更文件",
        &report.report.changed_files,
        MAX_PR_CHANGED_FILES,
    );
    append_list_section(
        &mut body,
        "已知问题",
        &report.report.known_issues,
        MAX_PR_LIST_ITEMS,
    );
    append_list_section(
        &mut body,
        "后续事项",
        &report.report.follow_up_items,
        MAX_PR_LIST_ITEMS,
    );
    body.push_str("\n---\n\n由 ColdStart 生成；合并操作必须由人工完成。\n");
    truncate_chars(&body, limit)
}

fn append_list_section(body: &mut String, title: &str, values: &[String], limit: usize) {
    body.push_str(&format!("### {title}\n\n"));
    if values.is_empty() {
        body.push_str("- 无\n\n");
        return;
    }
    for value in values.iter().take(limit) {
        body.push_str(&format!("- {}\n", list_text(value)));
    }
    if values.len() > limit {
        body.push_str(&format!("- 另有 {} 项未展开\n", values.len() - limit));
    }
    body.push('\n');
}

fn verification_label(status: coldstart_core::VerificationStatus) -> &'static str {
    match status {
        coldstart_core::VerificationStatus::NotRequired => "无需验证",
        coldstart_core::VerificationStatus::Running => "验证中",
        coldstart_core::VerificationStatus::Passed => "通过",
        coldstart_core::VerificationStatus::Failed => "失败",
        coldstart_core::VerificationStatus::Interrupted => "中断",
    }
}

fn review_label(status: ReviewStatus) -> &'static str {
    match status {
        ReviewStatus::NotRequired => "未启用",
        ReviewStatus::Running => "评审中",
        ReviewStatus::Passed => "通过",
        ReviewStatus::Blocked => "阻断",
        ReviewStatus::Failed => "失败",
        ReviewStatus::Waived => "人工豁免",
        ReviewStatus::Interrupted => "中断",
    }
}

fn safe_inline(value: &str) -> String {
    let value = sanitize_verification_output(value)
        .replace(['\r', '\n', '`'], " ")
        .trim()
        .to_owned();
    if value.is_empty() {
        "unknown".to_owned()
    } else {
        value
    }
}

fn list_text(value: &str) -> String {
    let value = sanitize_verification_output(value)
        .replace(['\r', '\n'], " ")
        .trim()
        .to_owned();
    if value.is_empty() {
        "(空)".to_owned()
    } else {
        truncate_chars(&value, MAX_PR_LIST_ITEM_CHARS)
    }
}

fn bounded_text(value: &str, limit: usize) -> String {
    let value = sanitize_verification_output(value).trim().to_owned();
    truncate_chars(&value, limit)
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_owned();
    }
    let mut result = value
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    result.push('…');
    result
}
