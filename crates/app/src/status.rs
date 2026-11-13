//! 项目状态视图。
//!
//! 对桌面端暴露项目根目录、身份和初始化状态；视图只读取已存在的清单与数据库，
//! 不会因为打开项目而隐式创建或修复状态。

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use coldstart_core::ProjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatus {
    pub project_id: ProjectId,
    pub name: String,
    pub root_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub event_count: u64,
}
