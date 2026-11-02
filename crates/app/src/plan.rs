use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use chrono::{Duration, Utc};
use coldstart_adapters::{
    GitAdapter, GitCommitVerification, ModelConfig, OpenAiCompatibleReviewModel, ReviewModel,
    ReviewModelRequest, ReviewStandardPrompt, SecretStore, VerificationCommandExecution,
    VerificationCommandExecutor, WindowsSecretStore, WindowsVerificationExecutor,
    sanitize_verification_output,
};
use coldstart_core::{
    ClaimResult, DEFAULT_LEASE_MINUTES, GitSyncRecord, GitSyncStatus, LeaseSummary, PlanContent,
    PlanPublishPreview, PlanRevision, PlanRevisionId, PlanStatus, Repository, RepositorySyncPolicy,
    ReviewFailure, ReviewResult, ReviewScope, ReviewSnapshot, ReviewStatus, TaskContext, TaskKind,
    TaskReferenceContent, VerificationCommandResult, VerificationFailureKind, VerificationResult,
    VerificationSnapshot, VerificationStatus, WorkBlockerRecord, WorkItem, WorkItemId,
    WorkReportInput, WorkReportRecord, compile_plan, has_blocking_findings,
};
use coldstart_storage::{BeginReview, BeginVerification, NewWorkBlocker, PendingReview};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    AppError, PlanGitGateway, PlanWorkspace,
    context::OpenProject,
    context_compiler::{compile_reference, compile_task_context},
    integration::git_auth_for_repository,
    plan_workspace::build_plan_workspace,
    review::{read_review_snapshot_standard, validate_review_standards_for_claim},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRequest {
    pub agent_profile: String,
    pub repository: Option<String>,
    pub request_id: String,
}

type ReviewClaimBases = BTreeMap<(String, String), String>;

#[derive(Clone)]
pub struct PlanService {
    secrets: Arc<dyn SecretStore>,
    git: Arc<dyn PlanGitGateway>,
    verification: Arc<dyn VerificationCommandExecutor>,
    review_model: Arc<dyn ReviewModel>,
}

impl PlanService {
    pub fn new() -> Self {
        Self::with_gateways(
            Arc::new(WindowsSecretStore::new()),
            Arc::new(GitAdapter::new()),
            Arc::new(WindowsVerificationExecutor),
        )
    }

    pub fn with_dependencies(secrets: Arc<dyn SecretStore>, git: GitAdapter) -> Self {
        Self::with_gateways(
            secrets,
            Arc::new(git),
            Arc::new(WindowsVerificationExecutor),
        )
    }

    pub fn with_gateways(
        secrets: Arc<dyn SecretStore>,
        git: Arc<dyn PlanGitGateway>,
        verification: Arc<dyn VerificationCommandExecutor>,
    ) -> Self {
        Self::with_review_model(
            secrets,
            git,
            verification,
            Arc::new(OpenAiCompatibleReviewModel::new()),
        )
    }

    pub fn with_review_model(
        secrets: Arc<dyn SecretStore>,
        git: Arc<dyn PlanGitGateway>,
        verification: Arc<dyn VerificationCommandExecutor>,
        review_model: Arc<dyn ReviewModel>,
    ) -> Self {
        Self {
            secrets,
            git,
            verification,
            review_model,
        }
    }

    pub fn load_workspace(&self, project_home: PathBuf) -> Result<PlanWorkspace, AppError> {
        build_plan_workspace(&OpenProject::open(project_home)?)
    }

