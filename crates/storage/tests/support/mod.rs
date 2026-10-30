#![allow(dead_code)]

use chrono::Utc;
use coldstart_core::{
    AcceptanceCriterion, AcceptancePolicy, AcceptanceResult, ArtifactFormat, ArtifactId,
    ArtifactMetadata, BlueprintContent, BlueprintEntityId, BlueprintGeneration, DependencyKind,
    DocumentAnchor, GenerationSource, ModuleDefinition, PlanContent, PlanRevisionId,
    PlatformDefinition, PlatformKind, Project, ProjectId, ReportOutcome, SourceReference,
    TaskDependency, TaskKind, TaskSpec, TraceableText, WorkReportInput,
};
use coldstart_storage::Database;
use tempfile::TempDir;

pub struct PlanFixture {
    _directory: TempDir,
    pub database_path: std::path::PathBuf,
    pub database: Database,
    pub project_id: ProjectId,
    pub plan_id: PlanRevisionId,
    pub tasks: Vec<TaskSpec>,
}

impl PlanFixture {
    pub fn new(tasks: Vec<TaskSpec>) -> Self {
        Self::build(tasks, true)
    }

    pub fn draft(tasks: Vec<TaskSpec>) -> Self {
        Self::build(tasks, false)
    }

    fn build(tasks: Vec<TaskSpec>, publish: bool) -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must exist");
        let database_path = directory.path().join("state.db");
        let project = Project::new("计划测试项目", directory.path().join("project"))
            .expect("project must be valid");
        let mut database = Database::open(&database_path).expect("database must open");
        database
            .initialize_project(&project)
            .expect("project must initialize");

        let artifact_id = ArtifactId::new();
        database
            .insert_artifact(&artifact(project.id, artifact_id), &anchors())
            .expect("artifact must insert");
        let blueprint = blueprint_content(project.id, artifact_id);
        let blueprint_revision = database
            .create_blueprint_revision(
                &project.id,
                blueprint,
                BlueprintGeneration {
                    source: GenerationSource::Manual,
                    model_provider: None,
                    model_name: None,
                    prompt_version: None,
                },
                vec![],
            )
            .expect("blueprint draft must create");
        database
            .publish_blueprint_revision(&project.id, &blueprint_revision.id)
            .expect("blueprint must publish");

        let plan = PlanContent {
            tasks: tasks.clone(),
            warnings: vec![],
        };
        let plan_revision = database
            .create_plan_revision(&project.id, blueprint_revision.id, plan, Utc::now())
            .expect("plan draft must create");
        if publish {
            database
                .publish_plan_revision(&project.id, &plan_revision.id, Utc::now())
                .expect("plan must publish");
        }

        Self {
            _directory: directory,
            database_path,
            database,
            project_id: project.id,
            plan_id: plan_revision.id,
            tasks,
        }
    }

    pub fn create_plan(&mut self, tasks: Vec<TaskSpec>) -> coldstart_core::PlanRevision {
        let blueprint = self
            .database
            .latest_blueprint(&self.project_id, coldstart_core::BlueprintStatus::Published)
            .expect("blueprint query must work")
            .expect("published blueprint must exist");
        self.database
            .create_plan_revision(
                &self.project_id,
                blueprint.id,
                PlanContent {
                    tasks,
                    warnings: vec![],
                },
                Utc::now(),
            )
            .expect("plan candidate must create")
    }

    pub fn task(&self, task_key: &str) -> TaskSpec {
        self.tasks
            .iter()
            .find(|task| task.task_key == task_key)
            .expect("task must exist")
            .clone()
    }
}

pub fn agent_task(task_key: &str, blocked_by: &[&str]) -> TaskSpec {
    TaskSpec {
        task_key: task_key.to_owned(),
        title: task_key.to_owned(),
        kind: TaskKind::AgentCoding,
        goal: "实现可验收任务".to_owned(),
        scope: vec!["当前模块".to_owned()],
        out_of_scope: vec!["其他模块".to_owned()],
        module_id: None,
        platform_id: None,
        repository_key: Some("server".to_owned()),
        base_ref: Some("main".to_owned()),
        work_branch: Some(format!("codex/{task_key}")),
        allowed_paths: vec!["src".to_owned()],
        context_refs: vec![],
        acceptance_criteria: vec![
            criterion("implementation", "实现完成", true),
            criterion("verification", "测试完成", true),
        ],
        acceptance_policy: AcceptancePolicy::AgentReport,
        evidence_requirements: vec!["测试摘要".to_owned()],
        priority: 100,
        critical_path: true,
        context_budget_tokens: 12_000,
        resource_keys: vec!["server".to_owned()],
        dependencies: blocked_by
            .iter()
            .map(|task_key| TaskDependency {
                task_key: (*task_key).to_owned(),
                kind: DependencyKind::Blocks,
            })
            .collect(),
    }
}

