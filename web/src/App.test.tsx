import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import App from './App';
import * as integrationApi from './api/integrationApi';
import * as issueApi from './api/issueApi';
import * as memoryApi from './api/memoryApi';
import * as planApi from './api/planApi';
import * as projectApi from './api/projectApi';
import * as requirementsApi from './api/requirementsApi';
import * as reviewApi from './api/reviewApi';
import type {
  BlueprintContent,
  BlueprintRevision,
  CapabilityStatusView,
  CredentialMetadata,
  DocumentAnchor,
  GitHubIssueListView,
  GitSyncRecord,
  InfoRequest,
  IntegrationWorkspace,
  PlanContent,
  PlanPublishPreview,
  PlanRevision,
  PlanWorkspace,
  ProjectMemoryItem,
  ProjectMemoryPage,
  ProjectStatus,
  ProjectWorkspace,
  PullRequestInsights,
  PullRequestRecord,
  Repository,
  RepositoryVerificationCommand,
  ReviewConfiguration,
  ReviewHistoryView,
  ReviewStandardView,
  ReviewWorkspace,
  TaskSpec,
  WorkItemView,
  WorkReportRecord,
} from './api/types';

vi.mock('./api/projectApi', () => ({
  createProject: vi.fn(),
  readProjectStatus: vi.fn(),
  readProjectWorkspace: vi.fn(),
  pickProjectHome: vi.fn(),
}));

vi.mock('./api/requirementsApi', () => ({
  importRequirementDocument: vi.fn(),
  generateBlueprint: vi.fn(),
  saveBlueprintDraft: vi.fn(),
  publishBlueprint: vi.fn(),
  resolveInfoRequest: vi.fn(),
  pickRequirementFile: vi.fn(),
}));

vi.mock('./api/planApi', () => ({
  readPlanWorkspace: vi.fn(),
  generatePlanDraft: vi.fn(),
  savePlanDraft: vi.fn(),
  previewPlanPublish: vi.fn(),
  publishPlan: vi.fn(),
  completeHumanWork: vi.fn(),
}));

vi.mock('./api/integrationApi', () => ({
  readIntegrationWorkspace: vi.fn(),
  validateRepository: vi.fn(),
  saveRepository: vi.fn(),
  readRepositoryVerificationCommands: vi.fn(),
  saveRepositoryVerificationCommands: vi.fn(),
  saveCredential: vi.fn(),
  deleteCredential: vi.fn(),
  updateCapability: vi.fn(),
  importGitHubSession: vi.fn(),
  startGitHubDeviceFlow: vi.fn(),
  pollGitHubDeviceFlow: vi.fn(),
  createGitHubPrivateRepository: vi.fn(),
}));

vi.mock('./api/issueApi', () => ({
  listGitHubIssues: vi.fn(),
  importGitHubIssue: vi.fn(),
}));

vi.mock('./api/reviewApi', () => ({
  readReviewWorkspace: vi.fn(),
  saveReviewConfiguration: vi.fn(),
  importReviewStandard: vi.fn(),
  createReviewStandard: vi.fn(),
  reviseReviewStandard: vi.fn(),
  setReviewStandardEnabled: vi.fn(),
  reorderReviewStandards: vi.fn(),
  deleteReviewStandard: vi.fn(),
  readReviewStandard: vi.fn(),
  waiveWorkReview: vi.fn(),
  createDraftPullRequest: vi.fn(),
  refreshPullRequest: vi.fn(),
  pickReviewStandardFile: vi.fn(),
}));

vi.mock('./api/memoryApi', () => ({
  readProjectMemory: vi.fn(),
}));

const projectPath = String.raw`D:\coldstart-projects\permafrost`;
const sourceId = '11111111-1111-4111-8111-111111111111';
const anchorId = 'section_1';

const status: ProjectStatus = {
  projectId: 'prj_01JCMQ4W7T3K8Y9F2N6R5D0XAB',
  name: '冻土实验室',
  rootPath: projectPath,
  createdAt: '2026-10-30T08:30:00Z',
  eventCount: 1,
};

const anchor: DocumentAnchor = {
  id: anchorId,
  label: '订单系统',
  text: '需要 Web、管理后台和微信小程序，文件上传使用阿里云 OSS。',
};

const source = {
  id: sourceId,
  projectId: status.projectId,
  fileName: 'requirements.md',
  format: 'markdown' as const,
  contentHash: 'a'.repeat(64),
  textHash: 'b'.repeat(64),
  byteSize: 256,
  importSource: String.raw`D:\docs\requirements.md`,
  importedAt: '2026-10-30T08:35:00Z',
};

const sourceReference = {
  artifactId: sourceId,
  anchorId,
  quote: anchor.text,
};

const content: BlueprintContent = {
  productGoal: {
    id: 'goal_product',
    text: '构建订单系统',
    sources: [sourceReference],
  },
  inScope: [],
  outOfScope: [],
  roles: [],
  platforms: [
    {
      id: 'platform_web',
      name: 'Web',
      kind: 'web',
      description: '前台网站',
      sources: [sourceReference],
    },
  ],
  modules: [
    {
      id: 'module_order',
      name: '订单模块',
      description: '处理订单',
      platforms: ['platform_web'],
      dependsOn: [],
      sources: [sourceReference],
    },
  ],
  technologies: [
    {
      id: 'technology_rust',
      category: '后端',
      name: 'Rust',
      reason: '可靠性高',
      sources: [sourceReference],
    },
  ],
  capabilities: [
    {
      id: 'capability_oss',
      name: '文件上传',
      provider: '阿里云 OSS',
      description: '保存用户文件',
      sources: [sourceReference],
    },
  ],
  releasePrerequisites: [
    {
      id: 'release_mini_program',
      name: '小程序提审',
      category: '小程序',
      description: '提交微信审核',
      leadTimeDays: 3,
      blocking: true,
      sources: [sourceReference],
    },
  ],
  assumptions: [],
  risks: [],
};

function makeRevision(overrides: Partial<BlueprintRevision> = {}): BlueprintRevision {
  return {
    id: '22222222-2222-4222-8222-222222222222',
    projectId: status.projectId,
    revisionNumber: 1,
    status: 'draft',
    content,
    contentHash: 'c'.repeat(64),
    sourceArtifactIds: [sourceId],
    generation: {
      source: 'model',
      modelProvider: 'OpenAI Compatible',
      modelName: 'test-model',
      promptVersion: 'blueprint-v1',
    },
    createdAt: '2026-10-30T09:00:00Z',
    publishedAt: null,
    ...overrides,
  };
}

function makeTask(overrides: Partial<TaskSpec> = {}): TaskSpec {
  return {
    taskKey: 'slice:order:web',
    title: '实现订单 Web 切片',
    kind: 'agent_coding',
    goal: '交付可验收的订单前台纵向切片',
    scope: ['订单创建', '订单查询'],
    outOfScope: ['支付渠道接入'],
    moduleId: 'module_order',
    platformId: 'platform_web',
    repositoryKey: 'web',
    baseRef: 'main',
    workBranch: 'codex/slice-order-web',
    allowedPaths: ['apps/web'],
    contextRefs: [],
    acceptanceCriteria: [
      {
        id: 'implementation',
        text: '订单流程可访问并通过测试',
        requiredEvidence: true,
      },
    ],
    acceptancePolicy: 'agent_report',
    evidenceRequirements: ['测试摘要'],
    priority: 100,
    criticalPath: true,
    contextBudgetTokens: 12_000,
    resourceKeys: ['web'],
    dependencies: [],
    ...overrides,
  };
}

const agentTask = makeTask();

function makePlanContent(overrides: Partial<PlanContent> = {}): PlanContent {
  return {
    tasks: [agentTask],
    warnings: [],
    ...overrides,
  };
}

function makePlanRevision(overrides: Partial<PlanRevision> = {}): PlanRevision {
  return {
    id: '44444444-4444-4444-8444-444444444444',
    projectId: status.projectId,
    revisionNumber: 1,
    blueprintRevisionId: makeRevision().id,
    blueprintContentHash: 'c'.repeat(64),
    templateVersion: 'plan-v1',
    status: 'draft',
    content: makePlanContent(),
    contentHash: 'd'.repeat(64),
    createdAt: '2026-10-30T11:00:00Z',
    publishedAt: null,
    ...overrides,
  };
}

function makePublishPreview(
  overrides: Partial<PlanPublishPreview> = {},
): PlanPublishPreview {
  return {
    baseRevisionId: null,
    baseRevisionNumber: null,
    candidateRevisionId: makePlanRevision().id,
    candidateRevisionNumber: 1,
    added: [
      {
        taskKey: agentTask.taskKey,
        title: agentTask.title,
        previousStatus: null,
        nextStatus: 'draft',
        contractChanged: false,
      },
    ],
    changed: [],
    unchanged: [],
    removed: [],
    carriedCompleted: 0,
    reopened: 0,
    cancelled: 0,
    activeLeaseCount: 0,
    publishable: true,
    blockers: [],
    ...overrides,
  };
}

function makeAgentWorkItem(
  overrides: Partial<WorkItemView['workItem']> = {},
): WorkItemView {
  return {
    task: agentTask,
    activeLease: null,
    latestReport: null,
    workItem: {
      id: '55555555-5555-4555-8555-555555555555',
      projectId: status.projectId,
      planRevisionId: makePlanRevision().id,
      taskSpecId: '66666666-6666-4666-8666-666666666666',
      taskKey: agentTask.taskKey,
      status: 'ready',
      contractState: 'current',
      priority: 100,
      createdAt: '2026-10-30T11:05:00Z',
      updatedAt: '2026-10-30T11:05:00Z',
      completedAt: null,
      ...overrides,
    },
  };
}

