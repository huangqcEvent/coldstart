//! Tauri 命令边界。
//!
//! 这些命令只负责参数转换、调用应用服务并序列化结果，业务规则留在 app 层。
//! 同步命令用于短操作，异步命令避免阻塞 UI；任何 Secret 都不回传前端。

use std::{path::PathBuf, sync::Arc};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use coldstart_adapters::{GitHubIssueListState, ModelConfig, OpenAiCompatibleModel};
use coldstart_app::{
    CodeInvestigationService, CreatePrivateRepositoryInput, DeliveryOverviewService,
    DeliveryOverviewView, DeviceFlowView, GitHubDevicePollView, GitHubIssueListView,
    GitHubIssueService, GitHubProjectOverviewView, GitHubProjectService, GitHubSessionView,
    IntegrationService, IntegrationWorkspace, PlanService, ProjectMemoryPage, ProjectMemoryService,
    ProjectService, ProjectStatus, ProjectWorkspace, PullRequestService, RepositoryDraft,
    RepositoryValidationView, RequirementsService, ReviewService, ReviewStandardContent,
    ReviewStandardImport, ReviewStandardManualInput, ReviewStandardRevisionInput,
    ReviewStandardView, ReviewWorkspace, SaveCredentialInput,
    read_requirement_image as read_requirement_image_content,
};
use coldstart_core::{
    ArtifactId, ArtifactImage, BlueprintContent, BlueprintEntityId, BlueprintRevisionId,
    CapabilityApiEndpointId, CapabilityApiEndpointInput, CapabilityStatus, CodeInvestigationId,
    CredentialId, InfoRequestId, PlanContent, PlanPublishPreview, PlanRevisionId, PullRequestId,
    PullRequestRecord, RepositoryVerificationCommand, ReviewConfiguration,
    ReviewConfigurationInput, ReviewStandardId, WorkItemId, WorkReportId, WorkReportRecord,
};
use tauri::State;

use crate::error::CommandError;

pub fn create_project_sync(name: String, path: String) -> Result<ProjectStatus, CommandError> {
    ProjectService::new()
        .create_project(&name, path.into())
        .map_err(Into::into)
}

#[tauri::command]
pub async fn create_project(name: String, path: String) -> Result<ProjectStatus, CommandError> {
    tauri::async_runtime::spawn_blocking(move || create_project_sync(name, path))
        .await
        .map_err(|error| CommandError {
            code: "task_join".to_owned(),
            message: error.to_string(),
        })?
}

pub fn read_project_status_sync(path: String) -> Result<ProjectStatus, CommandError> {
    ProjectService::new()
        .read_status(path.into())
        .map_err(Into::into)
}

#[tauri::command]
pub async fn read_project_status(path: String) -> Result<ProjectStatus, CommandError> {
    tauri::async_runtime::spawn_blocking(move || read_project_status_sync(path))
        .await
        .map_err(|error| CommandError {
            code: "task_join".to_owned(),
            message: error.to_string(),
        })?
}

pub fn read_requirements_workspace_sync(path: String) -> Result<ProjectWorkspace, CommandError> {
    RequirementsService::new(OpenAiCompatibleModel::new())
        .load_workspace(PathBuf::from(path))
        .map_err(Into::into)
}

#[tauri::command]
pub async fn read_requirements_workspace(path: String) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || read_requirements_workspace_sync(path))
        .await
        .map_err(join_error)?
}

pub fn load_delivery_overview_sync(path: String) -> Result<DeliveryOverviewView, CommandError> {
    DeliveryOverviewService::new()
        .load(PathBuf::from(path))
        .map_err(Into::into)
}

#[tauri::command]
pub async fn load_delivery_overview(path: String) -> Result<DeliveryOverviewView, CommandError> {
    tauri::async_runtime::spawn_blocking(move || load_delivery_overview_sync(path))
        .await
        .map_err(join_error)?
}

pub fn read_project_memory_sync(
    path: String,
    before_cursor: Option<i64>,
    limit: Option<u32>,
    include_heartbeats: bool,
) -> Result<ProjectMemoryPage, CommandError> {
    ProjectMemoryService::new()
        .read(
            PathBuf::from(path),
            before_cursor,
            limit,
            include_heartbeats,
        )
        .map_err(Into::into)
}

#[tauri::command]
pub async fn read_project_memory(
    path: String,
    before_cursor: Option<i64>,
    limit: Option<u32>,
    include_heartbeats: bool,
) -> Result<ProjectMemoryPage, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        read_project_memory_sync(path, before_cursor, limit, include_heartbeats)
    })
    .await
    .map_err(join_error)?
}

pub fn import_requirement_document_sync(
    path: String,
    file_path: String,
) -> Result<ProjectWorkspace, CommandError> {
    RequirementsService::new(OpenAiCompatibleModel::new())
        .import_document(PathBuf::from(path), PathBuf::from(file_path))
        .map_err(Into::into)
}

#[tauri::command]
pub async fn import_requirement_document(
    path: String,
    file_path: String,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || import_requirement_document_sync(path, file_path))
        .await
        .map_err(join_error)?
}

/// 供桌面端按需预览的来源图片；二进制以 base64 返回，不暴露本地路径。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementImagePreview {
    pub image: ArtifactImage,
    pub source_file_name: String,
    pub anchor_label: String,
    pub data_base64: String,
}

