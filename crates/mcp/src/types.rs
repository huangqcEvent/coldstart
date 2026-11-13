use coldstart_core::{AcceptanceResult, CommandRun, ReportOutcome, WorkReportInput};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClaimNextWorkArgs {
    #[schemars(description = "Stable agent profile name, for example codex")]
    pub agent_profile: String,
    #[schemars(
        description = "Optional logical repository key such as server, web, app, or mini_program"
    )]
    pub repository: Option<String>,
    #[schemars(description = "Caller-generated idempotency key for the claim request")]
    pub request_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectBriefArgs {}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectMemoryArgs {
    #[schemars(description = "Return events older than this positive event cursor")]
    pub before_cursor: Option<i64>,
    #[schemars(description = "Page size from 1 to 500; defaults to 100")]
    pub limit: Option<u32>,
    #[schemars(description = "Include work heartbeat events; defaults to false")]
    pub include_heartbeats: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CapabilityStatusArgs {
    #[schemars(description = "Blueprint capability ID")]
    pub capability_id: String,
    #[schemars(description = "One of development, staging, or production")]
    pub environment: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkLeaseArgs {
    pub work_id: String,
    pub lease_token: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadTaskReferenceArgs {
    pub work_id: String,
    pub lease_token: String,
    pub reference_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReportWorkBlockerArgs {
    pub work_id: String,
    pub lease_token: String,
    pub reason: String,
    pub needed_input: Vec<String>,
    pub proposal: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReleaseWorkArgs {
    pub work_id: String,
    pub lease_token: String,
    pub reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SubmitWorkReportArgs {
    pub work_id: String,
    pub lease_token: String,
    pub report: WorkReportArgs,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkReportArgs {
    pub status: WorkReportStatus,
    pub summary: String,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub changed_files: Vec<String>,
    pub acceptance_results: Vec<AcceptanceResultArgs>,
    pub commands_run: Vec<CommandRunArgs>,
    pub evidence_refs: Vec<String>,
    pub known_issues: Vec<String>,
    pub follow_up_items: Vec<String>,
    pub contract_hash: String,
    pub idempotency_key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkReportStatus {
    Completed,
    Failed,
    Blocked,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AcceptanceResultArgs {
    pub criterion_id: String,
    pub passed: bool,
    pub note: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CommandRunArgs {
    pub command: String,
    pub exit_code: Option<i32>,
    pub summary: String,
}

impl From<WorkReportArgs> for WorkReportInput {
    fn from(report: WorkReportArgs) -> Self {
        Self {
            status: match report.status {
                WorkReportStatus::Completed => ReportOutcome::Completed,
                WorkReportStatus::Failed => ReportOutcome::Failed,
                WorkReportStatus::Blocked => ReportOutcome::Blocked,
            },
            summary: report.summary,
            branch: report.branch,
            commit_sha: report.commit_sha,
            changed_files: report.changed_files,
            acceptance_results: report
                .acceptance_results
                .into_iter()
                .map(|item| AcceptanceResult {
                    criterion_id: item.criterion_id,
                    passed: item.passed,
                    note: item.note,
                    evidence_refs: item.evidence_refs,
                })
                .collect(),
            commands_run: report
                .commands_run
                .into_iter()
                .map(|item| CommandRun {
                    command: item.command,
                    exit_code: item.exit_code,
                    summary: item.summary,
                })
                .collect(),
            evidence_refs: report.evidence_refs,
            known_issues: report.known_issues,
            follow_up_items: report.follow_up_items,
            contract_hash: report.contract_hash,
            idempotency_key: report.idempotency_key,
        }
    }
}
