use std::path::PathBuf;

use coldstart_adapters::{ModelConfig, OpenAiCompatibleModel};
use coldstart_app::{
    PlanService, ProjectService, ProjectStatus, ProjectWorkspace, RequirementsService,
};
use coldstart_core::{
    BlueprintContent, BlueprintRevisionId, InfoRequestId, PlanContent, PlanRevisionId, WorkItemId,
};

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

pub fn generate_blueprint_sync(
    path: String,
    config: ModelConfig,
) -> Result<ProjectWorkspace, CommandError> {
    RequirementsService::new(OpenAiCompatibleModel::new())
        .generate_blueprint(PathBuf::from(path), config)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn generate_blueprint(
    path: String,
    config: ModelConfig,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || generate_blueprint_sync(path, config))
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

pub fn publish_plan_sync(
    path: String,
    revision_id: String,
) -> Result<ProjectWorkspace, CommandError> {
    let revision_id = parse_plan_revision_id(&revision_id)?;
    PlanService::new()
        .publish(PathBuf::from(&path), revision_id)
        .map_err(CommandError::from)?;
    read_requirements_workspace_sync(path)
}

#[tauri::command]
pub async fn publish_plan(
    path: String,
    revision_id: String,
) -> Result<ProjectWorkspace, CommandError> {
    tauri::async_runtime::spawn_blocking(move || publish_plan_sync(path, revision_id))
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

fn parse_revision_id(value: &str) -> Result<BlueprintRevisionId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "蓝图版本 ID 无效".to_owned(),
    })
}

fn parse_info_request_id(value: &str) -> Result<InfoRequestId, CommandError> {
    value.parse().map_err(|_| CommandError {
        code: "invalid_id".to_owned(),
        message: "信息请求 ID 无效".to_owned(),
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
}
