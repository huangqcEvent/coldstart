/**
 * 蓝图查看与编辑页面。
 *
 * 组合来源导入、手工编辑、信息请求和发布动作，并只在用户显式保存后刷新工作区。
 */
import { useEffect, useState } from 'react';

import type { BlueprintContent, ProjectWorkspace } from '../api/types';
import {
  canCreateManualBlueprint,
  cloneBlueprint,
  createManualBlueprint,
} from '../blueprintFactory';
import { BlueprintEditor } from './BlueprintEditor';

interface BlueprintViewProps {
  workspace: ProjectWorkspace;
  busy: boolean;
  busyLabel: string | null;
  onSave: (content: BlueprintContent) => Promise<boolean>;
  onPublish: (revisionId: string) => Promise<boolean>;
  onNavigateRequirements: () => void;
}

export function BlueprintView({
  workspace,
  busy,
  busyLabel,
  onSave,
  onPublish,
  onNavigateRequirements,
}: BlueprintViewProps) {
  const activeRevision = workspace.draft ?? workspace.published;
  const [content, setContent] = useState<BlueprintContent | null>(() =>
    activeRevision ? cloneBlueprint(activeRevision.content) : null,
  );
  const [isDirty, setIsDirty] = useState(false);
  const [isLocalDraft, setIsLocalDraft] = useState(false);

  useEffect(() => {
    const revision = workspace.draft ?? workspace.published;
    setContent(revision ? cloneBlueprint(revision.content) : null);
    setIsDirty(false);
    setIsLocalDraft(false);
  }, [workspace.draft?.id, workspace.published?.id]);

  const readOnly = workspace.draft === null && !isLocalDraft;
  const canCreateDraft = canCreateManualBlueprint(
    workspace.sources,
    workspace.sourceAnchors,
  );

  function handleCreateDraft() {
    const draft = workspace.published
      ? cloneBlueprint(workspace.published.content)
      : createManualBlueprint(workspace.sources, workspace.sourceAnchors);
    setContent(draft);
    setIsDirty(true);
    setIsLocalDraft(true);
  }

  async function handleSave() {
    if (!content) {
      return;
    }
    const saved = await onSave(content);
    if (saved) {
      setIsDirty(false);
      setIsLocalDraft(false);
    }
  }

  const saveBusy = busyLabel === '正在保存蓝图草稿';
  const publishBusy = busyLabel === '正在发布蓝图';

  return (
    <section className="content-view" aria-labelledby="blueprint-title">
      <header className="content-view__header">
        <div>
          <p className="eyebrow">交付蓝图</p>
          <h1 id="blueprint-title">蓝图编排</h1>
          <p className="content-view__lead">
            将需求转成可校验的产品、平台、模块与发布边界。发布版本不可变，人工修改会生成新的草稿版本。
          </p>
        </div>
        <div className="revision-summary">
          <span className="status-dot">
            {workspace.draft
              ? `草稿 v${workspace.draft.revisionNumber}`
              : workspace.published
                ? `已发布 v${workspace.published.revisionNumber}`
                : '尚无蓝图'}
          </span>
          {activeRevision ? (
            <code>{activeRevision.generation.source === 'model' ? 'MODEL' : 'MANUAL'}</code>
          ) : null}
        </div>
      </header>

      {!content ? (
        <section className="panel blueprint-empty">
          <div className="empty-state">
            <p className="panel__eyebrow">尚未生成</p>
            <strong>还没有蓝图草稿</strong>
            <p>
              你可以先生成模型草稿，也可以从来源文档创建一份最小人工草稿，再补齐平台、模块与技术决策。
            </p>
            <div className="view-actions">
              <button
                className="button button--primary"
                type="button"
                onClick={handleCreateDraft}
                disabled={!canCreateDraft || busy}
              >
                创建人工草稿
              </button>
              {!canCreateDraft ? (
                <button className="button button--secondary" type="button" onClick={onNavigateRequirements}>
                  先导入需求文档
                </button>
              ) : null}
            </div>
            {!canCreateDraft ? (
              <p className="field__hint field__hint--warning">
                人工草稿的产品目标必须引用一条真实来源锚点。
              </p>
            ) : null}
          </div>
        </section>
      ) : (
        <>
          <div className="blueprint-meta">
            <div>
              <span>平台</span>
              <strong>{content.platforms.length}</strong>
            </div>
            <div>
              <span>模块</span>
              <strong>{content.modules.length}</strong>
            </div>
            <div>
              <span>技术决策</span>
              <strong>{content.technologies.length}</strong>
            </div>
            <div>
              <span>发布条件</span>
              <strong>{content.releasePrerequisites.length}</strong>
            </div>
          </div>

          {readOnly && workspace.published ? (
            <section className="readonly-notice" aria-label="已发布蓝图">
              <div>
                <p className="panel__eyebrow">已发布 · 不可变</p>
                <strong>当前展示已发布版本 v{workspace.published.revisionNumber}</strong>
                <p>此版本只读。创建新草稿后可继续调整，发布时会生成新的不可变版本。</p>
              </div>
              <button
                className="button button--primary"
                type="button"
                onClick={handleCreateDraft}
                disabled={busy}
              >
                基于已发布版本创建新草稿
              </button>
            </section>
          ) : null}

          <BlueprintEditor
            content={content}
            artifacts={workspace.sources}
            sourceAnchors={workspace.sourceAnchors}
            readOnly={readOnly}
            onChange={(nextContent) => {
              setContent(nextContent);
              setIsDirty(true);
            }}
          />

          {!readOnly ? (
            <div className="editor-actions">
              <div>
                <p className="panel__eyebrow">保存与发布</p>
                <p>
                  保存会创建新的草稿版本；只有保存成功且未再修改的草稿才能发布。
                </p>
              </div>
              <div className="view-actions">
                <button
                  className="button button--secondary"
                  type="button"
                  onClick={() => void handleSave()}
                  disabled={busy}
                >
                  {saveBusy ? '保存中…' : '保存草稿'}
                </button>
                <button
                  className="button button--primary"
                  type="button"
                  onClick={() => {
                    if (workspace.draft) {
                      void onPublish(workspace.draft.id);
                    }
                  }}
                  disabled={busy || !workspace.draft || isDirty}
                >
                  {publishBusy ? '发布中…' : '发布当前草稿'}
                </button>
              </div>
              {isDirty ? (
                <p className="field__hint field__hint--warning editor-actions__hint">
                  当前有未保存修改，请先保存草稿再发布。
                </p>
              ) : null}
            </div>
          ) : null}
        </>
      )}
    </section>
  );
}
