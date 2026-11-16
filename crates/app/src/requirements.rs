//! 需求导入、蓝图生成与人工确认。
//!
//! 负责导入不可变来源、构造模型上下文、校验来源锚点和保存蓝图修订；模型输出
//! 不能绕过人工发布，也不能引用未导入材料。

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
    ArtifactId, ArtifactImage, ArtifactMetadata, BLUEPRINT_PROMPT_VERSION, BlueprintContent,
    BlueprintGeneration, BlueprintRevisionId, BlueprintStatus, GenerationSource, InfoRequestDraft,
    InfoRequestId, ProjectId, validate_blueprint_structure,
};
use coldstart_storage::Database;
use sha2::{Digest, Sha256};

use crate::{
    AppError, ProjectWorkspace, context::OpenProject, plan_workspace::build_plan_workspace,
};

pub const MAX_SOURCE_CONTEXT_CHARACTERS: usize = 80_000;
const MODEL_PROVIDER: &str = "OpenAI Compatible";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementImageContent {
    pub image: ArtifactImage,
    pub source_file_name: String,
    pub anchor_label: String,
    pub bytes: Vec<u8>,
}

/// 读取来源图片前再次校验相对路径、大小和哈希，避免 UI 或 Agent 读到被替换的文件。
pub fn read_requirement_image(
    project_home: PathBuf,
    artifact_id: ArtifactId,
    image_id: &str,
) -> Result<RequirementImageContent, AppError> {
    let project = OpenProject::open(project_home)?;
    let artifact = project
        .database
        .list_artifacts(&project.project.id)?
        .into_iter()
        .find(|artifact| artifact.id == artifact_id)
        .ok_or(AppError::Storage(
            coldstart_storage::StorageError::ArtifactNotFound(artifact_id),
        ))?;
    let images = project.database.load_artifact_images(&artifact_id)?;
    let image = images
        .into_iter()
        .find(|image| image.id == image_id)
        .ok_or_else(|| AppError::RequirementImageNotFound(image_id.to_owned()))?;
    let anchor_label = project
        .database
        .load_artifact_anchors(&artifact_id)?
        .into_iter()
        .find(|anchor| anchor.id == image.anchor_id)
        .map(|anchor| anchor.label)
        .unwrap_or_else(|| image.anchor_id.clone());
    let relative_path = Path::new(&image.relative_path);
    if relative_path.is_absolute()
        || !relative_path.starts_with("images")
        || relative_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(AppError::RequirementImageInvalid);
    }
    let artifact_directory = project
        .home
        .artifacts_directory()
        .join(artifact_id.to_string());
    let images_directory = fs::canonicalize(artifact_directory.join("images"))
        .map_err(|_| AppError::RequirementImageInvalid)?;
    let stored_path = fs::canonicalize(artifact_directory.join(relative_path))
        .map_err(|_| AppError::RequirementImageInvalid)?;
    if !stored_path.starts_with(&images_directory) {
        return Err(AppError::RequirementImageInvalid);
    }
    let bytes = fs::read(&stored_path).map_err(|_| AppError::RequirementImageInvalid)?;
    if bytes.len() as u64 != image.byte_size || sha256_hex(&bytes) != image.sha256 {
        return Err(AppError::RequirementImageInvalid);
    }
    Ok(RequirementImageContent {
        image,
        source_file_name: artifact.file_name,
        anchor_label,
        bytes,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

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
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| AppError::NonUtf8Path(file_path.clone()))?
            .to_owned();
        import_requirement_file_as(
            project_home,
            &file_path,
            file_name,
            file_path.to_string_lossy().into_owned(),
        )
    }

    /// 从已导入来源生成蓝图候选，校验模型结构和来源锚点，不自动发布。
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

pub(crate) fn import_requirement_file_as(
    project_home: PathBuf,
    file_path: &Path,
    file_name: String,
    import_source: String,
) -> Result<ProjectWorkspace, AppError> {
    let mut project = OpenProject::open(project_home)?;
    let extracted = extract_document(file_path)?;
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
        import_source,
        imported_at: Utc::now(),
    };
    let artifact_images = extracted
        .images
        .iter()
        .map(|image| ArtifactImage {
            id: image.id.clone(),
            anchor_id: image.anchor_id.clone(),
            file_name: image.file_name.clone(),
            mime_type: image.mime_type.clone(),
            byte_size: image.bytes.len() as u64,
            sha256: image.sha256.clone(),
            relative_path: format!("images/{}.{}", image.id, image_extension(&image.mime_type)),
        })
        .collect::<Vec<_>>();

    let result: Result<(), AppError> = (|| {
        fs::create_dir_all(&artifact_directory)?;
        fs::copy(file_path, artifact_directory.join(source_name))?;
        fs::write(
            artifact_directory.join("extracted.txt"),
            extracted.text.as_bytes(),
        )?;
        write_pretty_json(&artifact_directory.join("anchors.json"), &extracted.anchors)?;
        if !artifact_images.is_empty() {
            fs::create_dir_all(artifact_directory.join("images"))?;
            for (image, metadata) in extracted.images.iter().zip(&artifact_images) {
                fs::write(
                    artifact_directory.join(&metadata.relative_path),
                    &image.bytes,
                )?;
            }
            write_pretty_json(&artifact_directory.join("images.json"), &artifact_images)?;
        }
        project.database.insert_artifact_with_images(
            &artifact,
            &extracted.anchors,
            &artifact_images,
        )?;
        Ok(())
    })();

    match result {
        Ok(()) => build_workspace(project),
        Err(source) => Err(rollback_artifact(source, &artifact_directory)),
    }
}

pub(crate) fn build_workspace(project: OpenProject) -> Result<ProjectWorkspace, AppError> {
    let plan = build_plan_workspace(&project)?;
    let status = project.status()?;
    let sources = project.database.list_artifacts(&project.project.id)?;
    let mut source_anchors = BTreeMap::new();
    let mut source_images = BTreeMap::new();
    for source in &sources {
        source_anchors.insert(
            source.id.to_string(),
            project.database.load_artifact_anchors(&source.id)?,
        );
        source_images.insert(
            source.id.to_string(),
            project.database.load_artifact_images(&source.id)?,
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
        source_images,
        draft,
        published,
        inbox,
        facts,
        plan,
    })
}

fn image_extension(mime_type: &str) -> &'static str {
    match mime_type {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    }
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
