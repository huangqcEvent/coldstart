use std::path::PathBuf;

use chrono::{Duration, Utc};
use coldstart_core::{
    ClaimResult, DEFAULT_LEASE_MINUTES, LeaseSummary, PlanContent, PlanRevision, PlanRevisionId,
    TaskContext, TaskReferenceContent, WorkBlockerRecord, WorkItem, WorkItemId, WorkReportInput,
    WorkReportRecord, compile_plan,
};
use coldstart_storage::NewWorkBlocker;
use serde::{Deserialize, Serialize};

use crate::{
    AppError, PlanWorkspace,
    context::OpenProject,
    context_compiler::{compile_reference, compile_task_context},
    plan_workspace::build_plan_workspace,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRequest {
    pub agent_profile: String,
    pub repository: Option<String>,
    pub request_id: String,
}

#[derive(Debug, Default)]
pub struct PlanService;

impl PlanService {
    pub fn new() -> Self {
        Self
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
        Ok(project.database.claim_work(
            &project.project.id,
            &request.agent_profile,
            request.repository.as_deref(),
            &request.request_id,
            Utc::now(),
            default_lease_duration(),
        )?)
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
        Ok(project.database.submit_work_report(
            &work_item_id,
            &lease_token,
            report,
            Utc::now(),
            default_lease_duration(),
        )?)
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