    pub fn generate_draft(&self, project_home: PathBuf) -> Result<PlanWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let blueprint = project
            .database
            .latest_blueprint(
                &project.project.id,
                coldstart_core::BlueprintStatus::Published,
            )?
            .ok_or(AppError::PublishedBlueprintRequired)?;
        let content = compile_plan(&project.project.id, &blueprint.content)?;
        project.database.create_plan_revision(
            &project.project.id,
            blueprint.id,
            content,
            Utc::now(),
        )?;
        build_plan_workspace(&project)
    }

    pub fn save_draft(
        &self,
        project_home: PathBuf,
        content: PlanContent,
    ) -> Result<PlanWorkspace, AppError> {
        content
            .validate()
            .map_err(|error| AppError::InvalidPlan(error.to_string()))?;
        let mut project = OpenProject::open(project_home)?;
        let blueprint = project
            .database
            .latest_blueprint(
                &project.project.id,
                coldstart_core::BlueprintStatus::Published,
            )?
            .ok_or(AppError::PublishedBlueprintRequired)?;
        project.database.create_plan_revision(
            &project.project.id,
            blueprint.id,
            content,
            Utc::now(),
        )?;
        build_plan_workspace(&project)
    }

    pub fn preview_publish(
        &self,
        project_home: PathBuf,
        revision_id: PlanRevisionId,
    ) -> Result<PlanPublishPreview, AppError> {
        let mut project = OpenProject::open(project_home)?;
        Ok(project
            .database
            .preview_plan_revision(&project.project.id, &revision_id, Utc::now())?)
    }

    pub fn publish(
        &self,
        project_home: PathBuf,
        revision_id: PlanRevisionId,
    ) -> Result<PlanWorkspace, AppError> {
        self.publish_confirmed(project_home, revision_id, false)
    }

    pub fn publish_confirmed(
        &self,
        project_home: PathBuf,
        revision_id: PlanRevisionId,
        confirmed: bool,
    ) -> Result<PlanWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let preview = project.database.preview_plan_revision(
            &project.project.id,
            &revision_id,
            Utc::now(),
        )?;
        if preview.base_revision_id.is_some() && !confirmed {
            return Err(AppError::ReplanConfirmationRequired);
        }
        project
            .database
            .publish_plan_revision(&project.project.id, &revision_id, Utc::now())?;
        build_plan_workspace(&project)
    }

    pub fn complete_human_work(
        &self,
        project_home: PathBuf,
        work_item_id: WorkItemId,
        note: String,
        accepted: bool,
    ) -> Result<PlanWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        project.database.complete_human_work(
            &project.project.id,
            &work_item_id,
            &note,
            accepted,
            Utc::now(),
        )?;
        build_plan_workspace(&project)
    }

    pub fn claim_next(
        &self,
        project_home: PathBuf,
        request: ClaimRequest,
    ) -> Result<ClaimResult, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let review_base_commits =
            self.prepare_review_claim(&project, request.repository.as_deref())?;
        let claimed = project.database.claim_work_with_review_bases(
            &project.project.id,
            &request.agent_profile,
            request.repository.as_deref(),
            &request.request_id,
            Utc::now(),
            default_lease_duration(),
            review_base_commits.as_ref(),
        )?;
        let ClaimResult::Claimed(claimed) = claimed else {
            return Ok(claimed);
        };
        let task = project
            .database
            .task_spec_for_work_item(&claimed.work_item.id)?
            .ok_or(AppError::WorkItemNotFound(claimed.work_item.id))?;
        let Some(repository) = repository_for_task(&project, &task)? else {
            return Ok(ClaimResult::Claimed(claimed));
        };
        if let Err(error) = self.prepare_claim(&project, &task, &repository) {
            let reason = format!("Git preparation failed: {}", error.code());
            return match project.database.release_work(
                &claimed.work_item.id,
                &claimed.lease_token,
                &reason,
                Utc::now(),
                default_lease_duration(),
            ) {
                Ok(_) => Err(error),
                Err(release_error) => Err(AppError::GitPreparationFailed {
                    source: Box::new(error),
                    release_error: release_error.to_string(),
                }),
            };
        }
        if let Some(snapshot) = claimed.run.review_snapshot.as_ref() {
            let base_ref = task
                .base_ref
                .as_deref()
                .unwrap_or(&repository.default_branch);
            let actual_base = self.git.resolve_commit(&repository.local_path, base_ref)?;
            if actual_base != snapshot.base_commit {
                let reason = "Review base commit changed during claim preparation".to_owned();
                project.database.release_work(
                    &claimed.work_item.id,
                    &claimed.lease_token,
                    &reason,
                    Utc::now(),
                    default_lease_duration(),
                )?;
                return Err(AppError::ReviewNotConfigured(claimed.work_item.id));
            }
        }
        Ok(ClaimResult::Claimed(claimed))
    }

    pub fn get_context(
        &self,
        project_home: PathBuf,
        work_item_id: WorkItemId,
        lease_token: String,
    ) -> Result<TaskContext, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let authorization = project.database.authorize_work(
            &work_item_id,
            &lease_token,
            Utc::now(),
            default_lease_duration(),
        )?;
        let plan = published_plan_for(&project, &authorization.work_item)?;
        let work_items = project
            .database
            .list_work_items_for_plan(&project.project.id, &plan.id)?;
        compile_task_context(
            &project,
            &plan,
            &authorization.task_spec,
            &work_items,
            authorization.run.verification_snapshot.clone(),
            authorization.run.review_snapshot.as_ref(),
        )
    }

    pub fn read_reference(
        &self,
        project_home: PathBuf,
        work_item_id: WorkItemId,
        lease_token: String,
        reference_id: String,
    ) -> Result<TaskReferenceContent, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let authorization = project.database.authorize_work(
            &work_item_id,
            &lease_token,
            Utc::now(),
            default_lease_duration(),
        )?;
        let plan = published_plan_for(&project, &authorization.work_item)?;
        let work_items = project
            .database
            .list_work_items_for_plan(&project.project.id, &plan.id)?;
        compile_reference(
            &project,
            &plan,
            &authorization.task_spec,
            &reference_id,
            &work_items,
            authorization.run.review_snapshot.as_ref(),
        )?
        .ok_or(AppError::ReferenceNotFound(reference_id))
    }

    pub fn heartbeat(
        &self,
        project_home: PathBuf,
        work_item_id: WorkItemId,
        lease_token: String,
    ) -> Result<LeaseSummary, AppError> {
        let mut project = OpenProject::open(project_home)?;
        Ok(project.database.heartbeat_work(
            &work_item_id,
            &lease_token,
            Utc::now(),
            default_lease_duration(),
        )?)
    }

    pub fn report_blocker(
        &self,
        project_home: PathBuf,
        work_item_id: WorkItemId,
        lease_token: String,
        reason: String,
        needed_input: Vec<String>,
        proposal: Option<String>,
    ) -> Result<WorkBlockerRecord, AppError> {
        let mut project = OpenProject::open(project_home)?;
        Ok(project.database.report_work_blocker(
            &work_item_id,
            &lease_token,
            NewWorkBlocker {
                reason,
                needed_input,
                proposal,
            },
            Utc::now(),
            default_lease_duration(),
        )?)
    }

    pub fn submit_report(
        &self,
        project_home: PathBuf,
        work_item_id: WorkItemId,
        lease_token: String,
        report: WorkReportInput,
    ) -> Result<WorkReportRecord, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let task = project
            .database
            .task_spec_for_work_item(&work_item_id)?
            .ok_or(AppError::WorkItemNotFound(work_item_id))?;
        let repository = repository_for_task(&project, &task)?;
        let git_verification = if let Some(repository) = repository.as_ref() {
            let branch = report
                .branch
                .as_deref()
                .ok_or_else(|| AppError::ReportRejected("work branch is required".to_owned()))?;
            let commit_sha = report
                .commit_sha
                .as_deref()
                .ok_or_else(|| AppError::ReportRejected("commit SHA is required".to_owned()))?;
            let base_ref = task
                .base_ref
                .as_deref()
                .unwrap_or(&repository.default_branch);
            let verification = self.git.verify_report_commit(
                &repository.local_path,
                branch,
                base_ref,
                commit_sha,
                &task.allowed_paths,
                &repository.protected_branches,
            )?;
            ensure_reported_files_match(&report, &verification.changed_files)?;
            Some(verification)
        } else {
            None
        };
        let pending = match project.database.begin_work_verification(
            &work_item_id,
            &lease_token,
            report,
            Utc::now(),
            default_lease_duration(),
        )? {
            BeginVerification::AlreadyResolved(record) => {
                return resolved_verification_report(*record);
            }
            BeginVerification::Started(pending) => pending,
        };

        let repository = repository.ok_or(AppError::VerificationNotConfigured(work_item_id))?;
        let git_verification =
            git_verification.ok_or(AppError::VerificationNotConfigured(work_item_id))?;
        let started_at = pending
            .report
            .verification
            .as_ref()
            .map(|verification| verification.started_at)
            .unwrap_or_else(Utc::now);
        let result = self.run_repository_verification(
            &repository.local_path,
            &pending.snapshot,
            &git_verification,
            started_at,
        );
        if result.status != VerificationStatus::Passed {
            project.database.finish_work_verification_failure(
                &project.project.id,
                &work_item_id,
                &pending.report.id,
                result.clone(),
                Utc::now(),
                default_lease_duration(),
            )?;
            return Err(verification_outcome_error(&result));
        }

        if pending.review.is_some() {
            let begin = project.database.finish_work_verification_and_begin_review(
                &project.project.id,
                &work_item_id,
                &pending.report.id,
                result,
                Utc::now(),
            )?;
            return match begin {
                BeginReview::AlreadyResolved(record) => resolved_verification_report(record),
                BeginReview::Running(_) => Err(AppError::ReviewInProgress(work_item_id)),
                BeginReview::Started(review) => {
                    let record = self.execute_review(
                        &mut project,
                        &task,
                        &repository,
                        &git_verification,
                        review,
                    )?;
                    self.push_completed_report(
                        &mut project,
                        work_item_id,
                        &repository,
                        &git_verification,
                    )?;
                    Ok(record)
                }
            };
        }

        let record = project.database.finish_work_verification_success(
            &project.project.id,
            &work_item_id,
            &pending.report.id,
            result,
            Utc::now(),
        )?;
        if record.accepted {
            self.push_completed_report(&mut project, work_item_id, &repository, &git_verification)?;
        }
        Ok(record)
    }

    pub fn waive_review(
        &self,
        project_home: PathBuf,
        work_item_id: WorkItemId,
        report_id: coldstart_core::WorkReportId,
        reason: String,
    ) -> Result<WorkReportRecord, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let task = project
            .database
            .task_spec_for_work_item(&work_item_id)?
            .ok_or(AppError::WorkItemNotFound(work_item_id))?;
        let repository = repository_for_task(&project, &task)?
            .ok_or(AppError::ReviewNotConfigured(work_item_id))?;
        let existing = project
            .database
            .list_work_reports(&project.project.id)?
            .into_iter()
            .find(|report| report.id == report_id && report.work_item_id == work_item_id)
            .ok_or_else(|| AppError::ReportRejected("work report does not exist".to_owned()))?;
        let waived_by = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "local-operator".to_owned());
        let record = project.database.waive_work_review(
            &project.project.id,
            &work_item_id,
            &report_id,
            &reason,
            &waived_by,
            Utc::now(),
        )?;
        if record.accepted {
            let checkout =
                GitCommitVerification {
                    commit_sha: existing.report.commit_sha.clone().ok_or_else(|| {
                        AppError::ReportRejected("commit SHA is required".to_owned())
                    })?,
                    branch: existing.report.branch.clone().ok_or_else(|| {
                        AppError::ReportRejected("work branch is required".to_owned())
                    })?,
                    changed_files: existing.report.changed_files.clone(),
                };
            self.push_completed_report(&mut project, work_item_id, &repository, &checkout)?;
        }
        Ok(record)
    }

    fn execute_review(
        &self,
        project: &mut OpenProject,
        task: &coldstart_core::TaskSpec,
        repository: &Repository,
        checkout: &GitCommitVerification,
        pending: PendingReview,
    ) -> Result<WorkReportRecord, AppError> {
        let PendingReview { report, snapshot } = pending;
        let started_at = Utc::now();
        let api_key = match self.review_api_key(project, &snapshot) {
            Ok(api_key) => api_key,
            Err(error) => {
                return self.fail_review(
                    project,
                    report.work_item_id,
                    report.id,
                    &snapshot,
                    started_at,
                    "review_credential_missing",
                    "AI Review 模型凭据不可用".to_owned(),
                    error,
                );
            }
        };

        let diff = match self.git.extract_review_diff(
            &repository.local_path,
            &snapshot.base_commit,
            &checkout.commit_sha,
        ) {
            Ok(diff) => diff,
            Err(error) => {
                let app_error = AppError::Git(error);
                return self.fail_review(
                    project,
                    report.work_item_id,
                    report.id,
                    &snapshot,
                    started_at,
                    app_error.code(),
                    "无法从真实 Git 提交提取 Review diff".to_owned(),
                    app_error,
                );
            }
        };
        if diff.base_commit != snapshot.base_commit
            || diff.head_commit != checkout.commit_sha
            || !same_changed_files(&diff.changed_files(), &checkout.changed_files)
        {
            return self.fail_review(
                project,
                report.work_item_id,
                report.id,
                &snapshot,
                started_at,
                "review_failed",
                "Review diff 与领取快照或报告提交不一致".to_owned(),
                AppError::ReviewFailed {
                    details: json!({ "failureCode": "review_diff_mismatch" }),
                },
            );
        }

        let mut standards = Vec::with_capacity(snapshot.standards.len());
        for standard in &snapshot.standards {
            let content = match read_review_snapshot_standard(project, standard) {
                Ok(content) => content,
                Err(error) => {
                    return self.fail_review(
                        project,
                        report.work_item_id,
                        report.id,
                        &snapshot,
                        started_at,
                        "review_standard_invalid",
                        "AI Review 规范正文校验失败".to_owned(),
                        error,
                    );
                }
            };
            standards.push(ReviewStandardPrompt {
                name: standard.name.clone(),
                scope: match &standard.scope {
                    ReviewScope::Project => "project".to_owned(),
                    ReviewScope::Repository(repository_key) => repository_key.clone(),
                },
                content,
            });
        }

        let request = ReviewModelRequest {
            config: ModelConfig {
                base_url: snapshot.configuration.base_url.clone(),
                model: snapshot.configuration.model.clone(),
                api_key,
            },
            task_title: task.title.clone(),
            task_goal: task.goal.clone(),
            acceptance_criteria: task
                .acceptance_criteria
                .iter()
                .map(|criterion| criterion.text.clone())
                .collect(),
            standards,
            verification_summary: Some(
                "ColdStart 已在报告提交的 Git commit 上独立重跑全部仓库验证命令，并全部通过。"
                    .to_owned(),
            ),
            diff,
        };
        let output = match self.review_model.review(&request) {
            Ok(output) => output,
            Err(error) => {
                let code = error.code();
                let message = error.safe_message();
                let app_error = AppError::ReviewModelFailure {
                    code,
                    details: json!({
                        "failureCode": code,
                        "message": message,
                        "model": snapshot.configuration.model,
                    }),
                };
                return self.fail_review(
                    project,
                    report.work_item_id,
                    report.id,
                    &snapshot,
                    started_at,
                    code,
                    message,
                    app_error,
                );
            }
        };

        let blocked = has_blocking_findings(
            &output.findings,
            &snapshot.configuration.blocking_severities,
        );
        let finished_at = Utc::now();
        let review = ReviewResult {
            snapshot_hash: snapshot.content_hash(),
            status: if blocked {
                ReviewStatus::Blocked
            } else {
                ReviewStatus::Passed
            },
            summary: output.summary,
            findings: output.findings,
            model: snapshot.configuration.model.clone(),
            failure: None,
            started_at,
            finished_at: Some(finished_at),
            duration_ms: Some(
                (finished_at - started_at)
                    .num_milliseconds()
                    .max(0)
                    .try_into()
                    .unwrap_or(u64::MAX),
            ),
        };
        if blocked {
            project.database.finish_work_review_failure(
                &project.project.id,
                &report.work_item_id,
                &report.id,
                review.clone(),
                Utc::now(),
                default_lease_duration(),
            )?;
            return Err(AppError::ReviewBlocked {
                details: review_details_json(&review, report.id),
            });
        }
        project
            .database
            .finish_work_review_success(
                &project.project.id,
                &report.work_item_id,
                &report.id,
                review,
                Utc::now(),
            )
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    fn fail_review(
        &self,
        project: &mut OpenProject,
        work_item_id: WorkItemId,
        report_id: coldstart_core::WorkReportId,
        snapshot: &ReviewSnapshot,
        started_at: chrono::DateTime<Utc>,
        failure_code: &str,
        message: String,
        error: AppError,
    ) -> Result<WorkReportRecord, AppError> {
        let finished_at = Utc::now();
        let review = ReviewResult {
            snapshot_hash: snapshot.content_hash(),
            status: ReviewStatus::Failed,
            summary: "AI Review 未能完成；任务保持进行中且不会推送。".to_owned(),
            findings: Vec::new(),
            model: snapshot.configuration.model.clone(),
            failure: Some(ReviewFailure {
                code: failure_code.to_owned(),
                message,
            }),
            started_at,
            finished_at: Some(finished_at),
            duration_ms: Some(
                (finished_at - started_at)
                    .num_milliseconds()
                    .max(0)
                    .try_into()
                    .unwrap_or(u64::MAX),
            ),
        };
        project.database.finish_work_review_failure(
            &project.project.id,
            &work_item_id,
            &report_id,
            review,
            Utc::now(),
            default_lease_duration(),
        )?;
        Err(error)
    }

    fn review_api_key(
        &self,
        project: &OpenProject,
        snapshot: &ReviewSnapshot,
    ) -> Result<String, AppError> {
        let Some(credential_id) = snapshot.configuration.credential_id else {
            return Ok(String::new());
        };
        let credential = project
            .database
            .credential(&project.project.id, &credential_id)?
            .ok_or(AppError::ReviewCredentialMissing)?;
        self.secrets
            .get(&credential.secret_ref)
            .map_err(|_| AppError::ReviewCredentialMissing)
    }

    fn push_completed_report(
        &self,
        project: &mut OpenProject,
        work_item_id: WorkItemId,
        repository: &Repository,
        checkout: &GitCommitVerification,
    ) -> Result<(), AppError> {
        if project
            .database
            .list_git_syncs(&project.project.id)?
            .iter()
            .any(|sync| {
                sync.work_item_id == work_item_id
                    && sync.branch == checkout.branch
                    && sync.commit_sha == checkout.commit_sha
                    && sync.status == GitSyncStatus::Pushed
            })
        {
            return Ok(());
        }
        let push = git_auth_for_repository(project, repository, self.secrets.as_ref())
            .map_err(|error| error.to_string())
            .and_then(|auth| {
                self.git
                    .validate_remote(&repository.local_path, "origin", &repository.remote_url)
                    .map_err(|error| error.to_string())?;
                self.git
                    .push_branch(
                        &repository.local_path,
                        &checkout.branch,
                        &repository.protected_branches,
                        &auth,
                    )
                    .map_err(|error| error.to_string())
            });
        let sync = match push {
            Ok(()) => GitSyncRecord::new(
                project.project.id,
                work_item_id,
                repository.id,
                checkout.branch.clone(),
                checkout.commit_sha.clone(),
                GitSyncStatus::Pushed,
                None,
            ),
            Err(error) => GitSyncRecord::new(
                project.project.id,
                work_item_id,
                repository.id,
                checkout.branch.clone(),
                checkout.commit_sha.clone(),
                GitSyncStatus::Failed,
                Some(error),
            ),
        };
        project.database.record_git_sync(&sync)?;
        Ok(())
    }

    fn run_repository_verification(
        &self,
        repository: &Path,
        snapshot: &VerificationSnapshot,
        checkout: &GitCommitVerification,
        started_at: chrono::DateTime<Utc>,
    ) -> VerificationResult {
        let wall_started = Instant::now();
        let mut command_results = Vec::with_capacity(snapshot.commands.len());
        let mut failure_kind = None;
        let mut failure_command = None;

        for command in &snapshot.commands {
            if let Err(error) = self.git.ensure_verification_checkout(
                repository,
                &checkout.branch,
                &checkout.commit_sha,
            ) {
                command_results.push(VerificationCommandResult {
                    name: command.name.clone(),
                    exit_code: None,
                    duration_ms: 0,
                    output_summary: bounded_command_detail(
                        "",
                        &format!("Git invariant check failed before command: {error}"),
                    ),
                    truncated: false,
                    failure_kind: Some(VerificationFailureKind::GitInvariant),
                });
                failure_kind = Some(VerificationFailureKind::GitInvariant);
                failure_command = Some(command.name.clone());
                break;
            }

            let execution = self.verification.execute(command, repository);
            let after_check = self.git.ensure_verification_checkout(
                repository,
                &checkout.branch,
                &checkout.commit_sha,
            );
            let primary_failure = classify_command_execution(&execution);
            let command_failure = if after_check.is_err() {
                Some(VerificationFailureKind::GitInvariant)
            } else {
                primary_failure
            };
            let mut output_summary = bounded_command_detail("", execution.output_summary.as_str());
            let mut truncated = execution.truncated;
            if let Some(message) = execution
                .infrastructure_error
                .as_deref()
                .or(execution.launch_error.as_deref())
            {
                truncated |= append_bounded_command_detail(&mut output_summary, message);
            }
            if let Err(error) = &after_check {
                truncated |= append_bounded_command_detail(
                    &mut output_summary,
                    &format!("Git invariant check failed after command: {error}"),
                );
            }
            command_results.push(VerificationCommandResult {
                name: command.name.clone(),
                exit_code: execution.exit_code,
                duration_ms: execution.duration_ms,
                output_summary,
                truncated,
                failure_kind: command_failure,
            });
            if command_failure.is_some() {
                failure_kind = command_failure;
                failure_command = Some(command.name.clone());
                break;
            }
        }

        let status = if failure_kind.is_some() {
            VerificationStatus::Failed
        } else {
            VerificationStatus::Passed
        };
        VerificationResult {
            snapshot_hash: snapshot.content_hash(),
            status,
            started_at,
            finished_at: Some(Utc::now()),
            total_duration_ms: Some(
                u64::try_from(wall_started.elapsed().as_millis()).unwrap_or(u64::MAX),
            ),
            failure_kind,
            failure_command,
            commands: command_results,
        }
    }

    pub fn release(
        &self,
        project_home: PathBuf,
        work_item_id: WorkItemId,
        lease_token: String,
        reason: String,
    ) -> Result<WorkItem, AppError> {
        let mut project = OpenProject::open(project_home)?;
        Ok(project.database.release_work(
            &work_item_id,
            &lease_token,
            &reason,
            Utc::now(),
            default_lease_duration(),
        )?)
    }

    fn prepare_claim(
        &self,
        project: &OpenProject,
        task: &coldstart_core::TaskSpec,
        repository: &Repository,
    ) -> Result<(), AppError> {
        if task.kind != TaskKind::AgentCoding {
            return Ok(());
        }
        let Some(branch) = task.work_branch.as_deref() else {
            return Ok(());
        };
        let auth = git_auth_for_repository(project, repository, self.secrets.as_ref())?;
        self.git
            .validate_remote(&repository.local_path, "origin", &repository.remote_url)?;
        if repository.sync_policy == RepositorySyncPolicy::FetchBeforeClaim {
            self.git.fetch(&repository.local_path, &auth)?;
        }
        let base_ref = task
            .base_ref
            .as_deref()
            .unwrap_or(&repository.default_branch);
        self.git.create_work_branch(
            &repository.local_path,
            base_ref,
            branch,
            &repository.protected_branches,
            &auth,
        )?;
        Ok(())
    }

    fn prepare_review_claim(
        &self,
        project: &OpenProject,
        requested_repository: Option<&str>,
    ) -> Result<Option<ReviewClaimBases>, AppError> {
        let Some(configuration) = project.database.review_configuration(&project.project.id)?
        else {
            return Ok(None);
        };
        if !configuration.enabled {
            return Ok(None);
        }
        if !configuration.has_valid_consent() {
            return Err(AppError::ReviewConsentRequired);
        }
        if let Some(credential_id) = configuration.credential_id {
            let credential = project
                .database
                .credential(&project.project.id, &credential_id)?
                .ok_or(AppError::ReviewCredentialMissing)?;
            self.secrets
                .get(&credential.secret_ref)
                .map_err(|_| AppError::ReviewCredentialMissing)?;
        }
        validate_review_standards_for_claim(project)?;

        let Some(plan) = project
            .database
            .latest_plan(&project.project.id, PlanStatus::Published)?
        else {
            return Ok(Some(BTreeMap::new()));
        };
        let mut bases = BTreeMap::new();
        for task in plan.content.tasks.iter().filter(|task| {
            task.kind == TaskKind::AgentCoding
                && requested_repository
                    .is_none_or(|repository| task.repository_key.as_deref() == Some(repository))
        }) {
            let Some(repository_key) = task.repository_key.as_deref() else {
                continue;
            };
            let Some(repository) = project
                .database
                .repository_by_key(&project.project.id, repository_key)?
            else {
                continue;
            };
            let base_ref = task
                .base_ref
                .as_deref()
                .unwrap_or(&repository.default_branch);
            let commit = self.git.resolve_commit(&repository.local_path, base_ref)?;
            bases.insert(
                (
                    repository_key.to_owned(),
                    task.base_ref.clone().unwrap_or_default(),
                ),
                commit,
            );
        }
        Ok(Some(bases))
    }
}

