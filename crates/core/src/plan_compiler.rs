use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::{
    AcceptanceCriterion, AcceptancePolicy, BlueprintContent, BlueprintEntityId, DependencyKind,
    PlanContent, PlanValidationError, PlatformKind, ProjectId, TaskDependency, TaskKind, TaskSpec,
    validate_plan,
};

pub const PLAN_TEMPLATE_VERSION: &str = "plan-v1";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanCompileError {
    #[error("blueprint contains an invalid plan: {0}")]
    InvalidPlan(#[from] PlanValidationError),
    #[error("module {module_id} references unknown platform {platform_id}")]
    UnknownPlatform {
        module_id: BlueprintEntityId,
        platform_id: BlueprintEntityId,
    },
    #[error("module {0} references an unknown module dependency")]
    UnknownModuleDependency(BlueprintEntityId),
}

pub fn compile_plan(
    _project_id: &ProjectId,
    blueprint: &BlueprintContent,
) -> Result<PlanContent, PlanCompileError> {
    let platforms = blueprint
        .platforms
        .iter()
        .map(|platform| (platform.id.clone(), platform))
        .collect::<BTreeMap<_, _>>();
    let server_platforms = blueprint
        .platforms
        .iter()
        .filter(|platform| platform.kind == PlatformKind::Server)
        .collect::<Vec<_>>();
    let technologies = blueprint
        .technologies
        .iter()
        .map(|technology| technology.id.clone())
        .collect::<Vec<_>>();
    let module_ids = blueprint
        .modules
        .iter()
        .map(|module| module.id.clone())
        .collect::<BTreeSet<_>>();

    for module in &blueprint.modules {
        for platform_id in &module.platforms {
            if !platforms.contains_key(platform_id) {
                return Err(PlanCompileError::UnknownPlatform {
                    module_id: module.id.clone(),
                    platform_id: platform_id.clone(),
                });
            }
        }
        for dependency in &module.depends_on {
            if !module_ids.contains(dependency) {
                return Err(PlanCompileError::UnknownModuleDependency(
                    dependency.clone(),
                ));
            }
        }
    }

    let mut tasks = Vec::new();
    let mut warnings = Vec::new();

    for platform in &blueprint.platforms {
        tasks.push(platform_foundation(platform, &technologies));
    }

    for server in &server_platforms {
        tasks.push(contract_task(server, blueprint));
    }

    let mut slice_keys = BTreeMap::<(BlueprintEntityId, BlueprintEntityId), String>::new();
    for module in &blueprint.modules {
        for platform_id in &module.platforms {
            let key = format!("slice:{}:{}", module.id, platform_id);
            slice_keys.insert((module.id.clone(), platform_id.clone()), key);
        }
    }

    for module in &blueprint.modules {
        for platform_id in &module.platforms {
            let platform = platforms[platform_id];
            let task_key = slice_keys[&(module.id.clone(), platform_id.clone())].clone();
            let mut dependencies = vec![dependency(
                &format!("foundation:{platform_id}"),
                DependencyKind::Blocks,
            )];
            if platform.kind != PlatformKind::Server {
                for server in &server_platforms {
                    dependencies.push(dependency(
                        &format!("contracts:{}", server.id),
                        DependencyKind::Blocks,
                    ));
                }
                if let Some(server) = server_platforms.first()
                    && let Some(server_slice) =
                        slice_keys.get(&(module.id.clone(), server.id.clone()))
                {
                    dependencies.push(dependency(server_slice, DependencyKind::Blocks));
                }
            } else if let Some(server) = server_platforms
                .iter()
                .find(|server| server.id == *platform_id)
            {
                dependencies.push(dependency(
                    &format!("contracts:{}", server.id),
                    DependencyKind::Blocks,
                ));
            }
            for module_dependency in &module.depends_on {
                if let Some(dependency_key) =
                    slice_keys.get(&(module_dependency.clone(), platform_id.clone()))
                {
                    dependencies.push(dependency(dependency_key, DependencyKind::Blocks));
                }
            }
            tasks.push(module_slice(
                module,
                platform,
                &technologies,
                task_key,
                dependencies,
            ));
        }
    }

    for capability in &blueprint.capabilities {
        let prep_key = format!("capability-prep:{}", capability.id);
        tasks.push(TaskSpec {
            task_key: prep_key.clone(),
            title: format!("准备{}能力", capability.name),
            kind: TaskKind::HumanExternal,
            goal: format!(
                "确认 {} 的账号、环境与费用权限已准备，不向 ColdStart 输入任何明文密钥。",
                capability.provider
            ),
            scope: vec![capability.description.clone()],
            out_of_scope: vec!["不记录或提交 Secret 值".to_owned()],
            module_id: None,
            platform_id: None,
            repository_key: None,
            base_ref: None,
            work_branch: None,
            allowed_paths: vec![],
            context_refs: vec![capability.id.clone()],
            acceptance_criteria: vec![criterion(
                "readiness",
                "确认能力账号、环境和授权范围可用，且未向任务或报告写入 Secret",
                true,
            )],
            acceptance_policy: AcceptancePolicy::HumanAcceptance,
            evidence_requirements: vec!["人工说明".to_owned()],
            priority: 800,
            critical_path: false,
            context_budget_tokens: crate::DEFAULT_CONTEXT_BUDGET_TOKENS,
            resource_keys: vec![],
            dependencies: vec![],
        });

        let fallback_platform = server_platforms
            .first()
            .copied()
            .or_else(|| blueprint.platforms.first());
        if let Some(platform) = fallback_platform {
            let mut context_refs = vec![capability.id.clone(), platform.id.clone()];
            context_refs.extend(technologies.iter().cloned());
            tasks.push(TaskSpec {
                task_key: format!("capability-integration:{}", capability.id),
                title: format!("接入{}", capability.name),
                kind: TaskKind::AgentCoding,
                goal: format!(
                    "在 {} 中完成 {} 接入，读取环境变量名称而非明文 Secret。",
                    platform.name, capability.provider
                ),
                scope: vec![capability.description.clone()],
                out_of_scope: vec!["不连接生产环境".to_owned(), "不写入明文密钥".to_owned()],
                module_id: None,
                platform_id: Some(platform.id.clone()),
                repository_key: Some(repository_key_for_platform(platform.kind).to_owned()),
                base_ref: Some("main".to_owned()),
                work_branch: Some(branch_for_task(&format!(
                    "capability-integration:{}",
                    capability.id
                ))),
                allowed_paths: vec![".".to_owned()],
                context_refs,
                acceptance_criteria: vec![
                    criterion("implementation", "能力接入成功且错误路径可观测", true),
                    criterion("verification", "构建或测试摘要覆盖成功与失败路径", true),
                ],
                acceptance_policy: AcceptancePolicy::AgentReport,
                evidence_requirements: vec!["测试或构建摘要".to_owned()],
                priority: 650,
                critical_path: false,
                context_budget_tokens: crate::DEFAULT_CONTEXT_BUDGET_TOKENS,
                resource_keys: vec![repository_key_for_platform(platform.kind).to_owned()],
                dependencies: vec![
                    dependency(&prep_key, DependencyKind::Blocks),
                    dependency(
                        &format!("foundation:{}", platform.id),
                        DependencyKind::Blocks,
                    ),
                ],
            });
        }
    }

    for prerequisite in &blueprint.release_prerequisites {
        tasks.push(TaskSpec {
            task_key: format!("release-prereq:{}", prerequisite.id),
            title: prerequisite.name.clone(),
            kind: TaskKind::HumanExternal,
            goal: prerequisite.description.clone(),
            scope: vec![format!("类别：{}", prerequisite.category)],
            out_of_scope: vec!["ColdStart 不代替用户操作外部平台".to_owned()],
            module_id: None,
            platform_id: None,
            repository_key: None,
            base_ref: None,
            work_branch: None,
            allowed_paths: vec![],
            context_refs: vec![prerequisite.id.clone()],
            acceptance_criteria: vec![criterion(
                "readiness",
                "确认发布前置条件已满足，并记录必要的说明或证据引用",
                true,
            )],
            acceptance_policy: AcceptancePolicy::HumanAcceptance,
            evidence_requirements: vec!["人工说明或平台材料引用".to_owned()],
            priority: if prerequisite.blocking { 600 } else { 500 },
            critical_path: prerequisite.blocking,
            context_budget_tokens: crate::DEFAULT_CONTEXT_BUDGET_TOKENS,
            resource_keys: vec![],
            dependencies: vec![],
        });
    }

    for platform in &blueprint.platforms {
        if matches!(
            platform.kind,
            PlatformKind::Ios | PlatformKind::Android | PlatformKind::MiniProgram
        ) {
            tasks.push(TaskSpec {
                task_key: format!("prep:{}:store-account", platform.id),
                title: format!("准备{}发布账号", platform.name),
                kind: TaskKind::HumanExternal,
                goal: "确认开发者账号、组织主体、签名或平台审核权限已就绪。".to_owned(),
                scope: vec![format!("平台：{}", platform.name)],
                out_of_scope: vec!["不在本任务编辑证书私钥或密码".to_owned()],
                module_id: None,
                platform_id: Some(platform.id.clone()),
                repository_key: None,
                base_ref: None,
                work_branch: None,
                allowed_paths: vec![],
                context_refs: vec![platform.id.clone()],
                acceptance_criteria: vec![criterion(
                    "readiness",
                    "确认发布账号和主体权限可用，并记录人工说明",
                    true,
                )],
                acceptance_policy: AcceptancePolicy::HumanAcceptance,
                evidence_requirements: vec!["人工说明".to_owned()],
                priority: 550,
                critical_path: true,
                context_budget_tokens: crate::DEFAULT_CONTEXT_BUDGET_TOKENS,
                resource_keys: vec![],
                dependencies: vec![],
            });
        }
    }

    let all_capability_integrations = blueprint
        .capabilities
        .iter()
        .map(|capability| format!("capability-integration:{}", capability.id))
        .collect::<Vec<_>>();
    let all_release_prerequisites = blueprint
        .release_prerequisites
        .iter()
        .map(|prerequisite| format!("release-prereq:{}", prerequisite.id))
        .collect::<Vec<_>>();
    for platform in &blueprint.platforms {
        let mut dependencies = Vec::new();
        for module in &blueprint.modules {
            if module.platforms.contains(&platform.id) {
                dependencies.push(dependency(
                    &slice_keys[&(module.id.clone(), platform.id.clone())],
                    DependencyKind::Blocks,
                ));
            }
        }
        if matches!(
            platform.kind,
            PlatformKind::Ios | PlatformKind::Android | PlatformKind::MiniProgram
        ) {
            dependencies.push(dependency(
                &format!("prep:{}:store-account", platform.id),
                DependencyKind::Blocks,
            ));
        }
        dependencies.extend(
            all_capability_integrations
                .iter()
                .map(|key| dependency(key, DependencyKind::Blocks)),
        );
        dependencies.extend(
            all_release_prerequisites
                .iter()
                .map(|key| dependency(key, DependencyKind::Blocks)),
        );
        tasks.push(TaskSpec {
            task_key: format!("release:{}", platform.id),
            title: format!("{}发布就绪", platform.name),
            kind: TaskKind::Milestone,
            goal: "所有平台切片、能力准备和发布前置事项完成后，形成发布就绪里程碑。".to_owned(),
            scope: vec![format!("平台：{}", platform.name)],
            out_of_scope: vec!["不执行真实部署、提审或商店发布".to_owned()],
            module_id: None,
            platform_id: Some(platform.id.clone()),
            repository_key: None,
            base_ref: None,
            work_branch: None,
            allowed_paths: vec![],
            context_refs: vec![platform.id.clone()],
            acceptance_criteria: vec![],
            acceptance_policy: AcceptancePolicy::HumanAcceptance,
            evidence_requirements: vec![],
            priority: 100,
            critical_path: true,
            context_budget_tokens: crate::DEFAULT_CONTEXT_BUDGET_TOKENS,
            resource_keys: vec![],
            dependencies,
        });
    }

    if server_platforms.is_empty() {
        warnings.push("蓝图没有服务端平台，客户端切片不会生成服务端契约依赖".to_owned());
    }
    if blueprint.modules.is_empty() {
        warnings.push("蓝图没有模块，计划仅包含工程基础和发布门禁".to_owned());
    }
    if blueprint.release_prerequisites.is_empty() {
        warnings.push("蓝图没有发布前置条件，发布里程碑只依赖已完成任务".to_owned());
    }

    tasks.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.task_key.cmp(&right.task_key))
    });
    let content = PlanContent { tasks, warnings };
    validate_plan(&content.tasks)?;
    Ok(content)
}