pub fn read_requirement_image_sync(
    path: String,
    artifact_id: String,
    image_id: String,
) -> Result<RequirementImagePreview, CommandError> {
    let artifact_id = parse_artifact_id(&artifact_id)?;
    let content = read_requirement_image_content(PathBuf::from(path), artifact_id, &image_id)?;
    Ok(RequirementImagePreview {
        data_base64: BASE64_STANDARD.encode(&content.bytes),
        image: content.image,
        source_file_name: content.source_file_name,
        anchor_label: content.anchor_label,
    })
}

#[tauri::command]
pub async fn read_requirement_image(
    path: String,
    artifact_id: String,
    image_id: String,
) -> Result<RequirementImagePreview, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        read_requirement_image_sync(path, artifact_id, image_id)
    })
    .await
    .map_err(join_error)?
}

pub fn generate_blueprint_sync(
    path: String,
    config: ModelConfig,
    artifact_ids: Vec<String>,
) -> Result<ProjectWorkspace, CommandError> {
    let artifact_ids = artifact_ids
        .iter()
        .map(|artifact_id| parse_artifact_id(artifact_id))
        .collect::<Result<Vec<_>, _>>()?;
    RequirementsService::new(OpenAiCompatibleModel::new())
        .generate_blueprint(PathBuf::from(path), config, artifact_ids)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn generate_blueprint(
    path: String,
    config: ModelConfig,
    artifact_ids: Vec<String>,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        generate_blueprint_sync(path, config, artifact_ids)
    })
    .await
    .map_err(join_error)?
}

pub fn save_blueprint_draft_sync(
    path: String,
    content: BlueprintContent,
) -> Result<ProjectWorkspace, CommandError> {
    RequirementsService::new(OpenAiCompatibleModel::new())
        .save_blueprint(PathBuf::from(path), content)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn save_blueprint_draft(
    path: String,
    content: BlueprintContent,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || save_blueprint_draft_sync(path, content))
        .await
        .map_err(join_error)?
}

pub fn publish_blueprint_sync(
    path: String,
    revision_id: String,
) -> Result<ProjectWorkspace, CommandError> {
    let revision_id = parse_revision_id(&revision_id)?;
    RequirementsService::new(OpenAiCompatibleModel::new())
        .publish_blueprint(PathBuf::from(path), revision_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn publish_blueprint(
    path: String,
    revision_id: String,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || publish_blueprint_sync(path, revision_id))
        .await
        .map_err(join_error)?
}

pub fn resolve_info_request_sync(
    path: String,
    info_request_id: String,
    answer: String,
) -> Result<ProjectWorkspace, CommandError> {
    let request_id = parse_info_request_id(&info_request_id)?;
    RequirementsService::new(OpenAiCompatibleModel::new())
        .resolve_info_request(PathBuf::from(path), request_id, answer)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn resolve_info_request(
    path: String,
    info_request_id: String,
    answer: String,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        resolve_info_request_sync(path, info_request_id, answer)
    })
    .await
    .map_err(join_error)?
}

pub fn dismiss_info_request_sync(
    path: String,
    info_request_id: String,
    reason: String,
) -> Result<ProjectWorkspace, CommandError> {
    let request_id = parse_info_request_id(&info_request_id)?;
    RequirementsService::new(OpenAiCompatibleModel::new())
        .dismiss_info_request(PathBuf::from(path), request_id, reason)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn dismiss_info_request(
    path: String,
    info_request_id: String,
    reason: String,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        dismiss_info_request_sync(path, info_request_id, reason)
    })
    .await
    .map_err(join_error)?
}

pub fn create_code_investigation_sync(
    path: String,
    repository_key: String,
    base_ref: Option<String>,
    question: String,
) -> Result<ProjectWorkspace, CommandError> {
    CodeInvestigationService::new()
        .create(PathBuf::from(path), repository_key, base_ref, question)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn create_code_investigation(
    path: String,
    repository_key: String,
    base_ref: Option<String>,
    question: String,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        create_code_investigation_sync(path, repository_key, base_ref, question)
    })
    .await
    .map_err(join_error)?
}

pub fn confirm_code_investigation_sync(
    path: String,
    investigation_id: String,
) -> Result<ProjectWorkspace, CommandError> {
    let investigation_id = parse_code_investigation_id(&investigation_id)?;
    CodeInvestigationService::new()
        .confirm(PathBuf::from(path), investigation_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn confirm_code_investigation(
    path: String,
    investigation_id: String,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        confirm_code_investigation_sync(path, investigation_id)
    })
    .await
    .map_err(join_error)?
}

pub fn reject_code_investigation_sync(
    path: String,
    investigation_id: String,
    reason: String,
) -> Result<ProjectWorkspace, CommandError> {
    let investigation_id = parse_code_investigation_id(&investigation_id)?;
    CodeInvestigationService::new()
        .reject(PathBuf::from(path), investigation_id, reason)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn reject_code_investigation(
    path: String,
    investigation_id: String,
    reason: String,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        reject_code_investigation_sync(path, investigation_id, reason)
    })
    .await
    .map_err(join_error)?
}

pub fn read_plan_workspace_sync(path: String) -> Result<ProjectWorkspace, CommandError> {
    read_requirements_workspace_sync(path)
}

