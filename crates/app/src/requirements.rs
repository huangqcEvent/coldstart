//! 需求导入、蓝图生成与人工确认。
//!
//! 负责导入不可变来源、构造模型上下文、校验来源锚点和保存蓝图修订；模型输出
//! 不能绕过人工发布，也不能引用未导入材料。

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use coldstart_adapters::{
    BlueprintModel, ModelAnchor, ModelCodeEvidence, ModelCodeFinding, ModelConfig,
    ModelExtractionRequest, ModelFact, ModelSource, extract_document,
};
use coldstart_core::{
    ArtifactId, ArtifactImage, ArtifactMetadata, BLUEPRINT_PROMPT_VERSION, BlueprintContent,
    BlueprintGeneration, BlueprintRevisionId, BlueprintStatus, CodeInvestigation,
    CodeInvestigationStatus, GenerationSource, InfoRequestDraft, InfoRequestId, InfoRequestStatus,
    ProjectId, validate_blueprint_structure,
};
use coldstart_storage::Database;
use sha2::{Digest, Sha256};

use crate::{
    AppError, ProjectWorkspace,
    context::OpenProject,
    context_compiler::{fact_is_sensitive, latest_fact_versions},
    plan_workspace::build_plan_workspace,
};

pub const MAX_SOURCE_CONTEXT_CHARACTERS: usize = 80_000;
const MAX_CODE_EVIDENCE_INVESTIGATIONS: usize = 3;
const MAX_CODE_EVIDENCE_CHARACTERS: usize = 35_000;
const CODE_EVIDENCE_NOTE_RESERVE: usize = 512;
const MAX_INFO_REQUEST_DISMISS_REASON_CHARACTERS: usize = 4_000;
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
        artifact_ids: Vec<ArtifactId>,
    ) -> Result<ProjectWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        if artifact_ids.is_empty() {
            return Err(AppError::NoRequirementSources);
        }
        let artifacts = project.database.list_artifacts(&project.project.id)?;
        if artifacts.is_empty() {
            return Err(AppError::NoRequirementSources);
        }
        let selected_artifact_ids = artifact_ids.into_iter().collect::<BTreeSet<_>>();
        for artifact_id in &selected_artifact_ids {
            if !artifacts.iter().any(|artifact| artifact.id == *artifact_id) {
                return Err(AppError::Storage(
                    coldstart_storage::StorageError::ArtifactNotFound(*artifact_id),
                ));
            }
        }
        let mut model_sources = Vec::with_capacity(selected_artifact_ids.len());
        let mut context_characters = 0usize;
        for artifact in &artifacts {
            if !selected_artifact_ids.contains(&artifact.id) {
                continue;
            }
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
        let facts = project.database.list_project_facts(&project.project.id)?;
        let project_facts = latest_fact_versions(&facts)
            .into_iter()
            .filter(|fact| !fact_is_sensitive(fact))
            .map(|fact| ModelFact {
                category: fact.category.as_str().to_owned(),
                question: fact.question.clone(),
                value: fact.value.clone(),
            })
            .collect();
        let mut confirmed_investigations = project
            .database
            .list_code_investigations(&project.project.id)?
            .into_iter()
            .filter(|investigation| investigation.status == CodeInvestigationStatus::Confirmed)
            .collect::<Vec<_>>();
        confirmed_investigations.sort_by(|left, right| {
            right
                .confirmed_at
                .cmp(&left.confirmed_at)
                .then_with(|| right.id.to_string().cmp(&left.id.to_string()))
        });
        let code_evidence = bounded_code_evidence(confirmed_investigations);
        let model_name = config.model.clone();
        let request = ModelExtractionRequest {
            config,
            project_name: project.project.name.clone(),
            sources: model_sources,
            project_facts,
            code_evidence,
        };
        let mut proposal = self.model.propose_blueprint(&request)?;
        let info_requests = std::mem::take(&mut proposal.open_questions)
            .into_iter()
            .map(InfoRequestDraft::from_proposal)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AppError::InvalidBlueprint(error.to_string()))?;
        let content = BlueprintContent::from_proposal(&project.project.id, proposal)
            .map_err(|error| AppError::InvalidBlueprint(error.to_string()))?;
        validate_content(
            &project.database,
            project.project.id,
            &content,
            Some(&selected_artifact_ids),
        )?;
        let revision = project.database.create_blueprint_revision_with_source_ids(
            &project.project.id,
            content,
            BlueprintGeneration {
                source: GenerationSource::Model,
                model_provider: Some(MODEL_PROVIDER.to_owned()),
                model_name: Some(model_name),
                prompt_version: Some(BLUEPRINT_PROMPT_VERSION.to_owned()),
            },
            info_requests,
            selected_artifact_ids.into_iter().collect(),
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
        validate_content(&project.database, project.project.id, &content, None)?;
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
        // 手工保存会创建新的蓝图修订，但待确认问题仍属于同一轮需求澄清。
        // 因此发布门禁按项目内全部未解决的阻塞问题计算，避免换一个修订号绕过 Inbox。
        let blocking_requests = project
            .database
            .list_info_requests(&project.project.id)?
            .into_iter()
            .filter(|request| request.status == InfoRequestStatus::Open && request.blocking)
            .count();
        if blocking_requests > 0 {
            return Err(AppError::UnresolvedBlockingInfoRequests(blocking_requests));
        }
        validate_content(
            &project.database,
            project.project.id,
            &revision.content,
            None,
        )?;
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

    /// 关闭过时或不再适用的阻塞问题；原因进入请求记录与审计事件，不生成项目事实。
    pub fn dismiss_info_request(
        &self,
        project_home: PathBuf,
        request_id: InfoRequestId,
        reason: String,
    ) -> Result<ProjectWorkspace, AppError> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(AppError::InfoRequestDismissReasonEmpty);
        }
        if reason.chars().count() > MAX_INFO_REQUEST_DISMISS_REASON_CHARACTERS {
            return Err(AppError::InfoRequestDismissReasonTooLong {
                limit: MAX_INFO_REQUEST_DISMISS_REASON_CHARACTERS,
            });
        }
        let mut project = OpenProject::open(project_home)?;
        project
            .database
            .dismiss_info_request(&project.project.id, &request_id, reason)?;
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
    let code_investigations = project
        .database
        .list_code_investigations(&project.project.id)?;
    Ok(ProjectWorkspace {
        status,
        sources,
        source_anchors,
        source_images,
        draft,
        published,
        inbox,
        facts,
        code_investigations,
        plan,
    })
}

