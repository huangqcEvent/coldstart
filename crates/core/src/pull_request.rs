use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ProjectId, PullRequestId, RepositoryId, WorkItemId, WorkReportId, validate_branch_name,
};

pub const MAX_PULL_REQUEST_TITLE_CHARS: usize = 200;
pub const MAX_PULL_REQUEST_URL_CHARS: usize = 2_048;
pub const MAX_PULL_REQUEST_INSIGHT_ITEMS: usize = 100;
pub const MAX_PULL_REQUEST_INSIGHT_TEXT_CHARS: usize = 8_000;
pub const MAX_PULL_REQUEST_INSIGHT_AUTHOR_CHARS: usize = 100;
pub const MAX_PULL_REQUEST_INSIGHT_PATH_CHARS: usize = 1_024;
pub const MAX_PULL_REQUEST_CHECK_NAME_CHARS: usize = 200;
pub const MAX_PULL_REQUEST_CHECK_APP_CHARS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestState {
    Open,
    Closed,
    Merged,
}

impl PullRequestState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            "merged" => Some(Self::Merged),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestMergeability {
    Mergeable,
    Conflicted,
    Blocked,
    Behind,
    Unstable,
    Draft,
    Checking,
    #[default]
    Unknown,
}

impl PullRequestMergeability {
    pub fn from_signals(mergeable: Option<bool>, state: Option<&str>) -> Self {
        match state
            .map(str::trim)
            .filter(|state| !state.is_empty())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("clean") => Self::Mergeable,
            Some("dirty") => Self::Conflicted,
            Some("blocked" | "has_hooks") => Self::Blocked,
            Some("behind") => Self::Behind,
            Some("unstable") => Self::Unstable,
            Some("draft") => Self::Draft,
            Some("unknown") => Self::Checking,
            _ => match mergeable {
                Some(true) => Self::Mergeable,
                Some(false) => Self::Blocked,
                None => Self::Unknown,
            },
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mergeable => "mergeable",
            Self::Conflicted => "conflicted",
            Self::Blocked => "blocked",
            Self::Behind => "behind",
            Self::Unstable => "unstable",
            Self::Draft => "draft",
            Self::Checking => "checking",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestCheckStatus {
    Queued,
    InProgress,
    Completed,
    Waiting,
    Requested,
    Pending,
    Unknown,
}

impl PullRequestCheckStatus {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "queued" => Self::Queued,
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            "waiting" => Self::Waiting,
            "requested" => Self::Requested,
            "pending" => Self::Pending,
            _ => Self::Unknown,
        }
    }

    pub fn is_completed(self) -> bool {
        self == Self::Completed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestCheckConclusion {
    Success,
    Failure,
    Neutral,
    Cancelled,
    Skipped,
    TimedOut,
    ActionRequired,
    StartupFailure,
    Stale,
    Unknown,
}

impl PullRequestCheckConclusion {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "success" => Self::Success,
            "failure" => Self::Failure,
            "neutral" => Self::Neutral,
            "cancelled" => Self::Cancelled,
            "skipped" => Self::Skipped,
            "timed_out" => Self::TimedOut,
            "action_required" => Self::ActionRequired,
            "startup_failure" => Self::StartupFailure,
            "stale" => Self::Stale,
            _ => Self::Unknown,
        }
    }

