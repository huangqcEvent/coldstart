use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use chrono::{Duration, Utc};
use coldstart_adapters::{GitAdapter, SecretStore, WindowsSecretStore};
use coldstart_core::{
    ClaimResult, DEFAULT_LEASE_MINUTES, GitSyncRecord, GitSyncStatus, LeaseSummary, PlanContent,
    PlanRevision, PlanRevisionId, Repository, RepositorySyncPolicy, TaskContext, TaskKind,
    TaskReferenceContent, WorkBlockerRecord, WorkItem, WorkItemId, WorkReportInput,
    WorkReportRecord, compile_plan,
};
use coldstart_storage::NewWorkBlocker;
use serde::{Deserialize, Serialize};

use crate::{
    AppError, PlanWorkspace,
    context::OpenProject,
    context_compiler::{compile_reference, compile_task_context},
    integration::git_auth_for_repository,
    plan_workspace::build_plan_workspace,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRequest {
    pub agent_profile: String,
    pub repository: Option<String>,
    pub request_id: String,
}

#[derive(Clone)]
pub struct PlanService {
    secrets: Arc<dyn SecretStore>,
    git: GitAdapter,
}

impl PlanService {
    pub fn new() -> Self {
        Self::with_dependencies(Arc::new(WindowsSecretStore::new()), GitAdapter::new())
    }

    pub fn with_dependencies(secrets: Arc<dyn SecretStore>, git: GitAdapter) -> Self {
        Self { secrets, git }
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

    pub fn publish(
        &self,
        project_home: PathBuf,
        revision_id: PlanRevisionId,
    ) -> Result<PlanWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
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
        let claimed = project.database.claim_work(
            &project.project.id,
            &request.agent_profile,
            request.repository.as_deref(),
            &request.request_id,
            Utc::now(),
            default_lease_duration(),
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
        let work_items = project.database.list_work_items(&project.project.id)?;
        compile_task_context(&project, &plan, &authorization.task_spec, &work_items)
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
        let work_items = project.database.list_work_items(&project.project.id)?;
        compile_reference(
            &project,
            &plan,
            &authorization.task_spec,
            &reference_id,
            &work_items,
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
        let verification = if let Some(repository) = repository.as_ref() {
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
        let record = project.database.submit_work_report(
            &work_item_id,
            &lease_token,
            report,
            Utc::now(),
            default_lease_duration(),
        )?;
        if record.accepted
            && let (Some(repository), Some(verification)) = (repository.as_ref(), verification)
        {
            let auth = git_auth_for_repository(&project, repository, self.secrets.as_ref())?;
            let push = self
                .git
                .validate_remote(&repository.local_path, "origin", &repository.remote_url)
                .and_then(|()| {
                    self.git.push_branch(
                        &repository.local_path,
                        &verification.branch,
                        &repository.protected_branches,
                        &auth,
                    )
                });
            let sync = match push {
                Ok(()) => GitSyncRecord::new(
                    project.project.id,
                    work_item_id,
                    repository.id,
                    verification.branch,
                    verification.commit_sha,
                    GitSyncStatus::Pushed,
                    None,
                ),
                Err(error) => GitSyncRecord::new(
                    project.project.id,
                    work_item_id,
                    repository.id,
                    verification.branch,
                    verification.commit_sha,
                    GitSyncStatus::Failed,
                    Some(error.to_string()),
                ),
            };
            project.database.record_git_sync(&sync)?;
        }
        Ok(record)
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
}

impl Default for PlanService {
    fn default() -> Self {
        Self::new()
    }
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
