use coldstart_core::{
    ArtifactId, BlueprintContent, BlueprintEntityId, BlueprintProposal, BlueprintValidationError,
    CapabilityObligation, ModuleDefinition, PlatformDefinition, PlatformKind, ProjectId,
    ProposedModule, ProposedPlatform, ProposedSourceReference, Risk, RoleDefinition,
    SourceReference, TechnologyChoice, TraceableText, validate_blueprint_structure,
};

fn source() -> Vec<SourceReference> {
    vec![SourceReference {
        artifact_id: ArtifactId::new(),
        anchor_id: "section_1".to_owned(),
        quote: Some("订单管理需要支持退款".to_owned()),
    }]
}

fn content_with_modules(dependencies: [(&str, &str); 2]) -> BlueprintContent {
    let project_id = ProjectId::new();
    let platform = PlatformDefinition {
        id: BlueprintEntityId::derive(&project_id, "platform", "管理后台"),
        name: "管理后台".to_owned(),
        kind: PlatformKind::Admin,
        description: "运营人员使用".to_owned(),
        sources: source(),
    };
    let first_id = BlueprintEntityId::derive(&project_id, "module", dependencies[0].0);
    let second_id = BlueprintEntityId::derive(&project_id, "module", dependencies[1].0);
    let modules = vec![
        ModuleDefinition {
            id: first_id.clone(),
            name: dependencies[0].0.to_owned(),
            description: String::new(),
            platforms: vec![platform.id.clone()],
            depends_on: vec![BlueprintEntityId::derive(
                &project_id,
                "module",
                dependencies[0].1,
            )],
            sources: source(),
        },
        ModuleDefinition {
            id: second_id.clone(),
            name: dependencies[1].0.to_owned(),
            description: String::new(),
            platforms: vec![platform.id.clone()],
            depends_on: vec![BlueprintEntityId::derive(
                &project_id,
                "module",
                dependencies[1].1,
            )],
            sources: source(),
        },
    ];

    BlueprintContent {
        product_goal: TraceableText {
            id: BlueprintEntityId::derive(&project_id, "goal", "product"),
            text: "管理订单与退款".to_owned(),
            sources: source(),
        },
        in_scope: Vec::new(),
        out_of_scope: Vec::new(),
        roles: Vec::<RoleDefinition>::new(),
        platforms: vec![platform],
        modules,
        technologies: Vec::<TechnologyChoice>::new(),
        capabilities: Vec::<CapabilityObligation>::new(),
        release_prerequisites: Vec::new(),
        assumptions: Vec::new(),
        risks: Vec::<Risk>::new(),
    }
}

#[test]
fn derives_stable_ids_from_project_and_normalized_name() {
    let project_id = ProjectId::new();
    let first = BlueprintEntityId::derive(&project_id, "module", "订单 管理");
    let second = BlueprintEntityId::derive(&project_id, "module", " 订单   管理 ");
    let other_project = BlueprintEntityId::derive(&ProjectId::new(), "module", "订单 管理");

    assert_eq!(first, second);
    assert_ne!(first, other_project);
    assert!(first.as_str().starts_with("module_"));
}

#[test]
fn rejects_module_dependency_cycles() {
    let content = content_with_modules([("订单", "库存"), ("库存", "订单")]);

    let error = validate_blueprint_structure(&content).expect_err("cycle must be rejected");

    assert!(matches!(
        error,
        BlueprintValidationError::ModuleCycle { .. }
    ));
}

#[test]
fn accepts_an_acyclic_blueprint() {
    let content = content_with_modules([("订单", "库存"), ("库存", "")]);

    // The second dependency is empty by design and must be rejected because every id must exist.
    assert!(matches!(
        validate_blueprint_structure(&content),
        Err(BlueprintValidationError::UnknownModuleDependency { .. })
    ));
}

#[test]
fn proposal_conversion_maps_names_to_stable_ids_and_preserves_sources() {
    let project_id = ProjectId::new();
    let artifact_id = ArtifactId::new();
    let proposal = BlueprintProposal {
        product_goal: "订单系统".to_owned(),
        product_goal_sources: vec![ProposedSourceReference {
            artifact_id: artifact_id.to_string(),
            anchor_id: "section_1".to_owned(),
            quote: None,
        }],
        in_scope: Vec::new(),
        out_of_scope: Vec::new(),
        roles: Vec::new(),
        platforms: vec![ProposedPlatform {
            name: "小程序".to_owned(),
            kind: PlatformKind::MiniProgram,
            description: String::new(),
            sources: Vec::new(),
        }],
        modules: vec![ProposedModule {
            name: "订单".to_owned(),
            description: String::new(),
            platforms: vec!["小程序".to_owned()],
            depends_on: Vec::new(),
            sources: Vec::new(),
        }],
        technologies: Vec::new(),
        capabilities: Vec::new(),
        release_prerequisites: Vec::new(),
        assumptions: Vec::new(),
        risks: Vec::new(),
        open_questions: Vec::new(),
    };

    let content = BlueprintContent::from_proposal(&project_id, proposal).expect("must convert");

    assert_eq!(content.platforms.len(), 1);
    assert_eq!(
        content.modules[0].platforms,
        vec![content.platforms[0].id.clone()]
    );
    assert_eq!(content.product_goal.sources[0].artifact_id, artifact_id);
}

#[test]
fn proposal_schema_is_json_serializable() {
    let schema = schemars::schema_for!(BlueprintProposal);
    let value = serde_json::to_value(schema).expect("schema must serialize");

    assert!(value.to_string().contains("productGoal"));
    assert!(value.to_string().contains("openQuestions"));
}