function makeHumanWorkItem(
  overrides: Partial<WorkItemView['workItem']> = {},
): WorkItemView {
  const task = makeTask({
    taskKey: 'human:release:store_account',
    title: '准备应用商店账号',
    kind: 'human_external',
    goal: '创建开发者账号并确认可提交应用',
    scope: ['开发者账号', '商店资质'],
    outOfScope: ['上传安装包'],
    repositoryKey: null,
    workBranch: null,
    allowedPaths: [],
    acceptanceCriteria: [
      {
        id: 'account-ready',
        text: '账号可登录且具备提审权限',
        requiredEvidence: false,
      },
    ],
    acceptancePolicy: 'human_acceptance',
    evidenceRequirements: [],
    criticalPath: true,
    resourceKeys: [],
  });
  return {
    task,
    activeLease: null,
    latestReport: null,
    workItem: {
      id: '77777777-7777-4777-8777-777777777777',
      projectId: status.projectId,
      planRevisionId: makePlanRevision().id,
      taskSpecId: '88888888-8888-4888-8888-888888888888',
      taskKey: task.taskKey,
      status: 'ready',
      contractState: 'current',
      priority: 100,
      createdAt: '2026-10-30T11:05:00Z',
      updatedAt: '2026-10-30T11:05:00Z',
      completedAt: null,
      ...overrides,
    },
  };
}

function makePlanWorkspace(overrides: Partial<PlanWorkspace> = {}): PlanWorkspace {
  return {
    draft: null,
    published: null,
    workItems: [],
    leases: [],
    runs: [],
    reports: [],
    blockers: [],
    mcp: {
      command: String.raw`D:\coldstart\target\release\coldstart-mcp.exe`,
      projectHome: projectPath,
      configJson: '{"mcpServers":{"coldstart":{"command":"coldstart-mcp.exe"}}}',
    },
    ...overrides,
  };
}

const repository: Repository = {
  id: '99999999-9999-4999-8999-999999999999',
  projectId: status.projectId,
  key: 'web',
  name: 'Web 前台',
  localPath: String.raw`D:\repos\web`,
  remoteUrl: 'https://github.com/example/web.git',
  defaultBranch: 'main',
  protectedBranches: ['main'],
  authMode: 'credential',
  credentialId: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',
  syncPolicy: 'fetch_before_claim',
  createdAt: '2026-10-30T12:00:00Z',
  updatedAt: '2026-10-30T12:00:00Z',
};

const credential: CredentialMetadata = {
  id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',
  projectId: status.projectId,
  name: 'GitHub 发布账号',
  provider: 'github',
  kind: 'personal_access_token',
  environment: 'shared',
  secretRef: 'coldstart:credential:github',
  createdAt: '2026-10-30T12:01:00Z',
  updatedAt: '2026-10-30T12:01:00Z',
};

const capability: CapabilityStatusView = {
  projectId: status.projectId,
  capabilityId: 'capability_oss',
  name: '阿里云 OSS',
  provider: 'aliyun_oss',
  environment: 'development',
  status: 'configured',
  credentialId: credential.id,
  credentialRef: credential.secretRef,
  note: '开发环境已配置',
  updatedAt: '2026-10-30T12:02:00Z',
};

const gitSync: GitSyncRecord = {
  id: 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb',
  projectId: status.projectId,
  workItemId: '77777777-7777-4777-8777-777777777777',
  repositoryId: repository.id,
  branch: 'codex/order-web',
  commitSha: 'abcdef1',
  status: 'failed',
  errorMessage: 'network unavailable',
  createdAt: '2026-10-30T12:03:00Z',
};

const verificationCommand: RepositoryVerificationCommand = {
  name: 'web-test',
  executable: String.raw`D:\tools\web-test.exe`,
  args: ['--run'],
  timeoutSeconds: 300,
  position: 0,
};

function makeIntegrationWorkspace(
  overrides: Partial<IntegrationWorkspace> = {},
): IntegrationWorkspace {
  return {
    projectId: status.projectId,
    repositories: [repository],
    verificationCommands: [
      { repositoryKey: 'web', commands: [verificationCommand] },
    ],
    credentials: [credential],
    capabilities: [capability],
    gitSyncs: [gitSync],
    release: {
      prerequisites: [makeHumanWorkItem()],
      milestones: [makeAgentWorkItem()],
      approvalGates: [],
      humanTasks: [],
    },
    ...overrides,
  };
}

const infoRequest: InfoRequest = {
  id: '33333333-3333-4333-8333-333333333333',
  projectId: status.projectId,
  blueprintRevisionId: makeRevision().id,
  fingerprint: 'fingerprint-1',
  category: 'decision',
  question: 'OSS 使用哪个地域？',
  reason: '影响部署配置',
  example: '华东 1',
  defaultValue: null,
  impactScope: '文件上传',
  dueAt: null,
  blocking: true,
  status: 'open',
  answer: null,
  resolvedAt: null,
  createdAt: '2026-10-30T09:01:00Z',
};

const reviewConfiguration: ReviewConfiguration = {
  projectId: status.projectId,
  enabled: true,
  baseUrl: 'https://api.example.test/v1',
  model: 'review-model',
  credentialId: credential.id,
  blockingSeverities: ['critical', 'high'],
  consentFingerprint: 'f'.repeat(64),
  updatedAt: '2026-11-02T09:00:00Z',
};

const reviewStandard: ReviewStandardView = {
  id: '12121212-1212-4212-8212-121212121212',
  name: 'Web 团队规范',
  scope: { kind: 'repository', repositoryKey: 'web' },
  enabled: true,
  position: 0,
  currentRevisionId: '13131313-1313-4313-8313-131313131313',
  revisionNumber: 2,
  source: 'upload',
  fileName: 'web-standard.md',
  format: 'markdown',
  characterCount: 1280,
  contentHash: 'a'.repeat(64),
  createdAt: '2026-11-02T08:00:00Z',
  updatedAt: '2026-11-02T08:30:00Z',
};

function makeReviewHistory(
  statusValue: ReviewHistoryView['reviewStatus'],
  overrides: Partial<ReviewHistoryView> = {},
): ReviewHistoryView {
  return {
    reportId: '14141414-1414-4414-8414-141414141414',
    workItemId: '55555555-5555-4555-8555-555555555555',
    taskKey: 'slice:order:web',
    reviewStatus: statusValue,
    review: statusValue === 'interrupted'
      ? null
      : {
          snapshotHash: 'b'.repeat(64),
          status: statusValue,
          summary: statusValue === 'blocked'
            ? '发现一个高风险错误处理问题。'
            : '评审失败，模型响应无法解析。',
          findings: statusValue === 'blocked'
            ? [
                {
                  severity: 'high',
                  category: 'correctness',
                  file: 'src/order.ts',
                  line: 42,
                  message: '未处理订单失败分支。',
                  recommendation: '补充错误返回并增加测试。',
                  standard: 'Web 团队规范',
                },
              ]
            : [],
          model: 'review-model',
          failure: statusValue === 'failed'
            ? { code: 'review_response_invalid', message: '模型响应格式非法' }
            : null,
          startedAt: '2026-11-02T09:10:00Z',
          finishedAt: '2026-11-02T09:10:05Z',
          durationMs: 5000,
        },
    waiver: null,
    pullRequest: null,
    pullRequestInsights: null,
    createdAt: '2026-11-02T09:10:06Z',
    ...overrides,
  };
}

function makeReviewWorkspace(
  overrides: Partial<ReviewWorkspace> = {},
): ReviewWorkspace {
  return {
    projectId: status.projectId,
    configuration: reviewConfiguration,
    standards: [reviewStandard],
    recentReviews: [],
    ...overrides,
  };
}

const pullRequestUrl = ['https:', '', 'github.com', 'octocat', 'demo', 'pull', '21'].join('/');

function makePullRequest(
  overrides: Partial<PullRequestRecord> = {},
): PullRequestRecord {
  return {
    id: '16161616-1616-4616-8616-161616161616',
    projectId: status.projectId,
    workItemId: '55555555-5555-4555-8555-555555555555',
    reportId: '14141414-1414-4414-8414-141414141414',
    repositoryId: '17171717-1717-4717-8717-171717171717',
    number: 21,
    htmlUrl: pullRequestUrl,
    title: '[slice:order:web] 完成订单页面',
    headBranch: 'codex/slice-order-web',
    baseBranch: 'main',
    state: 'open',
    draft: true,
    createdAt: '2026-11-13T08:00:00Z',
    updatedAt: '2026-11-13T08:01:00Z',
    ...overrides,
  };
}

function makePullRequestInsights(
  overrides: Partial<PullRequestInsights> = {},
): PullRequestInsights {
  return {
    pullRequestId: '16161616-1616-4616-8616-161616161616',
    mergeability: 'conflicted',
    conflictDetails: {
      state: 'detected',
      paths: ['src/order.ts', 'src/payment.ts'],
      truncated: false,
    },
    deploymentDetails: {
      state: 'available',
      deployments: [
        {
          environment: 'production',
          state: 'success',
          description: '生产部署完成',
          statusUpdatedAt: '2026-11-13T08:06:00Z',
        },
      ],
      truncated: false,
    },
    checks: [
      {
        name: 'unit tests',
        status: 'completed',
        conclusion: 'success',
        detailsUrl: `${pullRequestUrl.replace('/pull/21', '')}/actions/runs/1`,
        appName: 'GitHub Actions',
      },
    ],
    reviews: [
      {
        author: 'reviewer',
        state: 'changes_requested',
        body: '请补充失败分支测试。',
        htmlUrl: `${pullRequestUrl}#pullrequestreview-1`,
      },
    ],
    reviewComments: [
      {
        author: 'reviewer',
        path: 'src/order.ts',
        line: 42,
        body: '这里需要在失败时回滚。',
        htmlUrl: `${pullRequestUrl}#discussion_r1`,
      },
    ],
    syncedAt: '2026-11-13T08:05:00Z',
    ...overrides,
  };
}

