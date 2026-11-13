use chrono::Utc;
use coldstart_core::{
    BlueprintContent, BlueprintEntityId, CapabilityApiEndpoint, ContextOmission, ContextReference,
    ContextReferenceKind, InfoRequestStatus, PlanRevision, ProjectFact, ReviewContextView,
    ReviewScope, ReviewSnapshot, TaskContext, TaskReferenceContent, TaskSpec, VerificationSnapshot,
    WorkItem, estimate_tokens, sha256_hex,
};

use crate::{
    AppError,
    context::OpenProject,
    integration::{CapabilityStatusView, load_capabilities_for_context},
    review::read_review_snapshot_standard,
};

#[derive(Debug)]
struct Section {
    priority: u8,
    id: String,
    kind: ContextReferenceKind,
    title: String,
    content: String,
}

pub(crate) fn compile_task_context(
    project: &OpenProject,
    plan: &PlanRevision,
    task: &TaskSpec,
    work_items: &[WorkItem],
    verification: Option<VerificationSnapshot>,
    review: Option<&ReviewSnapshot>,
) -> Result<TaskContext, AppError> {
    let blueprint = project
        .database
        .load_blueprint(&plan.blueprint_revision_id)?
        .ok_or(AppError::BlueprintNotFound(plan.blueprint_revision_id))?;
    let repository = repository_context(project, task)?;
    let capabilities = load_capabilities_for_context(project)?;
    let capability_api_endpoints = project
        .database
        .list_capability_api_endpoints(&project.project.id)?;
    let facts = project.database.list_project_facts(&project.project.id)?;
    let info_requests = project.database.list_info_requests(&project.project.id)?;
    let mut header = render_header(task);
    if estimate_tokens(&header) > task.context_budget_tokens {
        header = truncate_to_tokens(&header, task.context_budget_tokens);
    }
    let mut markdown = header;
    let mut included = vec![ContextReference {
        id: task.task_key.clone(),
        kind: ContextReferenceKind::TaskContract,
        title: task.title.clone(),
    }];
    let mut omitted = Vec::new();

    for section in build_sections(ContextSectionInputs {
        blueprint: &blueprint.content,
        task,
        work_items,
        repository,
        verification: verification.as_ref(),
        review,
        capabilities: &capabilities,
        capability_api_endpoints: &capability_api_endpoints,
        facts: &facts,
        info_requests: &info_requests,
    }) {
        let rendered = format!(
            "\n\n## {}\n\n{}\n",
            section.title,
            section.content.trim_end()
        );
        if estimate_tokens(&markdown) + estimate_tokens(&rendered) <= task.context_budget_tokens {
            markdown.push_str(&rendered);
            included.push(ContextReference {
                id: section.id,
                kind: section.kind,
                title: section.title,
            });
        } else {
            omitted.push(ContextOmission {
                reference_id: section.id,
                reason: format!("上下文预算 {} tokens 已满", task.context_budget_tokens),
            });
        }
    }

    if !omitted.is_empty() {
        let ids = omitted
            .iter()
            .map(|item| item.reference_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let footer = format!("\n\n## 省略内容\n\n由于上下文预算限制，以下引用未展开：{ids}\n");
        if estimate_tokens(&markdown) + estimate_tokens(&footer) <= task.context_budget_tokens {
            markdown.push_str(&footer);
        }
    }

    Ok(TaskContext {
        task_key: task.task_key.clone(),
        contract_hash: task.contract_hash(),
        estimated_tokens: estimate_tokens(&markdown),
        budget_tokens: task.context_budget_tokens,
        markdown,
        included,
        omitted,
        verification,
        review: review.map(ReviewContextView::from_snapshot),
        generated_at: Utc::now(),
    })
}

pub(crate) fn compile_reference(
    project: &OpenProject,
    plan: &PlanRevision,
    task: &TaskSpec,
    reference_id: &str,
    work_items: &[WorkItem],
    review: Option<&ReviewSnapshot>,
) -> Result<Option<TaskReferenceContent>, AppError> {
    let blueprint = project
        .database
        .load_blueprint(&plan.blueprint_revision_id)?
        .ok_or(AppError::BlueprintNotFound(plan.blueprint_revision_id))?;
    if let Some(id) = task
        .context_refs
        .iter()
        .find(|id| id.as_str() == reference_id)
        && let Some((title, content)) = render_blueprint_entity(&blueprint.content, id)
    {
        return Ok(Some(TaskReferenceContent {
            reference_id: reference_id.to_owned(),
            title,
            content_hash: sha256_hex(content.as_bytes()),
            content,
        }));
    }
    if task
        .dependencies
        .iter()
        .any(|dependency| dependency.task_key == reference_id)
        && let Some(item) = work_items.iter().find(|item| item.task_key == reference_id)
    {
        let content = format!(
            "任务：{}\n状态：{:?}\n计划修订：{}",
            item.task_key, item.status, item.plan_revision_id
        );
        return Ok(Some(TaskReferenceContent {
            reference_id: reference_id.to_owned(),
            title: format!("依赖任务 {}", reference_id),
            content_hash: sha256_hex(content.as_bytes()),
            content,
        }));
    }
    if let Some(standard) = review.and_then(|snapshot| {
        snapshot
            .standards
            .iter()
            .find(|standard| format!("review-standard:{}", standard.standard_id) == reference_id)
    }) {
        let content = read_review_snapshot_standard(project, standard)?;
        return Ok(Some(TaskReferenceContent {
            reference_id: reference_id.to_owned(),
            title: format!("评审规范：{}", standard.name),
            content_hash: sha256_hex(content.as_bytes()),
            content,
        }));
    }
    Ok(None)
}

fn render_header(task: &TaskSpec) -> String {
    let acceptance = task
        .acceptance_criteria
        .iter()
        .map(|criterion| {
            format!(
                "- [{}] {}{}",
                criterion.id,
                criterion.text,
                if criterion.required_evidence {
                    "（必须提供证据）"
                } else {
                    ""
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let blockers = task
        .blocking_dependencies()
        .map(|dependency| format!("- {dependency}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# ColdStart 当前任务\n\n## 任务契约\n\n- taskKey：{}\n- 标题：{}\n- 类型：{:?}\n- 目标：{}\n- 仓库：{}\n- 基线：{}\n- 工作分支：{}\n- 允许修改路径：{}\n- 契约哈希：{}\n\n## 范围\n\n{}\n\n## 明确不做\n\n{}\n\n## 阻塞依赖\n\n{}\n\n## 验收标准\n\n{}\n\n## 证据要求\n\n{}\n",
        task.task_key,
        task.title,
        task.kind,
        task.goal,
        task.repository_key.as_deref().unwrap_or("人工任务"),
        task.base_ref.as_deref().unwrap_or("-"),
        task.work_branch.as_deref().unwrap_or("-"),
        task.allowed_paths.join(", "),
        task.contract_hash(),
        bullet_list(&task.scope),
        bullet_list(&task.out_of_scope),
        if blockers.is_empty() {
            "- 无".to_owned()
        } else {
            blockers
        },
        acceptance,
        bullet_list(&task.evidence_requirements),
    )
}

struct ContextSectionInputs<'a> {
    blueprint: &'a BlueprintContent,
    task: &'a TaskSpec,
    work_items: &'a [WorkItem],
    repository: Option<Section>,
    verification: Option<&'a VerificationSnapshot>,
    review: Option<&'a ReviewSnapshot>,
    capabilities: &'a [CapabilityStatusView],
    capability_api_endpoints: &'a [CapabilityApiEndpoint],
    facts: &'a [ProjectFact],
    info_requests: &'a [coldstart_core::InfoRequest],
}

fn build_sections(inputs: ContextSectionInputs<'_>) -> Vec<Section> {
    let ContextSectionInputs {
        blueprint,
        task,
        work_items,
        repository,
        verification,
        review,
        capabilities,
        capability_api_endpoints,
        facts,
        info_requests,
    } = inputs;
    let mut sections = Vec::new();
    if let Some(repository) = repository {
        sections.push(repository);
    }
    if let Some(verification) = verification {
        let commands = verification
            .commands
            .iter()
            .map(|command| {
                format!(
                    "- {}：{} {}（超时 {} 秒）",
                    command.name,
                    command.executable.to_string_lossy(),
                    command.args.join(" "),
                    command.timeout_seconds
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(Section {
            priority: 95,
            id: "repository-verification".to_owned(),
            kind: ContextReferenceKind::Instruction,
            title: "仓库验证门禁".to_owned(),
            content: format!(
                "提交报告时 ColdStart 会在仓库根目录按以下顺序独立重跑全部命令；必须全部成功，任务才会完成。\n\n快照哈希：{}\n\n{}\n\n命令通过绝对 .exe 路径直接启动，不经过 Shell；不能跳过或临时替换。",
                verification.content_hash(),
                commands
            ),
        });
    }
    if let Some(review) = review {
        for standard in &review.standards {
            let scope = match &standard.scope {
                ReviewScope::Project => "项目通用".to_owned(),
                ReviewScope::Repository(repository_key) => {
                    format!("仓库 {repository_key}")
                }
            };
            sections.push(Section {
                priority: 85,
                id: format!("review-standard:{}", standard.standard_id),
                kind: ContextReferenceKind::ReviewStandard,
                title: format!("评审规范：{}", standard.name),
                content: format!(
                    "- 适用范围：{scope}\n- 修订：{}\n- 内容哈希：{}\n- 字符数：{}\n\n完整规范正文不自动注入上下文；调用 read_task_reference，并传入 referenceId=`review-standard:{}` 读取。提交后的 AI Review 会使用同一规范修订检查真实 Git diff。",
                    standard.revision_id,
                    standard.content_hash,
                    standard.character_count,
                    standard.standard_id
                ),
            });
        }
    }
    if !capabilities.is_empty() {
        let lines = capabilities
            .iter()
            .map(|capability| {
                format!(
                    "- {} / {} [{}]：{}；credentialRef={}",
                    capability.name,
                    capability.provider,
                    capability.environment,
                    capability.status.as_str(),
                    capability.credential_ref.as_deref().unwrap_or("未配置")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(Section {
            priority: 90,
            id: "capability-status".to_owned(),
            kind: ContextReferenceKind::ProjectFact,
            title: "三方能力状态".to_owned(),
            content: format!(
                "{lines}\n\ncredentialRef 仅表示系统凭据库中的引用。禁止读取、打印凭据或把 Secret 写入代码、配置和报告；缺少生产能力时必须报告阻塞。"
            ),
        });
    }
    let authorized_endpoints = capability_api_endpoints
        .iter()
        .filter(|endpoint| task.context_refs.contains(&endpoint.capability_id))
        .collect::<Vec<_>>();
    if !authorized_endpoints.is_empty() {
        let lines = authorized_endpoints
            .iter()
            .map(|endpoint| {
                let methods = endpoint
                    .allowed_methods
                    .iter()
                    .map(|method| method.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "- {}（endpointId={}）：能力 {} / 环境 {} / 方法 {} / 路径前缀 {}{}",
                    endpoint.name,
                    endpoint.id,
                    endpoint.capability_id,
                    endpoint.environment,
                    methods,
                    endpoint.path_prefix,
                    if endpoint.credential_id.is_some() {
                        " / 系统凭据已绑定"
                    } else {
                        " / 无凭据"
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(Section {
            priority: 91,
            id: "capability-api-endpoints".to_owned(),
            kind: ContextReferenceKind::ProjectFact,
            title: "可调用的能力 API".to_owned(),
            content: format!(
                "{lines}\n\n只有上述白名单端点可被当前任务调用。需要访问三方 API 时，调用 call_capability_api，并传入 endpointId、capabilityId、environment、workId、leaseToken、method、path；不要尝试读取凭据、拼接完整 URL 或绕过方法和路径限制。"
            ),
        });
    }
    let mut blueprint_lines = Vec::new();
    for id in &task.context_refs {
        if let Some((title, content)) = render_blueprint_entity(blueprint, id) {
            blueprint_lines.push(format!("### {title}\n{content}"));
        }
    }
    if !blueprint_lines.is_empty() {
        sections.push(Section {
            priority: 95,
            id: "blueprint-context".to_owned(),
            kind: ContextReferenceKind::BlueprintEntity,
            title: "相关蓝图".to_owned(),
            content: blueprint_lines.join("\n\n"),
        });
    }

    let latest_facts = latest_fact_versions(facts);
    if !latest_facts.is_empty() {
        let lines = latest_facts
            .iter()
            .map(|fact| {
                if fact_is_sensitive(fact) {
                    format!(
                        "- [{}] {}：已登记；敏感值不进入 Agent 上下文",
                        fact.category.as_str(),
                        fact.question
                    )
                } else {
                    format!(
                        "- [{}] {}：{}",
                        fact.category.as_str(),
                        fact.question,
                        fact.value
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(Section {
            priority: 80,
            id: "project-facts".to_owned(),
            kind: ContextReferenceKind::ProjectFact,
            title: "已确认项目事实".to_owned(),
            content: lines,
        });
    }

    let open_requests = info_requests
        .iter()
        .filter(|request| request.status == InfoRequestStatus::Open && request.blocking)
        .map(|request| {
            format!(
                "- {}（影响范围：{}；是否绑定本任务：否）",
                request.question, request.impact_scope
            )
        })
        .collect::<Vec<_>>();
    if !open_requests.is_empty() {
        sections.push(Section {
            priority: 40,
            id: "open-info-requests".to_owned(),
            kind: ContextReferenceKind::Instruction,
            title: "项目级未决信息".to_owned(),
            content: format!(
                "以下信息尚未确认，但当前数据没有任务级关联，因此不自动阻塞本任务：\n{}",
                open_requests.join("\n")
            ),
        });
    }

    let dependency_lines = task
        .dependencies
        .iter()
        .filter_map(|dependency| {
            work_items
                .iter()
                .find(|item| item.task_key == dependency.task_key)
                .map(|item| {
                    format!(
                        "- {}：{:?}（{:?}）",
                        dependency.task_key, item.status, dependency.kind
                    )
                })
        })
        .collect::<Vec<_>>();
    if !dependency_lines.is_empty() {
        sections.push(Section {
            priority: 70,
            id: "dependency-status".to_owned(),
            kind: ContextReferenceKind::DependencySummary,
            title: "依赖状态".to_owned(),
            content: dependency_lines.join("\n"),
        });
    }

    let references = task
        .context_refs
        .iter()
        .map(|id| format!("- {}", id.as_str()))
        .chain(
            task.dependencies
                .iter()
                .map(|dependency| format!("- {}", dependency.task_key)),
        )
        .collect::<Vec<_>>();
    if !references.is_empty() {
        sections.push(Section {
            priority: 20,
            id: "reference-catalog".to_owned(),
            kind: ContextReferenceKind::Instruction,
            title: "可按需读取的引用".to_owned(),
            content: format!(
                "调用 read_task_reference 读取以下白名单引用：\n{}",
                references.join("\n")
            ),
        });
    }
    sections.sort_by_key(|section| std::cmp::Reverse(section.priority));
    sections
}

fn repository_context(project: &OpenProject, task: &TaskSpec) -> Result<Option<Section>, AppError> {
    let Some(repository_key) = task.repository_key.as_deref() else {
        return Ok(None);
    };
    let Some(repository) = project
        .database
        .repository_by_key(&project.project.id, repository_key)?
    else {
        return Ok(None);
    };
    let credential_ref = match repository.credential_id {
        Some(credential_id) => project
            .database
            .credential(&project.project.id, &credential_id)?
            .map(|credential| credential.secret_ref),
        None => None,
    };
    let protected = if repository.protected_branches.is_empty() {
        "-".to_owned()
    } else {
        repository.protected_branches.join(", ")
    };
    Ok(Some(Section {
        priority: 98,
        id: "repository-context".to_owned(),
        kind: ContextReferenceKind::Instruction,
        title: "仓库与分支约束".to_owned(),
        content: format!(
            "- 逻辑键：{}\n- 本地路径：{}\n- 远程：{}\n- 默认分支：{}\n- 基线：{}\n- 任务分支：{}\n- 保护分支：{}\n- 认证模式：{}\n- 凭据引用：{}\n- 同步策略：{}\n\n只在本地路径内工作；不要切换、合并或删除保护分支。推送由 ColdStart 在报告通过后执行，不要读取或注入远程 Token，也不要强制推送。",
            repository.key,
            repository.local_path.to_string_lossy(),
            repository.remote_url,
            repository.default_branch,
            task.base_ref
                .as_deref()
                .unwrap_or(&repository.default_branch),
            task.work_branch.as_deref().unwrap_or("-"),
            protected,
            repository.auth_mode.as_str(),
            credential_ref.as_deref().unwrap_or("未配置"),
            repository.sync_policy.as_str(),
        ),
    }))
}

fn render_blueprint_entity(
    blueprint: &BlueprintContent,
    id: &BlueprintEntityId,
) -> Option<(String, String)> {
    if id == &blueprint.product_goal.id {
        return Some(("产品目标".to_owned(), blueprint.product_goal.text.clone()));
    }
    if let Some(platform) = blueprint.platforms.iter().find(|item| &item.id == id) {
        return Some((
            format!("平台：{}", platform.name),
            format!("类型：{:?}\n{}", platform.kind, platform.description),
        ));
    }
    if let Some(module) = blueprint.modules.iter().find(|item| &item.id == id) {
        return Some((
            format!("模块：{}", module.name),
            format!(
                "{}\n依赖模块：{}",
                module.description,
                module
                    .depends_on
                    .iter()
                    .map(BlueprintEntityId::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    if let Some(technology) = blueprint.technologies.iter().find(|item| &item.id == id) {
        return Some((
            format!("技术：{} / {}", technology.category, technology.name),
            technology.reason.clone(),
        ));
    }
    if let Some(capability) = blueprint.capabilities.iter().find(|item| &item.id == id) {
        return Some((
            format!("能力：{}", capability.name),
            format!(
                "提供商：{}\n{}",
                capability.provider, capability.description
            ),
        ));
    }
    if let Some(prerequisite) = blueprint
        .release_prerequisites
        .iter()
        .find(|item| &item.id == id)
    {
        return Some((
            format!("发布条件：{}", prerequisite.name),
            format!(
                "类别：{}\n{}\n提前期：{} 天\n是否阻塞：{}",
                prerequisite.category,
                prerequisite.description,
                prerequisite
                    .lead_time_days
                    .map_or_else(|| "未设置".to_owned(), |days| days.to_string()),
                if prerequisite.blocking { "是" } else { "否" }
            ),
        ));
    }
    if let Some(role) = blueprint.roles.iter().find(|item| &item.id == id) {
        return Some((format!("角色：{}", role.name), role.description.clone()));
    }
    None
}

pub(crate) fn latest_fact_versions(facts: &[ProjectFact]) -> Vec<&ProjectFact> {
    let mut latest = std::collections::BTreeMap::<&str, &ProjectFact>::new();
    for fact in facts {
        let entry = latest.entry(fact.fact_key.as_str()).or_insert(fact);
        if fact.version > entry.version {
            *entry = fact;
        }
    }
    latest.into_values().collect()
}

pub(crate) fn fact_is_sensitive(fact: &ProjectFact) -> bool {
    let lower = format!("{} {}", fact.question, fact.value).to_lowercase();
    matches!(
        fact.category,
        coldstart_core::InfoRequestCategory::Authorization
    ) || [
        "secret", "password", "api key", "apikey", "token", "密钥", "密码", "私钥",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn bullet_list(items: &[String]) -> String {
    if items.is_empty() {
        "- 无".to_owned()
    } else {
        items
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn truncate_to_tokens(text: &str, budget: u32) -> String {
    let mut result = String::new();
    for character in text.chars() {
        result.push(character);
        if estimate_tokens(&result) > budget {
            result.pop();
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_never_exceeds_the_budget() {
        let text = "冷启动".repeat(100);
        let truncated = truncate_to_tokens(&text, 12);
        assert!(estimate_tokens(&truncated) <= 12);
    }
}