pub fn repository_key_for_platform(kind: PlatformKind) -> &'static str {
    match kind {
        PlatformKind::Web => "web",
        PlatformKind::Server => "server",
        PlatformKind::Admin => "admin",
        PlatformKind::Ios | PlatformKind::Android => "app",
        PlatformKind::MiniProgram => "mini_program",
    }
}

fn platform_foundation(
    platform: &crate::PlatformDefinition,
    technologies: &[BlueprintEntityId],
) -> TaskSpec {
    let task_key = format!("foundation:{}", platform.id);
    let mut context_refs = vec![platform.id.clone()];
    context_refs.extend(technologies.iter().cloned());
    TaskSpec {
        task_key: task_key.clone(),
        title: format!("{}工程基础", platform.name),
        kind: TaskKind::AgentCoding,
        goal: format!(
            "建立 {} 的可构建、可测试、可继续开发的工程基线。",
            platform.name
        ),
        scope: vec![platform.description.clone()],
        out_of_scope: vec!["不实现业务模块功能".to_owned()],
        module_id: None,
        platform_id: Some(platform.id.clone()),
        repository_key: Some(repository_key_for_platform(platform.kind).to_owned()),
        base_ref: Some("main".to_owned()),
        work_branch: Some(branch_for_task(&task_key)),
        allowed_paths: vec![".".to_owned()],
        context_refs,
        acceptance_criteria: vec![
            criterion("implementation", "工程可构建，基础目录与运行入口完整", true),
            criterion("verification", "构建和基础测试结果有明确摘要", true),
        ],
        acceptance_policy: AcceptancePolicy::AgentReport,
        evidence_requirements: vec!["构建摘要".to_owned(), "测试摘要".to_owned()],
        priority: 900,
        critical_path: true,
        context_budget_tokens: crate::DEFAULT_CONTEXT_BUDGET_TOKENS,
        resource_keys: vec![repository_key_for_platform(platform.kind).to_owned()],
        dependencies: vec![],
    }
}