#[tauri::command]
pub async fn read_plan_workspace(path: String) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || read_plan_workspace_sync(path))
        .await
        .map_err(join_error)?
}

pub fn generate_plan_draft_sync(path: String) -> Result<ProjectWorkspace, CommandError> {
    PlanService::new()
        .generate_draft(PathBuf::from(&path))
        .map_err(CommandError::from)?;
    read_requirements_workspace_sync(path)
}

#[tauri::command]
pub async fn generate_plan_draft(path: String) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || generate_plan_draft_sync(path))
        .await
        .map_err(join_error)?
}

pub fn save_plan_draft_sync(
    path: String,
    content: PlanContent,
) -> Result<ProjectWorkspace, CommandError> {
    PlanService::new()
        .save_draft(PathBuf::from(&path), content)
        .map_err(CommandError::from)?;
    read_requirements_workspace_sync(path)
}

#[tauri::command]
pub async fn save_plan_draft(
    path: String,
    content: PlanContent,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || save_plan_draft_sync(path, content))
        .await
        .map_err(join_error)?
}

pub fn preview_plan_publish_sync(
    path: String,
    revision_id: String,
) -> Result<PlanPublishPreview, CommandError> {
    let revision_id = parse_plan_revision_id(&revision_id)?;
    PlanService::new()
        .preview_publish(PathBuf::from(path), revision_id)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn preview_plan_publish(
    path: String,
    revision_id: String,
) -> Result<PlanPublishPreview, CommandError> {
    tauri::async_runtime::spawn_blocking(move || preview_plan_publish_sync(path, revision_id))
        .await
        .map_err(join_error)?
}

pub fn publish_plan_sync(
    path: String,
    revision_id: String,
    confirmed: bool,
) -> Result<ProjectWorkspace, CommandError> {
    let revision_id = parse_plan_revision_id(&revision_id)?;
    PlanService::new()
        .publish_confirmed(PathBuf::from(&path), revision_id, confirmed)
        .map_err(CommandError::from)?;
    read_requirements_workspace_sync(path)
}

#[tauri::command]
pub async fn publish_plan(
    path: String,
    revision_id: String,
    confirmed: bool,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || publish_plan_sync(path, revision_id, confirmed))
        .await
        .map_err(join_error)?
}

pub fn complete_human_work_sync(
    path: String,
    work_item_id: String,
    note: String,
    accepted: bool,
) -> Result<ProjectWorkspace, CommandError> {
    let work_item_id = parse_work_item_id(&work_item_id)?;
    PlanService::new()
        .complete_human_work(PathBuf::from(&path), work_item_id, note, accepted)
        .map_err(CommandError::from)?;
    read_requirements_workspace_sync(path)
}

#[tauri::command]
pub async fn complete_human_work(
    path: String,
    work_item_id: String,
    note: String,
    accepted: bool,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        complete_human_work_sync(path, work_item_id, note, accepted)
    })
    .await
    .map_err(join_error)?
}

pub fn mark_work_integrated_sync(
    path: String,
    work_item_id: String,
    reference: String,
) -> Result<ProjectWorkspace, CommandError> {
    let work_item_id = parse_work_item_id(&work_item_id)?;
    PlanService::new()
        .mark_work_integrated(PathBuf::from(&path), work_item_id, reference)
        .map_err(CommandError::from)?;
    read_requirements_workspace_sync(path)
}

#[tauri::command]
pub async fn mark_work_integrated(
    path: String,
    work_item_id: String,
    reference: String,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        mark_work_integrated_sync(path, work_item_id, reference)
    })
    .await
    .map_err(join_error)?
}

pub fn read_integration_workspace_sync(
    service: &IntegrationService,
    path: String,
) -> Result<IntegrationWorkspace, CommandError> {
    service
        .load_workspace(PathBuf::from(path))
        .map_err(Into::into)
}

#[tauri::command]
pub async fn read_integration_workspace(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
) -> Result<IntegrationWorkspace, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        read_integration_workspace_sync(service.as_ref(), path)
    })
    .await
    .map_err(join_error)?
}

pub fn validate_repository_sync(
    service: &IntegrationService,
    path: String,
    draft: RepositoryDraft,
) -> Result<RepositoryValidationView, CommandError> {
    service
        .validate_repository(PathBuf::from(path), &draft)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn validate_repository(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
    draft: RepositoryDraft,
) -> Result<RepositoryValidationView, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        validate_repository_sync(service.as_ref(), path, draft)
    })
    .await
    .map_err(join_error)?
}

pub fn save_repository_sync(
    service: &IntegrationService,
    path: String,
    draft: RepositoryDraft,
) -> Result<IntegrationWorkspace, CommandError> {
    service
        .save_repository(PathBuf::from(path), draft)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn save_repository(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
    draft: RepositoryDraft,
) -> Result<IntegrationWorkspace, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        save_repository_sync(service.as_ref(), path, draft)
    })
    .await
    .map_err(join_error)?
}

pub fn read_repository_verification_commands_sync(
    service: &IntegrationService,
    path: String,
    repository_key: String,
) -> Result<Vec<RepositoryVerificationCommand>, CommandError> {
    service
        .load_verification_commands(PathBuf::from(path), &repository_key)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn read_repository_verification_commands(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
    repository_key: String,
) -> Result<Vec<RepositoryVerificationCommand>, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        read_repository_verification_commands_sync(service.as_ref(), path, repository_key)
    })
    .await
    .map_err(join_error)?
}

