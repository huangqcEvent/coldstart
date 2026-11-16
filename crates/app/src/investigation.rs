//! 历史项目只读代码调查编排。
//!
//! 桌面端创建调查请求，Codex 通过 MCP 主动领取并提交结构化证据；人工确认后，
//! 证据才会作为蓝图模型的独立参考输入。该服务不读取源码、不修改仓库、不启动 Agent。

use std::path::PathBuf;

use chrono::Utc;
use coldstart_adapters::sanitize_verification_output;
use coldstart_core::{
    CodeInvestigation, CodeInvestigationClaim, CodeInvestigationId, CodeInvestigationReport,
    validate_investigation_base_ref, validate_investigation_question,
};

use crate::{AppError, ProjectWorkspace, context::OpenProject, requirements::build_workspace};

pub struct CodeInvestigationService;

impl CodeInvestigationService {
    pub fn new() -> Self {
        Self
    }

    pub fn create(
        &self,
        project_home: PathBuf,
        repository_key: String,
        base_ref: Option<String>,
        question: String,
    ) -> Result<ProjectWorkspace, AppError> {
        validate_investigation_question(&question)
            .map_err(|error| AppError::InvalidCodeInvestigation(error.to_string()))?;
        let mut project = OpenProject::open(project_home)?;
        let repository = project
            .database
            .list_repositories(&project.project.id)?
            .into_iter()
            .find(|repository| repository.key == repository_key)
            .ok_or_else(|| AppError::RepositoryNotFound(repository_key.clone()))?;
        let base_ref = base_ref
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| repository.default_branch.clone());
        validate_investigation_base_ref(&base_ref)
            .map_err(|error| AppError::InvalidCodeInvestigation(error.to_string()))?;
        project.database.create_code_investigation(
            &project.project.id,
            &repository.key,
            &base_ref,
            question.trim(),
            Utc::now(),
        )?;
        build_workspace(project)
    }

    pub fn confirm(
        &self,
        project_home: PathBuf,
        investigation_id: CodeInvestigationId,
    ) -> Result<ProjectWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        project.database.confirm_code_investigation(
            &project.project.id,
            investigation_id,
            Utc::now(),
        )?;
        build_workspace(project)
    }

    pub fn reject(
        &self,
        project_home: PathBuf,
        investigation_id: CodeInvestigationId,
        reason: String,
    ) -> Result<ProjectWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        project.database.reject_code_investigation(
            &project.project.id,
            investigation_id,
            &reason,
        )?;
        build_workspace(project)
    }

    /// MCP 主动领取；返回一次性领取令牌和最小调查上下文，不暴露本地路径。
    pub fn claim_next(
        &self,
        project_home: PathBuf,
        agent_profile: String,
        repository_key: Option<String>,
    ) -> Result<Option<CodeInvestigationClaim>, AppError> {
        if agent_profile.trim().is_empty() {
            return Err(AppError::InvalidCodeInvestigation(
                "Agent profile cannot be empty".to_owned(),
            ));
        }
        let mut project = OpenProject::open(project_home)?;
        Ok(project.database.claim_next_code_investigation(
            &project.project.id,
            repository_key.as_deref(),
            agent_profile.trim(),
            Utc::now(),
        )?)
    }

    pub fn submit(
        &self,
        project_home: PathBuf,
        investigation_id: CodeInvestigationId,
        claim_token: String,
        mut report: CodeInvestigationReport,
    ) -> Result<CodeInvestigation, AppError> {
        report.commit_sha = report.commit_sha.trim().to_ascii_lowercase();
        report.summary = report.summary.trim().to_owned();
        report.idempotency_key = report.idempotency_key.trim().to_owned();
        for finding in &mut report.findings {
            finding.path = finding.path.trim().replace('\\', "/");
            finding.summary = finding.summary.trim().to_owned();
            finding.risk = finding.risk.trim().to_owned();
            finding.symbol = finding
                .symbol
                .take()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty());
            for path in &mut finding.suggested_change_paths {
                *path = path.trim().replace('\\', "/");
            }
        }
        if report_contains_sensitive_value(&report) {
            return Err(AppError::CodeInvestigationContainsSecret);
        }
        report
            .validate()
            .map_err(|error| AppError::InvalidCodeInvestigation(error.to_string()))?;
        let mut project = OpenProject::open(project_home)?;
        Ok(project.database.submit_code_investigation(
            &project.project.id,
            investigation_id,
            &claim_token,
            report,
            Utc::now(),
        )?)
    }
}

impl Default for CodeInvestigationService {
    fn default() -> Self {
        Self::new()
    }
}

fn report_contains_sensitive_value(report: &CodeInvestigationReport) -> bool {
    let values = std::iter::once(report.summary.as_str())
        .chain(report.limitations.iter().map(String::as_str))
        .chain(report.findings.iter().flat_map(|finding| {
            std::iter::once(finding.summary.as_str())
                .chain(std::iter::once(finding.risk.as_str()))
                .chain(finding.symbol.as_deref())
                .chain(finding.suggested_change_paths.iter().map(String::as_str))
                .chain(std::iter::once(finding.path.as_str()))
        }));
    values
        .into_iter()
        .any(|value| sanitize_verification_output(value) != value)
}