function makeWorkspace(overrides: Partial<ProjectWorkspace> = {}): ProjectWorkspace {
  return {
    status,
    sources: [],
    sourceAnchors: {},
    draft: null,
    published: null,
    inbox: [],
    facts: [],
    plan: makePlanWorkspace(),
    ...overrides,
  };
}

const memoryBlueprint: ProjectMemoryItem = {
  eventId: '15151515-1515-4515-8515-151515151515',
  occurredAt: '2026-11-12T09:02:00Z',
  phase: 'requirements_blueprint',
  eventType: 'blueprint_published',
  title: '蓝图已发布',
  summary: 'v2 成为当前有效的项目基线',
  reference: {
    referenceType: 'blueprint_revision',
    id: '16161616-1616-4616-8616-161616161616',
    label: '蓝图 v2',
  },
  isHeartbeat: false,
};

const memoryPlan: ProjectMemoryItem = {
  eventId: '17171717-1717-4717-8717-171717171717',
  occurredAt: '2026-11-12T09:01:00Z',
  phase: 'plan_execution',
  eventType: 'plan_published',
  title: '开发计划已发布',
  summary: 'v1 · 4 个任务进入可领取状态',
  reference: {
    referenceType: 'plan_revision',
    id: '18181818-1818-4818-8818-181818181818',
    label: '计划 v1',
  },
  isHeartbeat: false,
};

const memoryHeartbeat: ProjectMemoryItem = {
  eventId: '19191919-1919-4919-8919-191919191919',
  occurredAt: '2026-11-12T09:00:00Z',
  phase: 'plan_execution',
  eventType: 'work_heartbeat',
  title: '任务心跳已刷新',
  summary: '执行者仍在工作',
  reference: null,
  isHeartbeat: true,
};

function makeMemoryPage(overrides: Partial<ProjectMemoryPage> = {}): ProjectMemoryPage {
  return {
    projectId: status.projectId,
    totalCount: 3,
    visibleCount: 2,
    heartbeatCount: 1,
    phaseCounts: [
      { phase: 'requirements_blueprint', count: 1 },
      { phase: 'plan_execution', count: 1 },
      { phase: 'verification_review', count: 0 },
      { phase: 'repository_capability', count: 0 },
      { phase: 'system_configuration', count: 0 },
    ],
    items: [memoryBlueprint, memoryPlan],
    nextCursor: null,
    ...overrides,
  };
}

const workspaceWithSource = makeWorkspace({
  status: { ...status, eventCount: 2 },
  sources: [source],
  sourceAnchors: { [sourceId]: [anchor] },
});

async function openProject(workspace = makeWorkspace()) {
  const user = userEvent.setup();
  vi.mocked(projectApi.pickProjectHome).mockResolvedValue(projectPath);
  vi.mocked(projectApi.readProjectStatus).mockResolvedValue(workspace.status);
  vi.mocked(projectApi.readProjectWorkspace).mockResolvedValue(workspace);

  render(<App />);
  await user.click(screen.getByRole('button', { name: '打开项目' }));
  await screen.findByRole('heading', { name: workspace.status.name });
  return user;
}

async function openIntegrationView(
  workspace = makeIntegrationWorkspace(),
) {
  vi.mocked(integrationApi.readIntegrationWorkspace).mockResolvedValue(workspace);
  const user = await openProject();
  await user.click(screen.getByRole('button', { name: '仓库与能力' }));
  await screen.findByRole('heading', { name: '仓库与能力' });
  return user;
}

async function openReviewView(
  reviewWorkspace = makeReviewWorkspace(),
  integration = makeIntegrationWorkspace(),
) {
  vi.mocked(reviewApi.readReviewWorkspace).mockResolvedValue(reviewWorkspace);
  vi.mocked(integrationApi.readIntegrationWorkspace).mockResolvedValue(integration);
  const user = await openProject();
  await user.click(screen.getByRole('button', { name: 'AI 评审' }));
  await screen.findByRole('heading', { name: 'AI 评审' });
  return user;
}

beforeEach(() => {
  window.localStorage.clear();
  delete document.documentElement.dataset.theme;
  document.documentElement.style.colorScheme = '';
  vi.mocked(projectApi.createProject).mockReset();
  vi.mocked(projectApi.readProjectStatus).mockReset();
  vi.mocked(projectApi.readProjectWorkspace).mockReset();
  vi.mocked(projectApi.pickProjectHome).mockReset();
  vi.mocked(requirementsApi.importRequirementDocument).mockReset();
  vi.mocked(requirementsApi.generateBlueprint).mockReset();
  vi.mocked(requirementsApi.saveBlueprintDraft).mockReset();
  vi.mocked(requirementsApi.publishBlueprint).mockReset();
  vi.mocked(requirementsApi.resolveInfoRequest).mockReset();
  vi.mocked(requirementsApi.pickRequirementFile).mockReset();
  vi.mocked(planApi.readPlanWorkspace).mockReset();
  vi.mocked(planApi.generatePlanDraft).mockReset();
  vi.mocked(planApi.savePlanDraft).mockReset();
  vi.mocked(planApi.previewPlanPublish).mockReset();
  vi.mocked(planApi.publishPlan).mockReset();
  vi.mocked(planApi.completeHumanWork).mockReset();
  vi.mocked(reviewApi.readReviewWorkspace).mockReset();
  vi.mocked(reviewApi.saveReviewConfiguration).mockReset();
  vi.mocked(reviewApi.importReviewStandard).mockReset();
  vi.mocked(reviewApi.createReviewStandard).mockReset();
  vi.mocked(reviewApi.reviseReviewStandard).mockReset();
  vi.mocked(reviewApi.setReviewStandardEnabled).mockReset();
  vi.mocked(reviewApi.reorderReviewStandards).mockReset();
  vi.mocked(reviewApi.deleteReviewStandard).mockReset();
  vi.mocked(reviewApi.readReviewStandard).mockReset();
  vi.mocked(reviewApi.waiveWorkReview).mockReset();
  vi.mocked(reviewApi.createDraftPullRequest).mockReset();
  vi.mocked(reviewApi.refreshPullRequest).mockReset();
  vi.mocked(reviewApi.pickReviewStandardFile).mockReset();
  vi.mocked(memoryApi.readProjectMemory).mockReset();
  vi.mocked(integrationApi.readIntegrationWorkspace).mockReset();
  vi.mocked(integrationApi.validateRepository).mockReset();
  vi.mocked(integrationApi.saveRepository).mockReset();
  vi.mocked(integrationApi.readRepositoryVerificationCommands).mockReset();
  vi.mocked(integrationApi.saveRepositoryVerificationCommands).mockReset();
  vi.mocked(integrationApi.saveCredential).mockReset();
  vi.mocked(integrationApi.deleteCredential).mockReset();
  vi.mocked(integrationApi.updateCapability).mockReset();
  vi.mocked(integrationApi.importGitHubSession).mockReset();
  vi.mocked(integrationApi.startGitHubDeviceFlow).mockReset();
  vi.mocked(integrationApi.pollGitHubDeviceFlow).mockReset();
  vi.mocked(integrationApi.createGitHubPrivateRepository).mockReset();
  vi.mocked(issueApi.listGitHubIssues).mockReset();
  vi.mocked(issueApi.importGitHubIssue).mockReset();
  vi.mocked(integrationApi.readIntegrationWorkspace).mockResolvedValue(
    makeIntegrationWorkspace(),
  );
});