impl Default for PlanService {
    fn default() -> Self {
        Self::new()
    }
}

fn classify_command_execution(
    execution: &VerificationCommandExecution,
) -> Option<VerificationFailureKind> {
    if execution.infrastructure_error.is_some() {
        Some(VerificationFailureKind::Infrastructure)
    } else if execution.launch_error.is_some() {
        Some(VerificationFailureKind::LaunchFailed)
    } else if execution.timed_out {
        Some(VerificationFailureKind::Timeout)
    } else if execution.exit_code != Some(0) {
        Some(VerificationFailureKind::NonZeroExit)
    } else {
        None
    }
}

fn append_bounded_command_detail(summary: &mut String, detail: &str) -> bool {
    let cleaned = sanitize_verification_output(detail);
    if cleaned.trim().is_empty() {
        return false;
    }
    if !summary.is_empty() && !summary.ends_with('\n') {
        summary.push('\n');
    }
    summary.push_str(&cleaned);
    if summary.len() <= 64 * 1024 {
        return false;
    }
    let mut start = summary.len() - 64 * 1024;
    while !summary.is_char_boundary(start) {
        start += 1;
    }
    *summary = summary[start..].to_owned();
    true
}

fn bounded_command_detail(existing: &str, detail: &str) -> String {
    let mut summary = existing.to_owned();
    append_bounded_command_detail(&mut summary, detail);
    summary
}

