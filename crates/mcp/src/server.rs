//! MCP 工具契约与调用分发。
//!
//! 当前暴露 12 个受限工具，覆盖简报、记忆、领取、上下文、心跳、报告、阻塞、
//! 能力调用和完成交接。工具只接受显式参数，不启动 Codex，也不扩大 Secret 面。

use std::path::PathBuf;

use coldstart_app::{
    AppError, CallCapabilityApiRequest, CapabilityApiQueryParameter, CapabilityApiService,
    ClaimRequest, ContextHandoffService, IntegrationService, PlanService, ProjectMemoryService,
};
use coldstart_core::{BlueprintEntityId, CapabilityApiEndpointId, CapabilityApiMethod, WorkItemId};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ErrorData, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use serde::Serialize;

use crate::{
    error::{app_error_result, invalid_capability_id_result, invalid_id_result, tool_error_result},
    types::{
        CallCapabilityApiArgs, CapabilityStatusArgs, ClaimNextWorkArgs, ProjectBriefArgs,
        ProjectMemoryArgs, ReadTaskReferenceArgs, ReleaseWorkArgs, ReportWorkBlockerArgs,
        SubmitWorkReportArgs, WorkLeaseArgs,
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

    /// 返回当前租约任务与前序工作的接力摘要，校验租约且不改变状态。
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
    /// 读取项目能力与凭据可用状态，只返回安全元数据。
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

    /// 在有效租约和能力授权范围内调用三方 API，由本机注入 Secret 并返回脱敏结果。
    #[tool(
        description = "Call a configured third-party capability API within the active work lease. Secrets are injected locally and never returned."
    )]
    pub async fn call_capability_api(
        &self,
        Parameters(args): Parameters<CallCapabilityApiArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let capability_id = match args.capability_id.parse::<BlueprintEntityId>() {
            Ok(capability_id) => capability_id,
            Err(_) => return Ok(invalid_capability_id_result(&args.capability_id)),
        };
        let endpoint_id = match args.endpoint_id.parse::<CapabilityApiEndpointId>() {
            Ok(endpoint_id) => endpoint_id,
            Err(_) => return Ok(invalid_id_result("endpoint_id", &args.endpoint_id)),
        };
        let work_id = match Self::parse_work_id(&args.work_id) {
            Ok(work_id) => work_id,
            Err(result) => return Ok(result),
        };
        let method = match CapabilityApiMethod::parse(&args.method) {
            Some(method) => method,
            None => {
                return Ok(tool_error_result(
                    "invalid_capability_api_method",
                    "method must be GET, POST, PUT, PATCH, or DELETE",
                    Some(serde_json::json!({
                        "field": "method",
                        "value": args.method,
                    })),
                ));
            }
        };
        let request = CallCapabilityApiRequest {
            capability_id,
            environment: args.environment,
            endpoint_id,
            work_id,
            lease_token: args.lease_token,
            method,
            path: args.path,
            query: args
                .query
                .into_iter()
                .map(|item| CapabilityApiQueryParameter {
                    key: item.key,
                    value: item.value,
                })
                .collect(),
            body: args.body,
            content_type: args.content_type,
        };
        let project_home = self.project_home.clone();
        Self::execute(move || CapabilityApiService::new().call(project_home, request)).await
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

    /// 读取当前租约任务的编译上下文，校验令牌和契约版本。
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

    /// 读取任务允许引用的来源片段，拒绝白名单之外的制品或锚点。
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

    /// 延长有效租约；过期、旧契约或错误令牌返回稳定错误。
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

    /// 将 Agent 阻塞原因登记到当前租约任务，不自动完成任务或解除租约。
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

    /// 提交结构化完成报告并触发独立验证、Review 和推送门禁。
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

    /// 主动释放当前租约并把任务恢复为可领取状态，保留审计事件。
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
