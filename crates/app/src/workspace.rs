use std::collections::BTreeMap;

use coldstart_core::{
    ArtifactMetadata, BlueprintRevision, DocumentAnchor, InfoRequest, ProjectFact,
};
use serde::{Deserialize, Serialize};

use crate::ProjectStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorkspace {
    pub status: ProjectStatus,
    pub sources: Vec<ArtifactMetadata>,
    pub source_anchors: BTreeMap<String, Vec<DocumentAnchor>>,
    pub draft: Option<BlueprintRevision>,
    pub published: Option<BlueprintRevision>,
    pub inbox: Vec<InfoRequest>,
    pub facts: Vec<ProjectFact>,
}
