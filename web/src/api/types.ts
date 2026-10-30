export interface ProjectStatus {
  projectId: string;
  name: string;
  rootPath: string;
  createdAt: string;
  eventCount: number;
}

export interface CommandError {
  code: string;
  message: string;
}

export type ArtifactFormat = 'markdown' | 'text' | 'docx' | 'pdf';

export interface ArtifactMetadata {
  id: string;
  projectId: string;
  fileName: string;
  format: ArtifactFormat;
  contentHash: string;
  textHash: string;
  byteSize: number;
  importSource: string;
  importedAt: string;
}

export interface DocumentAnchor {
  id: string;
  label: string;
  text: string;
}

export interface SourceReference {
  artifactId: string;
  anchorId: string;
  quote: string | null;
}

export interface TraceableText {
  id: string;
  text: string;
  sources: SourceReference[];
}

export interface RoleDefinition {
  id: string;
  name: string;
  description: string;
  sources: SourceReference[];
}

export type PlatformKind =
  | 'web'
  | 'server'
  | 'admin'
  | 'ios'
  | 'android'
  | 'mini_program';

export interface PlatformDefinition {
  id: string;
  name: string;
  kind: PlatformKind;
  description: string;
  sources: SourceReference[];
}

export interface ModuleDefinition {
  id: string;
  name: string;
  description: string;
  platforms: string[];
  dependsOn: string[];
  sources: SourceReference[];
}

export interface TechnologyChoice {
  id: string;
  category: string;
  name: string;
  reason: string;
  sources: SourceReference[];
}

export interface CapabilityObligation {
  id: string;
  name: string;
  provider: string;
  description: string;
  sources: SourceReference[];
}

export interface ReleasePrerequisite {
  id: string;
  name: string;
  category: string;
  description: string;
  leadTimeDays: number | null;
  blocking: boolean;
  sources: SourceReference[];
}

export interface Assumption {
  id: string;
  text: string;
  sources: SourceReference[];
}

export type RiskSeverity = 'low' | 'medium' | 'high';

export interface Risk {
  id: string;
  text: string;
  severity: RiskSeverity;
  sources: SourceReference[];
}

export interface BlueprintContent {
  productGoal: TraceableText;
  inScope: TraceableText[];
  outOfScope: TraceableText[];
  roles: RoleDefinition[];
  platforms: PlatformDefinition[];
  modules: ModuleDefinition[];
  technologies: TechnologyChoice[];
  capabilities: CapabilityObligation[];
  releasePrerequisites: ReleasePrerequisite[];
  assumptions: Assumption[];
  risks: Risk[];
}

export type BlueprintStatus = 'draft' | 'published' | 'superseded';
export type GenerationSource = 'model' | 'manual';

export interface BlueprintGeneration {
  source: GenerationSource;
  modelProvider: string | null;
  modelName: string | null;
  promptVersion: string | null;
}

export interface BlueprintRevision {
  id: string;
  projectId: string;
  revisionNumber: number;
  status: BlueprintStatus;
  content: BlueprintContent;
  contentHash: string;
  sourceArtifactIds: string[];
  generation: BlueprintGeneration;
  createdAt: string;
  publishedAt: string | null;
}

export type InfoRequestCategory =
  | 'information'
  | 'decision'
  | 'authorization'
  | 'approval'
  | 'external_operation';

export type InfoRequestStatus = 'open' | 'answered' | 'dismissed';

export interface InfoRequest {
  id: string;
  projectId: string;
  blueprintRevisionId: string;
  fingerprint: string;
  category: InfoRequestCategory;
  question: string;
  reason: string;
  example: string | null;
  defaultValue: string | null;
  impactScope: string;
  dueAt: string | null;
  blocking: boolean;
  status: InfoRequestStatus;
  answer: string | null;
  resolvedAt: string | null;
  createdAt: string;
}

export interface ProjectFact {
  id: string;
  projectId: string;
  factKey: string;
  category: InfoRequestCategory;
  question: string;
  value: string;
  version: number;
  sourceInfoRequestId: string;
  createdAt: string;
}

export interface ProjectWorkspace {
  status: ProjectStatus;
  sources: ArtifactMetadata[];
  sourceAnchors: Record<string, DocumentAnchor[]>;
  draft: BlueprintRevision | null;
  published: BlueprintRevision | null;
  inbox: InfoRequest[];
  facts: ProjectFact[];
}

export interface ModelConfig {
  baseUrl: string;
  model: string;
  apiKey: string;
}