pub fn save_repository_verification_commands_sync(
    service: &IntegrationService,
    path: String,
    repository_key: String,
    commands: Vec<RepositoryVerificationCommand>,
) -> Result<Vec<RepositoryVerificationCommand>, CommandError> {
    service
        .save_verification_commands(PathBuf::from(path), &repository_key, commands)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn save_repository_verification_commands(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
    repository_key: String,
    commands: Vec<RepositoryVerificationCommand>,
) -> Result<Vec<RepositoryVerificationCommand>, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        save_repository_verification_commands_sync(service.as_ref(), path, repository_key, commands)
    })
    .await
    .map_err(join_error)?
}

pub fn save_credential_sync(
    service: &IntegrationService,
    path: String,
    input: SaveCredentialInput,
) -> Result<IntegrationWorkspace, CommandError> {
    service
        .save_credential(PathBuf::from(path), input)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn save_credential(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
    input: SaveCredentialInput,
) -> Result<IntegrationWorkspace, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        save_credential_sync(service.as_ref(), path, input)
    })
    .await
    .map_err(join_error)?
}

pub fn delete_credential_sync(
    service: &IntegrationService,
    path: String,
    credential_id: String,
) -> Result<IntegrationWorkspace, CommandError> {
    let credential_id = parse_credential_id(&credential_id)?;
    service
        .delete_credential(PathBuf::from(path), credential_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn delete_credential(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
    credential_id: String,
) -> Result<IntegrationWorkspace, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        delete_credential_sync(service.as_ref(), path, credential_id)
    })
    .await
    .map_err(join_error)?
}

#[allow(clippy::too_many_arguments)]
pub fn update_capability_sync(
    service: &IntegrationService,
    path: String,
    capability_id: String,
    environment: String,
    status: CapabilityStatus,
    credential_id: Option<String>,
    note: Option<String>,
) -> Result<IntegrationWorkspace, CommandError> {
    let capability_id = parse_capability_id(&capability_id)?;
    let credential_id = credential_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(parse_credential_id)
        .transpose()?;
    service
        .update_capability(
            PathBuf::from(path),
            capability_id,
            environment,
            status,
            credential_id,
            note,
        )
        .map_err(Into::into)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn update_capability(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
    capability_id: String,
    environment: String,
    status: CapabilityStatus,
    credential_id: Option<String>,
    note: Option<String>,
) -> Result<IntegrationWorkspace, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        update_capability_sync(
            service.as_ref(),
            path,
            capability_id,
            environment,
            status,
            credential_id,
            note,
        )
    })
    .await
    .map_err(join_error)?
}

pub fn save_capability_api_endpoint_sync(
    service: &IntegrationService,
    path: String,
    endpoint_id: Option<String>,
    input: CapabilityApiEndpointInput,
) -> Result<IntegrationWorkspace, CommandError> {
    let endpoint_id = endpoint_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(parse_capability_api_endpoint_id)
        .transpose()?;
    service
        .save_capability_api_endpoint(PathBuf::from(path), endpoint_id, input)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn save_capability_api_endpoint(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
    endpoint_id: Option<String>,
    input: CapabilityApiEndpointInput,
) -> Result<IntegrationWorkspace, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        save_capability_api_endpoint_sync(service.as_ref(), path, endpoint_id, input)
    })
    .await
    .map_err(join_error)?
}

pub fn delete_capability_api_endpoint_sync(
    service: &IntegrationService,
    path: String,
    endpoint_id: String,
) -> Result<IntegrationWorkspace, CommandError> {
    let endpoint_id = parse_capability_api_endpoint_id(&endpoint_id)?;
    service
        .delete_capability_api_endpoint(PathBuf::from(path), endpoint_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn delete_capability_api_endpoint(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
    endpoint_id: String,
) -> Result<IntegrationWorkspace, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        delete_capability_api_endpoint_sync(service.as_ref(), path, endpoint_id)
    })
    .await
    .map_err(join_error)?
}

pub fn import_github_session_sync(
    service: &IntegrationService,
    path: String,
) -> Result<GitHubSessionView, CommandError> {
    service
        .import_github_session(PathBuf::from(path))
        .map_err(Into::into)
}

#[tauri::command]
pub async fn import_github_session(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
) -> Result<GitHubSessionView, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || import_github_session_sync(service.as_ref(), path))
        .await
        .map_err(join_error)?
}

pub fn start_github_device_flow_sync(
    service: &IntegrationService,
    client_id: String,
) -> Result<DeviceFlowView, CommandError> {
    service
        .start_github_device_flow(&client_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn start_github_device_flow(
    service: State<'_, Arc<IntegrationService>>,
    client_id: String,
) -> Result<DeviceFlowView, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        start_github_device_flow_sync(service.as_ref(), client_id)
    })
    .await
    .map_err(join_error)?
}

pub fn poll_github_device_flow_sync(
    service: &IntegrationService,
    path: String,
    session_id: String,
) -> Result<GitHubDevicePollView, CommandError> {
    service
        .poll_github_device_flow(PathBuf::from(path), &session_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn poll_github_device_flow(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
    session_id: String,
) -> Result<GitHubDevicePollView, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        poll_github_device_flow_sync(service.as_ref(), path, session_id)
    })
    .await
    .map_err(join_error)?
}