/// 把已确认调查裁剪为有界模型上下文，优先保留最新报告和靠前的定位证据。
fn bounded_code_evidence(investigations: Vec<CodeInvestigation>) -> Vec<ModelCodeEvidence> {
    let total = investigations.len();
    let mut selected = Vec::new();
    let mut used_characters = 0usize;

    for investigation in investigations {
        if selected.len() >= MAX_CODE_EVIDENCE_INVESTIGATIONS {
            break;
        }
        let mut evidence = ModelCodeEvidence {
            investigation_id: investigation.id.to_string(),
            repository_key: investigation.repository_key,
            base_ref: investigation.base_ref,
            commit_sha: investigation.commit_sha.unwrap_or_default(),
            summary: investigation.summary.unwrap_or_default(),
            findings: Vec::new(),
            limitations: Vec::new(),
        };
        let base_characters = used_characters + code_evidence_characters(&evidence);
        if base_characters + CODE_EVIDENCE_NOTE_RESERVE > MAX_CODE_EVIDENCE_CHARACTERS
            && !selected.is_empty()
        {
            break;
        }

        let finding_count = investigation.findings.len();
        let mut included_findings = 0usize;
        for finding in investigation.findings {
            let finding = ModelCodeFinding {
                path: finding.path,
                line_start: finding.line_start,
                line_end: finding.line_end,
                symbol: finding.symbol,
                summary: finding.summary,
                risk: finding.risk,
                suggested_change_paths: finding.suggested_change_paths,
            };
            let next_characters = used_characters
                + code_evidence_characters(&evidence)
                + code_finding_characters(&finding);
            if next_characters + CODE_EVIDENCE_NOTE_RESERVE > MAX_CODE_EVIDENCE_CHARACTERS {
                break;
            }
            evidence.findings.push(finding);
            included_findings += 1;
        }
        if included_findings < finding_count {
            evidence.limitations.push(format!(
                "因上下文预算省略了 {} 条代码证据",
                finding_count - included_findings
            ));
        }

        let limitation_count = investigation.limitations.len();
        let mut included_limitations = 0usize;
        for limitation in investigation.limitations {
            if evidence.limitations.len() >= 20 {
                break;
            }
            let next_characters =
                used_characters + code_evidence_characters(&evidence) + limitation.chars().count();
            if next_characters + CODE_EVIDENCE_NOTE_RESERVE > MAX_CODE_EVIDENCE_CHARACTERS {
                break;
            }
            evidence.limitations.push(limitation);
            included_limitations += 1;
        }
        if included_limitations < limitation_count && evidence.limitations.len() < 20 {
            evidence.limitations.push(format!(
                "因上下文预算省略了 {} 条调查限制",
                limitation_count - included_limitations
            ));
        }

        used_characters += code_evidence_characters(&evidence);
        selected.push(evidence);
    }

    let omitted_investigations = total.saturating_sub(selected.len());
    if omitted_investigations > 0
        && let Some(last) = selected.last_mut()
    {
        let note = format!("另有 {omitted_investigations} 个已确认调查因数量或上下文预算未包含");
        if last.limitations.len() < 20 {
            last.limitations.push(note);
        } else if let Some(last_limitation) = last.limitations.last_mut() {
            *last_limitation = note;
        }
    }
    selected
}

