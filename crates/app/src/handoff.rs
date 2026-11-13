use std::{collections::BTreeSet, path::PathBuf};

use chrono::{DateTime, Duration, Utc};
use coldstart_adapters::sanitize_verification_output;
use coldstart_core::{
    BlueprintRevisionId, BlueprintStatus, DEFAULT_LEASE_MINUTES, InfoRequestCategory,
    PlanRevisionId, ProjectFact, ProjectId, ReviewStatus, VerificationStatus, WorkItemId,
    WorkItemStatus, WorkReportId,
};
use serde::{Deserialize, Serialize};

use crate::{
    AppError,
    context::OpenProject,
    context_compiler::{fact_is_sensitive, latest_fact_versions},
    memory::{ProjectMemoryItem, read_memory_page},
    plan_workspace::{PlanWorkspace, WorkItemView, build_plan_workspace},
};

const PROJECT_BRIEF_CURRENT_WORK_LIMIT: usize = 10;
const PROJECT_BRIEF_COMPLETED_WORK_LIMIT: usize = 5;
const PROJECT_BRIEF_FACT_LIMIT: usize = 20;
const PROJECT_BRIEF_INFO_REQUEST_LIMIT: usize = 10;
const PROJECT_BRIEF_BLOCKER_LIMIT: usize = 10;
const PROJECT_BRIEF_MEMORY_LIMIT: u32 = 10;
const WORK_HANDOFF_RELATED_WORK_LIMIT: usize = 8;
const SAFE_TEXT_LIMIT: usize = 500;
const SAFE_LIST_LIMIT: usize = 10;
const SAFE_FILE_LIMIT: usize = 30;
const SENSITIVE_FACT_VALUE: &str = "已登记（敏感值不进入 MCP）";