pub fn create_github_private_repository_sync(
    service: &IntegrationService,
    path: String,
    input: CreatePrivateRepositoryInput,
) -> Result<coldstart_adapters::GitHubRepository, CommandError> {
    service
        .create_github_private_repository(PathBuf::from(path), input)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn create_github_private_repository(
    service: State<'_, Arc<IntegrationService>>,
    path: String,
    input: CreatePrivateRepositoryInput,
) -> Result<coldstart_adapters::GitHubRepository, CommandError> {
    let service = Arc::clone(&service);
    tauri::async_runtime::spawn_blocking(move || {
        create_github_private_repository_sync(service.as_ref(), path, input)
    })
    .await
    .map_err(join_error)?
}

pub fn list_github_issues_sync(
    path: String,
    repository_key: String,
    state: GitHubIssueListState,
) -> Result<GitHubIssueListView, CommandError> {
    GitHubIssueService::new()
        .list_issues(PathBuf::from(path), &repository_key, state)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_github_issues(
    path: String,
    repository_key: String,
    state: GitHubIssueListState,
) -> Result<GitHubIssueListView, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        list_github_issues_sync(path, repository_key, state)
    })
    .await
    .map_err(join_error)?
}

pub fn import_github_issue_sync(
    path: String,
    repository_key: String,
    issue_number: u64,
) -> Result<ProjectWorkspace, CommandError> {
    GitHubIssueService::new()
        .import_issue(PathBuf::from(path), &repository_key, issue_number)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn import_github_issue(
    path: String,
    repository_key: String,
    issue_number: u64,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        import_github_issue_sync(path, repository_key, issue_number)
    })
    .await
    .map_err(join_error)?
}

pub fn list_github_projects_sync(
    path: String,
    repository_key: String,
) -> Result<GitHubProjectOverviewView, CommandError> {
    GitHubProjectService::new()
        .list_projects(PathBuf::from(path), &repository_key)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_github_projects(
    path: String,
    repository_key: String,
) -> Result<GitHubProjectOverviewView, CommandError> {
    tauri::async_runtime::spawn_blocking(move || list_github_projects_sync(path, repository_key))
        .await
        .map_err(join_error)?
}

pub fn load_review_workspace_sync(path: String) -> Result<ReviewWorkspace, CommandError> {
    ReviewService::new()
        .load_workspace(PathBuf::from(path))
        .map_err(Into::into)
}

#[tauri::command]
pub async fn load_review_workspace(path: String) -> Result<ReviewWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || load_review_workspace_sync(path))
        .await
        .map_err(join_error)?
}

pub fn save_review_configuration_sync(
    path: String,
    input: ReviewConfigurationInput,
) -> Result<ReviewConfiguration, CommandError> {
    ReviewService::new()
        .save_configuration(PathBuf::from(path), input)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn save_review_configuration(
    path: String,
    input: ReviewConfigurationInput,
) -> Result<ReviewConfiguration, CommandError> {
    tauri::async_runtime::spawn_blocking(move || save_review_configuration_sync(path, input))
        .await
        .map_err(join_error)?
}

pub fn import_review_standard_sync(
    path: String,
    input: ReviewStandardImport,
) -> Result<ReviewStandardView, CommandError> {
    ReviewService::new()
        .import_standard(PathBuf::from(path), input)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn import_review_standard(
    path: String,
    input: ReviewStandardImport,
) -> Result<ReviewStandardView, CommandError> {
    tauri::async_runtime::spawn_blocking(move || import_review_standard_sync(path, input))
        .await
        .map_err(join_error)?
}

pub fn create_review_standard_sync(
    path: String,
    input: ReviewStandardManualInput,
) -> Result<ReviewStandardView, CommandError> {
    ReviewService::new()
        .create_manual_standard(PathBuf::from(path), input)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn create_review_standard(
    path: String,
    input: ReviewStandardManualInput,
) -> Result<ReviewStandardView, CommandError> {
    tauri::async_runtime::spawn_blocking(move || create_review_standard_sync(path, input))
        .await
        .map_err(join_error)?
}

pub fn revise_review_standard_sync(
    path: String,
    standard_id: String,
    input: ReviewStandardRevisionInput,
) -> Result<ReviewStandardView, CommandError> {
    let standard_id = parse_review_standard_id(&standard_id)?;
    ReviewService::new()
        .revise_standard(PathBuf::from(path), standard_id, input)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn revise_review_standard(
    path: String,
    standard_id: String,
    input: ReviewStandardRevisionInput,
) -> Result<ReviewStandardView, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        revise_review_standard_sync(path, standard_id, input)
    })
    .await
    .map_err(join_error)?
}

pub fn set_review_standard_enabled_sync(
    path: String,
    standard_id: String,
    enabled: bool,
) -> Result<ReviewWorkspace, CommandError> {
    let standard_id = parse_review_standard_id(&standard_id)?;
    ReviewService::new()
        .set_standard_enabled(PathBuf::from(path), standard_id, enabled)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn set_review_standard_enabled(
    path: String,
    standard_id: String,
    enabled: bool,
) -> Result<ReviewWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        set_review_standard_enabled_sync(path, standard_id, enabled)
    })
    .await
    .map_err(join_error)?
}

