use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use coldstart_adapters::{
    BlueprintModel, ModelAnchor, ModelConfig, ModelExtractionRequest, ModelSource, extract_document,
};
use coldstart_core::{
    ArtifactId, ArtifactMetadata, BLUEPRINT_PROMPT_VERSION, BlueprintContent, BlueprintGeneration,
    BlueprintRevisionId, BlueprintStatus, GenerationSource, InfoRequestDraft, InfoRequestId,
    ProjectId, validate_blueprint_structure,
};
use coldstart_storage::Database;

use crate::{AppError, ProjectWorkspace, context::OpenProject};

pub const MAX_SOURCE_CONTEXT_CHARACTERS: usize = 80_000;
const MODEL_PROVIDER: &str = "OpenAI Compatible";

pub struct RequirementsService<M> {
    model: M,
}

impl<M: BlueprintModel> RequirementsService<M> {
    pub fn new(model: M) -> Self {
        Self { model }
    }

    pub fn load_workspace(&self, project_home: PathBuf) -> Result<ProjectWorkspace, AppError> {
        build_workspace(OpenProject::open(project_home)?)
    }

    pub fn import_document(
        &self,
        project_home: PathBuf,
        file_path: PathBuf,
    ) -> Result<ProjectWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let extracted = extract_document(&file_path)?;
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| AppError::NonUtf8Path(file_path.clone()))?
            .to_owned();
        let artifact_id = ArtifactId::new();
        let artifact_directory = project
            .home
            .artifacts_directory()
            .join(artifact_id.to_string());
        let source_name = format!("source.{}", extracted.format.extension());
        let artifact = ArtifactMetadata {
            id: artifact_id,
            project_id: project.project.id,
            file_name,
            format: extracted.format,
            content_hash: extracted.content_hash.clone(),
            text_hash: extracted.text_hash.clone(),
            byte_size: extracted.byte_size,
            import_source: file_path.to_string_lossy().into_owned(),
            imported_at: Utc::now(),
        };

        let result: Result<(), AppError> = (|| {
            fs::create_dir_all(&artifact_directory)?;
            fs::copy(&file_path, artifact_directory.join(source_name))?;
            fs::write(
                artifact_directory.join("extracted.txt"),
                extracted.text.as_bytes(),
            )?;
            write_pretty_json(&artifact_directory.join("anchors.json"), &extracted.anchors)?;
            project
                .database
                .insert_artifact(&artifact, &extracted.anchors)?;
            Ok(())
        })();

        match result {
            Ok(()) => build_workspace(project),
            Err(source) => Err(rollback_artifact(source, &artifact_directory)),
        }
    }

    pub fn generate_blueprint(
        &self,
        project_home: PathBuf,
        config: ModelConfig,
    ) -> Result<ProjectWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let artifacts = project.database.list_artifacts(&project.project.id)?;
        if artifacts.is_empty() {
            return Err(AppError::NoRequirementSources);
        }
        let mut model_sources = Vec::with_capacity(artifacts.len());
        let mut context_characters = 0usize;
        for artifact in &artifacts {
            let anchors = project.database.load_artifact_anchors(&artifact.id)?;
            for anchor in &anchors {
                context_characters = context_characters.saturating_add(anchor.text.chars().count());
            }
            if context_characters > MAX_SOURCE_CONTEXT_CHARACTERS {
                return Err(AppError::SourceContextTooLarge {
                    actual: context_characters,
                    limit: MAX_SOURCE_CONTEXT_CHARACTERS,
                });
            }
            model_sources.push(ModelSource {
                artifact_id: artifact.id.to_string(),
                file_name: artifact.file_name.clone(),
                format: artifact.format,
                content_hash: artifact.content_hash.clone(),
                anchors: anchors
                    .into_iter()
                    .map(|anchor| ModelAnchor {
                        anchor_id: anchor.id,
                        label: anchor.label,
                        text: anchor.text,
                    })
                    .collect(),
            });
        }
        let model_name = config.model.clone();
        let request = ModelExtractionRequest {
            config,
            project_name: project.project.name.clone(),
            sources: model_sources,
        };
        let mut proposal = self.model.propose_blueprint(&request)?;
        let info_requests = std::mem::take(&mut proposal.open_questions)
            .into_iter()
            .map(InfoRequestDraft::from_proposal)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::InvalidBlueprint(error.to_string()))?;
        let content = BlueprintContent::from_proposal(&project.project.id, proposal)
            .map_err(|error| AppError::InvalidBlueprint(error.to_string()))?;
        validate_content(&project.database, project.project.id, &content)?;
        let revision = project.database.create_blueprint_revision(
            &project.project.id,
            content,
            BlueprintGeneration {
                source: GenerationSource::Model,
                model_provider: Some(MODEL_PROVIDER.to_owned()),
                model_name: Some(model_name),
                prompt_version: Some(BLUEPRINT_PROMPT_VERSION.to_owned()),
            },
            info_requests,
        )?;
        debug_assert_eq!(revision.status, BlueprintStatus::Draft);
        build_workspace(project)
    }

    pub fn save_blueprint(
        &self,
        project_home: PathBuf,
        content: BlueprintContent,
    ) -> Result<ProjectWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        validate_content(&project.database, project.project.id, &content)?;
        project.database.create_blueprint_revision(
            &project.project.id,
            content,
            BlueprintGeneration {
                source: GenerationSource::Manual,
                model_provider: None,
                model_name: None,
                prompt_version: None,
            },
            Vec::new(),
        )?;
        build_workspace(project)
    }

    pub fn publish_blueprint(
        &self,
        project_home: PathBuf,
        revision_id: BlueprintRevisionId,
    ) -> Result<ProjectWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let revision = project
            .database
            .load_blueprint(&revision_id)?
            .filter(|revision| revision.project_id == project.project.id)
            .ok_or(AppError::BlueprintNotFound(revision_id))?;
        if revision.status != BlueprintStatus::Draft {
            return Err(AppError::BlueprintNotDraft(revision_id));
        }
        validate_content(&project.database, project.project.id, &revision.content)?;
        project
            .database
            .publish_blueprint_revision(&project.project.id, &revision_id)?;
        build_workspace(project)
    }

    pub fn resolve_info_request(
        &self,
        project_home: PathBuf,
        request_id: InfoRequestId,
        answer: String,
    ) -> Result<ProjectWorkspace, AppError> {
        if answer.trim().is_empty() {
            return Err(AppError::InfoRequestAnswerEmpty);
        }
        let mut project = OpenProject::open(project_home)?;
        project
            .database
            .resolve_info_request(&project.project.id, &request_id, &answer)?;
        build_workspace(project)
    }
}

