//! 应用级项目历史命令。
//!
//! 命令层只负责把项目路径交给历史存储，并把阻塞的文件校验放入后台线程；项目
//! 历史不进入任何项目数据库，也不会回传密钥或业务内容。

use std::path::PathBuf;

use coldstart_app::{ProjectHistoryStore, ProjectHistoryView};
use tauri::State;

use crate::error::CommandError;

fn join_error(error: tauri::Error) -> CommandError {
    CommandError {
        code: "task_join".to_owned(),
        message: error.to_string(),
    }
}

#[tauri::command]
pub async fn read_project_history(
    store: State<'_, ProjectHistoryStore>,
) -> Result<ProjectHistoryView, CommandError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.read())
        .await
        .map_err(join_error)?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn remember_recent_project(
    path: String,
    store: State<'_, ProjectHistoryStore>,
) -> Result<ProjectHistoryView, CommandError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.remember(&PathBuf::from(path)))
        .await
        .map_err(join_error)?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn forget_recent_project(
    path: String,
    store: State<'_, ProjectHistoryStore>,
) -> Result<ProjectHistoryView, CommandError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.forget(&PathBuf::from(path)))
        .await
        .map_err(join_error)?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn clear_project_history(
    store: State<'_, ProjectHistoryStore>,
) -> Result<ProjectHistoryView, CommandError> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.clear())
        .await
        .map_err(join_error)?
        .map_err(Into::into)
}
