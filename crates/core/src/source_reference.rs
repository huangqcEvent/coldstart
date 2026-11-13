//! 来源锚点引用。
//!
//! 蓝图和计划只能引用已导入的来源片段；锚点由制品、章节或页面位置组成，
//! 用于把模型输出追溯回人工提供的真实材料。

use serde::{Deserialize, Serialize};

use crate::ArtifactId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReference {
    pub artifact_id: ArtifactId,
    pub anchor_id: String,
    pub quote: Option<String>,
}