fn code_evidence_characters(evidence: &ModelCodeEvidence) -> usize {
    evidence.investigation_id.chars().count()
        + evidence.repository_key.chars().count()
        + evidence.base_ref.chars().count()
        + evidence.commit_sha.chars().count()
        + evidence.summary.chars().count()
        + evidence
            .findings
            .iter()
            .map(code_finding_characters)
            .sum::<usize>()
        + evidence
            .limitations
            .iter()
            .map(|limitation| limitation.chars().count())
            .sum::<usize>()
}

fn code_finding_characters(finding: &ModelCodeFinding) -> usize {
    finding.path.chars().count()
        + finding.summary.chars().count()
        + finding.risk.chars().count()
        + finding
            .symbol
            .as_deref()
            .map(|symbol| symbol.chars().count())
            .unwrap_or_default()
        + finding
            .suggested_change_paths
            .iter()
            .map(|path| path.chars().count())
            .sum::<usize>()
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
    allowed_artifact_ids: Option<&BTreeSet<ArtifactId>>,
) -> Result<(), AppError> {
    validate_blueprint_structure(content)
        .map_err(|error| AppError::InvalidBlueprint(error.to_string()))?;
    validate_source_references(database, project_id, content, allowed_artifact_ids)
}

fn validate_source_references(
    database: &Database,
    project_id: ProjectId,
    content: &BlueprintContent,
    allowed_artifact_ids: Option<&BTreeSet<ArtifactId>>,
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
        if allowed_artifact_ids.is_some_and(|allowed| !allowed.contains(&source.artifact_id)) {
            return Err(AppError::InvalidSourceReference(format!(
                "模型引用了本次变更集之外的来源文档 {}",
                source.artifact_id
            )));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use coldstart_core::{CodeFinding, CodeInvestigationId, CodeInvestigationStatus, RepositoryId};

    fn confirmed_investigation(index: usize, findings: Vec<CodeFinding>) -> CodeInvestigation {
        CodeInvestigation {
            id: CodeInvestigationId::new(),
            project_id: ProjectId::new(),
            repository_id: RepositoryId::new(),
            repository_key: format!("repo-{index}"),
            base_ref: "main".to_owned(),
            question: "历史代码如何组织？".to_owned(),
            status: CodeInvestigationStatus::Confirmed,
            agent_profile: Some("codex".to_owned()),
            lease_expires_at: None,
            commit_sha: Some("a".repeat(40)),
            summary: Some(format!("调查结论 {index}")),
            findings,
            limitations: Vec::new(),
            report_hash: Some("b".repeat(64)),
            rejection_reason: None,
            created_at: Utc::now(),
            claimed_at: None,
            submitted_at: Some(Utc::now()),
            confirmed_at: Some(Utc::now()),
        }
    }

    fn finding(index: usize, text: &str) -> CodeFinding {
        CodeFinding {
            path: format!("src/module_{index}.rs"),
            line_start: 10,
            line_end: 20,
            symbol: Some(format!("handler_{index}")),
            summary: text.to_owned(),
            risk: text.to_owned(),
            suggested_change_paths: vec![format!("src/module_{index}.rs")],
        }
    }

    #[test]
    fn code_evidence_uses_only_the_three_most_recent_investigations() {
        let investigations = (0..4)
            .map(|index| confirmed_investigation(index, vec![finding(index, "证据")]))
            .collect();
        let evidence = bounded_code_evidence(investigations);

        assert_eq!(evidence.len(), 3);
        assert_eq!(evidence[0].repository_key, "repo-0");
        assert!(
            evidence[2]
                .limitations
                .iter()
                .any(|limitation| limitation.contains("另有 1 个"))
        );
    }

    #[test]
    fn code_evidence_stays_within_the_model_context_budget() {
        let findings = (0..40)
            .map(|index| finding(index, &"证据".repeat(900)))
            .collect();
        let evidence = bounded_code_evidence(vec![confirmed_investigation(0, findings)]);

        assert_eq!(evidence.len(), 1);
        assert!(evidence[0].findings.len() < 40);
        assert!(code_evidence_characters(&evidence[0]) <= MAX_CODE_EVIDENCE_CHARACTERS);
        assert!(
            evidence[0]
                .limitations
                .iter()
                .any(|limitation| limitation.contains("省略"))
        );
    }
}
