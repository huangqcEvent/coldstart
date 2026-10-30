use coldstart_app::{ProjectService, ProjectStatus};

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

#[cfg(test)]
mod tests {
    use super::*;
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
}
