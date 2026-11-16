//! 任务级需求图片授权与安全投影。
//!
//! 图片不会自动进入 Agent 上下文。只有当前任务契约关联的模块、平台或上下文实体
//! 所引用的来源锚点，才能出现在图片清单中；清单不包含本地路径或图片二进制。

use std::collections::{BTreeMap, BTreeSet};

use coldstart_core::{ArtifactId, BlueprintContent, BlueprintEntityId, SourceReference, TaskSpec};
use serde::{Deserialize, Serialize};

use crate::{AppError, context::OpenProject};

/// 可安全返回给 Agent 或界面的图片摘要。
///
/// 该结构只携带溯源信息和完整性元数据，不包含制品目录、绝对路径或文件内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskImageSummary {
    pub artifact_id: ArtifactId,
    pub artifact_file_name: String,
    pub image_id: String,
    pub anchor_id: String,
    pub anchor_label: String,
    pub file_name: String,
    pub mime_type: String,
    pub byte_size: u64,
    pub sha256: String,
}

/// 已通过任务授权的图片原文，仅在显式读取时返回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskImageContent {
    pub image: TaskImageSummary,
    pub bytes: Vec<u8>,
}

/// 收集任务关联蓝图实体所引用的图片。
pub(crate) fn collect_task_images(
    project: &OpenProject,
    blueprint: &BlueprintContent,
    task: &TaskSpec,
) -> Result<Vec<TaskImageSummary>, AppError> {
    let selected = selected_entity_ids(task);
    let mut references = Vec::new();
    collect_entity_sources(
        &selected,
        &mut references,
        std::iter::once((
            &blueprint.product_goal.id,
            blueprint.product_goal.sources.as_slice(),
        )),
    );
    collect_entity_sources(
        &selected,
        &mut references,
        blueprint
            .in_scope
            .iter()
            .chain(&blueprint.out_of_scope)
            .map(|item| (&item.id, item.sources.as_slice())),
    );
    collect_entity_sources(
        &selected,
        &mut references,
        blueprint
            .roles
            .iter()
            .map(|item| (&item.id, item.sources.as_slice())),
    );
    collect_entity_sources(
        &selected,
        &mut references,
        blueprint
            .platforms
            .iter()
            .map(|item| (&item.id, item.sources.as_slice())),
    );
    collect_entity_sources(
        &selected,
        &mut references,
        blueprint
            .modules
            .iter()
            .map(|item| (&item.id, item.sources.as_slice())),
    );
    collect_entity_sources(
        &selected,
        &mut references,
        blueprint
            .technologies
            .iter()
            .map(|item| (&item.id, item.sources.as_slice())),
    );
    collect_entity_sources(
        &selected,
        &mut references,
        blueprint
            .capabilities
            .iter()
            .map(|item| (&item.id, item.sources.as_slice())),
    );
    collect_entity_sources(
        &selected,
        &mut references,
        blueprint
            .release_prerequisites
            .iter()
            .map(|item| (&item.id, item.sources.as_slice())),
    );
    collect_entity_sources(
        &selected,
        &mut references,
        blueprint
            .assumptions
            .iter()
            .map(|item| (&item.id, item.sources.as_slice())),
    );
    collect_entity_sources(
        &selected,
        &mut references,
        blueprint
            .risks
            .iter()
            .map(|item| (&item.id, item.sources.as_slice())),
    );

    let mut anchors_by_artifact = BTreeMap::<ArtifactId, BTreeSet<String>>::new();
    let mut seen_references = BTreeSet::<(ArtifactId, String)>::new();
    for reference in references {
        if seen_references.insert((reference.artifact_id, reference.anchor_id.clone())) {
            anchors_by_artifact
                .entry(reference.artifact_id)
                .or_default()
                .insert(reference.anchor_id);
        }
    }
    if anchors_by_artifact.is_empty() {
        return Ok(Vec::new());
    }

    let artifacts = project
        .database
        .list_artifacts(&project.project.id)?
        .into_iter()
        .map(|artifact| (artifact.id, artifact))
        .collect::<BTreeMap<_, _>>();
    let mut images = Vec::new();
    let mut seen_images = BTreeSet::<(ArtifactId, String)>::new();
    for (artifact_id, anchor_ids) in anchors_by_artifact {
        let Some(artifact) = artifacts.get(&artifact_id) else {
            continue;
        };
        let anchor_labels = project
            .database
            .load_artifact_anchors(&artifact_id)?
            .into_iter()
            .map(|anchor| (anchor.id, anchor.label))
            .collect::<BTreeMap<_, _>>();
        for image in project.database.load_artifact_images(&artifact_id)? {
            if !anchor_ids.contains(&image.anchor_id)
                || !seen_images.insert((artifact_id, image.id.clone()))
            {
                continue;
            }
            images.push(TaskImageSummary {
                artifact_id,
                artifact_file_name: artifact.file_name.clone(),
                image_id: image.id,
                anchor_label: anchor_labels
                    .get(&image.anchor_id)
                    .cloned()
                    .unwrap_or_else(|| image.anchor_id.clone()),
                anchor_id: image.anchor_id,
                file_name: image.file_name,
                mime_type: image.mime_type,
                byte_size: image.byte_size,
                sha256: image.sha256,
            });
        }
    }
    images.sort_by(|left, right| {
        left.artifact_file_name
            .cmp(&right.artifact_file_name)
            .then_with(|| left.anchor_label.cmp(&right.anchor_label))
            .then_with(|| left.image_id.cmp(&right.image_id))
    });
    Ok(images)
}

fn selected_entity_ids(task: &TaskSpec) -> BTreeSet<BlueprintEntityId> {
    let mut selected = task.context_refs.iter().cloned().collect::<BTreeSet<_>>();
    selected.extend(task.module_id.iter().cloned());
    selected.extend(task.platform_id.iter().cloned());
    selected
}

fn collect_entity_sources<'a>(
    selected: &BTreeSet<BlueprintEntityId>,
    references: &mut Vec<SourceReference>,
    entities: impl Iterator<Item = (&'a BlueprintEntityId, &'a [SourceReference])>,
) {
    for (entity_id, sources) in entities {
        if selected.contains(entity_id) {
            references.extend_from_slice(sources);
        }
    }
}
