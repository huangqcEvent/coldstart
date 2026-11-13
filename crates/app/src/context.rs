//! 项目级只读上下文读取。
//!
//! 组合项目、蓝图、计划与能力视图，供桌面端和 MCP 共享同一事实来源；该模块不写
//! 数据库、不刷新租约，也不改变任务状态。

use coldstart_core::Project;
use coldstart_storage::Database;

use crate::{AppError, ProjectHome, ProjectStatus};

pub(crate) struct OpenProject {
    pub(crate) project: Project,
    pub(crate) home: ProjectHome,
    pub(crate) database: Database,
}

impl OpenProject {
    pub(crate) fn open(project_home: std::path::PathBuf) -> Result<Self, AppError> {
        let home = ProjectHome::resolve(project_home)?;
        let root = home.absolute_root()?;
        let manifest = home.load_manifest()?;
        let project = manifest.to_project(root)?;
        let database_path = home.database_path();
        if !database_path.exists() {
            return Err(AppError::DatabaseMissing(database_path));
        }

        let database = Database::open(&database_path)?;
        let stored = database
            .load_project(&project.id, project.root_path.clone())?
            .ok_or(AppError::ProjectMissingFromDatabase(project.id))?;
        if stored.name != project.name || stored.created_at != project.created_at {
            return Err(AppError::ProjectMetadataMismatch(project.id));
        }
        if project.root_path.to_str().is_none() {
            return Err(AppError::NonUtf8Path(project.root_path));
        }

        Ok(Self {
            project,
            home,
            database,
        })
    }

    pub(crate) fn status(&self) -> Result<ProjectStatus, AppError> {
        Ok(ProjectStatus {
            project_id: self.project.id,
            name: self.project.name.clone(),
            root_path: self.project.root_path.clone(),
            created_at: self.project.created_at,
            event_count: self.database.event_count(&self.project.id)?,
        })
    }
}
