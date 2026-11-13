//! 项目蓝图领域模型。
//!
//! 蓝图汇总范围、模块、技术栈、三方能力和待确认问题。所有由模型生成的
//! 内容在进入持久化前都必须经过来源引用、结构化字段和范围边界校验。

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::entity_id::normalize_entity_name;
use crate::{
    ArtifactId, BlueprintEntityId, BlueprintRevisionId, InfoRequestCategory, ProjectId,
    SourceReference,
};

pub const BLUEPRINT_SCHEMA_VERSION: u32 = 1;
pub const BLUEPRINT_PROMPT_VERSION: &str = "blueprint-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceableText {
    pub id: BlueprintEntityId,
    pub text: String,
    pub sources: Vec<SourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleDefinition {
    pub id: BlueprintEntityId,
    pub name: String,
    pub description: String,
    pub sources: Vec<SourceReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlatformKind {
    Web,
    Server,
    Admin,
    Ios,
    Android,
    MiniProgram,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformDefinition {
    pub id: BlueprintEntityId,
    pub name: String,
    pub kind: PlatformKind,
    pub description: String,
    pub sources: Vec<SourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleDefinition {
    pub id: BlueprintEntityId,
    pub name: String,
    pub description: String,
    pub platforms: Vec<BlueprintEntityId>,
    pub depends_on: Vec<BlueprintEntityId>,
    pub sources: Vec<SourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TechnologyChoice {
    pub id: BlueprintEntityId,
    pub category: String,
    pub name: String,
    pub reason: String,
    pub sources: Vec<SourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityObligation {
    pub id: BlueprintEntityId,
    pub name: String,
    pub provider: String,
    pub description: String,
    pub sources: Vec<SourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleasePrerequisite {
    pub id: BlueprintEntityId,
    pub name: String,
    pub category: String,
    pub description: String,
    pub lead_time_days: Option<u32>,
    pub blocking: bool,
    pub sources: Vec<SourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Assumption {
    pub id: BlueprintEntityId,
    pub text: String,
    pub sources: Vec<SourceReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Risk {
    pub id: BlueprintEntityId,
    pub text: String,
    pub severity: RiskSeverity,
    pub sources: Vec<SourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlueprintContent {
    pub product_goal: TraceableText,
    pub in_scope: Vec<TraceableText>,
    pub out_of_scope: Vec<TraceableText>,
    pub roles: Vec<RoleDefinition>,
    pub platforms: Vec<PlatformDefinition>,
    pub modules: Vec<ModuleDefinition>,
    pub technologies: Vec<TechnologyChoice>,
    pub capabilities: Vec<CapabilityObligation>,
    pub release_prerequisites: Vec<ReleasePrerequisite>,
    pub assumptions: Vec<Assumption>,
    pub risks: Vec<Risk>,
}

impl BlueprintContent {
    /// 将模型提案转换为受来源锚点约束的蓝图修订，拒绝越界、重复或未溯源的字段。
    pub fn from_proposal(
        project_id: &ProjectId,
        proposal: BlueprintProposal,
    ) -> Result<Self, ProposalConversionError> {
        let product_goal = TraceableText {
            id: BlueprintEntityId::derive(project_id, "goal", "product"),
            text: proposal.product_goal.trim().to_owned(),
            sources: convert_sources(proposal.product_goal_sources)?,
        };
        let in_scope = proposal
            .in_scope
            .into_iter()
            .map(|item| convert_traceable_text(project_id, "scope_in", item))
            .collect::<Result<Vec<_>, _>>()?;
        let out_of_scope = proposal
            .out_of_scope
            .into_iter()
            .map(|item| convert_traceable_text(project_id, "scope_out", item))
            .collect::<Result<Vec<_>, _>>()?;
        let roles = proposal
            .roles
            .into_iter()
            .map(|role| {
                let name = clean_name(&role.name);
                Ok(RoleDefinition {
                    id: BlueprintEntityId::derive(project_id, "role", &name),
                    name,
                    description: role.description.trim().to_owned(),
                    sources: convert_sources(role.sources)?,
                })
            })
            .collect::<Result<Vec<_>, ProposalConversionError>>()?;
        let platforms = proposal
            .platforms
            .into_iter()
            .map(|platform| {
                let name = clean_name(&platform.name);
                Ok(PlatformDefinition {
                    id: BlueprintEntityId::derive(project_id, "platform", &name),
                    name,
                    kind: platform.kind,
                    description: platform.description.trim().to_owned(),
                    sources: convert_sources(platform.sources)?,
                })
            })
            .collect::<Result<Vec<_>, ProposalConversionError>>()?;

        let mut platform_ids = HashMap::new();
        for platform in &platforms {
            platform_ids.insert(normalize_entity_name(&platform.name), platform.id.clone());
        }

        let mut proposed_modules = Vec::with_capacity(proposal.modules.len());
        let mut module_ids = HashMap::new();
        for module in proposal.modules {
            let name = clean_name(&module.name);
            let id = BlueprintEntityId::derive(project_id, "module", &name);
            if module_ids
                .insert(normalize_entity_name(&name), id.clone())
                .is_some()
            {
                return Err(ProposalConversionError::DuplicateName {
                    kind: "module".to_owned(),
                    name,
                });
            }
            proposed_modules.push(ProposedModuleIds {
                id,
                name,
                description: module.description.trim().to_owned(),
                platform_names: module.platforms,
                dependency_names: module.depends_on,
                sources: convert_sources(module.sources)?,
            });
        }

        let modules = proposed_modules
            .into_iter()
            .map(|module| {
                let platforms = module
                    .platform_names
                    .into_iter()
                    .map(|name| {
                        platform_ids
                            .get(&normalize_entity_name(&name))
                            .cloned()
                            .ok_or_else(|| ProposalConversionError::UnknownPlatform {
                                module: module.name.clone(),
                                platform: name,
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let depends_on = module
                    .dependency_names
                    .into_iter()
                    .map(|name| {
                        module_ids
                            .get(&normalize_entity_name(&name))
                            .cloned()
                            .ok_or_else(|| ProposalConversionError::UnknownModuleDependency {
                                module: module.name.clone(),
                                dependency: name,
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ModuleDefinition {
                    id: module.id,
                    name: module.name,
                    description: module.description,
                    platforms,
                    depends_on,
                    sources: module.sources,
                })
            })
            .collect::<Result<Vec<_>, ProposalConversionError>>()?;

        let technologies = proposal
            .technologies
            .into_iter()
            .map(|technology| {
                let category = clean_name(&technology.category);
                let name = clean_name(&technology.name);
                Ok(TechnologyChoice {
                    id: BlueprintEntityId::derive(
                        project_id,
                        "tech",
                        &format!("{category}:{name}"),
                    ),
                    category,
                    name,
                    reason: technology.reason.trim().to_owned(),
                    sources: convert_sources(technology.sources)?,
                })
            })
            .collect::<Result<Vec<_>, ProposalConversionError>>()?;
        let capabilities = proposal
            .capabilities
            .into_iter()
            .map(|capability| {
                let name = clean_name(&capability.name);
                Ok(CapabilityObligation {
                    id: BlueprintEntityId::derive(project_id, "capability", &name),
                    name,
                    provider: capability.provider.trim().to_owned(),
                    description: capability.description.trim().to_owned(),
                    sources: convert_sources(capability.sources)?,
                })
            })
            .collect::<Result<Vec<_>, ProposalConversionError>>()?;
        let release_prerequisites = proposal
            .release_prerequisites
            .into_iter()
            .map(|prerequisite| {
                let name = clean_name(&prerequisite.name);
                Ok(ReleasePrerequisite {
                    id: BlueprintEntityId::derive(project_id, "release", &name),
                    name,
                    category: clean_name(&prerequisite.category),
                    description: prerequisite.description.trim().to_owned(),
                    lead_time_days: prerequisite.lead_time_days,
                    blocking: prerequisite.blocking,
                    sources: convert_sources(prerequisite.sources)?,
                })
            })
            .collect::<Result<Vec<_>, ProposalConversionError>>()?;
        let assumptions = proposal
            .assumptions
            .into_iter()
            .map(|assumption| {
                let text = assumption.text.trim().to_owned();
                Ok(Assumption {
                    id: BlueprintEntityId::derive(project_id, "assumption", &text),
                    sources: convert_sources(assumption.sources)?,
                    text,
                })
            })
            .collect::<Result<Vec<_>, ProposalConversionError>>()?;
        let risks = proposal
            .risks
            .into_iter()
            .map(|risk| {
                let text = risk.text.trim().to_owned();
                Ok(Risk {
                    id: BlueprintEntityId::derive(project_id, "risk", &text),
                    text,
                    severity: risk.severity,
                    sources: convert_sources(risk.sources)?,
                })
            })
            .collect::<Result<Vec<_>, ProposalConversionError>>()?;

        Ok(Self {
            product_goal,
            in_scope,
            out_of_scope,
            roles,
            platforms,
            modules,
            technologies,
            capabilities,
            release_prerequisites,
            assumptions,
            risks,
        })
    }

    /// 汇总蓝图内所有来源引用，供锚点校验和发布前完整性检查复用。
    pub fn source_references(&self) -> Vec<&SourceReference> {
        let mut references = Vec::new();
        references.extend(&self.product_goal.sources);
        for item in self.in_scope.iter().chain(&self.out_of_scope) {
            references.extend(&item.sources);
        }
        for item in &self.roles {
            references.extend(&item.sources);
        }
        for item in &self.platforms {
            references.extend(&item.sources);
        }
        for item in &self.modules {
            references.extend(&item.sources);
        }
        for item in &self.technologies {
            references.extend(&item.sources);
        }
        for item in &self.capabilities {
            references.extend(&item.sources);
        }
        for item in &self.release_prerequisites {
            references.extend(&item.sources);
        }
        for item in &self.assumptions {
            references.extend(&item.sources);
        }
        for item in &self.risks {
            references.extend(&item.sources);
        }
        references
    }
}

fn convert_traceable_text(
    project_id: &ProjectId,
    kind: &str,
    item: ProposedScopeItem,
) -> Result<TraceableText, ProposalConversionError> {
    let text = item.text.trim().to_owned();
    Ok(TraceableText {
        id: BlueprintEntityId::derive(project_id, kind, &text),
        text,
        sources: convert_sources(item.sources)?,
    })
}

fn convert_sources(
    sources: Vec<ProposedSourceReference>,
) -> Result<Vec<SourceReference>, ProposalConversionError> {
    let mut seen = HashSet::new();
    let mut converted = Vec::with_capacity(sources.len());
    for source in sources {
        let artifact_id = source
            .artifact_id
            .parse::<ArtifactId>()
            .map_err(|_| ProposalConversionError::InvalidArtifactId(source.artifact_id.clone()))?;
        let anchor_id = source.anchor_id.trim().to_owned();
        if anchor_id.is_empty() {
            return Err(ProposalConversionError::EmptyAnchor(source.artifact_id));
        }
        if !seen.insert((artifact_id, anchor_id.clone())) {
            continue;
        }
        let quote = source
            .quote
            .map(|quote| quote.trim().to_owned())
            .filter(|quote| !quote.is_empty());
        converted.push(SourceReference {
            artifact_id,
            anchor_id,
            quote,
        });
    }
    Ok(converted)
}

fn clean_name(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

struct ProposedModuleIds {
    id: BlueprintEntityId,
    name: String,
    description: String,
    platform_names: Vec<String>,
    dependency_names: Vec<String>,
    sources: Vec<SourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProposalConversionError {
    #[error("invalid artifact id in source reference: {0}")]
    InvalidArtifactId(String),
    #[error("source reference for artifact {0} has an empty anchor")]
    EmptyAnchor(String),
    #[error("duplicate {kind} name in model output: {name}")]
    DuplicateName { kind: String, name: String },
    #[error("module {module} references unknown platform: {platform}")]
    UnknownPlatform { module: String, platform: String },
    #[error("module {module} depends on unknown module: {dependency}")]
    UnknownModuleDependency { module: String, dependency: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlueprintProposal {
    pub product_goal: String,
    #[serde(default)]
    pub product_goal_sources: Vec<ProposedSourceReference>,
    #[serde(default)]
    pub in_scope: Vec<ProposedScopeItem>,
    #[serde(default)]
    pub out_of_scope: Vec<ProposedScopeItem>,
    #[serde(default)]
    pub roles: Vec<ProposedRole>,
    #[serde(default)]
    pub platforms: Vec<ProposedPlatform>,
    #[serde(default)]
    pub modules: Vec<ProposedModule>,
    #[serde(default)]
    pub technologies: Vec<ProposedTechnology>,
    #[serde(default)]
    pub capabilities: Vec<ProposedCapability>,
    #[serde(default)]
    pub release_prerequisites: Vec<ProposedReleasePrerequisite>,
    #[serde(default)]
    pub assumptions: Vec<ProposedNote>,
    #[serde(default)]
    pub risks: Vec<ProposedRisk>,
    #[serde(default)]
    pub open_questions: Vec<ProposedInfoRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedSourceReference {
    pub artifact_id: String,
    pub anchor_id: String,
    #[serde(default)]
    pub quote: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedScopeItem {
    pub text: String,
    #[serde(default)]
    pub sources: Vec<ProposedSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedRole {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub sources: Vec<ProposedSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedPlatform {
    pub name: String,
    pub kind: PlatformKind,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub sources: Vec<ProposedSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedModule {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub sources: Vec<ProposedSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedTechnology {
    pub category: String,
    pub name: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub sources: Vec<ProposedSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedCapability {
    pub name: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub sources: Vec<ProposedSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedReleasePrerequisite {
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub lead_time_days: Option<u32>,
    pub blocking: bool,
    #[serde(default)]
    pub sources: Vec<ProposedSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedNote {
    pub text: String,
    #[serde(default)]
    pub sources: Vec<ProposedSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedRisk {
    pub text: String,
    pub severity: RiskSeverity,
    #[serde(default)]
    pub sources: Vec<ProposedSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedInfoRequest {
    pub category: InfoRequestCategory,
    pub question: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub example: Option<String>,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub impact_scope: String,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub blocking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlueprintStatus {
    Draft,
    Published,
    Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationSource {
    Model,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintGeneration {
    pub source: GenerationSource,
    pub model_provider: Option<String>,
    pub model_name: Option<String>,
    pub prompt_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintRevision {
    pub id: BlueprintRevisionId,
    pub project_id: ProjectId,
    pub revision_number: u64,
    pub status: BlueprintStatus,
    pub content: BlueprintContent,
    pub content_hash: String,
    pub source_artifact_ids: Vec<ArtifactId>,
    pub generation: BlueprintGeneration,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BlueprintValidationError {
    #[error("blueprint field {field} cannot be empty")]
    EmptyText { field: String },
    #[error("blueprint collection {field} must not be empty")]
    EmptyCollection { field: String },
    #[error("blueprint entity id is empty: {field}")]
    EmptyEntityId { field: String },
    #[error("duplicate blueprint entity id: {0}")]
    DuplicateId(BlueprintEntityId),
    #[error("duplicate {kind} name: {name}")]
    DuplicateName { kind: String, name: String },
    #[error("module {module} references unknown platform id: {platform}")]
    UnknownModulePlatform { module: String, platform: String },
    #[error("module {module} depends on unknown module id: {dependency}")]
    UnknownModuleDependency { module: String, dependency: String },
    #[error("module {0} depends on itself")]
    ModuleDependsOnItself(String),
    #[error("module dependency cycle: {}", modules.join(" -> "))]
    ModuleCycle { modules: Vec<String> },
    #[error("blueprint text {field} exceeds {max} characters")]
    TextTooLong { field: String, max: usize },
    #[error("source quote for {artifact_id}/{anchor_id} exceeds {max} characters")]
    SourceQuoteTooLong {
        artifact_id: String,
        anchor_id: String,
        max: usize,
    },
}

/// 校验蓝图范围、模块、技术栈、能力与来源引用的结构完整性。
pub fn validate_blueprint_structure(
    content: &BlueprintContent,
) -> Result<(), BlueprintValidationError> {
    let mut ids = HashSet::new();
    let mut names_by_kind: HashMap<&'static str, HashSet<String>> = HashMap::new();

    validate_text("productGoal.text", &content.product_goal.text, 4_000, true)?;
    check_id(&content.product_goal.id, &mut ids)?;
    validate_sources(&content.product_goal.sources)?;
    validate_text_items("inScope", &content.in_scope, &mut ids)?;
    validate_text_items("outOfScope", &content.out_of_scope, &mut ids)?;
    validate_named_items(
        "role",
        content.roles.iter().map(|item| {
            (
                &item.id,
                item.name.as_str(),
                item.description.as_str(),
                item.sources.as_slice(),
            )
        }),
        &mut ids,
        &mut names_by_kind,
    )?;
    validate_named_items(
        "platform",
        content.platforms.iter().map(|item| {
            (
                &item.id,
                item.name.as_str(),
                item.description.as_str(),
                item.sources.as_slice(),
            )
        }),
        &mut ids,
        &mut names_by_kind,
    )?;

    if content.platforms.is_empty() {
        return Err(BlueprintValidationError::EmptyCollection {
            field: "platforms".to_owned(),
        });
    }

    let platform_ids = content
        .platforms
        .iter()
        .map(|platform| platform.id.clone())
        .collect::<HashSet<_>>();
    let module_names = content
        .modules
        .iter()
        .map(|module| (module.id.clone(), module.name.clone()))
        .collect::<HashMap<_, _>>();

    for module in &content.modules {
        check_id(&module.id, &mut ids)?;
        validate_name("module", &module.name)?;
        validate_text("module.description", &module.description, 4_000, false)?;
        remember_name("module", &module.name, &mut names_by_kind)?;
        validate_sources(&module.sources)?;
        for platform_id in &module.platforms {
            if !platform_ids.contains(platform_id) {
                return Err(BlueprintValidationError::UnknownModulePlatform {
                    module: module.name.clone(),
                    platform: platform_id.to_string(),
                });
            }
        }
        for dependency in &module.depends_on {
            if dependency == &module.id {
                return Err(BlueprintValidationError::ModuleDependsOnItself(
                    module.name.clone(),
                ));
            }
            if !module_names.contains_key(dependency) {
                return Err(BlueprintValidationError::UnknownModuleDependency {
                    module: module.name.clone(),
                    dependency: dependency.to_string(),
                });
            }
        }
    }

    if content.modules.is_empty() {
        return Err(BlueprintValidationError::EmptyCollection {
            field: "modules".to_owned(),
        });
    }
    validate_module_cycles(&content.modules)?;

    validate_named_items(
        "technology",
        content.technologies.iter().map(|item| {
            (
                &item.id,
                item.name.as_str(),
                item.reason.as_str(),
                item.sources.as_slice(),
            )
        }),
        &mut ids,
        &mut names_by_kind,
    )?;
    validate_named_items(
        "capability",
        content.capabilities.iter().map(|item| {
            (
                &item.id,
                item.name.as_str(),
                item.description.as_str(),
                item.sources.as_slice(),
            )
        }),
        &mut ids,
        &mut names_by_kind,
    )?;
    validate_named_items(
        "releasePrerequisite",
        content.release_prerequisites.iter().map(|item| {
            (
                &item.id,
                item.name.as_str(),
                item.description.as_str(),
                item.sources.as_slice(),
            )
        }),
        &mut ids,
        &mut names_by_kind,
    )?;

    for assumption in &content.assumptions {
        check_id(&assumption.id, &mut ids)?;
        validate_text("assumption.text", &assumption.text, 4_000, true)?;
        validate_sources(&assumption.sources)?;
    }
    for risk in &content.risks {
        check_id(&risk.id, &mut ids)?;
        validate_text("risk.text", &risk.text, 4_000, true)?;
        validate_sources(&risk.sources)?;
    }

    Ok(())
}

fn validate_named_items<'a>(
    kind: &'static str,
    items: impl Iterator<
        Item = (
            &'a BlueprintEntityId,
            &'a str,
            &'a str,
            &'a [SourceReference],
        ),
    >,
    ids: &mut HashSet<BlueprintEntityId>,
    names_by_kind: &mut HashMap<&'static str, HashSet<String>>,
) -> Result<(), BlueprintValidationError> {
    for (id, name, description, sources) in items {
        check_id(id, ids)?;
        validate_name(kind, name)?;
        validate_text(&format!("{kind}.description"), description, 4_000, false)?;
        remember_name(kind, name, names_by_kind)?;
        validate_sources(sources)?;
    }
    Ok(())
}

fn validate_text_items(
    field: &str,
    items: &[TraceableText],
    ids: &mut HashSet<BlueprintEntityId>,
) -> Result<(), BlueprintValidationError> {
    let mut texts = HashSet::new();
    for item in items {
        check_id(&item.id, ids)?;
        validate_text(field, &item.text, 4_000, true)?;
        if !texts.insert(normalize_entity_name(&item.text)) {
            return Err(BlueprintValidationError::DuplicateName {
                kind: field.to_owned(),
                name: item.text.clone(),
            });
        }
        validate_sources(&item.sources)?;
    }
    Ok(())
}

fn check_id(
    id: &BlueprintEntityId,
    ids: &mut HashSet<BlueprintEntityId>,
) -> Result<(), BlueprintValidationError> {
    if id.as_str().trim().is_empty() {
        return Err(BlueprintValidationError::EmptyEntityId {
            field: "id".to_owned(),
        });
    }
    if !ids.insert(id.clone()) {
        return Err(BlueprintValidationError::DuplicateId(id.clone()));
    }
    Ok(())
}

fn validate_name(kind: &str, value: &str) -> Result<(), BlueprintValidationError> {
    validate_text(&format!("{kind}.name"), value, 120, true)
}

fn validate_text(
    field: &str,
    value: &str,
    max: usize,
    required: bool,
) -> Result<(), BlueprintValidationError> {
    if required && value.trim().is_empty() {
        return Err(BlueprintValidationError::EmptyText {
            field: field.to_owned(),
        });
    }
    if value.chars().count() > max {
        return Err(BlueprintValidationError::TextTooLong {
            field: field.to_owned(),
            max,
        });
    }
    Ok(())
}

fn remember_name(
    kind: &'static str,
    value: &str,
    names_by_kind: &mut HashMap<&'static str, HashSet<String>>,
) -> Result<(), BlueprintValidationError> {
    if !names_by_kind
        .entry(kind)
        .or_default()
        .insert(normalize_entity_name(value))
    {
        return Err(BlueprintValidationError::DuplicateName {
            kind: kind.to_owned(),
            name: value.to_owned(),
        });
    }
    Ok(())
}

fn validate_sources(sources: &[SourceReference]) -> Result<(), BlueprintValidationError> {
    for source in sources {
        if source.anchor_id.trim().is_empty() {
            return Err(BlueprintValidationError::EmptyText {
                field: "source.anchorId".to_owned(),
            });
        }
        if let Some(quote) = &source.quote
            && quote.chars().count() > 500
        {
            return Err(BlueprintValidationError::SourceQuoteTooLong {
                artifact_id: source.artifact_id.to_string(),
                anchor_id: source.anchor_id.clone(),
                max: 500,
            });
        }
    }
    Ok(())
}

fn validate_module_cycles(modules: &[ModuleDefinition]) -> Result<(), BlueprintValidationError> {
    let module_names = modules
        .iter()
        .map(|module| (module.id.clone(), module.name.clone()))
        .collect::<HashMap<_, _>>();
    let adjacency = modules
        .iter()
        .map(|module| (module.id.clone(), module.depends_on.clone()))
        .collect::<HashMap<_, _>>();
    let mut state = HashMap::<BlueprintEntityId, u8>::new();
    let mut stack = Vec::<BlueprintEntityId>::new();

    for module in modules {
        visit_module(
            &module.id,
            &adjacency,
            &module_names,
            &mut state,
            &mut stack,
        )?;
    }
    Ok(())
}

fn visit_module(
    id: &BlueprintEntityId,
    adjacency: &HashMap<BlueprintEntityId, Vec<BlueprintEntityId>>,
    names: &HashMap<BlueprintEntityId, String>,
    state: &mut HashMap<BlueprintEntityId, u8>,
    stack: &mut Vec<BlueprintEntityId>,
) -> Result<(), BlueprintValidationError> {
    match state.get(id) {
        Some(1) => {
            let start = stack.iter().position(|item| item == id).unwrap_or(0);
            let mut modules = stack[start..]
                .iter()
                .map(|item| names.get(item).cloned().unwrap_or_else(|| item.to_string()))
                .collect::<Vec<_>>();
            modules.push(names.get(id).cloned().unwrap_or_else(|| id.to_string()));
            return Err(BlueprintValidationError::ModuleCycle { modules });
        }
        Some(2) => return Ok(()),
        _ => {}
    }

    state.insert(id.clone(), 1);
    stack.push(id.clone());
    if let Some(dependencies) = adjacency.get(id) {
        for dependency in dependencies {
            visit_module(dependency, adjacency, names, state, stack)?;
        }
    }
    stack.pop();
    state.insert(id.clone(), 2);
    Ok(())
}