fn contract_task(server: &crate::PlatformDefinition, blueprint: &BlueprintContent) -> TaskSpec {
    let task_key = format!("contracts:{}", server.id);
    let mut context_refs = vec![server.id.clone()];
    context_refs.extend(blueprint.modules.iter().map(|module| module.id.clone()));
    context_refs.extend(
        blueprint
            .technologies
            .iter()
            .map(|technology| technology.id.clone()),
    );
    TaskSpec {
        task_key: task_key.clone(),
        title: "服务端接口与共享契约".to_owned(),
        kind: TaskKind::AgentCoding,
        goal: "定义模块边界、数据模型、接口契约和错误约定，作为后续平台切片的共同基线。".to_owned(),
        scope: vec!["服务端接口、领域模型、共享类型和错误契约".to_owned()],
        out_of_scope: vec!["不实现完整业务切片".to_owned()],
        module_id: None,
        platform_id: Some(server.id.clone()),
        repository_key: Some("server".to_owned()),
        base_ref: Some("main".to_owned()),
        work_branch: Some(branch_for_task(&task_key)),
        allowed_paths: vec![".".to_owned()],
        context_refs,
        acceptance_criteria: vec![
            criterion(
                "contract",
                "模块接口、数据模型和错误约定完整且相互一致",
                true,
            ),
            criterion("verification", "契约测试或静态检查摘要可用", true),
        ],
        acceptance_policy: AcceptancePolicy::AgentReport,
        evidence_requirements: vec!["契约测试摘要".to_owned()],
        priority: 850,
        critical_path: true,
        context_budget_tokens: crate::DEFAULT_CONTEXT_BUDGET_TOKENS,
        resource_keys: vec!["server".to_owned()],
        dependencies: vec![dependency(
            &format!("foundation:{}", server.id),
            DependencyKind::Blocks,
        )],
    }
}

