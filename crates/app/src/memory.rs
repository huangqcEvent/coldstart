//! 项目记忆投影与分页。
//!
//! 把追加式事件转换为有阶段、可搜索、可追溯的时间线，并限制分页大小和文本长度；
//! 投影会隐藏心跳详情、租约令牌和敏感载荷。

use std::{collections::BTreeMap, path::PathBuf};

use chrono::{DateTime, Utc};
use coldstart_core::{DomainEventKind, GenerationSource, ProjectId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppError, context::OpenProject};

pub const DEFAULT_MEMORY_PAGE_LIMIT: u32 = 100;
pub const MAX_MEMORY_PAGE_LIMIT: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPhase {
    RequirementsBlueprint,
    PlanExecution,
    VerificationReview,
    RepositoryCapability,
    SystemConfiguration,
}

impl MemoryPhase {
    pub const ALL: [Self; 5] = [
        Self::RequirementsBlueprint,
        Self::PlanExecution,
        Self::VerificationReview,
        Self::RepositoryCapability,
        Self::SystemConfiguration,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryReferenceType {
    Artifact,
    BlueprintRevision,
    InfoRequest,
    PlanRevision,
    WorkItem,
    WorkReport,
    Repository,
    Credential,
    Capability,
    GitSync,
    PullRequest,
    ProjectSetting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMemoryReference {
    pub reference_type: MemoryReferenceType,
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMemoryItem {
    pub event_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub phase: MemoryPhase,
    pub event_type: String,
    pub title: String,
    pub summary: String,
    pub reference: Option<ProjectMemoryReference>,
    pub is_heartbeat: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPhaseCount {
    pub phase: MemoryPhase,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMemoryPage {
    pub project_id: ProjectId,
    pub total_count: u64,
    pub visible_count: u64,
    pub heartbeat_count: u64,
    pub phase_counts: Vec<MemoryPhaseCount>,
    pub items: Vec<ProjectMemoryItem>,
    pub next_cursor: Option<i64>,
}

#[derive(Debug, Default)]
pub struct ProjectMemoryService;

impl ProjectMemoryService {
    pub fn new() -> Self {
        Self
    }

    pub fn read(
        &self,
        project_home: PathBuf,
        before_cursor: Option<i64>,
        limit: Option<u32>,
        include_heartbeats: bool,
    ) -> Result<ProjectMemoryPage, AppError> {
        let project = OpenProject::open(project_home)?;
        read_memory_page(&project, before_cursor, limit, include_heartbeats)
    }
}

pub(crate) fn read_memory_page(
    project: &OpenProject,
    before_cursor: Option<i64>,
    limit: Option<u32>,
    include_heartbeats: bool,
) -> Result<ProjectMemoryPage, AppError> {
    let limit = limit.unwrap_or(DEFAULT_MEMORY_PAGE_LIMIT);
    if !(1..=MAX_MEMORY_PAGE_LIMIT).contains(&limit) {
        return Err(AppError::InvalidMemoryLimit {
            limit,
            min: 1,
            max: MAX_MEMORY_PAGE_LIMIT,
        });
    }
    if before_cursor.is_some_and(|cursor| cursor <= 0) {
        return Err(AppError::InvalidMemoryCursor(before_cursor.unwrap()));
    }

    let total_count = project.database.event_count(&project.project.id)?;
    let type_counts = project.database.event_type_counts(&project.project.id)?;
    let heartbeat_count = type_counts
        .iter()
        .find(|entry| entry.event_type == "work_heartbeat")
        .map(|entry| entry.count)
        .unwrap_or_default();
    let visible_count = total_count.saturating_sub(heartbeat_count);
    let mut phase_totals = MemoryPhase::ALL
        .into_iter()
        .map(|phase| (phase, 0_u64))
        .collect::<BTreeMap<_, _>>();
    for entry in &type_counts {
        if entry.event_type == "work_heartbeat" {
            continue;
        }
        *phase_totals
            .entry(phase_for_event_type(&entry.event_type))
            .or_default() += entry.count;
    }
    let phase_counts = MemoryPhase::ALL
        .into_iter()
        .map(|phase| MemoryPhaseCount {
            phase,
            count: phase_totals.get(&phase).copied().unwrap_or_default(),
        })
        .collect();

    let page = project.database.list_events(
        &project.project.id,
        before_cursor,
        limit,
        include_heartbeats,
    )?;
    let items = page
        .events
        .into_iter()
        .map(|stored| {
            let event_type = stored.event.kind.type_name().to_owned();
            let description = describe_event(&stored.event.kind);
            ProjectMemoryItem {
                event_id: stored.event.event_id,
                occurred_at: stored.event.occurred_at,
                phase: description.phase,
                event_type,
                title: description.title.to_owned(),
                summary: description.summary,
                reference: description.reference,
                is_heartbeat: description.is_heartbeat,
            }
        })
        .collect();

    Ok(ProjectMemoryPage {
        project_id: project.project.id,
        total_count,
        visible_count,
        heartbeat_count,
        phase_counts,
        items,
        next_cursor: page.next_cursor,
    })
}

struct EventDescription {
    phase: MemoryPhase,
    title: &'static str,
    summary: String,
    reference: Option<ProjectMemoryReference>,
    is_heartbeat: bool,
}

fn describe_event(kind: &DomainEventKind) -> EventDescription {
    match kind {
        DomainEventKind::ProjectInitialized { name } => EventDescription {
            phase: MemoryPhase::RequirementsBlueprint,
            title: "项目已初始化",
            summary: format!("创建项目「{}」，本地控制面开始记录", excerpt(name)),
            reference: None,
            is_heartbeat: false,
        },
        DomainEventKind::ArtifactImported {
            artifact_id,
            file_name,
            content_hash,
        } => EventDescription {
            phase: MemoryPhase::RequirementsBlueprint,
            title: "需求文档已导入",
            summary: format!(
                "{} · 内容指纹 {}",
                excerpt(file_name),
                short_hash(content_hash)
            ),
            reference: Some(reference(
                MemoryReferenceType::Artifact,
                artifact_id,
                file_name,
            )),
            is_heartbeat: false,
        },
        DomainEventKind::BlueprintDraftCreated {
            revision_id,
            revision_number,
            generation_source,
        } => EventDescription {
            phase: MemoryPhase::RequirementsBlueprint,
            title: "蓝图草稿已创建",
            summary: format!(
                "v{} · {}",
                revision_number,
                generation_source_label(generation_source)
            ),
            reference: Some(reference(
                MemoryReferenceType::BlueprintRevision,
                revision_id,
                format!("蓝图 v{revision_number}"),
            )),
            is_heartbeat: false,
        },
        DomainEventKind::BlueprintPublished {
            revision_id,
            revision_number,
        } => EventDescription {
            phase: MemoryPhase::RequirementsBlueprint,
            title: "蓝图已发布",
            summary: format!("v{revision_number} 成为当前有效的项目基线"),
            reference: Some(reference(
                MemoryReferenceType::BlueprintRevision,
                revision_id,
                format!("蓝图 v{revision_number}"),
            )),
            is_heartbeat: false,
        },
        DomainEventKind::InfoRequestCreated {
            request_id,
            question,
        } => EventDescription {
            phase: MemoryPhase::RequirementsBlueprint,
            title: "人工信息请求已创建",
            summary: excerpt(question),
            reference: Some(reference(
                MemoryReferenceType::InfoRequest,
                request_id,
                "人工信息请求",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::InfoRequestResolved {
            request_id,
            fact_id,
            version,
        } => EventDescription {
            phase: MemoryPhase::RequirementsBlueprint,
            title: "人工信息已确认",
            summary: format!("回答已写入项目事实 v{version}（事实 {fact_id}）"),
            reference: Some(reference(
                MemoryReferenceType::InfoRequest,
                request_id,
                "已回答的信息请求",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::PlanDraftCreated {
            revision_id,
            revision_number,
            task_count,
        } => EventDescription {
            phase: MemoryPhase::PlanExecution,
            title: "计划草稿已创建",
            summary: format!("v{revision_number} · {task_count} 个任务"),
            reference: Some(reference(
                MemoryReferenceType::PlanRevision,
                revision_id,
                format!("计划 v{revision_number}"),
            )),
            is_heartbeat: false,
        },
        DomainEventKind::PlanPublished {
            revision_id,
            revision_number,
            task_count,
        } => EventDescription {
            phase: MemoryPhase::PlanExecution,
            title: "开发计划已发布",
            summary: format!("v{revision_number} · {task_count} 个任务进入可领取状态"),
            reference: Some(reference(
                MemoryReferenceType::PlanRevision,
                revision_id,
                format!("计划 v{revision_number}"),
            )),
            is_heartbeat: false,
        },
        DomainEventKind::PlanSuperseded {
            revision_id,
            superseded_by,
        } => EventDescription {
            phase: MemoryPhase::PlanExecution,
            title: "旧计划已被替换",
            summary: format!("计划 {revision_id} 由新计划接替"),
            reference: Some(reference(
                MemoryReferenceType::PlanRevision,
                superseded_by,
                "接替后的计划",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::WorkClaimed {
            work_item_id,
            task_key,
            agent_profile,
            ..
        } => EventDescription {
            phase: MemoryPhase::PlanExecution,
            title: "Codex 已领取任务",
            summary: format!("{task_key} · 执行者 {agent_profile}"),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                task_key,
            )),
            is_heartbeat: false,
        },
        DomainEventKind::WorkStarted { work_item_id } => EventDescription {
            phase: MemoryPhase::PlanExecution,
            title: "任务执行已开始",
            summary: "执行者已进入任务工作区".to_owned(),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "执行中的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::WorkHeartbeat {
            work_item_id,
            expires_at,
            ..
        } => EventDescription {
            phase: MemoryPhase::PlanExecution,
            title: "任务心跳已刷新",
            summary: format!("执行者仍在工作，租约有效至 {}", expires_at.to_rfc3339()),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "有心跳的任务",
            )),
            is_heartbeat: true,
        },
        DomainEventKind::WorkBlocked {
            work_item_id,
            reason,
            ..
        } => EventDescription {
            phase: MemoryPhase::PlanExecution,
            title: "任务被阻塞",
            summary: excerpt(reason),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "阻塞的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::WorkReportSubmitted {
            work_item_id,
            report_id: _,
            accepted,
        } => EventDescription {
            phase: MemoryPhase::PlanExecution,
            title: "工作汇报已提交",
            summary: if *accepted {
                "报告字段与提交边界已通过".to_owned()
            } else {
                "报告未通过字段或提交边界校验".to_owned()
            },
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "提交报告的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::WorkCompleted {
            work_item_id,
            task_key,
        } => EventDescription {
            phase: MemoryPhase::PlanExecution,
            title: "任务已完成",
            summary: format!("{task_key} 已通过完成门禁"),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                task_key,
            )),
            is_heartbeat: false,
        },
        DomainEventKind::VerificationStarted {
            work_item_id,
            report_id,
            ..
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "本地验证已开始",
            summary: format!("针对工作汇报 {report_id} 执行仓库验证命令"),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "验证中的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::VerificationPassed {
            work_item_id,
            report_id,
            ..
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "本地验证已通过",
            summary: format!("工作汇报 {report_id} 的验证命令全部通过"),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "验证通过的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::VerificationFailed {
            work_item_id,
            report_id: _,
            command_name,
            failure_kind,
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "本地验证未通过",
            summary: match command_name {
                Some(command) => format!("{command} 失败 · {failure_kind}"),
                None => format!("验证失败 · {failure_kind}"),
            },
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "验证失败的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::VerificationInterrupted {
            work_item_id,
            report_id,
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "本地验证被中断",
            summary: format!("工作汇报 {report_id} 未完成验证"),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "验证中断的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::ReviewStarted {
            work_item_id,
            report_id,
            ..
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "AI Review 已开始",
            summary: format!("正在核对工作汇报 {report_id} 的真实 diff"),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "评审中的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::ReviewPassed {
            work_item_id,
            report_id: _,
            finding_count,
            ..
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "AI Review 已通过",
            summary: format!("发现 {finding_count} 个非阻断问题，任务可以继续完成"),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "Review 通过的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::ReviewBlocked {
            work_item_id,
            report_id: _,
            blocking_finding_count,
            ..
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "AI Review 阻断了完成",
            summary: format!("{blocking_finding_count} 个达到阻断级别的问题"),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "Review 阻塞的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::ReviewFailed {
            work_item_id,
            report_id: _,
            failure_code,
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "AI Review 无法完成",
            summary: format!("失败代码：{failure_code}"),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "Review 失败的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::ReviewInterrupted {
            work_item_id,
            report_id: _,
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "AI Review 被中断",
            summary: "评审未返回最终结论，任务不能据此完成".to_owned(),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "Review 中断的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::ReviewWaived {
            work_item_id,
            report_id: _,
            waived_by,
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "AI Review 已人工豁免",
            summary: format!("{waived_by} 已接受风险并豁免本次阻断"),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "Review 豁免的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::WorkReleased {
            work_item_id,
            reason,
            ..
        } => EventDescription {
            phase: MemoryPhase::PlanExecution,
            title: "任务租约已释放",
            summary: excerpt(reason),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "已释放的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::LeaseExpired { work_item_id, .. } => EventDescription {
            phase: MemoryPhase::PlanExecution,
            title: "任务租约已过期",
            summary: "执行者未在有效期内续约，任务回到可重新领取状态".to_owned(),
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "租约过期的任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::HumanWorkCompleted {
            work_item_id,
            accepted,
        } => EventDescription {
            phase: MemoryPhase::PlanExecution,
            title: "人工任务已处理",
            summary: if *accepted {
                "人工确认完成".to_owned()
            } else {
                "人工标记为阻塞".to_owned()
            },
            reference: Some(reference(
                MemoryReferenceType::WorkItem,
                work_item_id,
                "人工任务",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::RepositorySaved {
            repository_id,
            repository_key,
            ..
        } => EventDescription {
            phase: MemoryPhase::RepositoryCapability,
            title: "仓库配置已保存",
            summary: format!("仓库 {repository_key} 的本地映射已更新，远程地址不在时间线展示"),
            reference: Some(reference(
                MemoryReferenceType::Repository,
                repository_id,
                repository_key,
            )),
            is_heartbeat: false,
        },
        DomainEventKind::RepositoryVerificationCommandsUpdated {
            repository_id,
            repository_key,
            command_count,
        } => EventDescription {
            phase: MemoryPhase::RepositoryCapability,
            title: "仓库验证命令已更新",
            summary: format!("{repository_key} · {command_count} 条命令"),
            reference: Some(reference(
                MemoryReferenceType::Repository,
                repository_id,
                repository_key,
            )),
            is_heartbeat: false,
        },
        DomainEventKind::CredentialSaved {
            credential_id,
            provider,
            environment,
        } => EventDescription {
            phase: MemoryPhase::RepositoryCapability,
            title: "三方凭据已保存",
            summary: format!("{provider} · {environment} · 密钥内容不在时间线展示"),
            reference: Some(reference(
                MemoryReferenceType::Credential,
                credential_id,
                provider,
            )),
            is_heartbeat: false,
        },
        DomainEventKind::CredentialDeleted { credential_id } => EventDescription {
            phase: MemoryPhase::RepositoryCapability,
            title: "三方凭据已删除",
            summary: "凭据元数据已移除".to_owned(),
            reference: Some(reference(
                MemoryReferenceType::Credential,
                credential_id,
                "已删除的凭据",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::CapabilityUpdated {
            capability_id,
            environment,
            status,
        } => EventDescription {
            phase: MemoryPhase::RepositoryCapability,
            title: "三方能力状态已更新",
            summary: format!("{capability_id} · {environment} · {status}"),
            reference: Some(ProjectMemoryReference {
                reference_type: MemoryReferenceType::Capability,
                id: capability_id.clone(),
                label: capability_id.clone(),
            }),
            is_heartbeat: false,
        },
        DomainEventKind::GitSyncRecorded {
            sync_id, status, ..
        } => EventDescription {
            phase: MemoryPhase::RepositoryCapability,
            title: "Git 同步已记录",
            summary: format!("同步状态：{status}"),
            reference: Some(reference(
                MemoryReferenceType::GitSync,
                sync_id,
                "Git 同步记录",
            )),
            is_heartbeat: false,
        },
        DomainEventKind::PullRequestOpened {
            pull_request_id,
            number,
            state,
            ..
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "GitHub 草稿 PR 已创建",
            summary: format!("PR #{number} · {state} · 未自动合并"),
            reference: Some(reference(
                MemoryReferenceType::PullRequest,
                pull_request_id,
                format!("PR #{number}"),
            )),
            is_heartbeat: false,
        },
        DomainEventKind::PullRequestStateChanged {
            pull_request_id,
            number,
            previous_state,
            state,
            ..
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "GitHub PR 状态已更新",
            summary: format!("PR #{number} · {previous_state} -> {state}"),
            reference: Some(reference(
                MemoryReferenceType::PullRequest,
                pull_request_id,
                format!("PR #{number}"),
            )),
            is_heartbeat: false,
        },
        DomainEventKind::PullRequestInsightsSynced {
            pull_request_id,
            number,
            checks_state,
            checks_total,
            checks_failed,
            review_count,
            comment_count,
            ..
        } => EventDescription {
            phase: MemoryPhase::VerificationReview,
            title: "GitHub PR 验收信息已同步",
            summary: format!(
                "PR #{number} · Checks {checks_state} {checks_total} 项（失败 {checks_failed}）· 评审 {review_count} 条 · 评论 {comment_count} 条"
            ),
            reference: Some(reference(
                MemoryReferenceType::PullRequest,
                pull_request_id,
                format!("PR #{number}"),
            )),
            is_heartbeat: false,
        },
        DomainEventKind::ProjectSettingUpdated { setting_key } => EventDescription {
            phase: MemoryPhase::SystemConfiguration,
            title: "项目设置已更新",
            summary: format!("设置项：{setting_key}"),
            reference: Some(ProjectMemoryReference {
                reference_type: MemoryReferenceType::ProjectSetting,
                id: setting_key.clone(),
                label: setting_key.clone(),
            }),
            is_heartbeat: false,
        },
    }
}

fn phase_for_event_type(event_type: &str) -> MemoryPhase {
    match event_type {
        "project_initialized"
        | "artifact_imported"
        | "blueprint_draft_created"
        | "blueprint_published"
        | "info_request_created"
        | "info_request_resolved" => MemoryPhase::RequirementsBlueprint,
        "plan_draft_created"
        | "plan_published"
        | "plan_superseded"
        | "work_claimed"
        | "work_started"
        | "work_heartbeat"
        | "work_blocked"
        | "work_report_submitted"
        | "work_completed"
        | "work_released"
        | "lease_expired"
        | "human_work_completed" => MemoryPhase::PlanExecution,
        "verification_started"
        | "verification_passed"
        | "verification_failed"
        | "verification_interrupted"
        | "review_started"
        | "review_passed"
        | "review_blocked"
        | "review_failed"
        | "review_interrupted"
        | "review_waived"
        | "pull_request_opened"
        | "pull_request_state_changed"
        | "pull_request_insights_synced" => MemoryPhase::VerificationReview,
        "repository_saved"
        | "repository_verification_commands_updated"
        | "credential_saved"
        | "credential_deleted"
        | "capability_updated"
        | "git_sync_recorded" => MemoryPhase::RepositoryCapability,
        "project_setting_updated" => MemoryPhase::SystemConfiguration,
        _ => MemoryPhase::SystemConfiguration,
    }
}

fn reference(
    reference_type: MemoryReferenceType,
    id: impl ToString,
    label: impl Into<String>,
) -> ProjectMemoryReference {
    ProjectMemoryReference {
        reference_type,
        id: id.to_string(),
        label: label.into(),
    }
}

fn generation_source_label(source: &GenerationSource) -> &'static str {
    match source {
        GenerationSource::Model => "模型生成",
        GenerationSource::Manual => "人工保存",
    }
}

fn short_hash(value: &str) -> String {
    value.chars().take(12).collect()
}

fn excerpt(value: &str) -> String {
    const MAX_CHARACTERS: usize = 160;
    let value = value.trim();
    if value.chars().count() <= MAX_CHARACTERS {
        return value.to_owned();
    }
    let mut excerpt = value.chars().take(MAX_CHARACTERS - 1).collect::<String>();
    excerpt.push('…');
    excerpt
}
