use coldstart_core::{
    BlueprintContent, CapabilityObligation, DependencyKind, ModuleDefinition, PlatformDefinition,
    PlatformKind, ProjectId, ReleasePrerequisite, TaskKind, TraceableText, compile_plan,
};

fn blueprint(project_id: &ProjectId) -> BlueprintContent {
    BlueprintContent {
        product_goal: TraceableText {
            id: coldstart_core::BlueprintEntityId::derive(project_id, "goal", "product"),
            text: "提供订单服务".to_owned(),
            sources: vec![],
        },
        in_scope: vec![],
        out_of_scope: vec![],
        roles: vec![],
        platforms: vec![
            PlatformDefinition {
                id: coldstart_core::BlueprintEntityId::derive(project_id, "platform", "server"),
                name: "服务端".to_owned(),
                kind: PlatformKind::Server,
                description: "Rust 服务".to_owned(),
                sources: vec![],
            },
            PlatformDefinition {
                id: coldstart_core::BlueprintEntityId::derive(project_id, "platform", "admin"),
                name: "管理后台".to_owned(),
                kind: PlatformKind::Admin,
                description: "React 管理后台".to_owned(),
                sources: vec![],
            },
            PlatformDefinition {
                id: coldstart_core::BlueprintEntityId::derive(project_id, "platform", "mini"),
                name: "微信小程序".to_owned(),
                kind: PlatformKind::MiniProgram,
                description: "订单小程序".to_owned(),
                sources: vec![],
            },
        ],
        modules: vec![ModuleDefinition {
            id: coldstart_core::BlueprintEntityId::derive(project_id, "module", "orders"),
            name: "订单".to_owned(),
            description: "下单与查询".to_owned(),
            platforms: vec![
                coldstart_core::BlueprintEntityId::derive(project_id, "platform", "server"),
                coldstart_core::BlueprintEntityId::derive(project_id, "platform", "admin"),
                coldstart_core::BlueprintEntityId::derive(project_id, "platform", "mini"),
            ],
            depends_on: vec![],
            sources: vec![],
        }],
        technologies: vec![],
        capabilities: vec![CapabilityObligation {
            id: coldstart_core::BlueprintEntityId::derive(project_id, "capability", "oss"),
            name: "对象存储".to_owned(),
            provider: "阿里云 OSS".to_owned(),
            description: "上传订单附件".to_owned(),
            sources: vec![],
        }],
        release_prerequisites: vec![ReleasePrerequisite {
            id: coldstart_core::BlueprintEntityId::derive(project_id, "release", "icp"),
            name: "ICP 备案".to_owned(),
            category: "compliance".to_owned(),
            description: "完成域名备案".to_owned(),
            lead_time_days: Some(20),
            blocking: true,
            sources: vec![],
        }],
        assumptions: vec![],
        risks: vec![],
    }
}

#[test]
fn creates_platform_foundations_module_slices_and_release_gates() {
    let project_id = ProjectId::new();
    let content = compile_plan(&project_id, &blueprint(&project_id)).expect("plan must compile");
    let keys = content
        .tasks
        .iter()
        .map(|task| task.task_key.as_str())
        .collect::<Vec<_>>();

    assert!(keys.iter().any(|key| key.starts_with("foundation:")));
    assert!(keys.iter().any(|key| key.starts_with("contracts:")));
    assert!(keys.iter().any(|key| key.starts_with("slice:")));
    assert!(keys.iter().any(|key| key.starts_with("release:")));
    assert!(
        content
            .tasks
            .iter()
            .filter(|task| task.kind == TaskKind::AgentCoding)
            .all(|task| task.acceptance_criteria.len() >= 2)
    );
}

#[test]
fn client_slice_depends_on_the_matching_server_slice() {
    let project_id = ProjectId::new();
    let content = compile_plan(&project_id, &blueprint(&project_id)).expect("plan must compile");
    let admin_slice = content
        .tasks
        .iter()
        .find(|task| {
            task.task_key.starts_with("slice:") && task.repository_key.as_deref() == Some("admin")
        })
        .expect("admin slice must exist");

    assert!(admin_slice.dependencies.iter().any(|dependency| {
        dependency.kind == DependencyKind::Blocks
            && dependency.task_key.starts_with("slice:")
            && dependency.task_key.ends_with(&format!(
                ":{}",
                coldstart_core::BlueprintEntityId::derive(&project_id, "platform", "server")
            ))
    }));
}