pub fn human_task(task_key: &str) -> TaskSpec {
    TaskSpec {
        task_key: task_key.to_owned(),
        title: "准备账号".to_owned(),
        kind: TaskKind::HumanExternal,
        goal: "确认账号可用".to_owned(),
        scope: vec!["账号".to_owned()],
        out_of_scope: vec![],
        module_id: None,
        platform_id: None,
        repository_key: None,
        base_ref: None,
        work_branch: None,
        allowed_paths: vec![],
        context_refs: vec![],
        acceptance_criteria: vec![criterion("readiness", "账号可用", true)],
        acceptance_policy: AcceptancePolicy::HumanAcceptance,
        evidence_requirements: vec!["人工说明".to_owned()],
        priority: 80,
        critical_path: false,
        context_budget_tokens: 12_000,
        resource_keys: vec![],
        dependencies: vec![],
    }
}

pub fn report_for(task: &TaskSpec, idempotency_key: &str) -> WorkReportInput {
    WorkReportInput {
        status: ReportOutcome::Completed,
        summary: "完成实现并验证".to_owned(),
        branch: task.work_branch.clone(),
        commit_sha: Some("0123456789abcdef".to_owned()),
        changed_files: vec!["src/main.rs".to_owned()],
        acceptance_results: task
            .acceptance_criteria
            .iter()
            .map(|criterion| AcceptanceResult {
                criterion_id: criterion.id.clone(),
                passed: true,
                note: "通过".to_owned(),
                evidence_refs: vec!["test-summary".to_owned()],
            })
            .collect(),
        commands_run: vec![],
        evidence_refs: vec!["test-summary".to_owned()],
        known_issues: vec![],
        follow_up_items: vec![],
        contract_hash: task.contract_hash(),
        idempotency_key: idempotency_key.to_owned(),
    }
}

fn criterion(id: &str, text: &str, required_evidence: bool) -> AcceptanceCriterion {
    AcceptanceCriterion {
        id: id.to_owned(),
        text: text.to_owned(),
        required_evidence,
    }
}

fn artifact(project_id: ProjectId, id: ArtifactId) -> ArtifactMetadata {
    ArtifactMetadata {
        id,
        project_id,
        file_name: "requirements.md".to_owned(),
        format: ArtifactFormat::Markdown,
        content_hash: "content-hash".to_owned(),
        text_hash: "text-hash".to_owned(),
        byte_size: 128,
        import_source: "requirements.md".to_owned(),
        imported_at: Utc::now(),
    }
}

fn anchors() -> Vec<DocumentAnchor> {
    vec![DocumentAnchor {
        id: "section_1".to_owned(),
        label: "业务范围".to_owned(),
        text: "需要 Web 和服务端。".to_owned(),
    }]
}

fn blueprint_content(project_id: ProjectId, artifact_id: ArtifactId) -> BlueprintContent {
    let source = SourceReference {
        artifact_id,
        anchor_id: "section_1".to_owned(),
        quote: Some("需要 Web 和服务端。".to_owned()),
    };
    let server_id = BlueprintEntityId::derive(&project_id, "platform", "Server");
    let module_id = BlueprintEntityId::derive(&project_id, "module", "Orders");
    BlueprintContent {
        product_goal: TraceableText {
            id: BlueprintEntityId::derive(&project_id, "goal", "product"),
            text: "订单系统".to_owned(),
            sources: vec![source.clone()],
        },
        in_scope: vec![],
        out_of_scope: vec![],
        roles: vec![],
        platforms: vec![PlatformDefinition {
            id: server_id.clone(),
            name: "服务端".to_owned(),
            kind: PlatformKind::Server,
            description: "Rust 服务".to_owned(),
            sources: vec![source.clone()],
        }],
        modules: vec![ModuleDefinition {
            id: module_id,
            name: "订单".to_owned(),
            description: "订单流程".to_owned(),
            platforms: vec![server_id],
            depends_on: vec![],
            sources: vec![source],
        }],
        technologies: vec![],
        capabilities: vec![],
        release_prerequisites: vec![],
        assumptions: vec![],
        risks: vec![],
    }
}
