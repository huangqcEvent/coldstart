import { useEffect, useState } from 'react';

import * as reviewApi from '../api/reviewApi';
import type {
  ReviewScope,
  ReviewStandardContent,
  ReviewStandardImport,
  ReviewStandardManualInput,
  ReviewStandardRevisionInput,
} from '../api/types';

export type ReviewStandardEditorMode = 'upload' | 'manual' | 'view' | 'revise';

interface ReviewStandardEditorProps {
  mode: ReviewStandardEditorMode;
  content: ReviewStandardContent | null;
  busy: boolean;
  onClose: () => void;
  onImport: (input: ReviewStandardImport) => Promise<boolean>;
  onCreate: (input: ReviewStandardManualInput) => Promise<boolean>;
  onRevise: (
    standardId: string,
    input: ReviewStandardRevisionInput,
  ) => Promise<boolean>;
}

const repositoryKeys = ['server', 'admin', 'web', 'app', 'mini_program'];

function scopeValue(scope: ReviewScope | undefined) {
  if (scope?.kind === 'repository') {
    return `repository:${scope.repositoryKey}`;
  }
  return 'project';
}

function parseScope(value: string): ReviewScope {
  if (value.startsWith('repository:')) {
    return {
      kind: 'repository',
      repositoryKey: value.slice('repository:'.length),
    };
  }
  return { kind: 'project' };
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

function standardNameFromPath(path: string) {
  return fileName(path).replace(/\.[^.]+$/, '');
}

function formatTimestamp(value: string) {
  return new Date(value).toLocaleString('zh-CN', { hour12: false });
}

export function ReviewStandardEditor({
  mode,
  content,
  busy,
  onClose,
  onImport,
  onCreate,
  onRevise,
}: ReviewStandardEditorProps) {
  const [name, setName] = useState('');
  const [scope, setScope] = useState('project');
  const [text, setText] = useState('');
  const [filePath, setFilePath] = useState('');
  const [localError, setLocalError] = useState<string | null>(null);

  useEffect(() => {
    setName(content?.standard.name ?? '');
    setScope(scopeValue(content?.standard.scope));
    setText(content?.text ?? '');
    setFilePath('');
    setLocalError(null);
  }, [mode, content?.standard.id, content?.revisionId, content?.text]);

  async function chooseFile() {
    setLocalError(null);
    try {
      const selected = await reviewApi.pickReviewStandardFile();
      if (selected) {
        setFilePath(selected);
        if (!name.trim()) {
          setName(standardNameFromPath(selected));
        }
      }
    } catch (error) {
      setLocalError(String(error));
    }
  }

  async function submit() {
    const normalizedName = name.trim();
    if (!normalizedName) {
      setLocalError('请填写规范名称。');
      return;
    }

    if (mode === 'upload') {
      if (!filePath) {
        setLocalError('请选择要导入的规范文件。');
        return;
      }
      await onImport({
        name: normalizedName,
        scope: parseScope(scope),
        path: filePath,
      });
      return;
    }

    if (mode === 'manual' || mode === 'revise') {
      if (!text.trim()) {
        setLocalError('规范正文不能为空。');
        return;
      }
      const input = {
        name: normalizedName,
        scope: parseScope(scope),
        text,
      };
      if (mode === 'manual') {
        await onCreate(input);
      } else if (content) {
        await onRevise(content.standard.id, input);
      }
    }
  }

  const title =
    mode === 'upload'
      ? '上传评审规范'
      : mode === 'manual'
        ? '手工新建规范'
        : mode === 'revise'
          ? '创建规范新修订'
          : '查看规范原文';

  return (
    <div className="dialog-backdrop" role="presentation">
      <section
        className="dialog review-standard-editor"
        role="dialog"
        aria-modal="true"
        aria-labelledby="review-standard-editor-title"
      >
        <div className="dialog__head">
          <div>
            <p className="eyebrow">代码规范 · 版本化</p>
            <h2 id="review-standard-editor-title">{title}</h2>
          </div>
          <button
            className="error-banner__close"
            type="button"
            aria-label="关闭规范编辑器"
            onClick={onClose}
            disabled={busy}
          >
            <span aria-hidden="true">×</span>
          </button>
        </div>

        {mode === 'view' && content ? (
          <>
            <div className="review-standard-editor__meta">
              <span>修订 v{content.standard.revisionNumber}</span>
              <span>{content.standard.characterCount.toLocaleString()} 字符</span>
              <span>
                {content.standard.source === 'upload' ? '文件导入' : '手工维护'}
              </span>
              <span>更新 {formatTimestamp(content.standard.updatedAt)}</span>
            </div>
            {content.standard.fileName !== 'standard.md' ? (
              <p className="field__hint">原始文件：{content.standard.fileName}</p>
            ) : null}
            <pre className="review-standard-editor__preview">{content.text}</pre>
            <div className="dialog__actions">
              <button className="button button--ghost" type="button" onClick={onClose}>
                关闭
              </button>
            </div>
          </>
        ) : (
          <div className="dialog__form review-standard-editor__form">
            <label className="field">
              <span>规范名称</span>
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                maxLength={120}
                aria-label="评审规范名称"
                disabled={busy}
              />
            </label>
            <label className="field">
              <span>适用范围</span>
              <select
                value={scope}
                onChange={(event) => setScope(event.target.value)}
                aria-label="评审规范适用范围"
                disabled={busy}
              >
                <option value="project">全项目</option>
                {repositoryKeys.map((key) => (
                  <option value={`repository:${key}`} key={key}>
                    仓库 {key}
                  </option>
                ))}
              </select>
            </label>

            {mode === 'upload' ? (
              <div className="field integration-form__wide">
                <span>规范文件</span>
                <div className="review-standard-editor__file">
                  <code>{filePath ? fileName(filePath) : '尚未选择文件'}</code>
                  <button
                    className="button button--secondary"
                    type="button"
                    onClick={() => void chooseFile()}
                    disabled={busy}
                  >
                    选择文件
                  </button>
                </div>
                <p className="field__hint">
                  支持 Markdown、TXT、DOCX 和 PDF。ColdStart 会保存原文件与提取文本，
                  并在后续读取时校验哈希。
                </p>
              </div>
            ) : (
              <label className="field integration-form__wide">
                <span>规范正文（Markdown）</span>
                <textarea
                  value={text}
                  onChange={(event) => setText(event.target.value)}
                  rows={18}
                  aria-label="评审规范正文"
                  disabled={busy}
                />
                <small className="field__hint">
                  {text.length.toLocaleString()} 字符，单个规范最多 60,000 字符，并且不会静默截断。
                </small>
              </label>
            )}

            {localError ? (
              <p className="integration-error" role="alert">
                {localError}
              </p>
            ) : null}

            <div className="dialog__actions">
              <button
                className="button button--ghost"
                type="button"
                onClick={onClose}
                disabled={busy}
              >
                取消
              </button>
              <button
                className="button button--primary"
                type="button"
                onClick={() => void submit()}
                disabled={busy}
              >
                {busy
                  ? '保存中…'
                  : mode === 'upload'
                    ? '导入并保存'
                    : mode === 'revise'
                      ? '保存新修订'
                      : '创建规范'}
              </button>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