fn resolved_verification_report(record: WorkReportRecord) -> Result<WorkReportRecord, AppError> {
    match record.review_status {
        ReviewStatus::Running => return Err(AppError::ReviewInProgress(record.work_item_id)),
        ReviewStatus::Blocked => {
            return Err(AppError::ReviewBlocked {
                details: record
                    .review
                    .as_ref()
                    .map(|review| review_details_json(review, record.id))
                    .unwrap_or_else(|| json!({ "reviewStatus": "blocked" })),
            });
        }
        ReviewStatus::Failed => {
            return Err(review_failure_error(&record));
        }
        ReviewStatus::Interrupted => {
            return Err(AppError::ReviewInterrupted {
                details: record
                    .review
                    .as_ref()
                    .map(|review| review_details_json(review, record.id))
                    .unwrap_or_else(|| json!({ "failureCode": "review_interrupted" })),
            });
        }
        ReviewStatus::NotRequired | ReviewStatus::Passed | ReviewStatus::Waived => {}
    }
    match record.verification_status {
        VerificationStatus::Passed | VerificationStatus::NotRequired => Ok(record),
        VerificationStatus::Running => Err(AppError::VerificationInProgress(record.work_item_id)),
        VerificationStatus::Failed => Err(match record.verification.as_ref() {
            Some(result) => verification_outcome_error(result),
            None => AppError::VerificationFailed {
                details: json!({ "failureKind": "unknown" }),
            },
        }),
        VerificationStatus::Interrupted => Err(AppError::VerificationInterrupted {
            details: record
                .verification
                .as_ref()
                .map(verification_details)
                .unwrap_or_else(|| json!({ "failureKind": "interrupted" })),
        }),
    }
}