    pub fn is_failed(self) -> bool {
        matches!(
            self,
            Self::Failure
                | Self::Cancelled
                | Self::TimedOut
                | Self::ActionRequired
                | Self::StartupFailure
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestCheck {
    pub name: String,
    pub status: PullRequestCheckStatus,
    pub conclusion: Option<PullRequestCheckConclusion>,
    pub details_url: Option<String>,
    pub app_name: Option<String>,
}

impl PullRequestCheck {
    pub fn validate(&self) -> Result<(), PullRequestValidationError> {
        validate_required_insight_text(
            "check name",
            &self.name,
            MAX_PULL_REQUEST_CHECK_NAME_CHARS,
        )?;
        validate_optional_insight_text(
            "check app name",
            self.app_name.as_deref(),
            MAX_PULL_REQUEST_CHECK_APP_CHARS,
        )?;
        validate_optional_insight_url("check details URL", self.details_url.as_deref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
    Unknown,
}

impl PullRequestReviewState {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "approved" => Self::Approved,
            "changes_requested" => Self::ChangesRequested,
            "commented" => Self::Commented,
            "dismissed" => Self::Dismissed,
            "pending" => Self::Pending,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReview {
    pub author: String,
    pub state: PullRequestReviewState,
    pub body: String,
    pub html_url: Option<String>,
}

impl PullRequestReview {
    pub fn validate(&self) -> Result<(), PullRequestValidationError> {
        validate_required_insight_text(
            "review author",
            &self.author,
            MAX_PULL_REQUEST_INSIGHT_AUTHOR_CHARS,
        )?;
        validate_bounded_insight_text("review body", &self.body)?;
        validate_optional_insight_url("review URL", self.html_url.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReviewComment {
    pub author: String,
    pub path: String,
    pub line: Option<u64>,
    pub body: String,
    pub html_url: Option<String>,
}

impl PullRequestReviewComment {
    pub fn validate(&self) -> Result<(), PullRequestValidationError> {
        validate_required_insight_text(
            "review comment author",
            &self.author,
            MAX_PULL_REQUEST_INSIGHT_AUTHOR_CHARS,
        )?;
        validate_required_insight_text(
            "review comment path",
            &self.path,
            MAX_PULL_REQUEST_INSIGHT_PATH_CHARS,
        )?;
        if self.path.chars().any(char::is_control) {
            return Err(PullRequestValidationError::InvalidInsightText {
                field: "review comment path",
            });
        }
        if self.line == Some(0) {
            return Err(PullRequestValidationError::InvalidInsightLine);
        }
        validate_bounded_insight_text("review comment body", &self.body)?;
        validate_optional_insight_url("review comment URL", self.html_url.as_deref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestChecksState {
    None,
    Pending,
    Success,
    Failure,
}

impl PullRequestChecksState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PullRequestCheckSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub pending: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestInsights {
    pub pull_request_id: PullRequestId,
    #[serde(default)]
    pub mergeability: PullRequestMergeability,
    pub checks: Vec<PullRequestCheck>,
    pub reviews: Vec<PullRequestReview>,
    pub review_comments: Vec<PullRequestReviewComment>,
    pub synced_at: DateTime<Utc>,
}

impl PullRequestInsights {
    pub fn new(
        pull_request_id: PullRequestId,
        checks: Vec<PullRequestCheck>,
        reviews: Vec<PullRequestReview>,
        review_comments: Vec<PullRequestReviewComment>,
        synced_at: DateTime<Utc>,
    ) -> Result<Self, PullRequestValidationError> {
        let insights = Self {
            pull_request_id,
            mergeability: PullRequestMergeability::Unknown,
            checks,
            reviews,
            review_comments,
            synced_at,
        };
        insights.validate()?;
        Ok(insights)
    }

    pub fn with_mergeability(mut self, mergeability: PullRequestMergeability) -> Self {
        self.mergeability = mergeability;
        self
    }

    pub fn validate(&self) -> Result<(), PullRequestValidationError> {
        validate_insight_count("checks", self.checks.len())?;
        validate_insight_count("reviews", self.reviews.len())?;
        validate_insight_count("review comments", self.review_comments.len())?;
        for check in &self.checks {
            check.validate()?;
        }
        for review in &self.reviews {
            review.validate()?;
        }
        for comment in &self.review_comments {
            comment.validate()?;
        }
        Ok(())
    }

    pub fn check_summary(&self) -> PullRequestCheckSummary {
        let failed = self
            .checks
            .iter()
            .filter(|check| {
                check
                    .conclusion
                    .is_some_and(PullRequestCheckConclusion::is_failed)
            })
            .count();
        let pending = self
            .checks
            .iter()
            .filter(|check| !check.status.is_completed())
            .count();
        PullRequestCheckSummary {
            total: self.checks.len(),
            passed: self
                .checks
                .len()
                .saturating_sub(failed)
                .saturating_sub(pending),
            failed,
            pending,
        }
    }

    pub fn checks_state(&self) -> PullRequestChecksState {
        let summary = self.check_summary();
        if summary.total == 0 {
            PullRequestChecksState::None
        } else if summary.failed > 0 {
            PullRequestChecksState::Failure
        } else if summary.pending > 0 {
            PullRequestChecksState::Pending
        } else {
            PullRequestChecksState::Success
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestRecord {
    pub id: PullRequestId,
    pub project_id: ProjectId,
    pub work_item_id: WorkItemId,
    pub report_id: WorkReportId,
    pub repository_id: RepositoryId,
    pub number: u64,
    pub html_url: String,
    pub title: String,
    pub head_branch: String,
    pub base_branch: String,
    pub state: PullRequestState,
    pub draft: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PullRequestRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_id: ProjectId,
        work_item_id: WorkItemId,
        report_id: WorkReportId,
        repository_id: RepositoryId,
        number: u64,
        html_url: String,
        title: String,
        head_branch: String,
        base_branch: String,
        state: PullRequestState,
        draft: bool,
    ) -> Result<Self, PullRequestValidationError> {
        let now = Utc::now();
        let record = Self {
            id: PullRequestId::new(),
            project_id,
            work_item_id,
            report_id,
            repository_id,
            number,
            html_url,
            title,
            head_branch,
            base_branch,
            state,
            draft,
            created_at: now,
            updated_at: now,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn update_state(
        &mut self,
        state: PullRequestState,
        draft: bool,
    ) -> Result<bool, PullRequestValidationError> {
        if self.state == state && self.draft == draft {
            return Ok(false);
        }
        self.state = state;
        self.draft = draft;
        self.updated_at = Utc::now();
        self.validate()?;
        Ok(true)
    }

    pub fn validate(&self) -> Result<(), PullRequestValidationError> {
        if self.number == 0 {
            return Err(PullRequestValidationError::InvalidNumber);
        }
        let url = self.html_url.trim();
        if url.is_empty() {
            return Err(PullRequestValidationError::EmptyUrl);
        }
        if url.chars().count() > MAX_PULL_REQUEST_URL_CHARS {
            return Err(PullRequestValidationError::UrlTooLong);
        }
        if !url.starts_with("https://") {
            return Err(PullRequestValidationError::InsecureUrl);
        }
        if url.chars().any(char::is_control) {
            return Err(PullRequestValidationError::InvalidUrl);
        }
        let title = self.title.trim();
        if title.is_empty() {
            return Err(PullRequestValidationError::EmptyTitle);
        }
        if title.chars().count() > MAX_PULL_REQUEST_TITLE_CHARS {
            return Err(PullRequestValidationError::TitleTooLong);
        }
        validate_branch_name(&self.head_branch)
            .map_err(|_| PullRequestValidationError::InvalidBranch(self.head_branch.clone()))?;
        validate_branch_name(&self.base_branch)
            .map_err(|_| PullRequestValidationError::InvalidBranch(self.base_branch.clone()))?;
        if self.head_branch == self.base_branch {
            return Err(PullRequestValidationError::SameBranch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PullRequestValidationError {
    #[error("pull request number must be greater than zero")]
    InvalidNumber,
    #[error("pull request URL cannot be empty")]
    EmptyUrl,
    #[error("pull request URL must be at most {MAX_PULL_REQUEST_URL_CHARS} characters")]
    UrlTooLong,
    #[error("pull request URL must use HTTPS")]
    InsecureUrl,
    #[error("pull request URL contains invalid characters")]
    InvalidUrl,
    #[error("pull request title cannot be empty")]
    EmptyTitle,
    #[error("pull request title must be at most {MAX_PULL_REQUEST_TITLE_CHARS} characters")]
    TitleTooLong,
    #[error("invalid pull request branch: {0}")]
    InvalidBranch(String),
    #[error("pull request head and base branches must differ")]
    SameBranch,
    #[error("pull request insight list {field} exceeds {MAX_PULL_REQUEST_INSIGHT_ITEMS} items")]
    InsightListTooLong { field: &'static str },
    #[error("pull request insight text {field} exceeds {limit} characters")]
    InsightTextTooLong { field: &'static str, limit: usize },
    #[error("pull request insight field {field} cannot be empty")]
    EmptyInsightText { field: &'static str },
    #[error("pull request insight field {field} contains invalid characters")]
    InvalidInsightText { field: &'static str },
    #[error("pull request insight field {field} must use HTTPS")]
    InsecureInsightUrl { field: &'static str },
    #[error("pull request insight line number must be greater than zero")]
    InvalidInsightLine,
}

fn validate_insight_count(
    field: &'static str,
    count: usize,
) -> Result<(), PullRequestValidationError> {
    if count > MAX_PULL_REQUEST_INSIGHT_ITEMS {
        Err(PullRequestValidationError::InsightListTooLong { field })
    } else {
        Ok(())
    }
}

fn validate_required_insight_text(
    field: &'static str,
    value: &str,
    limit: usize,
) -> Result<(), PullRequestValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(PullRequestValidationError::EmptyInsightText { field });
    }
    if value.chars().count() > limit {
        return Err(PullRequestValidationError::InsightTextTooLong { field, limit });
    }
    if value.chars().any(char::is_control) {
        return Err(PullRequestValidationError::InvalidInsightText { field });
    }
    Ok(())
}

fn validate_optional_insight_text(
    field: &'static str,
    value: Option<&str>,
    limit: usize,
) -> Result<(), PullRequestValidationError> {
    if let Some(value) = value {
        validate_required_insight_text(field, value, limit)?;
    }
    Ok(())
}

fn validate_bounded_insight_text(
    field: &'static str,
    value: &str,
) -> Result<(), PullRequestValidationError> {
    if value.chars().count() > MAX_PULL_REQUEST_INSIGHT_TEXT_CHARS {
        Err(PullRequestValidationError::InsightTextTooLong {
            field,
            limit: MAX_PULL_REQUEST_INSIGHT_TEXT_CHARS,
        })
    } else {
        Ok(())
    }
}

fn validate_optional_insight_url(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), PullRequestValidationError> {
    let Some(value) = value else {
        return Ok(());
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(PullRequestValidationError::EmptyInsightText { field });
    }
    if value.chars().count() > MAX_PULL_REQUEST_URL_CHARS {
        return Err(PullRequestValidationError::InsightTextTooLong {
            field,
            limit: MAX_PULL_REQUEST_URL_CHARS,
        });
    }
    if !value.starts_with("https://") {
        return Err(PullRequestValidationError::InsecureInsightUrl { field });
    }
    if value.chars().any(char::is_control) {
        return Err(PullRequestValidationError::InvalidInsightText { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> PullRequestRecord {
        PullRequestRecord::new(
            ProjectId::new(),
            WorkItemId::new(),
            WorkReportId::new(),
            RepositoryId::new(),
            42,
            "https://github.com/octocat/demo/pull/42".to_owned(),
            "[task] 实现订单".to_owned(),
            "codex/task".to_owned(),
            "main".to_owned(),
            PullRequestState::Open,
            true,
        )
        .expect("pull request must be valid")
    }

    #[test]
    fn rejects_insecure_urls_and_identical_branches() {
        let mut value = record();
        value.html_url = "http://github.com/octocat/demo/pull/42".to_owned();
        assert_eq!(
            value.validate(),
            Err(PullRequestValidationError::InsecureUrl)
        );

        let mut value = record();
        value.base_branch = value.head_branch.clone();
        assert_eq!(
            value.validate(),
            Err(PullRequestValidationError::SameBranch)
        );
    }

    #[test]
    fn state_updates_are_idempotent() {
        let mut value = record();
        assert!(
            !value
                .update_state(PullRequestState::Open, true)
                .expect("state must validate")
        );
        assert!(
            value
                .update_state(PullRequestState::Merged, false)
                .expect("state must validate")
        );
        assert_eq!(value.state, PullRequestState::Merged);
        assert!(value.updated_at >= value.created_at);
    }

    fn check(
        name: &str,
        status: PullRequestCheckStatus,
        conclusion: Option<PullRequestCheckConclusion>,
    ) -> PullRequestCheck {
        PullRequestCheck {
            name: name.to_owned(),
            status,
            conclusion,
            details_url: Some("https://github.com/octocat/demo/runs/1".to_owned()),
            app_name: Some("GitHub Actions".to_owned()),
        }
    }

    #[test]
    fn insights_summarize_checks_without_treating_pending_as_success() {
        let insights = PullRequestInsights::new(
            PullRequestId::new(),
            vec![
                check(
                    "format",
                    PullRequestCheckStatus::Completed,
                    Some(PullRequestCheckConclusion::Success),
                ),
                check(
                    "test",
                    PullRequestCheckStatus::Completed,
                    Some(PullRequestCheckConclusion::Success),
                ),
                check(
                    "lint",
                    PullRequestCheckStatus::Completed,
                    Some(PullRequestCheckConclusion::Failure),
                ),
                check("build", PullRequestCheckStatus::InProgress, None),
            ],
            vec![],
            vec![],
            Utc::now(),
        )
        .expect("insights must validate");

        let summary = insights.check_summary();
        assert_eq!(summary.total, 4);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.pending, 1);
        assert_eq!(insights.checks_state(), PullRequestChecksState::Failure);
    }

    #[test]
    fn insights_distinguish_none_pending_and_success() {
        let empty =
            PullRequestInsights::new(PullRequestId::new(), vec![], vec![], vec![], Utc::now())
                .expect("empty insights must validate");
        assert_eq!(empty.checks_state(), PullRequestChecksState::None);

        let pending = PullRequestInsights::new(
            PullRequestId::new(),
            vec![check("build", PullRequestCheckStatus::Queued, None)],
            vec![],
            vec![],
            Utc::now(),
        )
        .expect("pending insights must validate");
        assert_eq!(pending.checks_state(), PullRequestChecksState::Pending);

        let success = PullRequestInsights::new(
            PullRequestId::new(),
            vec![check(
                "build",
                PullRequestCheckStatus::Completed,
                Some(PullRequestCheckConclusion::Success),
            )],
            vec![],
            vec![],
            Utc::now(),
        )
        .expect("successful insights must validate");
        assert_eq!(success.checks_state(), PullRequestChecksState::Success);
    }

    #[test]
    fn insights_reject_insecure_urls_oversized_text_and_more_than_one_hundred_items() {
        let mut insecure = check(
            "build",
            PullRequestCheckStatus::Completed,
            Some(PullRequestCheckConclusion::Success),
        );
        insecure.details_url = Some("http://github.com/octocat/demo".to_owned());
        let error = PullRequestInsights::new(
            PullRequestId::new(),
            vec![insecure],
            vec![],
            vec![],
            Utc::now(),
        )
        .expect_err("insecure URL must fail");
        assert!(matches!(
            error,
            PullRequestValidationError::InsecureInsightUrl { .. }
        ));

        let oversized = PullRequestReviewComment {
            author: "reviewer".to_owned(),
            path: "src/main.ts".to_owned(),
            line: Some(12),
            body: "x".repeat(MAX_PULL_REQUEST_INSIGHT_TEXT_CHARS + 1),
            html_url: None,
        };
        let error = PullRequestInsights::new(
            PullRequestId::new(),
            vec![],
            vec![],
            vec![oversized],
            Utc::now(),
        )
        .expect_err("oversized comment must fail");
        assert!(matches!(
            error,
            PullRequestValidationError::InsightTextTooLong { .. }
        ));

        let too_many = (0..=MAX_PULL_REQUEST_INSIGHT_ITEMS)
            .map(|index| {
                check(
                    &format!("check-{index}"),
                    PullRequestCheckStatus::Completed,
                    Some(PullRequestCheckConclusion::Success),
                )
            })
            .collect();
        let error =
            PullRequestInsights::new(PullRequestId::new(), too_many, vec![], vec![], Utc::now())
                .expect_err("item cap must be enforced");
        assert!(matches!(
            error,
            PullRequestValidationError::InsightListTooLong { field: "checks" }
        ));
    }

    #[test]
    fn mergeability_maps_github_signals_without_assuming_success() {
        assert_eq!(
            PullRequestMergeability::from_signals(Some(true), Some("clean")),
            PullRequestMergeability::Mergeable
        );
        assert_eq!(
            PullRequestMergeability::from_signals(Some(false), Some("dirty")),
            PullRequestMergeability::Conflicted
        );
        assert_eq!(
            PullRequestMergeability::from_signals(Some(true), Some("blocked")),
            PullRequestMergeability::Blocked
        );
        assert_eq!(
            PullRequestMergeability::from_signals(Some(true), Some("behind")),
            PullRequestMergeability::Behind
        );
        assert_eq!(
            PullRequestMergeability::from_signals(Some(true), Some("unstable")),
            PullRequestMergeability::Unstable
        );
        assert_eq!(
            PullRequestMergeability::from_signals(Some(true), Some("draft")),
            PullRequestMergeability::Draft
        );
        assert_eq!(
            PullRequestMergeability::from_signals(None, Some("unknown")),
            PullRequestMergeability::Checking
        );
        assert_eq!(
            PullRequestMergeability::from_signals(None, None),
            PullRequestMergeability::Unknown
        );
        assert_eq!(
            PullRequestMergeability::from_signals(Some(false), None),
            PullRequestMergeability::Blocked
        );
    }

    #[test]
    fn existing_insight_snapshots_default_mergeability_to_unknown() {
        let insights =
            PullRequestInsights::new(PullRequestId::new(), vec![], vec![], vec![], Utc::now())
                .expect("insights must validate");
        let mut value = serde_json::to_value(insights).expect("insights must serialize");
        value
            .as_object_mut()
            .expect("insights must be an object")
            .remove("mergeability");
        let restored: PullRequestInsights =
            serde_json::from_value(value).expect("legacy snapshot must deserialize");
        assert_eq!(restored.mergeability, PullRequestMergeability::Unknown);
    }
}
