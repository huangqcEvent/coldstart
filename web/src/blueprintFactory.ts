/**
 * 手工蓝图草稿工厂。
 *
 * 从现有蓝图或来源引用生成可编辑草稿，并保持 ID、锚点和默认值稳定；这里不负责
 * 发布或校验，最终结构仍由 Rust 领域层验证。
 */
import type {
  ArtifactMetadata,
  BlueprintContent,
  DocumentAnchor,
  SourceReference,
} from './api/types';

let entitySequence = 0;

export function createEntityId(kind: string) {
  entitySequence += 1;
  return `${kind}_${Date.now().toString(36)}${entitySequence.toString(36)}`;
}

/** 返回蓝图中的首个来源引用，供手工编辑初始化锚点。 */
export function firstSourceReference(
  sources: ArtifactMetadata[],
  sourceAnchors: Record<string, DocumentAnchor[]>,
): SourceReference | null {
  for (const source of sources) {
    const anchor = sourceAnchors[source.id]?.[0];
    if (anchor) {
      return {
        artifactId: source.id,
        anchorId: anchor.id,
        quote: null,
      };
    }
  }
  return null;
}

export function canCreateManualBlueprint(
  sources: ArtifactMetadata[],
  sourceAnchors: Record<string, DocumentAnchor[]>,
) {
  return firstSourceReference(sources, sourceAnchors) !== null;
}

export function createManualBlueprint(
  sources: ArtifactMetadata[],
  sourceAnchors: Record<string, DocumentAnchor[]>,
): BlueprintContent {
  const source = firstSourceReference(sources, sourceAnchors);
  const sourcesForInitialItem = source ? [source] : [];
  const platformId = createEntityId('platform');

  return {
    productGoal: {
      id: createEntityId('goal'),
      text: '待补充产品目标',
      sources: sourcesForInitialItem,
    },
    inScope: [],
    outOfScope: [],
    roles: [],
    platforms: [
      {
        id: platformId,
        name: 'Web',
        kind: 'web',
        description: '',
        sources: sourcesForInitialItem,
      },
    ],
    modules: [
      {
        id: createEntityId('module'),
        name: '核心业务',
        description: '',
        platforms: [platformId],
        dependsOn: [],
        sources: sourcesForInitialItem,
      },
    ],
    technologies: [],
    capabilities: [],
    releasePrerequisites: [],
    assumptions: [],
    risks: [],
  };
}

export function cloneBlueprint(content: BlueprintContent): BlueprintContent {
  return JSON.parse(JSON.stringify(content)) as BlueprintContent;
}