fn module_slice(
    module: &crate::ModuleDefinition,
    platform: &crate::PlatformDefinition,
    technologies: &[BlueprintEntityId],
    task_key: String,
    mut dependencies: Vec<TaskDependency>,
) -> TaskSpec {
    let mut context_refs = vec![module.id.clone(), platform.id.clone()];
    context_refs.extend(module.depends_on.iter().cloned());
    context_refs.extend(technologies.iter().cloned());
    dependencies.sort_by(|left, right| {
        left.task_key
            .cmp(&right.task_key)
            .then_with(|| format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind)))
    });
    dependencies.dedup_by(|left, right| left.task_key == right.task_key && left.kind == right.kind);
    TaskSpec {
        task_key: task_key.clone(),
        title: format!("{} / {}纵向切片", module.name, platform.name),
        kind: TaskKind::AgentCoding,
        goal: format!(
            "在 {} 中完成“{}”模块的纵向可验收切片：{}",
            platform.name, module.name, module.description
        ),
        scope: vec![format!("模块：{}", module.name), module.description.clone()],
        out_of_scope: vec!["不实现其他模块".to_owned(), "不连接生产环境".to_owned()],
        module_id: Some(module.id.clone()),
        platform_id: Some(platform.id.clone()),
        repository_key: Some(repository_key_for_platform(platform.kind).to_owned()),
        base_ref: Some("main".to_owned()),
        work_branch: Some(branch_for_task(&task_key)),
        allowed_paths: vec![".".to_owned()],
        context_refs,
        acceptance_criteria: vec![
            criterion(
                "implementation",
                "模块主路径和错误路径均已实现并可运行",
                true,
            ),
            criterion("verification", "相关测试或构建摘要覆盖本次修改", true),
        ],
        acceptance_policy: AcceptancePolicy::AgentReport,
        evidence_requirements: vec!["测试或构建摘要".to_owned()],
        priority: 700 + platform_priority(platform.kind),
        critical_path: platform.kind == PlatformKind::Server,
        context_budget_tokens: crate::DEFAULT_CONTEXT_BUDGET_TOKENS,
        resource_keys: vec![repository_key_for_platform(platform.kind).to_owned()],
        dependencies,
    }
}