describe('ColdStart project workspace', () => {
  it('shows the welcome view and gates project navigation', () => {
    render(<App />);

    expect(
      within(screen.getByLabelText('应用导航')).getByText('ColdStart'),
    ).toBeInTheDocument();
    expect(screen.getByText('冷启动工作台')).toBeInTheDocument();
    expect(screen.getByText('本地数据')).toBeInTheDocument();
    expect(
      within(screen.getByLabelText('项目工作流')).getByText('本地就绪'),
    ).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '冷启动' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '项目总览' })).toBeEnabled();
    expect(screen.getByRole('button', { name: '需求导入' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '蓝图编排' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '人工 Inbox' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '项目记忆' })).toBeDisabled();
  });

  it('toggles between light and dark themes and persists the choice', async () => {
    const user = userEvent.setup();
    const view = render(<App />);

    expect(document.documentElement).toHaveAttribute('data-theme', 'dark');
    await user.click(screen.getByRole('button', { name: '切换到浅色模式' }));

    expect(document.documentElement).toHaveAttribute('data-theme', 'light');
    expect(window.localStorage.getItem('coldstart.theme')).toBe('light');
    expect(
      screen.getByRole('button', { name: '切换到深色模式' }),
    ).toBeInTheDocument();

    view.unmount();
    render(<App />);

    expect(document.documentElement).toHaveAttribute('data-theme', 'light');
    expect(
      screen.getByRole('button', { name: '切换到深色模式' }),
    ).toBeInTheDocument();
  });

  it('shows a filterable project memory timeline and reloads heartbeats on demand', async () => {
    vi.mocked(memoryApi.readProjectMemory).mockImplementation(
      async (_path, options) =>
        makeMemoryPage(
          options.includeHeartbeats
            ? {
                items: [memoryHeartbeat, memoryBlueprint, memoryPlan],
                totalCount: 3,
                visibleCount: 2,
                heartbeatCount: 1,
              }
            : {},
        ),
    );
    const user = await openProject();
    await user.click(screen.getByRole('button', { name: '项目记忆' }));

    expect(
      await screen.findByRole('heading', { name: '项目记忆' }),
    ).toBeInTheDocument();
    expect(memoryApi.readProjectMemory).toHaveBeenCalledWith(projectPath, {
      beforeCursor: null,
      limit: 100,
      includeHeartbeats: false,
    });
    expect(screen.getByText('蓝图已发布')).toBeInTheDocument();
    expect(screen.getByText('开发计划已发布')).toBeInTheDocument();
    expect(screen.queryByText('任务心跳已刷新')).not.toBeInTheDocument();
    expect(screen.getByText('只读审计')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /计划与执行/ }));
    expect(screen.queryByText('蓝图已发布')).not.toBeInTheDocument();
    expect(screen.getByText('开发计划已发布')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /全部/ }));
    await user.type(screen.getByLabelText('搜索项目记忆'), '不存在的变更');
    expect(screen.getByText('当前筛选没有匹配结果')).toBeInTheDocument();

    await user.clear(screen.getByLabelText('搜索项目记忆'));
    await user.click(screen.getByLabelText('显示系统心跳'));
    expect(await screen.findByText('任务心跳已刷新')).toBeInTheDocument();
    expect(memoryApi.readProjectMemory).toHaveBeenLastCalledWith(projectPath, {
      beforeCursor: null,
      limit: 100,
      includeHeartbeats: true,
    });
  });

  it('loads older project memory pages with the cursor and de-duplicates items', async () => {
    const artifact = {
      ...memoryBlueprint,
      eventId: '20202020-2020-4020-8020-202020202020',
      occurredAt: '2026-11-12T08:00:00Z',
      phase: 'requirements_blueprint' as const,
      eventType: 'artifact_imported',
      title: '需求文档已导入',
      summary: 'requirements.md · 内容指纹 abcdef123456',
      reference: {
        referenceType: 'artifact' as const,
        id: sourceId,
        label: 'requirements.md',
      },
    };
    vi.mocked(memoryApi.readProjectMemory)
      .mockResolvedValueOnce(makeMemoryPage({ items: [memoryPlan], nextCursor: 20 }))
      .mockResolvedValueOnce(
        makeMemoryPage({ items: [memoryPlan, artifact], nextCursor: null }),
      );
    const user = await openProject();
    await user.click(screen.getByRole('button', { name: '项目记忆' }));
    await screen.findByText('开发计划已发布');

    await user.click(screen.getByRole('button', { name: '加载更多历史事件' }));

    expect(await screen.findByText('需求文档已导入')).toBeInTheDocument();
    expect(screen.getAllByText('开发计划已发布')).toHaveLength(1);
    expect(memoryApi.readProjectMemory).toHaveBeenLastCalledWith(projectPath, {
      beforeCursor: 20,
      limit: 100,
      includeHeartbeats: false,
    });
    expect(
      screen.queryByRole('button', { name: '加载更多历史事件' }),
    ).not.toBeInTheDocument();
  });

  it('shows a truthful empty state when a project has no memory events', async () => {
    vi.mocked(memoryApi.readProjectMemory).mockResolvedValue(
      makeMemoryPage({
        totalCount: 0,
        visibleCount: 0,
        heartbeatCount: 0,
        phaseCounts: [],
        items: [],
      }),
    );
    const user = await openProject();
    await user.click(screen.getByRole('button', { name: '项目记忆' }));

    expect(await screen.findByText('项目尚未产生事件')).toBeInTheDocument();
  });

  it('creates a project and loads its full workspace', async () => {
    const user = userEvent.setup();
    let resolveCreate!: (created: ProjectStatus) => void;
    vi.mocked(projectApi.pickProjectHome).mockResolvedValue(projectPath);
    vi.mocked(projectApi.createProject).mockImplementation(
      () =>
        new Promise<ProjectStatus>((resolve) => {
          resolveCreate = resolve;
        }),
    );
    vi.mocked(projectApi.readProjectWorkspace).mockResolvedValue(makeWorkspace());

    render(<App />);
    await user.click(screen.getByRole('button', { name: '新建项目' }));
    await user.type(screen.getByLabelText('项目名称'), status.name);
    await user.click(screen.getByRole('button', { name: '选择目录' }));
    await screen.findByText(projectPath);
    await user.click(screen.getByRole('button', { name: '创建项目' }));

    expect(projectApi.createProject).toHaveBeenCalledWith(status.name, projectPath);
    expect(await screen.findByText('正在创建项目')).toBeInTheDocument();
    resolveCreate(status);
    expect(await screen.findByRole('heading', { name: status.name })).toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(projectApi.readProjectWorkspace).toHaveBeenCalledWith(projectPath);
  });

  it('enables requirements, blueprint and inbox after opening a project', async () => {
    await openProject(workspaceWithSource);

    expect(screen.getByRole('button', { name: '需求导入' })).toBeEnabled();
    expect(screen.getByRole('button', { name: '蓝图编排' })).toBeEnabled();
    expect(screen.getByRole('button', { name: '人工 Inbox' })).toBeEnabled();
    expect(screen.getByText(/permafrost/)).toBeInTheDocument();
  });

  it('imports a requirement document and refreshes the event count', async () => {
    const user = await openProject(makeWorkspace());
    vi.mocked(requirementsApi.pickRequirementFile).mockResolvedValue(
      String.raw`D:\docs\requirements.md`,
    );
    vi.mocked(requirementsApi.importRequirementDocument).mockResolvedValue(
      workspaceWithSource,
    );

    await user.click(screen.getByRole('button', { name: '需求导入' }));
    await user.click(screen.getByRole('button', { name: '导入需求文档' }));

    expect(await screen.findByText('requirements.md')).toBeInTheDocument();
    expect(requirementsApi.importRequirementDocument).toHaveBeenCalledWith(
      projectPath,
      String.raw`D:\docs\requirements.md`,
    );

    await user.click(screen.getByRole('button', { name: '项目总览' }));
    expect(await screen.findByText(/2 个事件/)).toBeInTheDocument();
  });

  it('loads open GitHub issues and imports a selected issue as a requirement source', async () => {
    const user = await openProject(makeWorkspace());
    const issueList: GitHubIssueListView = {
      repositoryKey: 'web',
      repositoryName: 'Web 前台',
      repositoryLabel: 'octocat/demo',
      truncated: false,
      issues: [
        {
          id: 7,
          number: 7,
          title: '历史项目新增导出需求',
          state: 'open',
          bodyPreview: '希望支持 CSV 导出。\n\n需要包含筛选条件。',
          labels: ['backend', 'enhancement'],
          htmlUrl: 'https://github.com/octocat/demo/issues/7',
          comments: 3,
          updatedAt: '2026-11-13T10:00:00Z',
          sourceReference: 'github://octocat/demo/issues/7',
        },
      ],
    };
    const issueSource = {
      ...source,
      id: '18181818-1818-4818-8818-181818181818',
      fileName: 'github-issue-7.md',
      importSource: 'github://octocat/demo/issues/7',
    };
    vi.mocked(issueApi.listGitHubIssues).mockResolvedValue(issueList);
    vi.mocked(issueApi.importGitHubIssue).mockResolvedValue(
      makeWorkspace({
        sources: [issueSource],
        sourceAnchors: { [issueSource.id]: [anchor] },
      }),
    );

    await user.click(screen.getByRole('button', { name: '需求导入' }));
    await waitFor(() => {
      expect(integrationApi.readIntegrationWorkspace).toHaveBeenCalledWith(projectPath);
      expect(screen.getByLabelText('目标仓库')).toHaveValue('web');
    });
    await user.click(screen.getByRole('button', { name: '读取开放 Issue' }));

    expect(await screen.findByText('历史项目新增导出需求')).toBeInTheDocument();
    expect(screen.getByText('backend')).toBeInTheDocument();
    expect(screen.getByText(/3 条评论 · 评论未导入/)).toBeInTheDocument();
    expect(issueApi.listGitHubIssues).toHaveBeenCalledWith(projectPath, 'web');

    await user.click(screen.getByRole('button', { name: '导入为需求来源' }));

    expect(issueApi.importGitHubIssue).toHaveBeenCalledWith(projectPath, 'web', 7);
    expect(
      await screen.findByRole('button', { name: '已导入为来源' }),
    ).toBeDisabled();
    expect(screen.getByText('github-issue-7.md')).toBeInTheDocument();
  });

  it('shows a stable error when GitHub issues cannot be read', async () => {
    const user = await openProject(makeWorkspace());
    vi.mocked(issueApi.listGitHubIssues).mockRejectedValue({
      code: 'github_issue_ineligible',
      message: '仓库必须使用 GitHub OAuth Token 或 Personal Access Token',
    });

    await user.click(screen.getByRole('button', { name: '需求导入' }));
    await waitFor(() => {
      expect(screen.getByLabelText('目标仓库')).toHaveValue('web');
    });
    await user.click(screen.getByRole('button', { name: '读取开放 Issue' }));

    const alert = await screen.findByRole('alert');
    expect(within(alert).getByText('Issue 读取失败')).toBeInTheDocument();
    expect(
      within(alert).getByText('仓库必须使用 GitHub OAuth Token 或 Personal Access Token'),
    ).toBeInTheDocument();
    expect(within(alert).getByText('github_issue_ineligible')).toBeInTheDocument();
  });

  it('requires explicit model consent before generating and then shows the draft', async () => {
    const user = await openProject(workspaceWithSource);
    const draftWorkspace = makeWorkspace({
      sources: [source],
      sourceAnchors: { [sourceId]: [anchor] },
      draft: makeRevision(),
    });
    vi.mocked(requirementsApi.generateBlueprint).mockResolvedValue(draftWorkspace);

    await user.click(screen.getByRole('button', { name: '需求导入' }));
    await user.type(screen.getByLabelText('Base URL'), 'http://127.0.0.1:9000/v1');
    await user.type(screen.getByLabelText('模型名称'), 'test-model');
    await user.type(screen.getByLabelText('API Key'), 'secret-key');

    const generateButton = screen.getByRole('button', { name: '生成蓝图草稿' });
    expect(generateButton).toBeDisabled();
    await user.click(screen.getByRole('checkbox'));
    expect(generateButton).toBeEnabled();
    await user.click(generateButton);

    expect(requirementsApi.generateBlueprint).toHaveBeenCalledWith(projectPath, {
      baseUrl: 'http://127.0.0.1:9000/v1',
      model: 'test-model',
      apiKey: 'secret-key',
    });
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '生成蓝图草稿' })).toBeEnabled();
    });

    await user.click(screen.getByRole('button', { name: '蓝图编排' }));
    expect(await screen.findByText('草稿 v1')).toBeInTheDocument();
    expect(screen.getByLabelText('产品目标')).toHaveValue('构建订单系统');
    expect(screen.getAllByText(/requirements.md/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/订单系统/).length).toBeGreaterThan(0);
  });

  it('saves edited blueprint content as a new draft revision', async () => {
    const draft = makeRevision();
    const user = await openProject(
      makeWorkspace({ sources: [source], sourceAnchors: { [sourceId]: [anchor] }, draft }),
    );
    const savedDraft = makeRevision({
      id: '44444444-4444-4444-8444-444444444444',
      revisionNumber: 2,
      generation: {
        source: 'manual',
        modelProvider: null,
        modelName: null,
        promptVersion: null,
      },
    });
    vi.mocked(requirementsApi.saveBlueprintDraft).mockResolvedValue(
      makeWorkspace({
        sources: [source],
        sourceAnchors: { [sourceId]: [anchor] },
        draft: savedDraft,
      }),
    );

    await user.click(screen.getByRole('button', { name: '蓝图编排' }));
    const moduleName = screen.getByLabelText('模块名称 1');
    await user.clear(moduleName);
    await user.type(moduleName, '订单履约模块');
    await user.click(screen.getByRole('button', { name: '保存草稿' }));

    expect(requirementsApi.saveBlueprintDraft).toHaveBeenCalledTimes(1);
    const savedContent = vi.mocked(requirementsApi.saveBlueprintDraft).mock.calls[0][1];
    expect(savedContent.modules[0].name).toBe('订单履约模块');
    expect(await screen.findByText('草稿 v2')).toBeInTheDocument();
  });

  it('creates a manual draft with a valid product-goal source anchor', async () => {
    const user = await openProject(workspaceWithSource);
    vi.mocked(requirementsApi.saveBlueprintDraft).mockResolvedValue(
      makeWorkspace({
        sources: [source],
        sourceAnchors: { [sourceId]: [anchor] },
        draft: makeRevision({
          generation: {
            source: 'manual',
            modelProvider: null,
            modelName: null,
            promptVersion: null,
          },
        }),
      }),
    );

    await user.click(screen.getByRole('button', { name: '蓝图编排' }));
    await user.click(screen.getByRole('button', { name: '创建人工草稿' }));
    expect(screen.getByLabelText('产品目标')).toBeEnabled();
    await user.click(screen.getByRole('button', { name: '保存草稿' }));

    const manualContent = vi.mocked(requirementsApi.saveBlueprintDraft).mock.calls[0][1];
    expect(manualContent.productGoal.sources).toEqual([
      { artifactId: sourceId, anchorId, quote: null },
    ]);
    expect(manualContent.platforms).toHaveLength(1);
    expect(manualContent.modules).toHaveLength(1);
  });

  it('publishes the saved draft and switches to the immutable published view', async () => {
    const draft = makeRevision();
    const user = await openProject(
      makeWorkspace({ sources: [source], sourceAnchors: { [sourceId]: [anchor] }, draft }),
    );
    const published = makeRevision({
      status: 'published',
      publishedAt: '2026-10-30T10:00:00Z',
    });
    vi.mocked(requirementsApi.publishBlueprint).mockResolvedValue(
      makeWorkspace({
        sources: [source],
        sourceAnchors: { [sourceId]: [anchor] },
        published,
      }),
    );

    await user.click(screen.getByRole('button', { name: '蓝图编排' }));
    await user.click(screen.getByRole('button', { name: '发布当前草稿' }));

    expect(requirementsApi.publishBlueprint).toHaveBeenCalledWith(projectPath, draft.id);
    expect(await screen.findByText('已发布 v1')).toBeInTheDocument();
    expect(screen.getByText(/当前展示已发布版本 v1/)).toBeInTheDocument();
    expect(screen.getByLabelText('产品目标')).toBeDisabled();
  });

  it('answers an inbox request and records the resulting project fact', async () => {
    const user = await openProject(
      makeWorkspace({
        sources: [source],
        sourceAnchors: { [sourceId]: [anchor] },
        draft: makeRevision(),
        inbox: [infoRequest],
      }),
    );
    const answeredRequest: InfoRequest = {
      ...infoRequest,
      status: 'answered',
      answer: '华东 1',
      resolvedAt: '2026-10-30T10:10:00Z',
    };
    vi.mocked(requirementsApi.resolveInfoRequest).mockResolvedValue(
      makeWorkspace({
        sources: [source],
        sourceAnchors: { [sourceId]: [anchor] },
        draft: makeRevision(),
        inbox: [answeredRequest],
        facts: [
          {
            id: '55555555-5555-4555-8555-555555555555',
            projectId: status.projectId,
            factKey: 'oss_region',
            category: 'decision',
            question: infoRequest.question,
            value: '华东 1',
            version: 1,
            sourceInfoRequestId: infoRequest.id,
            createdAt: '2026-10-30T10:10:00Z',
          },
        ],
      }),
    );

    await user.click(screen.getByRole('button', { name: '人工 Inbox' }));
    expect(screen.getByRole('heading', { name: infoRequest.question })).toBeInTheDocument();
    await user.type(screen.getByLabelText(`回答：${infoRequest.question}`), '华东 1');
    await user.click(screen.getByRole('button', { name: `提交回答：${infoRequest.question}` }));

    expect(requirementsApi.resolveInfoRequest).toHaveBeenCalledWith(
      projectPath,
      infoRequest.id,
      '华东 1',
    );
    expect(
      await screen.findByText('当前没有需要人工补充的信息'),
    ).toBeInTheDocument();
    expect(screen.getByText('已回答与已关闭 · 1')).toBeInTheDocument();
    expect(screen.getAllByText('华东 1').length).toBeGreaterThan(0);
  });

  it('keeps the last good workspace when an action fails', async () => {
    const user = await openProject(workspaceWithSource);
    vi.mocked(requirementsApi.pickRequirementFile).mockResolvedValue(
      String.raw`D:\docs\empty.pdf`,
    );
    vi.mocked(requirementsApi.importRequirementDocument).mockRejectedValue({
      code: 'document_empty',
      message: '文档没有可抽取文本',
    });

    await user.click(screen.getByRole('button', { name: '需求导入' }));
    await user.click(screen.getByRole('button', { name: '导入需求文档' }));

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('document_empty');
    expect(alert).toHaveTextContent('文档没有可抽取文本');
    await user.click(within(alert).getByRole('button', { name: '关闭错误提示' }));

    expect(screen.getByRole('heading', { name: '需求导入' })).toBeInTheDocument();
    expect(screen.getByText('requirements.md')).toBeInTheDocument();
  });

  it('generates and publishes a plan, then displays ready work', async () => {
    const publishedBlueprint = makeRevision({
      status: 'published',
      publishedAt: '2026-10-30T10:00:00Z',
    });
    const user = await openProject(
      makeWorkspace({
        sources: [source],
        sourceAnchors: { [sourceId]: [anchor] },
        published: publishedBlueprint,
      }),
    );
    const draftPlan = makePlanRevision();
    vi.mocked(planApi.generatePlanDraft).mockResolvedValue(
      makeWorkspace({
        sources: [source],
        sourceAnchors: { [sourceId]: [anchor] },
        published: publishedBlueprint,
        plan: makePlanWorkspace({ draft: draftPlan }),
      }),
    );
    vi.mocked(planApi.publishPlan).mockResolvedValue(
      makeWorkspace({
        sources: [source],
        sourceAnchors: { [sourceId]: [anchor] },
        published: publishedBlueprint,
        plan: makePlanWorkspace({
          published: makePlanRevision({
            status: 'published',
            publishedAt: '2026-10-30T11:10:00Z',
          }),
          workItems: [makeAgentWorkItem()],
        }),
      }),
    );
    vi.mocked(planApi.previewPlanPublish).mockResolvedValue(
      makePublishPreview({
        candidateRevisionId: draftPlan.id,
        candidateRevisionNumber: 1,
      }),
    );

    await user.click(screen.getByRole('button', { name: '计划看板' }));
    await user.click(screen.getByRole('button', { name: '编译开发计划' }));
    expect(await screen.findByText('计划草稿 v1')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '发布计划' }));
    expect(planApi.previewPlanPublish).toHaveBeenCalledWith(
      projectPath,
      draftPlan.id,
    );
    const dialog = await screen.findByRole('dialog', {
      name: '确认发布计划 v1',
    });
    expect(within(dialog).getByText('新增')).toBeInTheDocument();
    await user.click(
      within(dialog).getByRole('button', { name: '确认发布首个计划' }),
    );

    expect(await screen.findByText('可领取')).toBeInTheDocument();
    expect(planApi.publishPlan).toHaveBeenCalledWith(
      projectPath,
      draftPlan.id,
      true,
    );
  });

  it('blocks replacement publishing while an active lease remains', async () => {
    const publishedPlan = makePlanRevision({
      status: 'published',
      publishedAt: '2026-10-30T10:00:00Z',
    });
    const draftPlan = makePlanRevision({
      id: 'abababab-abab-4bab-8bab-abababababab',
      revisionNumber: 2,
    });
    const user = await openProject(
      makeWorkspace({
        plan: makePlanWorkspace({
          draft: draftPlan,
          published: publishedPlan,
          workItems: [
            makeAgentWorkItem({
              status: 'in_progress',
            }),
          ],
        }),
      }),
    );
    vi.mocked(planApi.previewPlanPublish).mockResolvedValue(
      makePublishPreview({
        baseRevisionId: publishedPlan.id,
        baseRevisionNumber: 1,
        candidateRevisionId: draftPlan.id,
        candidateRevisionNumber: 2,
        added: [],
        unchanged: [
          {
            taskKey: agentTask.taskKey,
            title: agentTask.title,
            previousStatus: 'done',
            nextStatus: 'done',
            contractChanged: false,
          },
        ],
        carriedCompleted: 1,
        activeLeaseCount: 1,
        publishable: false,
        blockers: ['仍有 1 个活动租约，必须完成、释放或等待过期后才能替换活动计划'],
      }),
    );

    await user.click(screen.getByRole('button', { name: '计划看板' }));
    await user.click(screen.getByRole('button', { name: '发布计划' }));

    const dialog = await screen.findByRole('dialog', {
      name: '确认替换计划 v2',
    });
    expect(
      within(dialog).getByText(
        '仍有 1 个活动租约，必须完成、释放或等待过期后才能替换活动计划',
      ),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole('button', { name: '确认替换活动计划' }),
    ).toBeDisabled();
    expect(planApi.publishPlan).not.toHaveBeenCalled();
  });

  it('completes a human plan task from inbox and unlocks the plan', async () => {
    const humanTask = makeHumanWorkItem();
    const user = await openProject(
      makeWorkspace({
        plan: makePlanWorkspace({
          published: makePlanRevision({ status: 'published' }),
          workItems: [humanTask],
        }),
      }),
    );
    vi.mocked(planApi.completeHumanWork).mockResolvedValue(
      makeWorkspace({
        plan: makePlanWorkspace({
          published: makePlanRevision({ status: 'published' }),
          workItems: [
            makeHumanWorkItem({
              status: 'done',
              completedAt: '2026-10-30T11:20:00Z',
            }),
          ],
        }),
      }),
    );

    await user.click(screen.getByRole('button', { name: '人工 Inbox' }));
    await user.type(
      screen.getByLabelText('完成说明：准备应用商店账号'),
      '账号已可用',
    );
    await user.click(
      screen.getByRole('button', { name: '确认完成：准备应用商店账号' }),
    );

    expect(planApi.completeHumanWork).toHaveBeenCalledWith(
      projectPath,
      humanTask.workItem.id,
      '账号已可用',
      true,
    );
  });

  it('saves a validated repository draft from the integration workspace', async () => {
    const user = await openIntegrationView(
      makeIntegrationWorkspace({ repositories: [] }),
    );
    const localPath = String.raw`D:\work\coldstart-web`;
    const remoteUrl = 'https://github.com/example/coldstart-web.git';
    vi.mocked(integrationApi.saveRepository).mockResolvedValue(
      makeIntegrationWorkspace({
        repositories: [{ ...repository, localPath, remoteUrl }],
      }),
    );

    await user.type(screen.getByLabelText('仓库名称'), 'Web 前台');
    await user.type(screen.getByLabelText('本地路径'), localPath);
    await user.type(screen.getByLabelText('远程 URL'), remoteUrl);
    await user.click(screen.getByRole('button', { name: '保存仓库' }));

    await waitFor(() =>
      expect(integrationApi.saveRepository).toHaveBeenCalledWith(projectPath, {
        key: 'web',
        name: 'Web 前台',
        localPath,
        remoteUrl,
        defaultBranch: 'main',
        protectedBranches: ['main'],
        authMode: 'none',
        credentialId: null,
        syncPolicy: 'fetch_before_claim',
      }),
    );
  });

  it('adds, edits, reorders, deletes and saves repository verification commands', async () => {
    const user = await openIntegrationView(
      makeIntegrationWorkspace({ verificationCommands: [] }),
    );
    expect(screen.getByText('该仓库没有验证命令')).toBeInTheDocument();
    const executable = String.raw`D:\tools\verify.exe`;
    const saved: RepositoryVerificationCommand[] = [
      {
        name: 'test-suite',
        executable,
        args: ['run', '--fast'],
        timeoutSeconds: 120,
        position: 0,
      },
    ];
    vi.mocked(
      integrationApi.saveRepositoryVerificationCommands,
    ).mockResolvedValue(saved);

    await user.click(screen.getByRole('button', { name: '新增验证命令' }));
    await user.type(screen.getByLabelText('验证命令名称 1'), 'build-web');
    await user.type(screen.getByLabelText('验证命令可执行文件 1'), executable);
    await user.type(screen.getByLabelText('验证命令参数 1'), 'build\n--release');
    await user.click(screen.getByRole('button', { name: '新增验证命令' }));
    await user.type(screen.getByLabelText('验证命令名称 2'), 'test-suite');
    await user.type(screen.getByLabelText('验证命令可执行文件 2'), executable);
    await user.type(screen.getByLabelText('验证命令参数 2'), 'run\n--fast');
    await user.clear(screen.getByLabelText('验证命令超时 2'));
    await user.type(screen.getByLabelText('验证命令超时 2'), '120');
    await user.click(screen.getByRole('button', { name: '上移命令 2' }));
    await user.click(screen.getByRole('button', { name: '删除命令 2' }));
    await user.click(screen.getByRole('button', { name: '保存验证命令' }));

    await waitFor(() =>
      expect(
        integrationApi.saveRepositoryVerificationCommands,
      ).toHaveBeenCalledWith(projectPath, 'web', saved),
    );
  });

  it('shows the latest failed verification and its sanitized output', async () => {
    const verificationReport: WorkReportRecord = {
      id: 'cccccccc-cccc-4ccc-8ccc-cccccccccccc',
      workItemId: '55555555-5555-4555-8555-555555555555',
      workRunId: 'dddddddd-dddd-4ddd-8ddd-dddddddddddd',
      contractHash: 'a'.repeat(64),
      report: {
        status: 'completed',
        summary: '实现订单接口',
        branch: agentTask.workBranch,
        commitSha: 'abcdef1',
        changedFiles: ['src/main.rs'],
        acceptanceResults: [],
        commandsRun: [],
        evidenceRefs: [],
        knownIssues: [],
        followUpItems: [],
        contractHash: 'a'.repeat(64),
        idempotencyKey: 'verification-report-1',
      },
      reportHash: 'e'.repeat(64),
      accepted: false,
      rejectionReasons: ['repository verification failed'],
      verificationStatus: 'failed',
      verification: {
        snapshotHash: 'f'.repeat(64),
        status: 'failed',
        startedAt: '2026-10-30T12:10:00Z',
        finishedAt: '2026-10-30T12:10:01Z',
        totalDurationMs: 1000,
        failureKind: 'non_zero_exit',
        failureCommand: 'web-test',
        commands: [
          {
            name: 'web-test',
            exitCode: 1,
            durationMs: 42,
            outputSummary: 'FAIL src/main.test.ts\nAuthorization: [REDACTED]',
            truncated: false,
            failureKind: 'non_zero_exit',
          },
        ],
      },
      reviewStatus: 'not_required',
      review: null,
      reviewWaiver: null,
      createdAt: '2026-10-30T12:10:02Z',
    };
    const publishedPlan = makePlanRevision({
      status: 'published',
      publishedAt: '2026-10-30T12:00:00Z',
    });
    const workItem = makeAgentWorkItem({ status: 'in_progress' });
    workItem.latestReport = verificationReport;
    const user = await openProject(
      makeWorkspace({
        plan: makePlanWorkspace({
          published: publishedPlan,
          workItems: [workItem],
        }),
      }),
    );

    await user.click(screen.getByRole('button', { name: '计划看板' }));
    expect(await screen.findByText('仓库验证 · 未通过')).toBeInTheDocument();
    const failureLine = screen.getByText('web-test').closest('p');
    expect(failureLine).not.toBeNull();
    expect(failureLine).toHaveTextContent(/失败命令\s*web-test/);
    expect(failureLine).toHaveTextContent(/退出码\s*1/);
    expect(failureLine).toHaveTextContent(/耗时\s*42\s*ms/);
    await user.click(screen.getByText('查看脱敏输出摘要'));
    expect(
      screen.getByText(/Authorization: \[REDACTED\]/),
    ).toBeInTheDocument();
  });

  it('stores a credential secret without rendering it afterward', async () => {
    const user = await openIntegrationView(
      makeIntegrationWorkspace({ credentials: [] }),
    );
    const secret = 'plaintext-secret-must-disappear';
    vi.mocked(integrationApi.saveCredential).mockResolvedValue(
      makeIntegrationWorkspace({
        credentials: [{ ...credential, name: '阿里云 OSS 凭据' }],
      }),
    );

    await user.type(screen.getByLabelText('凭据名称'), '阿里云 OSS 凭据');
    await user.type(screen.getByLabelText('提供方'), 'aliyun');
    await user.type(
      screen.getByLabelText('Secret（仅写入 Windows 凭据库）'),
      secret,
    );
    await user.click(screen.getByRole('button', { name: '保存凭据' }));

    await waitFor(() =>
      expect(integrationApi.saveCredential).toHaveBeenCalledWith(projectPath, {
        name: '阿里云 OSS 凭据',
        provider: 'aliyun',
        kind: 'api_key',
        environment: 'development',
        secret,
      }),
    );
    await screen.findByText('阿里云 OSS 凭据');
    expect(
      screen.getByLabelText('Secret（仅写入 Windows 凭据库）'),
    ).toHaveValue('');
    expect(document.body.textContent).not.toContain(secret);
  });

  it('updates a capability environment status and credential reference', async () => {
    const user = await openIntegrationView();
    vi.mocked(integrationApi.updateCapability).mockResolvedValue(
      makeIntegrationWorkspace({
        capabilities: [{ ...capability, status: 'verified' }],
      }),
    );

    await user.selectOptions(
      screen.getByLabelText('能力状态：阿里云 OSS / development'),
      'verified',
    );
    await user.click(
      screen.getByRole('button', {
        name: '更新能力：阿里云 OSS / development',
      }),
    );

    expect(integrationApi.updateCapability).toHaveBeenCalledWith(
      projectPath,
      capability.capabilityId,
      'development',
      'verified',
      credential.id,
      '开发环境已配置',
    );
    expect(await screen.findByText('verified')).toBeInTheDocument();
  });

  it('shows the GitHub device code and checks authorization status', async () => {
    const user = await openIntegrationView();
    vi.mocked(integrationApi.startGitHubDeviceFlow).mockResolvedValue({
      sessionId: 'device-session-1',
      userCode: 'ABCD-EFGH',
      verificationUri: 'https://github.com/login/device',
      expiresIn: 900,
      interval: 5,
    });
    vi.mocked(integrationApi.pollGitHubDeviceFlow).mockResolvedValue({
      state: 'pending',
      sessionId: 'device-session-1',
      interval: 5,
    });

    await user.type(screen.getByLabelText('GitHub OAuth Client ID'), 'client-123');
    await user.click(screen.getByRole('button', { name: '启动 GitHub 授权' }));

    expect(await screen.findByText('ABCD-EFGH')).toBeInTheDocument();
    expect(screen.getByText('https://github.com/login/device')).toBeInTheDocument();
    await user.click(
      screen.getByRole('button', { name: '检查 GitHub 授权状态' }),
    );
    expect(integrationApi.pollGitHubDeviceFlow).toHaveBeenCalledWith(
      projectPath,
      'device-session-1',
    );
  });

  it('requires confirmation before creating a private GitHub repository', async () => {
    const user = await openIntegrationView();
    vi.mocked(integrationApi.createGitHubPrivateRepository).mockResolvedValue({
      id: 42,
      name: 'coldstart-demo',
      fullName: 'example/coldstart-demo',
      private: true,
      cloneUrl: 'https://github.com/example/coldstart-demo.git',
      sshUrl: 'git@github.com:example/coldstart-demo.git',
      htmlUrl: 'https://github.com/example/coldstart-demo',
      defaultBranch: 'main',
    });

    await user.type(screen.getByLabelText('私有仓库名称'), 'coldstart-demo');
    await user.type(screen.getByLabelText('仓库描述'), 'ColdStart demo');
    await user.click(
      screen.getByRole('button', { name: '创建 GitHub 私有仓库' }),
    );

    expect(integrationApi.createGitHubPrivateRepository).not.toHaveBeenCalled();
    expect(
      screen.getByRole('dialog', { name: '确认创建 GitHub 私有仓库' }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '确认创建' }));

    expect(
      integrationApi.createGitHubPrivateRepository,
    ).toHaveBeenCalledWith(projectPath, {
      credentialId: credential.id,
      repositoryKey: repository.key,
      name: 'coldstart-demo',
      description: 'ColdStart demo',
    });
    expect(await screen.findByText('example/coldstart-demo')).toBeInTheDocument();
  });

  it('summarizes release gates without offering deploy or submission actions', async () => {
    const user = await openIntegrationView();
    await user.click(screen.getByRole('button', { name: '发布中心' }));

    expect(
      await screen.findByRole('heading', { name: '发布中心' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('heading', { name: '发布前置条件' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '发布里程碑' })).toBeInTheDocument();
    expect(
      screen.getByText('只汇总状态，不部署、不打包、不提审'),
    ).toBeInTheDocument();
    expect(screen.getByText('准备应用商店账号')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '进入人工 Inbox' }));
    expect(
      await screen.findByRole('heading', { name: '人工 Inbox' }),
    ).toBeInTheDocument();
  });

  it('saves an enabled AI Review configuration only after explicit consent', async () => {
    const user = await openReviewView(
      makeReviewWorkspace({ configuration: null, standards: [] }),
    );
    vi.mocked(reviewApi.saveReviewConfiguration).mockResolvedValue(reviewConfiguration);
    vi.mocked(reviewApi.readReviewWorkspace).mockResolvedValue(
      makeReviewWorkspace({ configuration: reviewConfiguration, standards: [] }),
    );

    await user.click(screen.getByLabelText('启用 AI Review'));
    await user.clear(screen.getByLabelText('Review 模型 Base URL'));
    await user.type(
      screen.getByLabelText('Review 模型 Base URL'),
      'https://api.example.test/v1',
    );
    await user.type(screen.getByLabelText('Review 模型名称'), 'review-model');
    await user.selectOptions(
      screen.getByLabelText('Review 模型凭据'),
      credential.id,
    );
    await user.click(screen.getByLabelText('确认 AI Review 隐私边界'));
    await user.click(screen.getByRole('button', { name: '保存 Review 配置' }));

    await waitFor(() =>
      expect(reviewApi.saveReviewConfiguration).toHaveBeenCalledWith(projectPath, {
        enabled: true,
        baseUrl: 'https://api.example.test/v1',
        model: 'review-model',
        credentialId: credential.id,
        blockingSeverities: ['critical', 'high'],
        consentConfirmed: true,
      }),
    );
    expect(
      await screen.findByText('AI Review 配置已保存；新配置只影响下一次领取。'),
    ).toBeInTheDocument();
  });

  it('requires consent again after changing the review endpoint or model', async () => {
    const user = await openReviewView();
    const consent = screen.getByLabelText('确认 AI Review 隐私边界');

    await user.click(consent);
    expect(consent).toBeChecked();

    await user.type(screen.getByLabelText('Review 模型 Base URL'), '/v2');
    expect(consent).not.toBeChecked();

    await user.click(consent);
    expect(consent).toBeChecked();

    await user.type(screen.getByLabelText('Review 模型名称'), '-next');
    expect(consent).not.toBeChecked();
  });

  it('imports a repository-scoped review standard from a file', async () => {
    const user = await openReviewView(makeReviewWorkspace({ standards: [] }));
    const standardPath = String.raw`D:\docs\web-standard.md`;
    vi.mocked(reviewApi.pickReviewStandardFile).mockResolvedValue(standardPath);
    vi.mocked(reviewApi.importReviewStandard).mockResolvedValue(reviewStandard);
    vi.mocked(reviewApi.readReviewWorkspace).mockResolvedValue(makeReviewWorkspace());

    await user.click(screen.getByRole('button', { name: '上传规范' }));
    await user.click(screen.getByRole('button', { name: '选择文件' }));
    await waitFor(() =>
      expect(screen.getByLabelText('评审规范名称')).toHaveValue('web-standard'),
    );
    await user.selectOptions(
      screen.getByLabelText('评审规范适用范围'),
      'repository:web',
    );
    await user.click(screen.getByRole('button', { name: '导入并保存' }));

    await waitFor(() =>
      expect(reviewApi.importReviewStandard).toHaveBeenCalledWith(projectPath, {
        name: 'web-standard',
        scope: { kind: 'repository', repositoryKey: 'web' },
        path: standardPath,
      }),
    );
    expect(await screen.findByText('Web 团队规范')).toBeInTheDocument();
  });

  it('creates a draft pull request only for a review that passed the gates', async () => {
    const passed = makeReviewHistory('passed');
    const blocked = makeReviewHistory('blocked', {
      reportId: '18181818-1818-4818-8818-181818181818',
      taskKey: 'slice:order:blocked',
    });
    const created = makePullRequest();
    vi.mocked(reviewApi.createDraftPullRequest).mockResolvedValue(created);
    const user = await openReviewView(
      makeReviewWorkspace({ recentReviews: [passed, blocked] }),
    );
    expect(
      screen.getAllByRole('button', { name: '创建草稿 PR' }),
    ).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: '创建草稿 PR' }));
    await waitFor(() =>
      expect(reviewApi.createDraftPullRequest).toHaveBeenCalledWith(
        projectPath,
        passed.reportId,
      ),
    );
    expect(
      await screen.findByText('草稿 PR #21 已创建；ColdStart 不会自动合并或创建 Release。'),
    ).toBeInTheDocument();
  });

  it('opens and refreshes an existing draft pull request', async () => {
    const draft = makePullRequest();
    const passed = makeReviewHistory('passed', { pullRequest: draft });
    const merged = makePullRequest({ state: 'merged', draft: false });
    const insights = makePullRequestInsights();
    vi.mocked(reviewApi.refreshPullRequest).mockResolvedValue(merged);
    const user = await openReviewView(
      makeReviewWorkspace({ recentReviews: [passed] }),
    );
    const link = screen.getByRole('link', { name: '在 GitHub 打开' });
    expect(link).toHaveAttribute('href', draft.htmlUrl);
    expect(link).toHaveAttribute('target', '_blank');
    expect(link).toHaveAttribute('rel', 'noreferrer');
    expect(screen.getByText('草稿 PR · 开放')).toBeInTheDocument();
    expect(
      screen.getByText('尚未同步合并就绪、Checks 与 Review 评论。点击“刷新状态”从 GitHub 只读获取。'),
    ).toBeInTheDocument();
    vi.mocked(reviewApi.readReviewWorkspace).mockResolvedValue(
      makeReviewWorkspace({
        recentReviews: [{ ...passed, pullRequest: merged, pullRequestInsights: insights }],
      }),
    );
    await user.click(screen.getByRole('button', { name: '刷新状态' }));
    await waitFor(() =>
      expect(reviewApi.refreshPullRequest).toHaveBeenCalledWith(
        projectPath,
        draft.id,
      ),
    );
    expect(await screen.findByText('PR · 已合并')).toBeInTheDocument();
    expect(
      await screen.findByText('PR #21 状态、合并就绪、Checks 与 Review 评论已刷新。'),
    ).toBeInTheDocument();
    expect(screen.getByText('PR 已合并，无需合并前检查')).toBeInTheDocument();
    expect(screen.getByText('Checks 全部通过')).toBeInTheDocument();
    expect(screen.getByText('1 项要求修改')).toBeInTheDocument();
  });

  it('shows synced checks, review conclusions and inline comments as read-only data', async () => {
    const passed = makeReviewHistory('passed', {
      pullRequest: makePullRequest({ draft: false }),
      pullRequestInsights: makePullRequestInsights(),
    });
    const user = await openReviewView(
      makeReviewWorkspace({ recentReviews: [passed] }),
    );
    await user.click(screen.getByText('查看 Checks、Review 与评论'));
    expect(screen.queryByText('1 项 Checks 失败')).not.toBeInTheDocument();
    expect(screen.getByText('合并冲突')).toBeInTheDocument();
    expect(screen.getByText('合并前检查 · 2 项待人工确认')).toBeInTheDocument();
    expect(screen.getByText('存在合并冲突')).toBeInTheDocument();
    expect(screen.getByText('本地定位到 2 个冲突文件')).toBeInTheDocument();
    expect(screen.getByText('1 个环境部署成功')).toBeInTheDocument();
    expect(screen.getByText('production')).toBeInTheDocument();
    expect(screen.getByText('生产部署完成')).toBeInTheDocument();
    expect(screen.getByText('src/order.ts')).toBeInTheDocument();
    expect(screen.getByText('src/payment.ts')).toBeInTheDocument();
    expect(
      screen.getByText('由本地 merge-tree 计算，未修改工作区；GitHub 状态仍是最终依据。'),
    ).toBeInTheDocument();
    expect(
      screen.getByText('部署成功不代表已获准发布或合并；ColdStart 不创建、审批、取消或重试部署。'),
    ).toBeInTheDocument();
    expect(screen.getAllByText('1 项 Review 要求修改').length).toBeGreaterThan(0);
    expect(screen.getByText('请补充失败分支测试。')).toBeInTheDocument();
    expect(screen.getByText('这里需要在失败时回滚。')).toBeInTheDocument();
    expect(screen.getByText('src/order.ts:42')).toBeInTheDocument();
    const commentLink = screen.getByRole('link', { name: '查看评论' });
    expect(commentLink).toHaveAttribute(
      'href',
      `${pullRequestUrl}#discussion_r1`,
    );
    expect(commentLink).toHaveAttribute('target', '_blank');
    expect(commentLink).toHaveAttribute('rel', 'noreferrer');
  });

  it('explains unavailable local conflict diagnostics without claiming resolution', async () => {
    const unavailable = makeReviewHistory('passed', {
      pullRequest: makePullRequest({ draft: false }),
      pullRequestInsights: makePullRequestInsights({
        conflictDetails: { state: 'unavailable', paths: [], truncated: false },
      }),
    });
    await openReviewView(
      makeReviewWorkspace({ recentReviews: [unavailable] }),
    );
    expect(screen.getByText('无法在本地定位冲突文件')).toBeInTheDocument();
    expect(
      screen.getByText('本地仓库缺少 PR 基准/头提交，或 Git 合并诊断失败。请先获取对应提交后再次刷新。'),
    ).toBeInTheDocument();
    expect(screen.queryByText('冲突已解决')).not.toBeInTheDocument();
  });

  it('explains legacy conflict snapshots without inventing a local result', async () => {
    const legacy = makeReviewHistory('passed', {
      pullRequest: makePullRequest({ draft: false }),
      pullRequestInsights: makePullRequestInsights({
        conflictDetails: { state: 'not_checked', paths: [], truncated: false },
      }),
    });
    await openReviewView(makeReviewWorkspace({ recentReviews: [legacy] }));
    expect(screen.getByText('尚未执行本地冲突定位')).toBeInTheDocument();
    expect(
      screen.getByText('这是阶段 15 之前的快照。再次刷新后会尝试从本地仓库定位冲突路径。'),
    ).toBeInTheDocument();
  });

  it('does not treat a local clean merge as resolved when GitHub still reports conflict', async () => {
    const notReproduced = makeReviewHistory('passed', {
      pullRequest: makePullRequest({ draft: false }),
      pullRequestInsights: makePullRequestInsights({
        conflictDetails: { state: 'not_reproduced', paths: [], truncated: false },
      }),
    });
    await openReviewView(
      makeReviewWorkspace({ recentReviews: [notReproduced] }),
    );
    expect(screen.getByText('本地未复现冲突')).toBeInTheDocument();
    expect(
      screen.getByText('GitHub 仍标记存在合并冲突。可能本地缺少最新目标分支或提交对象，请先同步后再次刷新；这不表示冲突已解决。'),
    ).toBeInTheDocument();
  });

  it('shows failed and in-progress deployment states without merge permission', async () => {
    const deployments = makeReviewHistory('passed', {
      pullRequest: makePullRequest({ draft: false, state: 'merged' }),
      pullRequestInsights: makePullRequestInsights({
        deploymentDetails: {
          state: 'available',
          deployments: [
            {
              environment: 'production',
              state: 'failure',
              description: '生产部署失败',
              statusUpdatedAt: '2026-11-13T08:07:00Z',
            },
            {
              environment: 'staging',
              state: 'in_progress',
              description: '预发部署中',
              statusUpdatedAt: null,
            },
          ],
          truncated: false,
        },
      }),
    });
    await openReviewView(makeReviewWorkspace({ recentReviews: [deployments] }));
    expect(screen.getByText('1 个环境部署失败')).toBeInTheDocument();
    expect(screen.getByText('生产部署失败')).toBeInTheDocument();
    expect(screen.getByText('staging')).toBeInTheDocument();
    expect(screen.getByText('部署中')).toBeInTheDocument();
    expect(screen.getByText('尚未获取部署状态记录')).toBeInTheDocument();
  });

  it('explains when the pull request head has no deployments', async () => {
    const noDeployments = makeReviewHistory('passed', {
      pullRequest: makePullRequest({ draft: false }),
      pullRequestInsights: makePullRequestInsights({
        deploymentDetails: {
          state: 'no_deployments',
          deployments: [],
          truncated: false,
        },
      }),
    });
    await openReviewView(
      makeReviewWorkspace({ recentReviews: [noDeployments] }),
    );
    expect(screen.getByText('无部署记录')).toBeInTheDocument();
    expect(
      screen.getByText('GitHub 未返回当前 PR 头提交的部署记录。'),
    ).toBeInTheDocument();
  });

  it('keeps a PR snapshot when deployment data is unavailable', async () => {
    const unavailable = makeReviewHistory('passed', {
      pullRequest: makePullRequest({ draft: false }),
      pullRequestInsights: makePullRequestInsights({
        deploymentDetails: {
          state: 'unavailable',
          deployments: [],
          truncated: false,
        },
      }),
    });
    await openReviewView(
      makeReviewWorkspace({ recentReviews: [unavailable] }),
    );
    expect(screen.getByText('部署状态不可用')).toBeInTheDocument();
    expect(
      screen.getByText(
        '本次刷新未能读取 GitHub Deployment；PR 状态、Checks、Review 与评论不受影响。',
      ),
    ).toBeInTheDocument();
  });

  it('explains legacy deployment snapshots without inventing a status', async () => {
    const legacy = makeReviewHistory('passed', {
      pullRequest: makePullRequest({ draft: false }),
      pullRequestInsights: makePullRequestInsights({
        deploymentDetails: {
          state: 'not_checked',
          deployments: [],
          truncated: false,
        },
      }),
    });
    await openReviewView(makeReviewWorkspace({ recentReviews: [legacy] }));
    expect(screen.getByText('部署状态未同步')).toBeInTheDocument();
    expect(
      screen.getByText(
        '这是阶段 16 之前的快照。再次刷新后会读取当前 PR 头提交的部署状态。',
      ),
    ).toBeInTheDocument();
  });

  it('offers a two-step audit waiver only for blocked reviews', async () => {
    const blocked = makeReviewHistory('blocked');
    const failed = makeReviewHistory('failed', {
      reportId: '15151515-1515-4515-8515-151515151515',
      taskKey: 'slice:order:failed',
    });
    const user = await openReviewView(
      makeReviewWorkspace({ recentReviews: [blocked, failed] }),
    );

    expect(
      screen.getAllByRole('button', { name: '人工豁免…' }),
    ).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: '人工豁免…' }));
    await user.type(
      screen.getByLabelText('人工豁免理由'),
      '该错误分支由框架统一兜底，已有集成测试覆盖。',
    );
    await user.click(screen.getByRole('button', { name: '继续确认' }));
    expect(reviewApi.waiveWorkReview).not.toHaveBeenCalled();

    vi.mocked(reviewApi.waiveWorkReview).mockResolvedValue(
      {} as WorkReportRecord,
    );
    vi.mocked(reviewApi.readReviewWorkspace).mockResolvedValue(
      makeReviewWorkspace({
        recentReviews: [
          {
            ...blocked,
            reviewStatus: 'waived',
            waiver: {
              reason: '该错误分支由框架统一兜底，已有集成测试覆盖。',
              waivedBy: 'tester',
              waivedAt: '2026-11-02T09:20:00Z',
              findingsHash: 'c'.repeat(64),
              standardsHash: 'd'.repeat(64),
            },
          },
          failed,
        ],
      }),
    );
    vi.mocked(planApi.readPlanWorkspace).mockResolvedValue(makeWorkspace());

    await user.click(screen.getByRole('button', { name: '确认豁免并推送' }));

    await waitFor(() =>
      expect(reviewApi.waiveWorkReview).toHaveBeenCalledWith(
        projectPath,
        blocked.workItemId,
        blocked.reportId,
        '该错误分支由框架统一兜底，已有集成测试覆盖。',
      ),
    );
    expect(planApi.readPlanWorkspace).toHaveBeenCalledWith(projectPath);
    expect(
      await screen.findByText('人工豁免已记录；任务完成并进入推送流程。'),
    ).toBeInTheDocument();
  });
});
