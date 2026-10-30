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

export type PlanStatus = 'draft' | 'published' | 'superseded';
export type TaskKind =
  | 'agent_coding'
  | 'human_external'
  | 'approval_gate'
  | 'milestone';
export type AcceptancePolicy = 'agent_report' | 'human_acceptance';
export type WorkItemStatus =
  | 'draft'
  | 'ready'
  | 'claimed'
  | 'in_progress'
  | 'submitted'
  | 'done'
  | 'blocked'
  | 'failed'
  | 'reopened'
  | 'cancelled';
export type ContractState = 'current' | 'stale';
export type DependencyKind = 'blocks' | 'informs' | 'conflicts_with';
export type LeaseStatus = 'active' | 'released' | 'expired' | 'consumed';
export type WorkRunStatus =
  | 'claimed'
  | 'running'
  | 'submitted'
  | 'completed'
  | 'failed'
  | 'expired'
  | 'released'
  | 'blocked';
export type ReportOutcome = 'completed' | 'failed' | 'blocked';

export interface TaskDependency {
  taskKey: string;
  kind: DependencyKind;
}

export interface AcceptanceCriterion {
  id: string;
  text: string;
  requiredEvidence: boolean;
}

export interface TaskSpec {
  taskKey: string;
  title: string;
  kind: TaskKind;
  goal: string;
  scope: string[];
  outOfScope: string[];
  moduleId: string | null;
  platformId: string | null;
  repositoryKey: string | null;
  baseRef: string | null;
  workBranch: string | null;
  allowedPaths: string[];
  contextRefs: string[];
  acceptanceCriteria: AcceptanceCriterion[];
  acceptancePolicy: AcceptancePolicy;
  evidenceRequirements: string[];
  priority: number;
  criticalPath: boolean;
  contextBudgetTokens: number;
  resourceKeys: string[];
  dependencies: TaskDependency[];
}

export interface PlanContent {
  tasks: TaskSpec[];
  warnings: string[];
}

export interface PlanRevision {
  id: string;
  projectId: string;
  revisionNumber: number;
  blueprintRevisionId: string;
  blueprintContentHash: string;
  templateVersion: string;
  status: PlanStatus;
  content: PlanContent;
  contentHash: string;
  createdAt: string;
  publishedAt: string | null;
}

export interface WorkItem {
  id: string;
  projectId: string;
  planRevisionId: string;
  taskSpecId: string;
  taskKey: string;
  status: WorkItemStatus;
  contractState: ContractState;
  priority: number;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
}

export interface LeaseSummary {
  id: string;
  workItemId: string;
  agentProfile: string;
  status: LeaseStatus;
  issuedAt: string;
  expiresAt: string;
  lastHeartbeatAt: string;
  releasedAt: string | null;
}

export interface WorkRun {
  id: string;
  workItemId: string;
  leaseId: string;
  agentProfile: string;
  claimRequestId: string;
  status: WorkRunStatus;
  startedAt: string;
  endedAt: string | null;
}

export interface AcceptanceResult {
  criterionId: string;
  passed: boolean;
  note: string;
  evidenceRefs: string[];
}

export interface CommandRun {
  command: string;
  exitCode: number | null;
  summary: string;
}

export interface WorkReportInput {
  status: ReportOutcome;
  summary: string;
  branch: string | null;
  commitSha: string | null;
  changedFiles: string[];
  acceptanceResults: AcceptanceResult[];
  commandsRun: CommandRun[];
  evidenceRefs: string[];
  knownIssues: string[];
  followUpItems: string[];
  contractHash: string;
  idempotencyKey: string;
}

export interface WorkReportRecord {
  id: string;
  workItemId: string;
  workRunId: string;
  contractHash: string;
  report: WorkReportInput;
  reportHash: string;
  accepted: boolean;
  rejectionReasons: string[];
  createdAt: string;
}

export interface WorkBlockerRecord {
  workItemId: string;
  reason: string;
  neededInput: string[];
  proposal: string | null;
  createdAt: string;
}

export interface WorkItemView {
  workItem: WorkItem;
  task: TaskSpec;
  activeLease: LeaseSummary | null;
  latestReport: WorkReportRecord | null;
}

export interface McpConnectionInfo {
  command: string;
  projectHome: string;
  configJson: string;
}

export interface PlanWorkspace {
  draft: PlanRevision | null;
  published: PlanRevision | null;
  workItems: WorkItemView[];
  leases: LeaseSummary[];
  runs: WorkRun[];
  reports: WorkReportRecord[];
  blockers: WorkBlockerRecord[];
  mcp: McpConnectionInfo;
}

export interface ProjectWorkspace {
  status: ProjectStatus;
  sources: ArtifactMetadata[];
  sourceAnchors: Record<string, DocumentAnchor[]>;
  draft: BlueprintRevision | null;
  published: BlueprintRevision | null;
  inbox: InfoRequest[];
  facts: ProjectFact[];
  plan: PlanWorkspace;
}

export interface ModelConfig {
  baseUrl: string;
  model: string;
  apiKey: string;
}
