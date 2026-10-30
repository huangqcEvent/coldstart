use std::{collections::BTreeMap, path::Path};

use coldstart_core::{
    LeaseStatus, LeaseSummary, PlanRevision, PlanStatus, TaskSpec, WorkBlockerRecord, WorkItem,
    WorkItemStatus, WorkReportRecord, WorkRun,
};
use serde::{Deserialize, Serialize};

use crate::{AppError, context::OpenProject};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemView {
    pub work_item: WorkItem,
    pub task: TaskSpec,
    pub active_lease: Option<LeaseSummary>,
    pub latest_report: Option<WorkReportRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConnectionInfo {
    pub command: String,
    pub project_home: String,
    pub config_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanWorkspace {
    pub draft: Option<PlanRevision>,
    pub published: Option<PlanRevision>,
    pub work_items: Vec<WorkItemView>,
    pub leases: Vec<LeaseSummary>,
    pub runs: Vec<WorkRun>,
    pub reports: Vec<WorkReportRecord>,
    pub blockers: Vec<WorkBlockerRecord>,
    pub mcp: McpConnectionInfo,
}

impl PlanWorkspace {
    pub fn ready_count(&self) -> usize {
        self.work_items
            .iter()
            .filter(|view| view.work_item.status == WorkItemStatus::Ready)
            .count()
    }
}

pub(crate) fn build_plan_workspace(project: &OpenProject) -> Result<PlanWorkspace, AppError> {
    let draft = project
        .database
        .latest_plan(&project.project.id, PlanStatus::Draft)?;
    let published = project
        .database
        .latest_plan(&project.project.id, PlanStatus::Published)?;
    let work_items = project.database.list_work_items(&project.project.id)?;
    let leases = project.database.list_leases(&project.project.id)?;
    let runs = project.database.list_work_runs(&project.project.id)?;
    let reports = project.database.list_work_reports(&project.project.id)?;
    let blockers = project
        .database
        .list_unresolved_blockers(&project.project.id)?;

    let active_leases = leases
        .iter()
        .filter(|lease| lease.status == LeaseStatus::Active)
        .map(|lease| (lease.work_item_id, lease.clone()))
        .collect::<BTreeMap<_, _>>();
    let latest_reports = reports.iter().fold(BTreeMap::new(), |mut reports, report| {
        reports
            .entry(report.work_item_id)
            .or_insert_with(|| report.clone());
        reports
    });
    let mut views = Vec::with_capacity(work_items.len());
    for work_item in work_items {
        let task = project
            .database
            .task_spec_for_work_item(&work_item.id)?
            .ok_or(AppError::WorkItemNotFound(work_item.id))?;
        views.push(WorkItemView {
            active_lease: active_leases.get(&work_item.id).cloned(),
            latest_report: latest_reports.get(&work_item.id).cloned(),
            work_item,
            task,
        });
    }

    let project_home = project.project.root_path.to_string_lossy().into_owned();
    let command = mcp_binary_path(&project.project.root_path);
    let config_json = serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": {
            "coldstart": {
                "command": command,
                "args": ["--project-home", project_home],
            }
        }
    }))?;

    Ok(PlanWorkspace {
        draft,
        published,
        work_items: views,
        leases,
        runs,
        reports,
        blockers,
        mcp: McpConnectionInfo {
            command,
            project_home,
            config_json,
        },
    })
}

fn mcp_binary_path(project_home: &Path) -> String {
    let _ = project_home;
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(directory) = current_exe.parent()
    {
        let candidate = directory.join("coldstart-mcp.exe");
        if candidate.exists() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    "coldstart-mcp.exe".to_owned()
}
