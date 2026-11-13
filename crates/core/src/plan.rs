//! 计划、任务契约与工作状态模型。
//!
//! 计划发布后形成不可变修订；任务契约冻结目标、路径、依赖和验收标准。状态迁移、
//! 报告校验和契约哈希都集中在这里，避免 MCP 与桌面端各自解释状态机。

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Component, Path},
    str::FromStr,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    BlueprintEntityId, BlueprintRevisionId, LeaseId, PlanRevisionId, ProjectId, TaskSpecId,
    VerificationSnapshot, WorkItemId, WorkReportId, WorkRunId,
};

pub const PLAN_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_LEASE_MINUTES: i64 = 30;
pub const MAX_PLAN_TASKS: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Draft,
    Published,
    Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    AgentCoding,
    HumanExternal,
    ApprovalGate,
    Milestone,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptancePolicy {
    #[default]
    AgentReport,
    HumanAcceptance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatus {
    Draft,
    Ready,
    Claimed,
    InProgress,
    Submitted,
    Done,
    Blocked,
    Failed,
    Reopened,
    Cancelled,
}

impl WorkItemStatus {
    pub fn can_transition_to(self, next: Self) -> bool {
        use WorkItemStatus::*;
        matches!(
            (self, next),
            (Draft, Ready)
                | (Draft, Cancelled)
                | (Ready, Claimed)
                | (Ready, Blocked)
                | (Ready, Cancelled)
                | (Claimed, InProgress)
                | (Claimed, Ready)
                | (Claimed, Blocked)
                | (InProgress, Submitted)
                | (InProgress, Blocked)
                | (InProgress, Failed)
                | (InProgress, Reopened)
                | (Submitted, Done)
                | (Submitted, InProgress)
                | (Blocked, Ready)
                | (Blocked, Failed)
                | (Failed, Reopened)
                | (Done, Reopened)
                | (Reopened, Ready)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractState {
    Current,
    Stale,
}

pub fn inherited_work_status(previous: WorkItemStatus, contract_changed: bool) -> WorkItemStatus {
    if contract_changed {
        return WorkItemStatus::Reopened;
    }
    match previous {
        WorkItemStatus::Claimed | WorkItemStatus::InProgress | WorkItemStatus::Submitted => {
            WorkItemStatus::Reopened
        }
        status => status,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    Blocks,
    Informs,
    ConflictsWith,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseStatus {
    Active,
    Released,
    Expired,
    Consumed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkRunStatus {
    Claimed,
    Running,
    Submitted,
    Completed,
    Failed,
    Expired,
    Released,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportOutcome {
    Completed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskDependency {
    pub task_key: String,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcceptanceCriterion {
    pub id: String,
    pub text: String,
    pub required_evidence: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskSpec {
    pub task_key: String,
    pub title: String,
    pub kind: TaskKind,
    pub goal: String,
    pub scope: Vec<String>,
    pub out_of_scope: Vec<String>,
    pub module_id: Option<BlueprintEntityId>,
    pub platform_id: Option<BlueprintEntityId>,
    pub repository_key: Option<String>,
    pub base_ref: Option<String>,
    pub work_branch: Option<String>,
    pub allowed_paths: Vec<String>,
    pub context_refs: Vec<BlueprintEntityId>,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    pub acceptance_policy: AcceptancePolicy,
    pub evidence_requirements: Vec<String>,
    pub priority: i64,
    pub critical_path: bool,
    pub context_budget_tokens: u32,
    pub resource_keys: Vec<String>,
    pub dependencies: Vec<TaskDependency>,
}

impl TaskSpec {
    pub fn contract_hash(&self) -> String {
        sha256_hex(&serde_json::to_vec(self).expect("task spec must serialize"))
    }

    pub fn blocking_dependencies(&self) -> impl Iterator<Item = &str> {
        self.dependencies
            .iter()
            .filter(|dependency| dependency.kind == DependencyKind::Blocks)
            .map(|dependency| dependency.task_key.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanContent {
    pub tasks: Vec<TaskSpec>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanTaskDiff {
    pub task_key: String,
    pub title: String,
    pub previous_status: Option<WorkItemStatus>,
    pub next_status: Option<WorkItemStatus>,
    pub contract_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanPublishPreview {
    pub base_revision_id: Option<PlanRevisionId>,
    pub base_revision_number: Option<u64>,
    pub candidate_revision_id: PlanRevisionId,
    pub candidate_revision_number: u64,
    pub added: Vec<PlanTaskDiff>,
    pub changed: Vec<PlanTaskDiff>,
    pub unchanged: Vec<PlanTaskDiff>,
    pub removed: Vec<PlanTaskDiff>,
    pub carried_completed: usize,
    pub reopened: usize,
    pub cancelled: usize,
    pub active_lease_count: usize,
    pub publishable: bool,
    pub blockers: Vec<String>,
}

impl PlanContent {
    pub fn validate(&self) -> Result<(), PlanValidationError> {
        validate_plan(&self.tasks)
    }

    pub fn content_hash(&self) -> String {
        sha256_hex(&serde_json::to_vec(self).expect("plan content must serialize"))
    }

    pub fn task(&self, task_key: &str) -> Option<&TaskSpec> {
        self.tasks.iter().find(|task| task.task_key == task_key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRevision {
    pub id: PlanRevisionId,
    pub project_id: ProjectId,
    pub revision_number: u64,
    pub blueprint_revision_id: BlueprintRevisionId,
    pub blueprint_content_hash: String,
    pub template_version: String,
    pub status: PlanStatus,
    pub content: PlanContent,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub id: WorkItemId,
    pub project_id: ProjectId,
    pub plan_revision_id: PlanRevisionId,
    pub task_spec_id: TaskSpecId,
    pub task_key: String,
    pub status: WorkItemStatus,
    pub contract_state: ContractState,
    pub priority: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseSummary {
    pub id: LeaseId,
    pub work_item_id: WorkItemId,
    pub agent_profile: String,
    pub status: LeaseStatus,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_heartbeat_at: DateTime<Utc>,
    pub released_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkRun {
    pub id: WorkRunId,
    pub work_item_id: WorkItemId,
    pub lease_id: LeaseId,
    pub agent_profile: String,
    pub claim_request_id: String,
    pub status: WorkRunStatus,
    #[serde(default)]
    pub verification_snapshot: Option<VerificationSnapshot>,
    #[serde(default)]
    pub verification_snapshot_hash: Option<String>,
    #[serde(default)]
    pub review_snapshot: Option<crate::ReviewSnapshot>,
    #[serde(default)]
    pub review_snapshot_hash: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcceptanceResult {
    pub criterion_id: String,
    pub passed: bool,
    pub note: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandRun {
    pub command: String,
    pub exit_code: Option<i32>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkReportInput {
    pub status: ReportOutcome,
    pub summary: String,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub changed_files: Vec<String>,
    pub acceptance_results: Vec<AcceptanceResult>,
    pub commands_run: Vec<CommandRun>,
    pub evidence_refs: Vec<String>,
    pub known_issues: Vec<String>,
    pub follow_up_items: Vec<String>,
    pub contract_hash: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkReportRecord {
    pub id: WorkReportId,
    pub work_item_id: WorkItemId,
    pub work_run_id: WorkRunId,
    pub contract_hash: String,
    pub report: WorkReportInput,
    pub report_hash: String,
    pub accepted: bool,
    pub rejection_reasons: Vec<String>,
    #[serde(default)]
    pub verification_status: crate::VerificationStatus,
    #[serde(default)]
    pub verification: Option<crate::VerificationResult>,
    #[serde(default)]
    pub review_status: crate::ReviewStatus,
    #[serde(default)]
    pub review: Option<crate::ReviewResult>,
    #[serde(default)]
    pub review_waiver: Option<crate::ReviewWaiver>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkBlockerRecord {
    pub work_item_id: WorkItemId,
    pub reason: String,
    pub needed_input: Vec<String>,
    pub proposal: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkAuthorization {
    pub work_item: WorkItem,
    pub task_spec: TaskSpec,
    pub lease: LeaseSummary,
    pub run: WorkRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimFailure {
    NoActivePlan,
    AllDone,
    WaitingDependencies,
    WaitingHuman,
    NoMatchingRepository,
    VerificationNotConfigured,
    ReviewNotConfigured,
    ReviewConsentRequired,
    ReviewCredentialMissing,
}

impl ClaimFailure {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NoActivePlan => "no_active_plan",
            Self::AllDone => "all_done",
            Self::WaitingDependencies => "waiting_dependencies",
            Self::WaitingHuman => "waiting_human",
            Self::NoMatchingRepository => "no_matching_repository",
            Self::VerificationNotConfigured => "verification_not_configured",
            Self::ReviewNotConfigured => "review_not_configured",
            Self::ReviewConsentRequired => "review_consent_required",
            Self::ReviewCredentialMissing => "review_credential_missing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimedWork {
    pub work_item: WorkItem,
    pub lease: LeaseSummary,
    pub run: WorkRun,
    pub lease_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ClaimResult {
    Claimed(ClaimedWork),
    Empty(ClaimFailure),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanValidationError {
    #[error("plan must contain at least one task")]
    EmptyPlan,
    #[error("plan contains {actual} tasks; the limit is {limit}")]
    TooManyTasks { actual: usize, limit: usize },
    #[error("duplicate task key: {0}")]
    DuplicateTaskKey(String),
    #[error("invalid task key: {0}")]
    InvalidTaskKey(String),
    #[error("task {task_key}: {reason}")]
    InvalidTask { task_key: String, reason: String },
    #[error("task {task_key} depends on missing task {dependency}")]
    MissingDependency {
        task_key: String,
        dependency: String,
    },
    #[error("task {0} cannot depend on itself")]
    SelfDependency(String),
    #[error("task {task_key} contains duplicate dependency {dependency}")]
    DuplicateDependency {
        task_key: String,
        dependency: String,
    },
    #[error("blocking dependency cycle detected involving: {0}")]
    BlockingCycle(String),
}

/// 校验计划任务的唯一键、依赖闭包、允许路径和必填契约字段。
pub fn validate_plan(tasks: &[TaskSpec]) -> Result<(), PlanValidationError> {
    if tasks.is_empty() {
        return Err(PlanValidationError::EmptyPlan);
    }
    if tasks.len() > MAX_PLAN_TASKS {
        return Err(PlanValidationError::TooManyTasks {
            actual: tasks.len(),
            limit: MAX_PLAN_TASKS,
        });
    }

    let mut by_key = BTreeMap::<&str, &TaskSpec>::new();
    for task in tasks {
        if !is_safe_task_key(&task.task_key) {
            return Err(PlanValidationError::InvalidTaskKey(task.task_key.clone()));
        }
        if by_key.insert(&task.task_key, task).is_some() {
            return Err(PlanValidationError::DuplicateTaskKey(task.task_key.clone()));
        }
    }

    for task in tasks {
        validate_task(task, &by_key)?;
    }
    validate_blocking_cycles(&by_key)
}

fn validate_task(
    task: &TaskSpec,
    by_key: &BTreeMap<&str, &TaskSpec>,
) -> Result<(), PlanValidationError> {
    if task.title.trim().is_empty() {
        return invalid_task(task, "title cannot be empty");
    }
    if task.goal.trim().is_empty() {
        return invalid_task(task, "goal cannot be empty");
    }
    if task.context_budget_tokens == 0 || task.context_budget_tokens > 100_000 {
        return invalid_task(task, "context budget must be between 1 and 100000 tokens");
    }
    if task.acceptance_criteria.is_empty() && task.kind != TaskKind::Milestone {
        return invalid_task(task, "acceptance criteria cannot be empty");
    }
    let mut criterion_ids = BTreeSet::new();
    for criterion in &task.acceptance_criteria {
        if criterion.id.trim().is_empty() || criterion.text.trim().is_empty() {
            return invalid_task(task, "acceptance criterion id and text must be non-empty");
        }
        if !criterion_ids.insert(&criterion.id) {
            return invalid_task(task, "acceptance criterion ids must be unique");
        }
    }

    if task.kind == TaskKind::AgentCoding {
        require_non_empty(task, "repository key", task.repository_key.as_deref())?;
        require_non_empty(task, "base ref", task.base_ref.as_deref())?;
        require_non_empty(task, "work branch", task.work_branch.as_deref())?;
        if task.allowed_paths.is_empty() {
            return invalid_task(task, "agent coding tasks must declare allowed paths");
        }
        if task.acceptance_policy != AcceptancePolicy::AgentReport {
            return invalid_task(task, "agent coding tasks require agent_report acceptance");
        }
    }
    for path in &task.allowed_paths {
        if !is_safe_relative_path(path) {
            return invalid_task(task, &format!("unsafe allowed path: {path}"));
        }
    }
    if let Some(branch) = &task.work_branch
        && !is_safe_branch(branch)
    {
        return invalid_task(task, &format!("unsafe work branch: {branch}"));
    }

    let mut dependency_pairs = BTreeSet::new();
    for dependency in &task.dependencies {
        if dependency.task_key == task.task_key {
            return Err(PlanValidationError::SelfDependency(task.task_key.clone()));
        }
        if !by_key.contains_key(dependency.task_key.as_str()) {
            return Err(PlanValidationError::MissingDependency {
                task_key: task.task_key.clone(),
                dependency: dependency.task_key.clone(),
            });
        }
        let pair = (dependency.task_key.as_str(), dependency.kind);
        if !dependency_pairs.insert(pair) {
            return Err(PlanValidationError::DuplicateDependency {
                task_key: task.task_key.clone(),
                dependency: dependency.task_key.clone(),
            });
        }
    }
    Ok(())
}

fn require_non_empty(
    task: &TaskSpec,
    field: &str,
    value: Option<&str>,
) -> Result<(), PlanValidationError> {
    if value.is_none_or(|value| value.trim().is_empty()) {
        invalid_task(task, &format!("{field} cannot be empty"))
    } else {
        Ok(())
    }
}

fn invalid_task<T>(task: &TaskSpec, reason: &str) -> Result<T, PlanValidationError> {
    Err(PlanValidationError::InvalidTask {
        task_key: task.task_key.clone(),
        reason: reason.to_owned(),
    })
}

fn validate_blocking_cycles(by_key: &BTreeMap<&str, &TaskSpec>) -> Result<(), PlanValidationError> {
    fn visit<'a>(
        key: &'a str,
        by_key: &BTreeMap<&'a str, &'a TaskSpec>,
        visiting: &mut BTreeSet<&'a str>,
        visited: &mut BTreeSet<&'a str>,
    ) -> Result<(), PlanValidationError> {
        if visited.contains(key) {
            return Ok(());
        }
        if !visiting.insert(key) {
            return Err(PlanValidationError::BlockingCycle(key.to_owned()));
        }
        let task = by_key[key];
        for dependency in task.blocking_dependencies() {
            visit(dependency, by_key, visiting, visited)?;
        }
        visiting.remove(key);
        visited.insert(key);
        Ok(())
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for key in by_key.keys().copied() {
        visit(key, by_key, &mut visiting, &mut visited)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReportValidationError {
    #[error("only a completed report can finish work")]
    IncompleteOutcome,
    #[error("report summary cannot be empty")]
    EmptySummary,
    #[error("report contract hash does not match the active task contract")]
    ContractMismatch,
    #[error("report idempotency key cannot be empty")]
    EmptyIdempotencyKey,
    #[error("report references unknown acceptance criterion {0}")]
    UnknownAcceptanceCriterion(String),
    #[error("report is missing acceptance criterion {0}")]
    MissingAcceptanceCriterion(String),
    #[error("acceptance criterion {0} is reported more than once")]
    DuplicateAcceptanceCriterion(String),
    #[error("acceptance criterion {0} did not pass")]
    FailedAcceptanceCriterion(String),
    #[error("acceptance criterion {0} requires evidence")]
    MissingAcceptanceEvidence(String),
    #[error("changed file is outside allowed paths: {0}")]
    PathOutsideContract(String),
    #[error("unsafe changed file path: {0}")]
    UnsafeChangedPath(String),
    #[error("agent coding report did not use branch {0}")]
    WrongBranch(String),
    #[error("agent coding report must list at least one changed file")]
    EmptyChangedFiles,
}

/// 校验 Agent 报告与任务契约、提交范围和验证结果的绑定关系。
pub fn validate_work_report(
    task: &TaskSpec,
    report: &WorkReportInput,
) -> Result<(), ReportValidationError> {
    if report.status != ReportOutcome::Completed {
        return Err(ReportValidationError::IncompleteOutcome);
    }
    if report.summary.trim().is_empty() {
        return Err(ReportValidationError::EmptySummary);
    }
    if report.contract_hash != task.contract_hash() {
        return Err(ReportValidationError::ContractMismatch);
    }
    if report.idempotency_key.trim().is_empty() {
        return Err(ReportValidationError::EmptyIdempotencyKey);
    }

    let criteria = task
        .acceptance_criteria
        .iter()
        .map(|criterion| (criterion.id.as_str(), criterion))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    for result in &report.acceptance_results {
        let Some(criterion) = criteria.get(result.criterion_id.as_str()) else {
            return Err(ReportValidationError::UnknownAcceptanceCriterion(
                result.criterion_id.clone(),
            ));
        };
        if !seen.insert(result.criterion_id.as_str()) {
            return Err(ReportValidationError::DuplicateAcceptanceCriterion(
                result.criterion_id.clone(),
            ));
        }
        if !result.passed {
            return Err(ReportValidationError::FailedAcceptanceCriterion(
                result.criterion_id.clone(),
            ));
        }
        if criterion.required_evidence
            && result
                .evidence_refs
                .iter()
                .all(|item| item.trim().is_empty())
            && report
                .evidence_refs
                .iter()
                .all(|item| item.trim().is_empty())
        {
            return Err(ReportValidationError::MissingAcceptanceEvidence(
                result.criterion_id.clone(),
            ));
        }
    }
    for criterion in &task.acceptance_criteria {
        if !seen.contains(criterion.id.as_str()) {
            return Err(ReportValidationError::MissingAcceptanceCriterion(
                criterion.id.clone(),
            ));
        }
    }

    if task.kind == TaskKind::AgentCoding {
        let required_branch = task
            .work_branch
            .as_deref()
            .expect("validated agent task must have a branch");
        if report.branch.as_deref() != Some(required_branch) {
            return Err(ReportValidationError::WrongBranch(
                required_branch.to_owned(),
            ));
        }
        if report.changed_files.is_empty() {
            return Err(ReportValidationError::EmptyChangedFiles);
        }
    }
    for changed_file in &report.changed_files {
        if !is_safe_relative_path(changed_file) {
            return Err(ReportValidationError::UnsafeChangedPath(
                changed_file.clone(),
            ));
        }
        if !path_matches_any(changed_file, &task.allowed_paths) {
            return Err(ReportValidationError::PathOutsideContract(
                changed_file.clone(),
            ));
        }
    }
    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

pub fn is_safe_task_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 220
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | ':' | '.')
        })
}

/// 判断路径是否为仓库内规范相对路径，拒绝绝对路径、穿越、反斜杠和控制字符。
pub fn is_safe_relative_path(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.contains('\0') {
        return false;
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return false;
    }
    if value.len() >= 2 && value.as_bytes()[1] == b':' {
        return false;
    }
    path.components().all(|component| match component {
        Component::Normal(_) | Component::CurDir => true,
        Component::ParentDir | Component::RootDir | Component::Prefix(_) => false,
    })
}

fn is_safe_branch(value: &str) -> bool {
    !value.trim().is_empty()
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains("..")
        && !value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
}

fn path_matches_any(changed_file: &str, allowed_paths: &[String]) -> bool {
    let changed = normalize_relative_path(changed_file);
    allowed_paths.iter().any(|allowed| {
        let allowed = normalize_relative_path(allowed);
        allowed == "."
            || changed == allowed
            || changed
                .strip_prefix(&allowed)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}

fn normalize_relative_path(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_end_matches('/')
        .to_lowercase()
}

macro_rules! impl_string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = PlanEnumError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($value => Ok(Self::$variant)),+,
                    _ => Err(PlanEnumError {
                        kind: stringify!($name),
                        value: value.to_owned(),
                    }),
                }
            }
        }
    };
}

impl_string_enum!(PlanStatus {
    Draft => "draft",
    Published => "published",
    Superseded => "superseded",
});
impl_string_enum!(TaskKind {
    AgentCoding => "agent_coding",
    HumanExternal => "human_external",
    ApprovalGate => "approval_gate",
    Milestone => "milestone",
});
impl_string_enum!(AcceptancePolicy {
    AgentReport => "agent_report",
    HumanAcceptance => "human_acceptance",
});
impl_string_enum!(WorkItemStatus {
    Draft => "draft",
    Ready => "ready",
    Claimed => "claimed",
    InProgress => "in_progress",
    Submitted => "submitted",
    Done => "done",
    Blocked => "blocked",
    Failed => "failed",
    Reopened => "reopened",
    Cancelled => "cancelled",
});
impl_string_enum!(ContractState {
    Current => "current",
    Stale => "stale",
});
impl_string_enum!(DependencyKind {
    Blocks => "blocks",
    Informs => "informs",
    ConflictsWith => "conflicts_with",
});
impl_string_enum!(LeaseStatus {
    Active => "active",
    Released => "released",
    Expired => "expired",
    Consumed => "consumed",
});
impl_string_enum!(WorkRunStatus {
    Claimed => "claimed",
    Running => "running",
    Submitted => "submitted",
    Completed => "completed",
    Failed => "failed",
    Expired => "expired",
    Released => "released",
    Blocked => "blocked",
});
impl_string_enum!(ReportOutcome {
    Completed => "completed",
    Failed => "failed",
    Blocked => "blocked",
});

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown {kind} value: {value}")]
pub struct PlanEnumError {
    kind: &'static str,
    value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(key: &str, dependency: Option<&str>) -> TaskSpec {
        TaskSpec {
            task_key: key.to_owned(),
            title: key.to_owned(),
            kind: TaskKind::AgentCoding,
            goal: "实现一个可验收切片".to_owned(),
            scope: vec!["当前模块".to_owned()],
            out_of_scope: vec!["其他模块".to_owned()],
            module_id: None,
            platform_id: None,
            repository_key: Some("server".to_owned()),
            base_ref: Some("main".to_owned()),
            work_branch: Some(format!("codex/{key}")),
            allowed_paths: vec!["src".to_owned()],
            context_refs: vec![],
            acceptance_criteria: vec![AcceptanceCriterion {
                id: "implementation".to_owned(),
                text: "实现完成".to_owned(),
                required_evidence: true,
            }],
            acceptance_policy: AcceptancePolicy::AgentReport,
            evidence_requirements: vec!["测试摘要".to_owned()],
            priority: 100,
            critical_path: true,
            context_budget_tokens: 12_000,
            resource_keys: vec!["server".to_owned()],
            dependencies: dependency
                .map(|key| TaskDependency {
                    task_key: key.to_owned(),
                    kind: DependencyKind::Blocks,
                })
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn rejects_a_cycle_in_blocking_dependencies() {
        let error = validate_plan(&[task("a", Some("b")), task("b", Some("a"))])
            .expect_err("a blocks cycle must be rejected");
        assert!(error.to_string().contains("cycle"));
    }

    #[test]
    fn contract_hash_is_stable_and_changes_with_the_contract() {
        let original = task("slice:module:server", None);
        let mut changed = original.clone();
        changed.allowed_paths.push("migrations".to_owned());

        assert_eq!(original.contract_hash(), original.contract_hash());
        assert_ne!(original.contract_hash(), changed.contract_hash());
    }

    #[test]
    fn work_item_status_rejects_skipping_claim() {
        assert!(!WorkItemStatus::Ready.can_transition_to(WorkItemStatus::InProgress));
        assert!(WorkItemStatus::Ready.can_transition_to(WorkItemStatus::Claimed));
        assert!(WorkItemStatus::Claimed.can_transition_to(WorkItemStatus::InProgress));
    }

    #[test]
    fn replanning_inherits_only_safe_task_state() {
        use WorkItemStatus::*;

        assert_eq!(inherited_work_status(Done, false), Done);
        assert_eq!(inherited_work_status(Ready, false), Ready);
        assert_eq!(inherited_work_status(Cancelled, false), Cancelled);
        assert_eq!(inherited_work_status(Done, true), Reopened);
        assert_eq!(inherited_work_status(Cancelled, true), Reopened);
        assert_eq!(inherited_work_status(InProgress, false), Reopened);
        assert_eq!(inherited_work_status(Submitted, true), Reopened);
    }

    #[test]
    fn plan_publish_preview_round_trips_as_camel_case() {
        use WorkItemStatus::{Done, Reopened};

        let preview = PlanPublishPreview {
            base_revision_id: Some(PlanRevisionId::new()),
            base_revision_number: Some(1),
            candidate_revision_id: PlanRevisionId::new(),
            candidate_revision_number: 2,
            added: vec![],
            changed: vec![PlanTaskDiff {
                task_key: "slice:orders".to_owned(),
                title: "订单切片".to_owned(),
                previous_status: Some(Done),
                next_status: Some(Reopened),
                contract_changed: true,
            }],
            unchanged: vec![],
            removed: vec![],
            carried_completed: 0,
            reopened: 1,
            cancelled: 0,
            active_lease_count: 0,
            publishable: true,
            blockers: vec![],
        };

        let json = serde_json::to_string(&preview).expect("preview must serialize");
        assert!(json.contains("\"candidateRevisionNumber\":2"));
        assert!(json.contains("\"contractChanged\":true"));
        let restored: PlanPublishPreview =
            serde_json::from_str(&json).expect("preview must deserialize");
        assert_eq!(restored, preview);
    }
}
