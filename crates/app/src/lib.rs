//! ColdStart 应用编排层。
//!
//! 负责组合领域规则、存储和适配器，向桌面端与 MCP 暴露稳定服务接口；所有外部
//! 不可逆动作都必须经过显式授权和既有状态机。

mod capability_api;
mod context;
mod context_compiler;
mod delivery_overview;
mod error;
mod git_gateway;
mod github_access;
mod handoff;
mod integration;
mod issue;
mod memory;
mod plan;
mod plan_workspace;
mod project_history;
mod project_home;
mod project_overview;
mod pull_request;
mod requirements;
mod review;
mod service;
mod status;
mod task_images;
mod workspace;

pub use capability_api::{
    CallCapabilityApiRequest, CapabilityApiCallView, CapabilityApiQueryParameter,
    CapabilityApiService,
};
pub use delivery_overview::{
    DeliveryChecksView, DeliveryGitSyncView, DeliveryOverviewService, DeliveryOverviewView,
    DeliveryPullRequestView, DeliveryRepositoryState, DeliveryTaskCounts, RepositoryDeliveryView,
};
pub use error::AppError;
pub use git_gateway::PlanGitGateway;
pub use handoff::{
    BriefBlocker, BriefBlueprint, BriefFact, BriefInfoRequest, BriefPlan, BriefProject,
    BriefRepository, BriefStatusCount, BriefWork, BriefWorkReport, ContextHandoffService,
    ProjectBrief, WorkHandoff,
};
pub use integration::{
    CapabilityStatusView, CreatePrivateRepositoryInput, DeviceFlowView, GitHubDevicePollView,
    GitHubSessionView, IntegrationService, IntegrationWorkspace, ReleaseCenterView,
    RepositoryDraft, RepositoryValidationView, RepositoryVerificationView, SaveCredentialInput,
};
pub use issue::{GitHubIssueListView, GitHubIssueService, GitHubIssueView};
pub use memory::{
    DEFAULT_MEMORY_PAGE_LIMIT, MAX_MEMORY_PAGE_LIMIT, MemoryPhase, MemoryPhaseCount,
    MemoryReferenceType, ProjectMemoryItem, ProjectMemoryPage, ProjectMemoryReference,
    ProjectMemoryService,
};
pub use plan::{ClaimRequest, PlanService};
pub use plan_workspace::{McpConnectionInfo, PlanWorkspace, WorkItemView};
pub use project_history::{ProjectHistoryStore, ProjectHistoryView, RecentProjectView};
pub use project_home::ProjectHome;
pub use project_overview::{
    GitHubProjectFieldView, GitHubProjectItemView, GitHubProjectOverviewView, GitHubProjectService,
    GitHubProjectView,
};
pub use pull_request::PullRequestService;
pub use requirements::{
    MAX_SOURCE_CONTEXT_CHARACTERS, RequirementImageContent, RequirementsService,
    read_requirement_image,
};
pub use review::{
    ReviewHistoryView, ReviewService, ReviewStandardContent, ReviewStandardImport,
    ReviewStandardManualInput, ReviewStandardRevisionInput, ReviewStandardView, ReviewWorkspace,
};
pub use service::ProjectService;
pub use status::ProjectStatus;
pub use task_images::{TaskImageContent, TaskImageSummary};
pub use workspace::ProjectWorkspace;
