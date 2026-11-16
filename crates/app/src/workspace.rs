//! 项目工作区聚合视图。
//!
//! 把需求、蓝图、计划、仓库、能力、Review 和历史状态组合成桌面端可渲染的只读
//! 快照；不包含 Secret、租约令牌或未脱敏的外部响应。

use std::collections::BTreeMap;

use coldstart_core::{
    ArtifactImage, ArtifactMetadata, BlueprintRevision, DocumentAnchor, InfoRequest, ProjectFact,
};
use serde::{Deserialize, Serialize};

use crate::{PlanWorkspace, ProjectStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorkspace {
    pub status: ProjectStatus,
    pub sources: Vec<ArtifactMetadata>,
    pub source_anchors: BTreeMap<String, Vec<DocumentAnchor>>,
    pub source_images: BTreeMap<String, Vec<ArtifactImage>>,
    pub draft: Option<BlueprintRevision>,
    pub published: Option<BlueprintRevision>,
    pub inbox: Vec<InfoRequest>,
    pub facts: Vec<ProjectFact>,
    pub plan: PlanWorkspace,
}
