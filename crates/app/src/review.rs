use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use chrono::Utc;
use coldstart_adapters::{DocumentError, extract_document};
use coldstart_core::{
    ArtifactFormat, MAX_REVIEW_STANDARD_CHARS, MAX_REVIEW_STANDARDS, ProjectId,
    PullRequestInsights, PullRequestRecord, ReviewConfiguration, ReviewConfigurationInput,
    ReviewScope, ReviewStandard, ReviewStandardId, ReviewStandardRevisionId,
    ReviewStandardSnapshot, ReviewStandardSource, ReviewStatus, ReviewWaiver, WorkItemId,
    WorkReportId, sha256_hex,
};
use coldstart_storage::NewReviewStandardRevision;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppError, context::OpenProject};

const REVIEW_STANDARDS_DIRECTORY: &str = "review-standards";
const MAX_RECENT_REVIEWS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewWorkspace {
    pub project_id: ProjectId,
    pub configuration: Option<ReviewConfiguration>,
    pub standards: Vec<ReviewStandardView>,
    pub recent_reviews: Vec<ReviewHistoryView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewStandardView {
    pub id: ReviewStandardId,
    pub name: String,
    pub scope: ReviewScope,
    pub enabled: bool,
    pub position: u32,
    pub current_revision_id: ReviewStandardRevisionId,
    pub revision_number: u64,
    pub source: ReviewStandardSource,
    pub file_name: String,
    pub format: ArtifactFormat,
    pub character_count: usize,
    pub content_hash: String,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewHistoryView {
    pub report_id: WorkReportId,
    pub work_item_id: WorkItemId,
    pub task_key: String,
    pub review_status: ReviewStatus,
    pub review: Option<coldstart_core::ReviewResult>,
    pub waiver: Option<ReviewWaiver>,
    pub pull_request: Option<PullRequestRecord>,
    pub pull_request_insights: Option<PullRequestInsights>,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewStandardContent {
    pub standard: ReviewStandardView,
    pub revision_id: ReviewStandardRevisionId,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewStandardImport {
    pub name: String,
    pub scope: ReviewScope,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewStandardManualInput {
    pub name: String,
    pub scope: ReviewScope,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewStandardRevisionInput {
    pub name: String,
    pub scope: ReviewScope,
    pub text: String,
}

#[derive(Clone, Default)]
pub struct ReviewService;

impl ReviewService {
    pub fn new() -> Self {
        Self
    }

    pub fn load_workspace(&self, project_home: PathBuf) -> Result<ReviewWorkspace, AppError> {
        let project = OpenProject::open(project_home)?;
        build_review_workspace(&project)
    }

    pub fn save_configuration(
        &self,
        project_home: PathBuf,
        input: ReviewConfigurationInput,
    ) -> Result<ReviewConfiguration, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let existing = project.database.review_configuration(&project.project.id)?;
        let consent_confirmed = input.consent_confirmed;
        let mut configuration = ReviewConfiguration::from_input(project.project.id, input.clone())
            .map_err(|error| AppError::ReviewStandardInvalid(error.to_string()))?;
        if configuration.enabled {
            let expected = configuration.expected_consent_fingerprint();
            if existing
                .as_ref()
                .is_some_and(|current| current.consent_fingerprint.as_deref() == Some(&expected))
            {
                configuration.consent_fingerprint = Some(expected);
            } else if !consent_confirmed {
                return Err(AppError::ReviewConsentRequired);
            }
        }
        if let Some(credential_id) = configuration.credential_id
            && project
                .database
                .credential(&project.project.id, &credential_id)?
                .is_none()
        {
            return Err(AppError::ReviewCredentialMissing);
        }
        configuration = project.database.save_review_configuration(&configuration)?;
        Ok(configuration)
    }

    pub fn import_standard(
        &self,
        project_home: PathBuf,
        input: ReviewStandardImport,
    ) -> Result<ReviewStandardView, AppError> {
        let mut project = OpenProject::open(project_home)?;
        validate_standard_name(&input.name)?;
        let extracted = extract_document(&input.path).map_err(map_document_error)?;
        validate_standard_text(&extracted.text)?;
        let file_name = input
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| AppError::ReviewStandardInvalid("规范文件名不是有效 UTF-8".to_owned()))?
            .to_owned();
        let original = fs::read(&input.path)?;
        let standard_id = ReviewStandardId::new();
        let revision_id = ReviewStandardRevisionId::new();
        let storage_relative_path = persist_standard_files(
            &project,
            standard_id,
            revision_id,
            Some((&file_name, extracted.format, &original)),
            &extracted.text,
        )?;
        let result = project.database.create_review_standard_revision(
            &project.project.id,
            NewReviewStandardRevision {
                standard_id,
                revision_id,
                name: input.name,
                scope: input.scope,
                source: ReviewStandardSource::Upload,
                file_name,
                format: extracted.format,
                content_hash: extracted.content_hash,
                text_hash: extracted.text_hash,
                character_count: extracted.text.chars().count(),
                storage_relative_path: storage_relative_path.clone(),
            },
            Utc::now(),
        );
        finish_standard_write(&project, standard_id, revision_id, result)
    }

    pub fn create_manual_standard(
        &self,
        project_home: PathBuf,
        input: ReviewStandardManualInput,
    ) -> Result<ReviewStandardView, AppError> {
        let mut project = OpenProject::open(project_home)?;
        validate_standard_name(&input.name)?;
        validate_standard_text(&input.text)?;
        let standard_id = ReviewStandardId::new();
        let revision_id = ReviewStandardRevisionId::new();
        let normalized = input.text.trim().to_owned();
        let storage_relative_path =
            persist_standard_files(&project, standard_id, revision_id, None, &normalized)?;
        let result = project.database.create_review_standard_revision(
            &project.project.id,
            NewReviewStandardRevision {
                standard_id,
                revision_id,
                name: input.name,
                scope: input.scope,
                source: ReviewStandardSource::Manual,
                file_name: "standard.md".to_owned(),
                format: ArtifactFormat::Markdown,
                content_hash: sha256_hex(normalized.as_bytes()),
                text_hash: sha256_hex(normalized.as_bytes()),
                character_count: normalized.chars().count(),
                storage_relative_path,
            },
            Utc::now(),
        );
        finish_standard_write(&project, standard_id, revision_id, result)
    }

    pub fn revise_standard(
        &self,
        project_home: PathBuf,
        standard_id: ReviewStandardId,
        input: ReviewStandardRevisionInput,
    ) -> Result<ReviewStandardView, AppError> {
        let mut project = OpenProject::open(project_home)?;
        validate_standard_name(&input.name)?;
        validate_standard_text(&input.text)?;
        let existing = project
            .database
            .list_review_standards(&project.project.id)?
            .into_iter()
            .find(|standard| standard.id == standard_id)
            .ok_or_else(|| AppError::ReviewStandardInvalid("规范不存在或已删除".to_owned()))?;
        let revision_id = ReviewStandardRevisionId::new();
        let normalized = input.text.trim().to_owned();
        let storage_relative_path =
            persist_standard_files(&project, standard_id, revision_id, None, &normalized)?;
        let result = project.database.create_review_standard_revision(
            &project.project.id,
            NewReviewStandardRevision {
                standard_id: existing.id,
                revision_id,
                name: input.name,
                scope: input.scope,
                source: ReviewStandardSource::Manual,
                file_name: "standard.md".to_owned(),
                format: ArtifactFormat::Markdown,
                content_hash: sha256_hex(normalized.as_bytes()),
                text_hash: sha256_hex(normalized.as_bytes()),
                character_count: normalized.chars().count(),
                storage_relative_path,
            },
            Utc::now(),
        );
        finish_standard_write(&project, standard_id, revision_id, result)
    }

    pub fn set_standard_enabled(
        &self,
        project_home: PathBuf,
        standard_id: ReviewStandardId,
        enabled: bool,
    ) -> Result<ReviewWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        project.database.set_review_standard_enabled(
            &project.project.id,
            &standard_id,
            enabled,
            Utc::now(),
        )?;
        build_review_workspace(&project)
    }

    pub fn reorder_standards(
        &self,
        project_home: PathBuf,
        ordered_ids: Vec<ReviewStandardId>,
    ) -> Result<ReviewWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        project
            .database
            .reorder_review_standards(&project.project.id, &ordered_ids, Utc::now())?;
        build_review_workspace(&project)
    }

    pub fn delete_standard(
        &self,
        project_home: PathBuf,
        standard_id: ReviewStandardId,
    ) -> Result<ReviewWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        project.database.soft_delete_review_standard(
            &project.project.id,
            &standard_id,
            Utc::now(),
        )?;
        build_review_workspace(&project)
    }

    pub fn read_standard(
        &self,
        project_home: PathBuf,
        standard_id: ReviewStandardId,
    ) -> Result<ReviewStandardContent, AppError> {
        let project = OpenProject::open(project_home)?;
        let standard = project
            .database
            .list_review_standards(&project.project.id)?
            .into_iter()
            .find(|standard| standard.id == standard_id)
            .ok_or_else(|| AppError::ReviewStandardInvalid("规范不存在或已删除".to_owned()))?;
        let text = read_standard_revision_text(&project, &standard, &standard.current_revision.id)?;
        Ok(ReviewStandardContent {
            revision_id: standard.current_revision.id,
            standard: standard_view(standard),
            text,
        })
    }
}

pub(crate) fn validate_review_standards_for_claim(project: &OpenProject) -> Result<(), AppError> {
    let standards = project
        .database
        .list_review_standards(&project.project.id)?
        .into_iter()
        .filter(|standard| standard.enabled && standard.deleted_at.is_none())
        .collect::<Vec<_>>();
    if standards.len() > MAX_REVIEW_STANDARDS {
        return Err(AppError::ReviewStandardInvalid(format!(
            "启用的规范不能超过 {MAX_REVIEW_STANDARDS} 个"
        )));
    }
    let total_characters = standards
        .iter()
        .map(|standard| standard.current_revision.character_count)
        .sum::<usize>();
    if total_characters > MAX_REVIEW_STANDARD_CHARS {
        return Err(AppError::ReviewInputTooLarge {
            actual: total_characters,
            limit: MAX_REVIEW_STANDARD_CHARS,
        });
    }
    for standard in &standards {
        read_standard_revision_text(project, standard, &standard.current_revision.id)?;
    }
    Ok(())
}

pub(crate) fn read_review_snapshot_standard(
    project: &OpenProject,
    standard: &ReviewStandardSnapshot,
) -> Result<String, AppError> {
    let revision = project
        .database
        .review_standard_revision(&project.project.id, &standard.revision_id)?
        .ok_or_else(|| AppError::ReviewStandardInvalid("规范修订不存在".to_owned()))?;
    read_revision_text(project, &revision)
}

fn build_review_workspace(project: &OpenProject) -> Result<ReviewWorkspace, AppError> {
    let configuration = project.database.review_configuration(&project.project.id)?;
    let standards = project
        .database
        .list_review_standards(&project.project.id)?
        .into_iter()
        .map(standard_view)
        .collect();
    let pull_requests = project
        .database
        .list_pull_requests(&project.project.id)?
        .into_iter()
        .map(|record| (record.report_id, record))
        .collect::<BTreeMap<_, _>>();
    let mut recent_reviews = Vec::new();
    for report in project
        .database
        .list_work_reports(&project.project.id)?
        .into_iter()
        .filter(|report| {
            report.review_status != ReviewStatus::NotRequired
                || (report.accepted
                    && report.report.status == coldstart_core::ReportOutcome::Completed)
        })
        .take(MAX_RECENT_REVIEWS)
    {
        let task_key = project
            .database
            .load_work_item(&report.work_item_id)?
            .map(|item| item.task_key)
            .unwrap_or_else(|| report.work_item_id.to_string());
        let pull_request = pull_requests.get(&report.id).cloned();
        let pull_request_insights = match &pull_request {
            Some(record) => project
                .database
                .load_pull_request_insights(&project.project.id, &record.id)?,
            None => None,
        };
        recent_reviews.push(ReviewHistoryView {
            report_id: report.id,
            work_item_id: report.work_item_id,
            task_key,
            review_status: report.review_status,
            review: report.review,
            waiver: report.review_waiver,
            pull_request,
            pull_request_insights,
            created_at: report.created_at,
        });
    }
    Ok(ReviewWorkspace {
        project_id: project.project.id,
        configuration,
        standards,
        recent_reviews,
    })
}

fn standard_view(standard: ReviewStandard) -> ReviewStandardView {
    ReviewStandardView {
        id: standard.id,
        name: standard.name,
        scope: standard.scope,
        enabled: standard.enabled,
        position: standard.position,
        current_revision_id: standard.current_revision.id,
        revision_number: standard.current_revision.revision_number,
        source: standard.current_revision.source,
        file_name: standard.current_revision.file_name,
        format: standard.current_revision.format,
        character_count: standard.current_revision.character_count,
        content_hash: standard.current_revision.content_hash,
        created_at: standard.current_revision.created_at,
        updated_at: standard.updated_at,
    }
}

fn validate_standard_name(name: &str) -> Result<(), AppError> {
    if name.trim().is_empty() {
        return Err(AppError::ReviewStandardInvalid(
            "规范名称不能为空".to_owned(),
        ));
    }
    if name.chars().count() > 120 {
        return Err(AppError::ReviewStandardInvalid(
            "规范名称不能超过 120 个字符".to_owned(),
        ));
    }
    Ok(())
}

fn validate_standard_text(text: &str) -> Result<(), AppError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(AppError::ReviewStandardInvalid(
            "规范正文不能为空".to_owned(),
        ));
    }
    let actual = trimmed.chars().count();
    if actual > MAX_REVIEW_STANDARD_CHARS {
        return Err(AppError::ReviewInputTooLarge {
            actual,
            limit: MAX_REVIEW_STANDARD_CHARS,
        });
    }
    Ok(())
}

