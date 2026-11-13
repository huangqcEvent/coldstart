//! 项目创建与状态读取服务。
//!
//! 将项目目录初始化、清单读取和数据库身份校验组合为桌面端入口；所有失败都必须
//! 可恢复，不能留下与清单不一致的半成品。

use std::path::PathBuf;

use crate::{AppError, ProjectStatus, context::OpenProject, project_home::ProjectHome};

#[derive(Debug, Default)]
pub struct ProjectService;

impl ProjectService {
    pub fn new() -> Self {
        Self
    }

    pub fn create_project(
        &self,
        name: &str,
        project_home: PathBuf,
    ) -> Result<ProjectStatus, AppError> {
        ProjectHome::at(project_home.clone()).initialize(name)?;
        self.read_status(project_home)
    }

    pub fn read_status(&self, project_home: PathBuf) -> Result<ProjectStatus, AppError> {
        OpenProject::open(project_home)?.status()
    }
}
