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