fn map_document_error(error: DocumentError) -> AppError {
    match error {
        DocumentError::TooLarge { limit } => AppError::ReviewInputTooLarge {
            actual: usize::try_from(limit).unwrap_or(usize::MAX),
            limit: MAX_REVIEW_STANDARD_CHARS,
        },
        DocumentError::TextTooLarge { limit } => AppError::ReviewInputTooLarge {
            actual: limit,
            limit: MAX_REVIEW_STANDARD_CHARS,
        },
        other => AppError::ReviewStandardInvalid(other.to_string()),
    }
}

fn persist_standard_files(
    project: &OpenProject,
    standard_id: ReviewStandardId,
    revision_id: ReviewStandardRevisionId,
    original: Option<(&str, ArtifactFormat, &[u8])>,
    text: &str,
) -> Result<String, AppError> {
    let standards_root = project
        .project
        .root_path
        .join(".coldstart")
        .join(REVIEW_STANDARDS_DIRECTORY);
    fs::create_dir_all(&standards_root)?;
    let temporary = standards_root.join(format!(".tmp-{}", Uuid::new_v4().simple()));
    let final_directory = standards_root
        .join(standard_id.to_string())
        .join(revision_id.to_string());
    let result = (|| {
        fs::create_dir_all(&temporary)?;
        if let Some((file_name, _, bytes)) = original {
            fs::write(temporary.join(file_name), bytes)?;
        } else {
            fs::write(temporary.join("standard.md"), text.as_bytes())?;
        }
        fs::write(temporary.join("extracted.txt"), text.as_bytes())?;
        if final_directory.exists() {
            return Err(AppError::ReviewStandardInvalid(
                "规范修订目录已经存在".to_owned(),
            ));
        }
        fs::create_dir_all(
            final_directory
                .parent()
                .expect("review standard revision directory has a parent"),
        )?;
        fs::rename(&temporary, &final_directory)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&temporary);
        return Err(error);
    }
    Ok(format!(
        "{REVIEW_STANDARDS_DIRECTORY}/{standard_id}/{revision_id}/extracted.txt"
    ))
}