fn review_failure_error(record: &WorkReportRecord) -> AppError {
    let failure = record
        .review
        .as_ref()
        .and_then(|review| review.failure.as_ref());
    let code = match failure.map(|failure| failure.code.as_str()) {
        Some("review_timeout") => "review_timeout",
        Some("review_response_invalid") => "review_response_invalid",
        Some("review_input_too_large") => "review_input_too_large",
        Some("review_credential_missing") => "review_credential_missing",
        Some("review_standard_invalid") => "review_standard_invalid",
        _ => "review_failed",
    };
    let details = record
        .review
        .as_ref()
        .map(|review| review_details_json(review, record.id))
        .unwrap_or_else(|| json!({ "failureCode": code }));
    AppError::ReviewModelFailure { code, details }
}

fn verification_outcome_error(result: &VerificationResult) -> AppError {
    let details = verification_details(result);
    match result.failure_kind {
        Some(VerificationFailureKind::Timeout) => AppError::VerificationTimeout { details },
        Some(VerificationFailureKind::Interrupted) => AppError::VerificationInterrupted { details },
        _ => AppError::VerificationFailed { details },
    }
}

fn verification_details(result: &VerificationResult) -> serde_json::Value {
    let failed_command = result
        .failure_command
        .as_ref()
        .and_then(|name| result.commands.iter().find(|command| &command.name == name));
    json!({
        "snapshotHash": result.snapshot_hash,
        "failureKind": result.failure_kind,
        "command": result.failure_command,
        "exitCode": failed_command.and_then(|command| command.exit_code),
        "durationMs": failed_command.map(|command| command.duration_ms),
        "outputSummary": failed_command
            .map(|command| command.output_summary.as_str())
            .unwrap_or_default(),
    })
}