pub fn reorder_review_standards_sync(
    path: String,
    ordered_ids: Vec<String>,
) -> Result<ReviewWorkspace, CommandError> {
    let ordered_ids = ordered_ids
        .iter()
        .map(|value| parse_review_standard_id(value))
        .collect::<Result<Vec<_>, _>>()?;
    ReviewService::new()
        .reorder_standards(PathBuf::from(path), ordered_ids)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn reorder_review_standards(
    path: String,
    ordered_ids: Vec<String>,
) -> Result<ReviewWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || reorder_review_standards_sync(path, ordered_ids))
        .await
        .map_err(join_error)?
}

pub fn delete_review_standard_sync(
    path: String,
    standard_id: String,
) -> Result<ReviewWorkspace, CommandError> {
    let standard_id = parse_review_standard_id(&standard_id)?;
    ReviewService::new()
        .delete_standard(PathBuf::from(path), standard_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn delete_review_standard(
    path: String,
    standard_id: String,
) -> Result<ReviewWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || delete_review_standard_sync(path, standard_id))
        .await
        .map_err(join_error)?
}

pub fn read_review_standard_sync(
    path: String,
    standard_id: String,
) -> Result<ReviewStandardContent, CommandError> {
    let standard_id = parse_review_standard_id(&standard_id)?;
    ReviewService::new()
        .read_standard(PathBuf::from(path), standard_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn read_review_standard(
    path: String,
    standard_id: String,
) -> Result<ReviewStandardContent, CommandError> {
    tauri::async_runtime::spawn_blocking(move || read_review_standard_sync(path, standard_id))
        .await
        .map_err(join_error)?
}

pub fn waive_work_review_sync(
    path: String,
    work_item_id: String,
    report_id: String,
    reason: String,
) -> Result<WorkReportRecord, CommandError> {
    let work_item_id = parse_work_item_id(&work_item_id)?;
    let report_id = parse_work_report_id(&report_id)?;
    PlanService::new()
        .waive_review(PathBuf::from(path), work_item_id, report_id, reason)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn waive_work_review(
    path: String,
    work_item_id: String,
    report_id: String,
    reason: String,
) -> Result<WorkReportRecord, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        waive_work_review_sync(path, work_item_id, report_id, reason)
    })
    .await
    .map_err(join_error)?
}

pub fn create_draft_pull_request_sync(
    path: String,
    report_id: String,
) -> Result<PullRequestRecord, CommandError> {
    let report_id = parse_work_report_id(&report_id)?;
    PullRequestService::new()
        .create_draft(PathBuf::from(path), report_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn create_draft_pull_request(
    path: String,
    report_id: String,
) -> Result<PullRequestRecord, CommandError> {
    tauri::async_runtime::spawn_blocking(move || create_draft_pull_request_sync(path, report_id))
        .await
        .map_err(join_error)?
}

pub fn refresh_pull_request_sync(
    path: String,
    pull_request_id: String,
) -> Result<PullRequestRecord, CommandError> {
    let pull_request_id = parse_pull_request_id(&pull_request_id)?;
    PullRequestService::new()
        .refresh(PathBuf::from(path), pull_request_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn refresh_pull_request(
    path: String,
    pull_request_id: String,
) -> Result<PullRequestRecord, CommandError> {
    tauri::async_runtime::spawn_blocking(move || refresh_pull_request_sync(path, pull_request_id))
        .await
        .map_err(join_error)?
}

fn parse_revision_id(value: &str) -> Result<BlueprintRevisionId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "蓝图版本 ID 无效".to_owned(),
    })
}

fn parse_artifact_id(value: &str) -> Result<ArtifactId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "需求来源 ID 无效".to_owned(),
    })
}

fn parse_info_request_id(value: &str) -> Result<InfoRequestId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "信息请求 ID 无效".to_owned(),
    })
}

fn parse_code_investigation_id(value: &str) -> Result<CodeInvestigationId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "历史代码调查 ID 无效".to_owned(),
    })
}

fn parse_plan_revision_id(value: &str) -> Result<PlanRevisionId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "计划版本 ID 无效".to_owned(),
    })
}

fn parse_work_item_id(value: &str) -> Result<WorkItemId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "工作项 ID 无效".to_owned(),
    })
}

fn parse_credential_id(value: &str) -> Result<CredentialId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "凭据 ID 无效".to_owned(),
    })
}

fn parse_capability_id(value: &str) -> Result<BlueprintEntityId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "能力 ID 无效".to_owned(),
    })
}

fn parse_capability_api_endpoint_id(value: &str) -> Result<CapabilityApiEndpointId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "能力 API 端点 ID 无效".to_owned(),
    })
}

fn parse_review_standard_id(value: &str) -> Result<ReviewStandardId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "评审规范 ID 无效".to_owned(),
    })
}

fn parse_work_report_id(value: &str) -> Result<WorkReportId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "工作报告 ID 无效".to_owned(),
    })
}

fn parse_pull_request_id(value: &str) -> Result<PullRequestId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "Pull Request ID 无效".to_owned(),
    })
}