fn platform_priority(kind: PlatformKind) -> i64 {
    match kind {
        PlatformKind::Server => 50,
        PlatformKind::Web => 40,
        PlatformKind::Admin => 30,
        PlatformKind::Ios | PlatformKind::Android => 20,
        PlatformKind::MiniProgram => 10,
    }
}

fn criterion(id: &str, text: &str, required_evidence: bool) -> AcceptanceCriterion {
    AcceptanceCriterion {
        id: id.to_owned(),
        text: text.to_owned(),
        required_evidence,
    }
}

fn dependency(task_key: &str, kind: DependencyKind) -> TaskDependency {
    TaskDependency {
        task_key: task_key.to_owned(),
        kind,
    }
}

fn branch_for_task(task_key: &str) -> String {
    let slug = task_key
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    let slug = if slug.len() > 100 { &slug[..100] } else { slug };
    format!("codex/{slug}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_keys_cover_all_supported_platforms() {
        assert_eq!(repository_key_for_platform(PlatformKind::Server), "server");
        assert_eq!(repository_key_for_platform(PlatformKind::Admin), "admin");
        assert_eq!(repository_key_for_platform(PlatformKind::Ios), "app");
        assert_eq!(
            repository_key_for_platform(PlatformKind::MiniProgram),
            "mini_program"
        );
    }
}