fn default_lease_duration() -> Duration {
    Duration::minutes(DEFAULT_LEASE_MINUTES)
}

fn published_plan_for(
    project: &OpenProject,
    work_item: &WorkItem,
) -> Result<PlanRevision, AppError> {
    let plan = project
        .database
        .load_plan(&work_item.plan_revision_id)?
        .filter(|plan| plan.project_id == project.project.id)
        .ok_or(AppError::PlanNotFound(work_item.plan_revision_id))?;
    Ok(plan)
}

fn repository_for_task(
    project: &OpenProject,
    task: &coldstart_core::TaskSpec,
) -> Result<Option<Repository>, AppError> {
    let Some(repository_key) = task.repository_key.as_deref() else {
        return Ok(None);
    };
    Ok(project
        .database
        .repository_by_key(&project.project.id, repository_key)?)
}

fn ensure_reported_files_match(
    report: &WorkReportInput,
    actual_files: &[String],
) -> Result<(), AppError> {
    let reported = report
        .changed_files
        .iter()
        .map(|path| path.replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    let actual = actual_files
        .iter()
        .map(|path| path.replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    if reported == actual {
        Ok(())
    } else {
        Err(AppError::ReportRejected(format!(
            "reported changed files do not match the commit; reported {reported:?}, actual {actual:?}"
        )))
    }
}

fn same_changed_files(left: &[String], right: &[String]) -> bool {
    let left = left
        .iter()
        .map(|path| path.replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    let right = right
        .iter()
        .map(|path| path.replace('\\', "/"))
        .collect::<BTreeSet<_>>();
    left == right
}

fn review_details_json(
    review: &ReviewResult,
    report_id: coldstart_core::WorkReportId,
) -> serde_json::Value {
    json!({
        "reportId": report_id,
        "snapshotHash": review.snapshot_hash,
        "reviewStatus": review.status,
        "summary": review.summary,
        "findings": review.findings,
        "model": review.model,
        "durationMs": review.duration_ms,
    })
}