fn join_error(error: tauri::Error) -> CommandError {
    CommandError {
        code: "task_join".to_owned(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn path_string(path: &std::path::Path) -> String {
        path.to_str()
            .expect("temporary path must be valid UTF-8")
            .to_owned()
    }

    #[test]
    fn create_project_sync_creates_a_project() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));

        let status =
            create_project_sync("Demo Project".to_owned(), home).expect("project must be created");

        assert_eq!(status.name, "Demo Project");
        assert_eq!(status.event_count, 1);
    }

    #[test]
    fn read_project_status_sync_reads_the_created_project() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Demo Project".to_owned(), home.clone())
            .expect("project must be created");

        let status = read_project_status_sync(home).expect("project status must load");

        assert_eq!(status.name, "Demo Project");
        assert_eq!(status.event_count, 1);
    }

    #[test]
    fn duplicate_create_returns_already_initialized() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Demo Project".to_owned(), home.clone())
            .expect("project must be created");

        let error = create_project_sync("Demo Project".to_owned(), home)
            .expect_err("duplicate initialization must fail");

        assert_eq!(error.code, "already_initialized");
    }

    #[test]
    fn empty_directory_read_returns_manifest_missing() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(temp.path());

        let error =
            read_project_status_sync(home).expect_err("empty directory must not contain a project");

        assert_eq!(error.code, "manifest_missing");
    }

    #[test]
    fn command_error_serializes_code_and_message() {
        let error = CommandError {
            code: "already_initialized".to_owned(),
            message: "project is already initialized".to_owned(),
        };

        let value = serde_json::to_value(error).expect("command error must serialize");

        assert_eq!(
            value,
            serde_json::json!({
                "code": "already_initialized",
                "message": "project is already initialized"
            })
        );
    }

    #[test]
    fn read_requirements_workspace_sync_reports_manifest_missing() {
        let temp = TempDir::new().expect("temporary directory must exist");

        let error = read_requirements_workspace_sync(path_string(temp.path()))
            .expect_err("empty directory must fail");

        assert_eq!(error.code, "manifest_missing");
    }

    #[test]
    fn import_requirement_document_sync_updates_the_workspace() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Demo Project".to_owned(), home.clone())
            .expect("project must be created");
        let source = temp.path().join("requirements.md");
        fs::write(
            &source,
            "# 订单系统\n\n需要 Web 和微信小程序，文件上传使用阿里云 OSS。\n",
        )
        .expect("source must be written");

        let workspace = import_requirement_document_sync(home.clone(), path_string(&source))
            .expect("document must import");
        let loaded = read_requirements_workspace_sync(home).expect("workspace must load");

        assert_eq!(workspace.sources.len(), 1);
        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.status.event_count, 2);
    }

    #[test]
    fn read_requirement_image_sync_returns_base64_without_local_paths() {
        const IMAGE_BYTES: &[u8] = b"\x89PNG\r\n\x1a\ndesktop-image";
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Image Project".to_owned(), home.clone())
            .expect("project must be created");
        fs::create_dir_all(temp.path().join("images")).expect("image directory must create");
        fs::write(temp.path().join("images/order.png"), IMAGE_BYTES).expect("image must write");
        let source = temp.path().join("requirements.md");
        fs::write(
            &source,
            "# 订单原型\n\n![订单列表](images/order.png)\n\n## 发布\n\n小程序提审。\n",
        )
        .expect("source must be written");

        let workspace = import_requirement_document_sync(home.clone(), path_string(&source))
            .expect("document must import");
        let artifact = &workspace.sources[0];
        let image = &workspace.source_images[&artifact.id.to_string()][0];
        let preview =
            read_requirement_image_sync(home.clone(), artifact.id.to_string(), image.id.clone())
                .expect("image must read");

        assert_eq!(preview.image.mime_type, "image/png");
        assert_eq!(preview.source_file_name, "requirements.md");
        assert_eq!(preview.anchor_label, "订单原型");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&preview.data_base64)
                .expect("preview must be valid base64"),
            IMAGE_BYTES
        );
        let serialized = serde_json::to_string(&preview).expect("preview must serialize");
        assert!(!serialized.contains(&home));
        assert!(!serialized.contains("images/order.png"));
    }

    #[test]
    fn generate_blueprint_sync_without_sources_returns_a_stable_error() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Demo Project".to_owned(), home.clone())
            .expect("project must be created");

        let error = generate_blueprint_sync(
            home,
            ModelConfig {
                base_url: "https://example.invalid/v1".to_owned(),
                model: "test-model".to_owned(),
                api_key: "secret".to_owned(),
            },
            Vec::new(),
        )
        .expect_err("missing sources must fail");

        assert_eq!(error.code, "no_requirement_sources");
    }

    #[test]
    fn invalid_revision_and_request_ids_are_rejected_before_storage() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Demo Project".to_owned(), home.clone())
            .expect("project must be created");

        let revision_error = publish_blueprint_sync(home.clone(), "not-a-uuid".to_owned())
            .expect_err("invalid revision id must fail");
        let request_error =
            resolve_info_request_sync(home, "not-a-uuid".to_owned(), "answer".to_owned())
                .expect_err("invalid request id must fail");

        assert_eq!(revision_error.code, "invalid_id");
        assert_eq!(request_error.code, "invalid_id");
    }

    #[test]
    fn empty_info_request_answer_is_rejected() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Demo Project".to_owned(), home.clone())
            .expect("project must be created");

        let error =
            resolve_info_request_sync(home, InfoRequestId::new().to_string(), "  ".to_owned())
                .expect_err("empty answer must fail");

        assert_eq!(error.code, "info_request_answer_empty");
    }

    #[test]
    fn info_request_dismissal_requires_a_valid_id_and_reason() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Demo Project".to_owned(), home.clone())
            .expect("project must be created");

        let id_error = dismiss_info_request_sync(
            home.clone(),
            "not-an-id".to_owned(),
            "问题已过时".to_owned(),
        )
        .expect_err("invalid request id must fail");
        let reason_error =
            dismiss_info_request_sync(home, InfoRequestId::new().to_string(), "  ".to_owned())
                .expect_err("empty dismissal reason must fail");

        assert_eq!(id_error.code, "invalid_id");
        assert_eq!(reason_error.code, "info_request_dismiss_reason_empty");
    }

    #[test]
    fn review_commands_manage_manual_standards_without_hidden_inputs() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Review Project".to_owned(), home.clone())
            .expect("project must be created");

        let initial = load_review_workspace_sync(home.clone()).expect("workspace must load");
        assert!(initial.configuration.is_none());
        assert!(initial.standards.is_empty());

        let standard = create_review_standard_sync(
            home.clone(),
            ReviewStandardManualInput {
                name: "Team Rust Standards".to_owned(),
                scope: coldstart_core::ReviewScope::Repository("server".to_owned()),
                text: "# Rust\n\n- Return stable errors\n- Test failure paths\n".to_owned(),
            },
        )
        .expect("manual standard must be created");
        assert_eq!(standard.name, "Team Rust Standards");
        assert!(standard.enabled);

        let content = read_review_standard_sync(home.clone(), standard.id.to_string())
            .expect("standard content must be readable");
        assert_eq!(content.revision_id, standard.current_revision_id);
        assert!(content.text.contains("Test failure paths"));

        let disabled =
            set_review_standard_enabled_sync(home.clone(), standard.id.to_string(), false)
                .expect("standard must be disabled");
        assert!(!disabled.standards[0].enabled);

        let reordered = reorder_review_standards_sync(home.clone(), vec![standard.id.to_string()])
            .expect("standard order must save");
        assert_eq!(reordered.standards.len(), 1);

        let deleted = delete_review_standard_sync(home, standard.id.to_string())
            .expect("standard must be soft deleted");
        assert!(deleted.standards.is_empty());
    }

    #[test]
    fn review_command_ids_are_validated_before_storage() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Review Project".to_owned(), home.clone())
            .expect("project must be created");

        let error = read_review_standard_sync(home, "not-an-id".to_owned())
            .expect_err("invalid standard id must fail");
        assert_eq!(error.code, "invalid_id");
    }

    #[test]
    fn project_memory_command_returns_safe_paginated_events() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Memory Project".to_owned(), home.clone())
            .expect("project must be created");

        let page = read_project_memory_sync(home.clone(), None, Some(50), false)
            .expect("memory page must load");
        let serialized = serde_json::to_string(&page).expect("memory page must serialize");

        assert_eq!(page.total_count, 1);
        assert_eq!(page.visible_count, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].event_type, "project_initialized");
        assert!(!serialized.contains("payloadJson"));
        assert!(!serialized.contains("payload_json"));
    }

    #[test]
    fn project_memory_command_rejects_invalid_page_bounds() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Memory Project".to_owned(), home.clone())
            .expect("project must be created");

        let limit_error = read_project_memory_sync(home.clone(), None, Some(0), false)
            .expect_err("zero limit must fail");
        let cursor_error = read_project_memory_sync(home, Some(0), Some(100), false)
            .expect_err("zero cursor must fail");

        assert_eq!(limit_error.code, "invalid_memory_limit");
        assert_eq!(cursor_error.code, "invalid_memory_cursor");
    }

    #[test]
    fn code_investigation_command_ids_are_validated_before_storage() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Investigation Project".to_owned(), home.clone())
            .expect("project must be created");

        let confirm_error = confirm_code_investigation_sync(home.clone(), "not-an-id".to_owned())
            .expect_err("invalid investigation id must fail");
        let reject_error =
            reject_code_investigation_sync(home, "not-an-id".to_owned(), "证据不足".to_owned())
                .expect_err("invalid investigation id must fail");

        assert_eq!(confirm_error.code, "invalid_id");
        assert_eq!(reject_error.code, "invalid_id");
    }

    #[test]
    fn code_investigation_create_and_reject_validate_required_text() {
        let temp = TempDir::new().expect("temporary directory must exist");
        let home = path_string(&temp.path().join("demo"));
        create_project_sync("Investigation Project".to_owned(), home.clone())
            .expect("project must be created");

        let question_error =
            create_code_investigation_sync(home.clone(), "web".to_owned(), None, "  ".to_owned())
                .expect_err("empty investigation question must fail");
        let rejection_error = reject_code_investigation_sync(
            home,
            CodeInvestigationId::new().to_string(),
            "  ".to_owned(),
        )
        .expect_err("empty rejection reason must fail");

        assert_eq!(question_error.code, "invalid_code_investigation");
        assert_eq!(rejection_error.code, "code_investigation_rejection_empty");
    }
}