const WORK_STATUS_ORDER: [WorkItemStatus; 10] = [
    WorkItemStatus::Draft,
    WorkItemStatus::Ready,
    WorkItemStatus::Claimed,
    WorkItemStatus::InProgress,
    WorkItemStatus::Submitted,
    WorkItemStatus::Done,
    WorkItemStatus::Blocked,
    WorkItemStatus::Failed,
    WorkItemStatus::Reopened,
    WorkItemStatus::Cancelled,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefProject {
    pub id: ProjectId,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefBlueprint {
    pub revision_id: BlueprintRevisionId,
    pub revision_number: u64,
    pub content_hash: String,
    pub published_at: Option<DateTime<Utc>>,
    pub product_goal: String,
    pub platforms: Vec<String>,
    pub module_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefStatusCount {
    pub status: WorkItemStatus,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefPlan {
    pub revision_id: PlanRevisionId,
    pub revision_number: u64,
    pub published_at: Option<DateTime<Utc>>,
    pub task_count: usize,
    pub status_counts: Vec<BriefStatusCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefRepository {
    pub key: String,
    pub name: String,
    pub default_branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefWorkReport {
    pub report_id: WorkReportId,
    pub accepted: bool,
    pub summary: String,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub changed_files: Vec<String>,
    pub known_issues: Vec<String>,
    pub follow_up_items: Vec<String>,
    pub verification_status: VerificationStatus,
    pub review_status: ReviewStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefWork {
    pub work_item_id: WorkItemId,
    pub task_key: String,
    pub title: String,
    pub status: WorkItemStatus,
    pub priority: i64,
    pub repository_key: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub latest_report: Option<BriefWorkReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefBlocker {
    pub work_item_id: WorkItemId,
    pub task_key: Option<String>,
    pub reason: String,
    pub needed_input: Vec<String>,
    pub proposal: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefInfoRequest {
    pub category: InfoRequestCategory,
    pub question: String,
    pub reason: String,
    pub impact_scope: String,
    pub blocking: bool,
    pub due_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefFact {
    pub category: InfoRequestCategory,
    pub question: String,
    pub value: String,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBrief {
    pub project: BriefProject,
    pub blueprint: Option<BriefBlueprint>,
    pub plan: Option<BriefPlan>,
    pub repositories: Vec<BriefRepository>,
    pub current_work: Vec<BriefWork>,
    pub recent_completed_work: Vec<BriefWork>,
    pub blockers: Vec<BriefBlocker>,
    pub open_info_requests: Vec<BriefInfoRequest>,
    pub confirmed_facts: Vec<BriefFact>,
    pub recent_memory: Vec<ProjectMemoryItem>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkHandoff {
    pub current_work: BriefWork,
    pub related_work: Vec<BriefWork>,
    pub blockers: Vec<BriefBlocker>,
    pub open_info_requests: Vec<BriefInfoRequest>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct ContextHandoffService;

impl ContextHandoffService {
    pub fn new() -> Self {
        Self
    }

    pub fn project_brief(&self, project_home: PathBuf) -> Result<ProjectBrief, AppError> {
        let project = OpenProject::open(project_home)?;
        self.build_project_brief(&project)
    }

    pub fn work_handoff(
        &self,
        project_home: PathBuf,
        work_item_id: WorkItemId,
        lease_token: String,
    ) -> Result<WorkHandoff, AppError> {
        let mut project = OpenProject::open(project_home)?;
        project.database.authorize_work(
            &work_item_id,
            &lease_token,
            Utc::now(),
            Duration::minutes(DEFAULT_LEASE_MINUTES),
        )?;
        self.build_work_handoff(&project, work_item_id)
    }

    fn build_project_brief(&self, project: &OpenProject) -> Result<ProjectBrief, AppError> {
        let workspace = build_plan_workspace(project)?;
        let blueprint = load_current_blueprint(project, &workspace)?;
        let repositories = project
            .database
            .list_repositories(&project.project.id)?
            .into_iter()
            .map(|repository| BriefRepository {
                key: safe_text(&repository.key, 120),
                name: safe_text(&repository.name, SAFE_TEXT_LIMIT),
                default_branch: safe_text(&repository.default_branch, 200),
            })
            .collect();
        let mut current_work = workspace
            .work_items
            .iter()
            .filter(|view| {
                !matches!(
                    view.work_item.status,
                    WorkItemStatus::Done | WorkItemStatus::Cancelled
                )
            })
            .map(brief_work)
            .collect::<Vec<_>>();
        current_work.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| left.task_key.cmp(&right.task_key))
        });
        current_work.truncate(PROJECT_BRIEF_CURRENT_WORK_LIMIT);

        let mut recent_completed_work = workspace
            .work_items
            .iter()
            .filter(|view| view.work_item.status == WorkItemStatus::Done)
            .map(brief_work)
            .collect::<Vec<_>>();
        recent_completed_work.sort_by(|left, right| {
            right
                .completed_at
                .cmp(&left.completed_at)
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| left.task_key.cmp(&right.task_key))
        });
        recent_completed_work.truncate(PROJECT_BRIEF_COMPLETED_WORK_LIMIT);

        let facts = project.database.list_project_facts(&project.project.id)?;
        let confirmed_facts = latest_fact_versions(&facts)
            .into_iter()
            .take(PROJECT_BRIEF_FACT_LIMIT)
            .map(brief_fact)
            .collect();
        let open_info_requests = project
            .database
            .list_info_requests(&project.project.id)?
            .into_iter()
            .filter(|request| request.status == coldstart_core::InfoRequestStatus::Open)
            .take(PROJECT_BRIEF_INFO_REQUEST_LIMIT)
            .map(|request| BriefInfoRequest {
                category: request.category,
                question: safe_text(&request.question, SAFE_TEXT_LIMIT),
                reason: safe_text(&request.reason, SAFE_TEXT_LIMIT),
                impact_scope: safe_text(&request.impact_scope, SAFE_TEXT_LIMIT),
                blocking: request.blocking,
                due_at: request.due_at,
            })
            .collect();
        let blockers = brief_blockers(&workspace, None);
        let memory = read_memory_page(project, None, Some(PROJECT_BRIEF_MEMORY_LIMIT), false)?;

        Ok(ProjectBrief {
            project: BriefProject {
                id: project.project.id,
                name: safe_text(&project.project.name, 200),
                created_at: project.project.created_at,
            },
            blueprint,
            plan: workspace.published.as_ref().map(|plan| BriefPlan {
                revision_id: plan.id,
                revision_number: plan.revision_number,
                published_at: plan.published_at,
                task_count: workspace.work_items.len(),
                status_counts: status_counts(&workspace.work_items),
            }),
            repositories,
            current_work,
            recent_completed_work,
            blockers,
            open_info_requests,
            confirmed_facts,
            recent_memory: memory.items,
            generated_at: Utc::now(),
        })
    }

    fn build_work_handoff(
        &self,
        project: &OpenProject,
        work_item_id: WorkItemId,
    ) -> Result<WorkHandoff, AppError> {
        let workspace = build_plan_workspace(project)?;
        let current = workspace
            .work_items
            .iter()
            .find(|view| view.work_item.id == work_item_id)
            .ok_or(AppError::WorkItemNotFound(work_item_id))?;
        let mut related = workspace
            .work_items
            .iter()
            .filter(|view| {
                view.work_item.id != work_item_id
                    && !matches!(
                        view.work_item.status,
                        WorkItemStatus::Draft | WorkItemStatus::Cancelled
                    )
                    && related_to_task(&current.task, &view.task)
            })
            .map(|view| {
                let dependency = current
                    .task
                    .dependencies
                    .iter()
                    .any(|dependency| dependency.task_key == view.task.task_key);
                (dependency, view)
            })
            .collect::<Vec<_>>();
        related.sort_by(|(left_dependency, left), (right_dependency, right)| {
            right_dependency
                .cmp(left_dependency)
                .then_with(|| right.work_item.updated_at.cmp(&left.work_item.updated_at))
                .then_with(|| left.task.task_key.cmp(&right.task.task_key))
        });
        related.truncate(WORK_HANDOFF_RELATED_WORK_LIMIT);
        let related_work = related
            .into_iter()
            .map(|(_, view)| brief_work(view))
            .collect::<Vec<_>>();
        let mut related_ids = related_work
            .iter()
            .map(|work| work.work_item_id)
            .collect::<BTreeSet<_>>();
        related_ids.insert(work_item_id);
        let blockers = brief_blockers(&workspace, Some(&related_ids));
        let open_info_requests = project
            .database
            .list_info_requests(&project.project.id)?
            .into_iter()
            .filter(|request| request.status == coldstart_core::InfoRequestStatus::Open)
            .take(PROJECT_BRIEF_INFO_REQUEST_LIMIT)
            .map(|request| BriefInfoRequest {
                category: request.category,
                question: safe_text(&request.question, SAFE_TEXT_LIMIT),
                reason: safe_text(&request.reason, SAFE_TEXT_LIMIT),
                impact_scope: safe_text(&request.impact_scope, SAFE_TEXT_LIMIT),
                blocking: request.blocking,
                due_at: request.due_at,
            })
            .collect();

        Ok(WorkHandoff {
            current_work: brief_work(current),
            related_work,
            blockers,
            open_info_requests,
            generated_at: Utc::now(),
        })
    }
}

fn load_current_blueprint(
    project: &OpenProject,
    workspace: &PlanWorkspace,
) -> Result<Option<BriefBlueprint>, AppError> {
    let revision = match workspace.published.as_ref() {
        Some(plan) => project
            .database
            .load_blueprint(&plan.blueprint_revision_id)?,
        None => project
            .database
            .latest_blueprint(&project.project.id, BlueprintStatus::Published)?,
    };
    Ok(revision.map(|revision| BriefBlueprint {
        revision_id: revision.id,
        revision_number: revision.revision_number,
        content_hash: revision.content_hash,
        published_at: revision.published_at,
        product_goal: safe_text(&revision.content.product_goal.text, SAFE_TEXT_LIMIT),
        platforms: revision
            .content
            .platforms
            .iter()
            .map(|platform| safe_text(&platform.name, 200))
            .collect(),
        module_count: revision.content.modules.len(),
    }))
}

fn status_counts(work_items: &[WorkItemView]) -> Vec<BriefStatusCount> {
    WORK_STATUS_ORDER
        .into_iter()
        .map(|status| BriefStatusCount {
            status,
            count: work_items
                .iter()
                .filter(|view| view.work_item.status == status)
                .count() as u64,
        })
        .collect()
}

fn brief_work(view: &WorkItemView) -> BriefWork {
    BriefWork {
        work_item_id: view.work_item.id,
        task_key: safe_text(&view.task.task_key, 200),
        title: safe_text(&view.task.title, SAFE_TEXT_LIMIT),
        status: view.work_item.status,
        priority: view.task.priority,
        repository_key: view
            .task
            .repository_key
            .as_deref()
            .map(|key| safe_text(key, 120)),
        updated_at: view.work_item.updated_at,
        completed_at: view.work_item.completed_at,
        latest_report: view.latest_report.as_ref().map(|record| BriefWorkReport {
            report_id: record.id,
            accepted: record.accepted,
            summary: safe_text(&record.report.summary, SAFE_TEXT_LIMIT),
            branch: record
                .report
                .branch
                .as_deref()
                .map(|value| safe_text(value, 240)),
            commit_sha: record
                .report
                .commit_sha
                .as_deref()
                .map(|value| safe_text(value, 120)),
            changed_files: record
                .report
                .changed_files
                .iter()
                .take(SAFE_FILE_LIMIT)
                .map(|value| safe_text(value, 240))
                .collect(),
            known_issues: safe_list(&record.report.known_issues),
            follow_up_items: safe_list(&record.report.follow_up_items),
            verification_status: record.verification_status,
            review_status: record.review_status,
            created_at: record.created_at,
        }),
    }
}

fn brief_fact(fact: &ProjectFact) -> BriefFact {
    BriefFact {
        category: fact.category,
        question: safe_text(&fact.question, SAFE_TEXT_LIMIT),
        value: if fact_is_sensitive(fact) {
            SENSITIVE_FACT_VALUE.to_owned()
        } else {
            safe_text(&fact.value, SAFE_TEXT_LIMIT)
        },
        version: fact.version,
    }
}

fn brief_blockers(
    workspace: &PlanWorkspace,
    included_work_items: Option<&BTreeSet<WorkItemId>>,
) -> Vec<BriefBlocker> {
    workspace
        .blockers
        .iter()
        .filter(|blocker| included_work_items.is_none_or(|ids| ids.contains(&blocker.work_item_id)))
        .take(PROJECT_BRIEF_BLOCKER_LIMIT)
        .map(|blocker| BriefBlocker {
            work_item_id: blocker.work_item_id,
            task_key: workspace
                .work_items
                .iter()
                .find(|view| view.work_item.id == blocker.work_item_id)
                .map(|view| safe_text(&view.task.task_key, 200)),
            reason: safe_text(&blocker.reason, SAFE_TEXT_LIMIT),
            needed_input: safe_list(&blocker.needed_input),
            proposal: blocker
                .proposal
                .as_deref()
                .map(|value| safe_text(value, SAFE_TEXT_LIMIT)),
            created_at: blocker.created_at,
        })
        .collect()
}

fn related_to_task(
    current: &coldstart_core::TaskSpec,
    candidate: &coldstart_core::TaskSpec,
) -> bool {
    current
        .dependencies
        .iter()
        .any(|dependency| dependency.task_key == candidate.task_key)
        || (current.repository_key.is_some() && current.repository_key == candidate.repository_key)
        || (current.module_id.is_some() && current.module_id == candidate.module_id)
}

fn safe_list(values: &[String]) -> Vec<String> {
    values
        .iter()
        .take(SAFE_LIST_LIMIT)
        .map(|value| safe_text(value, SAFE_TEXT_LIMIT))
        .filter(|value| !value.is_empty())
        .collect()
}

fn safe_text(value: &str, limit: usize) -> String {
    let sanitized = sanitize_verification_output(value.trim());
    if sanitized.chars().count() <= limit {
        return sanitized;
    }
    let mut truncated = sanitized
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}