fn build_workspace(project: OpenProject) -> Result<ProjectWorkspace, AppError> {
    let status = project.status()?;
    let sources = project.database.list_artifacts(&project.project.id)?;
    let mut source_anchors = BTreeMap::new();
    for source in &sources {
        source_anchors.insert(
            source.id.to_string(),
            project.database.load_artifact_anchors(&source.id)?,
        );
    }
    let draft = project
        .database
        .latest_blueprint(&project.project.id, BlueprintStatus::Draft)?;
    let published = project
        .database
        .latest_blueprint(&project.project.id, BlueprintStatus::Published)?;
    let inbox = project.database.list_info_requests(&project.project.id)?;
    let facts = project.database.list_project_facts(&project.project.id)?;
    Ok(ProjectWorkspace {
        status,
        sources,
        source_anchors,
        draft,
        published,
        inbox,
        facts,
    })
}

fn validate_content(
    database: &Database,
    project_id: ProjectId,
    content: &BlueprintContent,
) -> Result<(), AppError> {
    validate_blueprint_structure(content)
        .map_err(|error| AppError::InvalidBlueprint(error.to_string()))?;
    validate_source_references(database, project_id, content)
}

fn validate_source_references(
    database: &Database,
    project_id: ProjectId,
    content: &BlueprintContent,
) -> Result<(), AppError> {
    let references = content.source_references();
    if references.is_empty() || content.product_goal.sources.is_empty() {
        return Err(AppError::InvalidSourceReference(
            "蓝图必须包含至少一条产品目标来源引用".to_owned(),
        ));
    }

    let artifacts = database.list_artifacts(&project_id)?;
    let mut anchors_by_artifact = HashMap::<ArtifactId, HashSet<String>>::new();
    for artifact in artifacts {
        let anchors = database.load_artifact_anchors(&artifact.id)?;
        anchors_by_artifact.insert(
            artifact.id,
            anchors.into_iter().map(|anchor| anchor.id).collect(),
        );
    }
    for source in references {
        let Some(anchor_ids) = anchors_by_artifact.get(&source.artifact_id) else {
            return Err(AppError::InvalidSourceReference(format!(
                "来源文档 {} 不属于当前项目",
                source.artifact_id
            )));
        };
        if !anchor_ids.contains(&source.anchor_id) {
            return Err(AppError::InvalidSourceReference(format!(
                "来源文档 {} 中不存在锚点 {}",
                source.artifact_id, source.anchor_id
            )));
        }
    }
    Ok(())
}

fn write_pretty_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    let mut encoded = serde_json::to_vec_pretty(value)?;
    encoded.push(b'\n');
    fs::write(path, encoded)?;
    Ok(())
}

fn rollback_artifact(source: AppError, directory: &Path) -> AppError {
    match fs::remove_dir_all(directory) {
        Ok(()) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => source,
        Err(_) => AppError::RollbackFailed {
            source: Box::new(source),
            residual_paths: vec![directory.to_path_buf()],
        },
    }
}
