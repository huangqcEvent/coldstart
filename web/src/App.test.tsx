import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import App from './App';
import * as planApi from './api/planApi';
import * as projectApi from './api/projectApi';
import * as requirementsApi from './api/requirementsApi';
import type {
  BlueprintContent,
  BlueprintRevision,
  DocumentAnchor,
  InfoRequest,
  PlanContent,
  PlanRevision,
  PlanWorkspace,
  ProjectStatus,
  ProjectWorkspace,
  TaskSpec,
  WorkItemView,
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
  publishPlan: vi.fn(),
  completeHumanWork: vi.fn(),
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

beforeEach(() => {
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
  vi.mocked(planApi.publishPlan).mockReset();
  vi.mocked(planApi.completeHumanWork).mockReset();
});

describe('ColdStart project workspace', () => {
  it('shows the welcome view and gates project navigation', () => {
    render(<App />);

    expect(screen.getByText('COLDSTART / 冷启动')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '冷启动' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '项目总览' })).toBeEnabled();
    expect(screen.getByRole('button', { name: '需求导入' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '蓝图编排' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '人工 Inbox' })).toBeDisabled();
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

    await user.click(screen.getByRole('button', { name: '计划看板' }));
    await user.click(screen.getByRole('button', { name: '编译开发计划' }));
    expect(await screen.findByText('计划草稿 v1')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '发布计划' }));

    expect(await screen.findByText('可领取')).toBeInTheDocument();
    expect(planApi.publishPlan).toHaveBeenCalledWith(projectPath, draftPlan.id);
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
});
