/**
 * Web 端与 Tauri 命令共享的 TypeScript 类型。
 *
 * 类型字段与 Rust serde DTO 保持 camelCase 契约；敏感值只允许以引用或状态出现，
 * 不应在这里新增明文 Secret、Token 或租约令牌字段。
 */
export interface ProjectStatus {
  projectId: string;
  name: string;
  rootPath: string;
  createdAt: string;
  eventCount: number;
}

/** 应用级最近项目；`available` 表示项目清单与数据库当前仍可打开。 */
export interface RecentProjectView {
  rootPath: string;
  name: string;
  lastOpenedAt: string;
  available: boolean;
}

/** 应用数据目录中的最近项目历史，不包含项目业务数据或凭据。 */
export interface ProjectHistoryView {
  schemaVersion: number;
  lastProjectPath: string | null;
  projects: RecentProjectView[];
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

/** 来源文档中的内嵌图片；二进制只在显式预览时按需读取。 */
export interface ArtifactImage {
  id: string;
  anchorId: string;
  fileName: string;
  mimeType: string;
  byteSize: number;
  sha256: string;
  relativePath: string;
}

/** 桌面端按需读取的图片内容和安全溯源信息。 */
export interface RequirementImagePreview {
  image: ArtifactImage;
  sourceFileName: string;
  anchorLabel: string;
  dataBase64: string;
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

export type CodeInvestigationStatus =
  | 'requested'
  | 'claimed'
  | 'submitted'
  | 'confirmed'
  | 'rejected';

export interface CodeFinding {
  path: string;
  lineStart: number;
  lineEnd: number;
  symbol: string | null;
  summary: string;
  risk: string;
  suggestedChangePaths: string[];
}

export interface CodeInvestigation {
  id: string;
  projectId: string;
  repositoryId: string;
  repositoryKey: string;
  baseRef: string;
  question: string;
  status: CodeInvestigationStatus;
  agentProfile: string | null;
  leaseExpiresAt: string | null;
  commitSha: string | null;
  summary: string | null;
  findings: CodeFinding[];
  limitations: string[];
  reportHash: string | null;
  rejectionReason: string | null;
  createdAt: string;
  claimedAt: string | null;
  submittedAt: string | null;
  confirmedAt: string | null;
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

export interface PlanTaskDiff {
  taskKey: string;
  title: string;
  previousStatus: WorkItemStatus | null;
  nextStatus: WorkItemStatus | null;
  contractChanged: boolean;
}

export interface PlanPublishPreview {
  baseRevisionId: string | null;
  baseRevisionNumber: number | null;
  candidateRevisionId: string;
  candidateRevisionNumber: number;
  added: PlanTaskDiff[];
  changed: PlanTaskDiff[];
  unchanged: PlanTaskDiff[];
  removed: PlanTaskDiff[];
  carriedCompleted: number;
  reopened: number;
  cancelled: number;
  activeLeaseCount: number;
  publishable: boolean;
  blockers: string[];
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
  integratedAt: string | null;
  integrationReference: string | null;
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
  verificationStatus: VerificationStatus;
  verification: VerificationResult | null;
  reviewStatus: ReviewStatus;
  review: ReviewResult | null;
  reviewWaiver: ReviewWaiver | null;
  createdAt: string;
}

export type VerificationStatus =
  | 'not_required'
  | 'running'
  | 'passed'
  | 'failed'
  | 'interrupted';

export type VerificationFailureKind =
  | 'non_zero_exit'
  | 'launch_failed'
  | 'timeout'
  | 'git_invariant'
  | 'infrastructure'
  | 'interrupted';

export interface VerificationCommandResult {
  name: string;
  exitCode: number | null;
  durationMs: number;
  outputSummary: string;
  truncated: boolean;
  failureKind: VerificationFailureKind | null;
}

export interface VerificationResult {
  snapshotHash: string;
  status: VerificationStatus;
  startedAt: string;
  finishedAt: string | null;
  totalDurationMs: number | null;
  failureKind: VerificationFailureKind | null;
  failureCommand: string | null;
  commands: VerificationCommandResult[];
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

export type MemoryPhase =
  | 'requirements_blueprint'
  | 'plan_execution'
  | 'verification_review'
  | 'repository_capability'
  | 'system_configuration';

export type MemoryReferenceType =
  | 'artifact'
  | 'blueprint_revision'
  | 'info_request'
  | 'plan_revision'
  | 'work_item'
  | 'work_report'
  | 'repository'
  | 'credential'
  | 'capability'
  | 'git_sync'
  | 'pull_request'
  | 'code_investigation'
  | 'project_setting';

export interface ProjectMemoryReference {
  referenceType: MemoryReferenceType;
  id: string;
  label: string;
}

export interface ProjectMemoryItem {
  eventId: string;
  occurredAt: string;
  phase: MemoryPhase;
  eventType: string;
  title: string;
  summary: string;
  reference: ProjectMemoryReference | null;
  isHeartbeat: boolean;
}

export interface MemoryPhaseCount {
  phase: MemoryPhase;
  count: number;
}

export interface ProjectMemoryPage {
  projectId: string;
  totalCount: number;
  visibleCount: number;
  heartbeatCount: number;
  phaseCounts: MemoryPhaseCount[];
  items: ProjectMemoryItem[];
  nextCursor: number | null;
}

export interface ProjectWorkspace {
  status: ProjectStatus;
  sources: ArtifactMetadata[];
  sourceAnchors: Record<string, DocumentAnchor[]>;
  sourceImages: Record<string, ArtifactImage[]>;
  draft: BlueprintRevision | null;
  published: BlueprintRevision | null;
  inbox: InfoRequest[];
  facts: ProjectFact[];
  codeInvestigations: CodeInvestigation[];
  plan: PlanWorkspace;
}

export interface ModelConfig {
  baseUrl: string;
  model: string;
  apiKey: string;
}

export type RepositoryAuthMode = 'none' | 'credential' | 'ssh_agent';
export type RepositorySyncPolicy = 'manual' | 'fetch_before_claim';
export type CredentialKind =
  | 'api_key'
  | 'oauth_token'
  | 'personal_access_token'
  | 'username_password'
  | 'signing_certificate'
  | 'other';
export type CredentialEnvironment =
  | 'shared'
  | 'development'
  | 'staging'
  | 'production';
export type CapabilityStatus =
  | 'missing'
  | 'configured'
  | 'verified'
  | 'auth_required'
  | 'disabled';
export type GitSyncStatus = 'pushed' | 'failed';

export interface Repository {
  id: string;
  projectId: string;
  key: string;
  name: string;
  localPath: string;
  remoteUrl: string;
  defaultBranch: string;
  protectedBranches: string[];
  authMode: RepositoryAuthMode;
  credentialId: string | null;
  syncPolicy: RepositorySyncPolicy;
  createdAt: string;
  updatedAt: string;
}

export interface RepositoryDraft {
  key: string;
  name: string;
  localPath: string;
  remoteUrl: string;
  defaultBranch: string;
  protectedBranches: string[];
  authMode: RepositoryAuthMode;
  credentialId: string | null;
  syncPolicy: RepositorySyncPolicy;
}

export interface GitRepositoryStatus {
  path: string;
  branch: string | null;
  headSha: string | null;
  clean: boolean;
  changedPaths: string[];
}

export interface RepositoryValidation {
  repositoryKey: string;
  remoteUrl: string;
  status: GitRepositoryStatus;
}

export interface RepositoryVerificationCommand {
  name: string;
  executable: string;
  args: string[];
  timeoutSeconds: number;
  position: number;
}

export interface RepositoryVerificationView {
  repositoryKey: string;
  commands: RepositoryVerificationCommand[];
}

export interface CredentialMetadata {
  id: string;
  projectId: string;
  name: string;
  provider: string;
  kind: CredentialKind;
  environment: CredentialEnvironment;
  secretRef: string;
  createdAt: string;
  updatedAt: string;
}

export interface SaveCredentialInput {
  name: string;
  provider: string;
  kind: CredentialKind;
  environment: CredentialEnvironment;
  secret: string;
}

export interface CapabilityStatusView {
  projectId: string;
  capabilityId: string;
  name: string;
  provider: string;
  environment: string;
  status: CapabilityStatus;
  credentialId: string | null;
  credentialRef: string | null;
  note: string | null;
  updatedAt: string;
}

export type CapabilityApiMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';

export interface CapabilityApiEndpoint {
  id: string;
  projectId: string;
  capabilityId: string;
  environment: string;
  name: string;
  baseUrl: string;
  pathPrefix: string;
  allowedMethods: CapabilityApiMethod[];
  credentialId: string | null;
  authHeader: string | null;
  authScheme: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface CapabilityApiEndpointInput {
  capabilityId: string;
  environment: string;
  name: string;
  baseUrl: string;
  pathPrefix: string;
  allowedMethods: CapabilityApiMethod[];
  credentialId: string | null;
  authHeader: string | null;
  authScheme: string | null;
}

export interface GitSyncRecord {
  id: string;
  projectId: string;
  workItemId: string;
  repositoryId: string;
  branch: string;
  commitSha: string;
  status: GitSyncStatus;
  errorMessage: string | null;
  createdAt: string;
}

export interface ReleaseCenter {
  prerequisites: WorkItemView[];
  milestones: WorkItemView[];
  approvalGates: WorkItemView[];
  humanTasks: WorkItemView[];
}

export interface IntegrationWorkspace {
  projectId: string;
  repositories: Repository[];
  verificationCommands: RepositoryVerificationView[];
  credentials: CredentialMetadata[];
  capabilities: CapabilityStatusView[];
  capabilityApiEndpoints: CapabilityApiEndpoint[];
  gitSyncs: GitSyncRecord[];
  release: ReleaseCenter;
}

export type DeliveryRepositoryState =
  | 'attention'
  | 'active'
  | 'ready'
  | 'complete'
  | 'idle';

export interface DeliveryTaskCounts {
  total: number;
  done: number;
  ready: number;
  active: number;
  blocked: number;
  failed: number;
  submitted: number;
  reopened: number;
  cancelled: number;
  draft: number;
}

export interface DeliveryGitSyncView {
  status: GitSyncStatus;
  branch: string;
  commitSha: string;
  errorMessage: string | null;
  createdAt: string;
}

export interface DeliveryChecksView {
  state: PullRequestChecksState;
  total: number;
  failed: number;
  pending: number;
}

export interface DeliveryPullRequestView {
  number: number;
  title: string;
  htmlUrl: string;
  state: PullRequestState;
  draft: boolean;
  checks: DeliveryChecksView | null;
  mergeability: PullRequestMergeability | null;
  conflictPathCount: number;
  changesRequestedReviewCount: number;
  reviewStatus: ReviewStatus | null;
  syncedAt: string | null;
}

export interface RepositoryDeliveryView {
  repositoryKey: string;
  repositoryName: string;
  remoteUrl: string;
  defaultBranch: string;
  state: DeliveryRepositoryState;
  tasks: DeliveryTaskCounts;
  verificationCommandCount: number;
  latestGitSync: DeliveryGitSyncView | null;
  pullRequest: DeliveryPullRequestView | null;
  nextAction: string;
}

export interface DeliveryOverviewView {
  projectId: string;
  planRevisionNumber: number | null;
  repositories: RepositoryDeliveryView[];
  unassignedAgentTaskCount: number;
}

export type GitHubIssueState = 'open' | 'closed';
export type GitHubIssueListState = 'open' | 'closed' | 'all';

export interface GitHubIssueView {
  id: number;
  number: number;
  title: string;
  state: GitHubIssueState;
  bodyPreview: string;
  labels: string[];
  htmlUrl: string;
  comments: number;
  updatedAt: string;
  sourceReference: string;
}

export interface GitHubIssueListView {
  repositoryKey: string;
  repositoryName: string;
  repositoryLabel: string;
  state: GitHubIssueListState;
  issues: GitHubIssueView[];
  truncated: boolean;
}

export type GitHubProjectItemKind =
  | 'issue'
  | 'pull_request'
  | 'draft_issue'
  | 'unavailable';
export type GitHubProjectItemState = 'open' | 'closed' | 'merged';

export interface GitHubProjectFieldView {
  name: string;
  value: string;
}

export interface GitHubProjectItemView {
  id: string;
  kind: GitHubProjectItemKind;
  number: number | null;
  title: string;
  state: GitHubProjectItemState | null;
  htmlUrl: string | null;
  updatedAt: string | null;
  fields: GitHubProjectFieldView[];
}

export interface GitHubProjectView {
  id: string;
  number: number;
  title: string;
  htmlUrl: string;
  closed: boolean;
  updatedAt: string;
  totalItems: number;
  items: GitHubProjectItemView[];
  truncated: boolean;
}

export interface GitHubProjectOverviewView {
  repositoryKey: string;
  repositoryName: string;
  repositoryLabel: string;
  projects: GitHubProjectView[];
  truncated: boolean;
}

export type ReviewSeverity = 'critical' | 'high' | 'medium' | 'low';

export type ReviewCategory =
  | 'security'
  | 'correctness'
  | 'tests'
  | 'maintainability'
  | 'style'
  | 'performance'
  | 'other';

export type ReviewScope =
  | { kind: 'project' }
  | { kind: 'repository'; repositoryKey: string };

export type ReviewStandardSource = 'upload' | 'manual';

export type ReviewStatus =
  | 'not_required'
  | 'running'
  | 'passed'
  | 'blocked'
  | 'failed'
  | 'waived'
  | 'interrupted';

export interface ReviewConfigurationInput {
  enabled: boolean;
  baseUrl: string;
  model: string;
  credentialId: string | null;
  blockingSeverities: ReviewSeverity[];
  consentConfirmed: boolean;
}

export interface ReviewConfiguration {
  projectId: string;
  enabled: boolean;
  baseUrl: string;
  model: string;
  credentialId: string | null;
  blockingSeverities: ReviewSeverity[];
  consentFingerprint: string | null;
  updatedAt: string;
}

export interface ReviewStandardView {
  id: string;
  name: string;
  scope: ReviewScope;
  enabled: boolean;
  position: number;
  currentRevisionId: string;
  revisionNumber: number;
  source: ReviewStandardSource;
  fileName: string;
  format: ArtifactFormat;
  characterCount: number;
  contentHash: string;
  createdAt: string;
  updatedAt: string;
}

export interface ReviewStandardContent {
  standard: ReviewStandardView;
  revisionId: string;
  text: string;
}

export interface ReviewStandardImport {
  name: string;
  scope: ReviewScope;
  path: string;
}

export interface ReviewStandardManualInput {
  name: string;
  scope: ReviewScope;
  text: string;
}

export interface ReviewStandardRevisionInput {
  name: string;
  scope: ReviewScope;
  text: string;
}

export interface ReviewFinding {
  severity: ReviewSeverity;
  category: ReviewCategory;
  file: string;
  line: number | null;
  message: string;
  recommendation: string;
  standard: string | null;
}

export interface ReviewFailure {
  code: string;
  message: string;
}

export interface ReviewResult {
  snapshotHash: string;
  status: ReviewStatus;
  summary: string;
  findings: ReviewFinding[];
  model: string;
  failure: ReviewFailure | null;
  startedAt: string;
  finishedAt: string | null;
  durationMs: number | null;
}

export interface ReviewWaiver {
  reason: string;
  waivedBy: string;
  waivedAt: string;
  findingsHash: string;
  standardsHash: string;
}

export type PullRequestState = 'open' | 'closed' | 'merged';

export type PullRequestCheckStatus =
  | 'queued'
  | 'in_progress'
  | 'completed'
  | 'waiting'
  | 'requested'
  | 'pending'
  | 'unknown';

export type PullRequestCheckConclusion =
  | 'success'
  | 'failure'
  | 'neutral'
  | 'cancelled'
  | 'skipped'
  | 'timed_out'
  | 'action_required'
  | 'startup_failure'
  | 'stale'
  | 'unknown';

export type PullRequestChecksState = 'none' | 'pending' | 'success' | 'failure';

export type PullRequestMergeability =
  | 'mergeable'
  | 'conflicted'
  | 'blocked'
  | 'behind'
  | 'unstable'
  | 'draft'
  | 'checking'
  | 'unknown';

export type PullRequestConflictState =
  | 'not_checked'
  | 'not_applicable'
  | 'detected'
  | 'not_reproduced'
  | 'unavailable';

export interface PullRequestConflictDetails {
  state: PullRequestConflictState;
  paths: string[];
  truncated: boolean;
}

export type PullRequestDeploymentState =
  | 'success'
  | 'failure'
  | 'error'
  | 'inactive'
  | 'in_progress'
  | 'queued'
  | 'pending'
  | 'unknown';

export type PullRequestDeploymentsState =
  | 'not_checked'
  | 'no_deployments'
  | 'available'
  | 'unavailable';

export interface PullRequestDeployment {
  environment: string;
  state: PullRequestDeploymentState;
  description: string | null;
  statusUpdatedAt: string | null;
}

export interface PullRequestDeploymentDetails {
  state: PullRequestDeploymentsState;
  deployments: PullRequestDeployment[];
  truncated: boolean;
}

export type PullRequestReviewState =
  | 'approved'
  | 'changes_requested'
  | 'commented'
  | 'dismissed'
  | 'pending'
  | 'unknown';

export interface PullRequestCheck {
  name: string;
  status: PullRequestCheckStatus;
  conclusion: PullRequestCheckConclusion | null;
  detailsUrl: string | null;
  appName: string | null;
}

export interface PullRequestReview {
  author: string;
  state: PullRequestReviewState;
  body: string;
  htmlUrl: string | null;
}

export interface PullRequestReviewComment {
  author: string;
  path: string;
  line: number | null;
  body: string;
  htmlUrl: string | null;
}

export interface PullRequestInsights {
  pullRequestId: string;
  mergeability: PullRequestMergeability;
  conflictDetails: PullRequestConflictDetails;
  deploymentDetails: PullRequestDeploymentDetails;
  checks: PullRequestCheck[];
  reviews: PullRequestReview[];
  reviewComments: PullRequestReviewComment[];
  syncedAt: string;
}

export interface PullRequestRecord {
  id: string;
  projectId: string;
  workItemId: string;
  reportId: string;
  repositoryId: string;
  number: number;
  htmlUrl: string;
  title: string;
  headBranch: string;
  baseBranch: string;
  state: PullRequestState;
  draft: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface ReviewHistoryView {
  reportId: string;
  workItemId: string;
  taskKey: string;
  reviewStatus: ReviewStatus;
  review: ReviewResult | null;
  waiver: ReviewWaiver | null;
  pullRequest: PullRequestRecord | null;
  pullRequestInsights: PullRequestInsights | null;
  createdAt: string;
}

export interface ReviewWorkspace {
  projectId: string;
  configuration: ReviewConfiguration | null;
  standards: ReviewStandardView[];
  recentReviews: ReviewHistoryView[];
}

export interface GitHubSession {
  credentialId: string;
  credentialRef: string;
  login: string;
  name: string | null;
  avatarUrl: string | null;
  scopes: string[];
}

export interface GitHubDeviceFlow {
  sessionId: string;
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
}

export type GitHubDevicePoll =
  | { state: 'pending'; sessionId: string; interval: number }
  | { state: 'slow_down'; sessionId: string; interval: number }
  | { state: 'authorized'; session: GitHubSession }
  | { state: 'access_denied' }
  | { state: 'expired' };

export interface CreatePrivateRepositoryInput {
  credentialId: string;
  repositoryKey: string;
  name: string;
  description: string | null;
}

export interface GitHubRepository {
  id: number;
  name: string;
  fullName: string;
  private: boolean;
  cloneUrl: string;
  sshUrl: string;
  htmlUrl: string;
  defaultBranch: string;
}
