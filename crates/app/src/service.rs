use std::path::PathBuf;

use coldstart_storage::Database;

use crate::{AppError, ProjectStatus, project_home::ProjectHome};

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
        let home = ProjectHome::at(project_home);
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
        let event_count = database.event_count(&project.id)?;
        if project.root_path.to_str().is_none() {
            return Err(AppError::NonUtf8Path(project.root_path));
        }

        Ok(ProjectStatus {
            project_id: project.id,
            name: project.name,
            root_path: project.root_path,
            created_at: project.created_at,
            event_count,
        })
    }
}
