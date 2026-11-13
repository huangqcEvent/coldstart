use std::path::PathBuf;

use coldstart_app::{
    AppError, ClaimRequest, ContextHandoffService, IntegrationService, PlanService,
    ProjectMemoryService,
};
use coldstart_core::{BlueprintEntityId, WorkItemId};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ErrorData, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use serde::Serialize;

use crate::{
    error::{app_error_result, invalid_capability_id_result, invalid_id_result},
    types::{
        CapabilityStatusArgs, ClaimNextWorkArgs, ProjectBriefArgs, ProjectMemoryArgs,
        ReadTaskReferenceArgs, ReleaseWorkArgs, ReportWorkBlockerArgs, SubmitWorkReportArgs,
        WorkLeaseArgs,
    },
};

#[derive(Debug, Clone)]
pub struct ColdStartMcp {
    project_home: PathBuf,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl ColdStartMcp {
    pub fn new(project_home: PathBuf) -> Result<Self, AppError> {
        PlanService::new().load_workspace(project_home.clone())?;
        Ok(Self {
            project_home,
            tool_router: Self::tool_router(),
        })
    }

    async fn execute<T, F>(action: F) -> Result<CallToolResult, ErrorData>
    where
        T: Serialize + Send + 'static,
        F: FnOnce() -> Result<T, AppError> + Send + 'static,
    {
        match tokio::task::spawn_blocking(action).await {
            Ok(Ok(value)) => {
                let value = serde_json::to_value(value).map_err(|error| {
                    ErrorData::internal_error(
                        format!("failed to serialize MCP tool result: {error}"),
                        None,
                    )
                })?;
                Ok(CallToolResult::structured(value))
            }
            Ok(Err(error)) => Ok(app_error_result(error)),
            Err(error) => Err(ErrorData::internal_error(
                format!("MCP blocking task failed: {error}"),
                None,
            )),
        }
    }

    fn parse_work_id(value: &str) -> Result<WorkItemId, CallToolResult> {
        value
            .parse()
            .map_err(|_| invalid_id_result("work_id", value))
    }
}

#[tool_router]
impl ColdStartMcp {
    #[tool(
        description = "Read a bounded, safe project brief before claiming work. Never returns secrets, local paths, remote URLs, or lease tokens."
    )]
    pub async fn get_project_brief(
        &self,
        Parameters(_args): Parameters<ProjectBriefArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_home = self.project_home.clone();
        Self::execute(move || ContextHandoffService::new().project_brief(project_home)).await
    }

    #[tool(
        description = "Read a safe, cursor-paginated project memory timeline. Heartbeats are hidden by default."
    )]
    pub async fn get_project_memory(
        &self,
        Parameters(args): Parameters<ProjectMemoryArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_home = self.project_home.clone();
        Self::execute(move || {
            ProjectMemoryService::new().read(
                project_home,
                args.before_cursor,
                args.limit,
                args.include_heartbeats.unwrap_or(false),
            )
        })
        .await
    }

    #[tool(
        description = "Read safe cross-task handoff details for a work item authorized by the lease token."
    )]
    pub async fn get_work_handoff(
        &self,
        Parameters(args): Parameters<WorkLeaseArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let work_id = match Self::parse_work_id(&args.work_id) {
            Ok(work_id) => work_id,
            Err(result) => return Ok(result),
        };
        let project_home = self.project_home.clone();
        Self::execute(move || {
            ContextHandoffService::new().work_handoff(project_home, work_id, args.lease_token)
        })
        .await
    }
    #[tool(
        description = "Read a capability's configured status for one environment. Returns only a credential reference, never a secret."
    )]
    pub async fn get_capability_status(
        &self,
        Parameters(args): Parameters<CapabilityStatusArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let capability_id = match args.capability_id.parse::<BlueprintEntityId>() {
            Ok(capability_id) => capability_id,
            Err(_) => return Ok(invalid_capability_id_result(&args.capability_id)),
        };
        let project_home = self.project_home.clone();
        Self::execute(move || {
            IntegrationService::new().get_capability_status(
                project_home,
                &capability_id,
                &args.environment,
            )
        })
        .await
    }

    #[tool(
        description = "Atomically claim the next eligible coding task. Reuses request_id for idempotent retries."
    )]
    pub async fn claim_next_work(
        &self,
        Parameters(args): Parameters<ClaimNextWorkArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_home = self.project_home.clone();
        Self::execute(move || {
            PlanService::new().claim_next(
                project_home,
                ClaimRequest {
                    agent_profile: args.agent_profile,
                    repository: args.repository,
                    request_id: args.request_id,
                },
            )
        })
        .await
    }

    #[tool(
        description = "Return the budgeted task context for a work item authorized by the lease token."
    )]
    pub async fn get_work_context(
        &self,
        Parameters(args): Parameters<WorkLeaseArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let work_id = match Self::parse_work_id(&args.work_id) {
            Ok(work_id) => work_id,
            Err(result) => return Ok(result),
        };
        let project_home = self.project_home.clone();
        Self::execute(move || {
            PlanService::new().get_context(project_home, work_id, args.lease_token)
        })
        .await
    }

    #[tool(
        description = "Read one additional whitelisted task reference without exposing unrestricted project data."
    )]
    pub async fn read_task_reference(
        &self,
        Parameters(args): Parameters<ReadTaskReferenceArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let work_id = match Self::parse_work_id(&args.work_id) {
            Ok(work_id) => work_id,
            Err(result) => return Ok(result),
        };
        let project_home = self.project_home.clone();
        Self::execute(move || {
            PlanService::new().read_reference(
                project_home,
                work_id,
                args.lease_token,
                args.reference_id,
            )
        })
        .await
    }

    #[tool(description = "Renew the active work lease and record agent activity.")]
    pub async fn heartbeat_work(
        &self,
        Parameters(args): Parameters<WorkLeaseArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let work_id = match Self::parse_work_id(&args.work_id) {
            Ok(work_id) => work_id,
            Err(result) => return Ok(result),
        };
        let project_home = self.project_home.clone();
        Self::execute(move || PlanService::new().heartbeat(project_home, work_id, args.lease_token))
            .await
    }

    #[tool(description = "Report a blocker and the minimum human input needed to continue.")]
    pub async fn report_work_blocker(
        &self,
        Parameters(args): Parameters<ReportWorkBlockerArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let work_id = match Self::parse_work_id(&args.work_id) {
            Ok(work_id) => work_id,
            Err(result) => return Ok(result),
        };
        let project_home = self.project_home.clone();
        Self::execute(move || {
            PlanService::new().report_blocker(
                project_home,
                work_id,
                args.lease_token,
                args.reason,
                args.needed_input,
                args.proposal,
            )
        })
        .await
    }

    #[tool(
        description = "Submit a structured work report. The report must include contractHash and a unique idempotencyKey."
    )]
    pub async fn submit_work_report(
        &self,
        Parameters(args): Parameters<SubmitWorkReportArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let work_id = match Self::parse_work_id(&args.work_id) {
            Ok(work_id) => work_id,
            Err(result) => return Ok(result),
        };
        let project_home = self.project_home.clone();
        let report = args.report.into();
        Self::execute(move || {
            PlanService::new().submit_report(project_home, work_id, args.lease_token, report)
        })
        .await
    }

    #[tool(description = "Release an active work lease when the task cannot continue.")]
    pub async fn release_work(
        &self,
        Parameters(args): Parameters<ReleaseWorkArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let work_id = match Self::parse_work_id(&args.work_id) {
            Ok(work_id) => work_id,
            Err(result) => return Ok(result),
        };
        let project_home = self.project_home.clone();
        Self::execute(move || {
            PlanService::new().release(project_home, work_id, args.lease_token, args.reason)
        })
        .await
    }
}

#[tool_handler]
impl ServerHandler for ColdStartMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("coldstart", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "ColdStart work protocol. Read the safe project brief or project memory when resuming. Claim one task at a time, read its context and optional handoff, heartbeat while working, and submit a structured report. Never bypass lease checks.",
            )
    }
}