fn finish_standard_write(
    project: &OpenProject,
    standard_id: ReviewStandardId,
    revision_id: ReviewStandardRevisionId,
    result: Result<ReviewStandard, coldstart_storage::StorageError>,
) -> Result<ReviewStandardView, AppError> {
    match result {
        Ok(standard) => Ok(standard_view(standard)),
        Err(error) => {
            let revision_directory = project
                .project
                .root_path
                .join(".coldstart")
                .join(REVIEW_STANDARDS_DIRECTORY)
                .join(standard_id.to_string())
                .join(revision_id.to_string());
            let _ = fs::remove_dir_all(revision_directory);
            Err(error.into())
        }
    }
}

fn read_standard_revision_text(
    project: &OpenProject,
    standard: &ReviewStandard,
    revision_id: &ReviewStandardRevisionId,
) -> Result<String, AppError> {
    let revision = if standard.current_revision.id == *revision_id {
        standard.current_revision.clone()
    } else {
        project
            .database
            .review_standard_revision(&project.project.id, revision_id)?
            .ok_or_else(|| AppError::ReviewStandardInvalid("规范修订不存在".to_owned()))?
    };
    read_revision_text(project, &revision)
}

fn read_revision_text(
    project: &OpenProject,
    revision: &coldstart_core::ReviewStandardRevision,
) -> Result<String, AppError> {
    let path = safe_project_path(
        &project.project.root_path.join(".coldstart"),
        &revision.storage_relative_path,
    )?;
    let text = fs::read_to_string(path)
        .map_err(|_| AppError::ReviewStandardInvalid("规范正文文件缺失或无法读取".to_owned()))?;
    let actual_hash = sha256_hex(text.as_bytes());
    if actual_hash != revision.text_hash || text.chars().count() != revision.character_count {
        return Err(AppError::ReviewStandardInvalid(
            "规范正文哈希或字符数校验失败".to_owned(),
        ));
    }
    Ok(text)
}

fn safe_project_path(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AppError::ReviewStandardInvalid(
            "规范存储路径不安全".to_owned(),
        ));
    }
    Ok(root.join(relative_path))
}
