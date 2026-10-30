use serde::{Deserialize, Serialize};

use crate::ArtifactId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReference {
    pub artifact_id: ArtifactId,
    pub anchor_id: String,
    pub quote: Option<String>,
}
