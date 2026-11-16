use std::{path::PathBuf, sync::Arc};

use coldstart_app::{
    DeliveryOverviewService, DeliveryRepositoryState, PlanService, ProjectService,
};
use coldstart_core::{
    ArtifactId, BlueprintContent, BlueprintEntityId, BlueprintGeneration, GenerationSource,
    ModuleDefinition, PlatformDefinition, PlatformKind, ProjectId, SourceReference, TraceableText,
};
use coldstart_storage::Database;
use tempfile::TempDir;

#[path = "support/mod.rs"]
mod support;

struct Fixture {
    _directory: TempDir,
    home: PathBuf,
    project_id: ProjectId,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let home = directory.path().join("project");
        let status = ProjectService::new()
            .create_project("多仓库交付总览测试", home.clone())
            .expect("project must create");
        let database_path = home.join(".coldstart").join("state.db");
        let mut database = Database::open(&database_path).expect("database must open");
        let artifact_id = support::seed_requirement_artifact(&mut database, status.project_id);
        let revision = database
            .create_blueprint_revision(
                &status.project_id,
                blueprint_content(status.project_id, artifact_id),
                BlueprintGeneration {
                    source: GenerationSource::Manual,
                    model_provider: None,
                    model_name: None,
                    prompt_version: None,
                },
                vec![],
            )
            .expect("blueprint must create");
        database
            .publish_blueprint_revision(&status.project_id, &revision.id)
            .expect("blueprint must publish");
        support::configure_repository(&home, status.project_id, "server", false);
        support::configure_repository(&home, status.project_id, "web", true);
        Self {
            _directory: directory,
            home,
            project_id: status.project_id,
        }
    }

    fn event_count(&self) -> u64 {
        Database::open(&self.home.join(".coldstart").join("state.db"))
            .expect("database must open")
            .event_count(&self.project_id)
            .expect("event count must load")
    }
}

fn blueprint_content(project_id: ProjectId, artifact_id: ArtifactId) -> BlueprintContent {
    let source = SourceReference {
        artifact_id,
        anchor_id: "section_1".to_owned(),
        quote: Some("需要 Web、服务端和管理后台订单系统".to_owned()),
    };
    let server = BlueprintEntityId::derive(&project_id, "platform", "Server");
    let web = BlueprintEntityId::derive(&project_id, "platform", "Web");
    BlueprintContent {
        product_goal: TraceableText {
            id: BlueprintEntityId::derive(&project_id, "goal", "product"),
            text: "构建多端订单系统".to_owned(),
            sources: vec![source.clone()],
        },
        in_scope: vec![],
        out_of_scope: vec![],
        roles: vec![],
        platforms: vec![
            PlatformDefinition {
                id: server.clone(),
                name: "服务端".to_owned(),
                kind: PlatformKind::Server,
                description: "订单 API".to_owned(),
                sources: vec![source.clone()],
            },
            PlatformDefinition {
                id: web.clone(),
                name: "Web".to_owned(),
                kind: PlatformKind::Web,
                description: "订单前台".to_owned(),
                sources: vec![source.clone()],
            },
        ],
        modules: vec![
            ModuleDefinition {
                id: BlueprintEntityId::derive(&project_id, "module", "ServerOrders"),
                name: "服务端订单".to_owned(),
                description: "订单 API".to_owned(),
                platforms: vec![server],
                depends_on: vec![],
                sources: vec![source.clone()],
            },
            ModuleDefinition {
                id: BlueprintEntityId::derive(&project_id, "module", "WebOrders"),
                name: "Web 订单".to_owned(),
                description: "订单页面".to_owned(),
                platforms: vec![web],
                depends_on: vec![],
                sources: vec![source],
            },
        ],
        technologies: vec![],
        capabilities: vec![],
        release_prerequisites: vec![],
        assumptions: vec![],
        risks: vec![],
    }
}

#[test]
fn reads_delivery_overview_from_local_facts_without_writing_events() {
    let fixture = Fixture::new();
    let plan = PlanService::with_gateways(
        Arc::new(coldstart_adapters::MemorySecretStore::default()),
        Arc::new(support::FakePlanGit::passing()),
        Arc::new(support::ScriptedExecutor::default()),
    );
    let draft = plan
        .generate_draft(fixture.home.clone())
        .expect("plan draft must generate")
        .draft
        .expect("draft must exist");
    plan.publish(fixture.home.clone(), draft.id)
        .expect("plan must publish");
    let events_before = fixture.event_count();

    let overview = DeliveryOverviewService::new()
        .load(fixture.home.clone())
        .expect("delivery overview must load");

    assert_eq!(overview.project_id, fixture.project_id);
    assert_eq!(overview.plan_revision_number, Some(1));
    assert_eq!(overview.unassigned_agent_task_count, 0);
    assert_eq!(overview.repositories.len(), 2);
    assert_eq!(overview.repositories[0].repository_key, "server");
    assert_eq!(overview.repositories[1].repository_key, "web");
    assert!(overview.repositories[0].tasks.total > 0);
    assert_eq!(overview.repositories[0].verification_command_count, 0);
    assert_eq!(
        overview.repositories[0].state,
        DeliveryRepositoryState::Attention
    );
    assert_eq!(
        overview.repositories[0].next_action,
        "先为该仓库配置本地验证命令"
    );
    assert_eq!(overview.repositories[1].verification_command_count, 1);
    assert_eq!(
        overview.repositories[1].state,
        DeliveryRepositoryState::Ready
    );
    assert!(overview.repositories[1].next_action.contains("主动领取"));
    assert_eq!(fixture.event_count(), events_before);
}
