import { useId, useState } from 'react';

import type { ArtifactMetadata, ModelConfig } from '../api/types';

interface RequirementsViewProps {
  sources: ArtifactMetadata[];
  busy: boolean;
  busyLabel: string | null;
  onImport: () => void;
  onGenerate: (config: ModelConfig) => void;
}

const formatLabels: Record<ArtifactMetadata['format'], string> = {
  markdown: 'Markdown',
  text: 'TXT',
  docx: 'DOCX',
  pdf: 'PDF',
};

export function RequirementsView({
  sources,
  busy,
  busyLabel,
  onImport,
  onGenerate,
}: RequirementsViewProps) {
  const baseUrlId = useId();
  const modelId = useId();
  const apiKeyId = useId();
  const consentId = useId();
  const [baseUrl, setBaseUrl] = useState('');
  const [model, setModel] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [consent, setConsent] = useState(false);

  const configReady =
    baseUrl.trim().length > 0 && model.trim().length > 0 && sources.length > 0;
  const canGenerate = configReady && consent && !busy;

  return (
    <section className="content-view" aria-labelledby="requirements-title">
      <header className="content-view__header">
        <div>
          <p className="eyebrow">需求与来源</p>
          <h1 id="requirements-title">需求导入</h1>
          <p className="content-view__lead">
            原文件、抽取文本和锚点会一起固化到项目目录。模型只能引用这些真实锚点生成蓝图。
          </p>
        </div>
        <button
          className="button button--primary"
          type="button"
          onClick={onImport}
          disabled={busy}
        >
          {busyLabel === '正在导入需求文档' ? '导入中…' : '导入需求文档'}
        </button>
      </header>

      <div className="split-layout">
        <section className="panel" aria-labelledby="source-list-title">
          <div className="panel__head">
            <div>
              <p className="panel__eyebrow">文件与锚点</p>
              <h2 id="source-list-title">来源文档</h2>
            </div>
            <span className="count-badge">{sources.length}</span>
          </div>

          {sources.length === 0 ? (
            <div className="empty-state">
              <strong>还没有来源文档</strong>
              <p>支持 Markdown、TXT、DOCX、PDF，单个文件不超过 20 MiB。</p>
            </div>
          ) : (
            <ul className="source-list" aria-label="来源文档列表">
              {sources.map((source, index) => (
                <li className="source-item" key={source.id}>
                  <span className="source-item__index">
                    {String(index + 1).padStart(2, '0')}
                  </span>
                  <div className="source-item__body">
                    <div className="source-item__title-row">
                      <strong>{source.fileName}</strong>
                      <span className="format-badge">
                        {formatLabels[source.format]}
                      </span>
                    </div>
                    <p className="mono source-item__hash">
                      SHA-256 {source.contentHash.slice(0, 16)}…
                    </p>
                    <p className="source-item__meta">
                      {(source.byteSize / 1024).toFixed(1)} KiB · 已导入{' '}
                      {source.importedAt}
                    </p>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="panel" aria-labelledby="model-config-title">
          <div className="panel__head">
            <div>
              <p className="panel__eyebrow">模型连接</p>
              <h2 id="model-config-title">生成模型</h2>
            </div>
            <span className="status-dot">仅本次会话</span>
          </div>

          <div className="form-grid form-grid--single">
            <label className="field" htmlFor={baseUrlId}>
              <span>Base URL</span>
              <input
                id={baseUrlId}
                type="url"
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
                placeholder="https://api.example.com/v1"
                autoComplete="off"
                disabled={busy}
              />
            </label>
            <label className="field" htmlFor={modelId}>
              <span>模型名称</span>
              <input
                id={modelId}
                type="text"
                value={model}
                onChange={(event) => setModel(event.target.value)}
                placeholder="例如：gpt-4.1-mini"
                autoComplete="off"
                disabled={busy}
              />
            </label>
            <label className="field" htmlFor={apiKeyId}>
              <span>API Key</span>
              <input
                id={apiKeyId}
                type="password"
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                placeholder="仅保存在当前界面内存"
                autoComplete="off"
                disabled={busy}
              />
            </label>
          </div>

          <label className="consent-row" htmlFor={consentId}>
            <input
              id={consentId}
              type="checkbox"
              checked={consent}
              onChange={(event) => setConsent(event.target.checked)}
              disabled={busy}
            />
            <span>
              我确认将当前需求文档发送到所配置的模型服务。API Key
              不会写入项目文件、数据库、事件或日志。
            </span>
          </label>

          <p className="field__hint">
            生成前会检查来源总量不超过 80,000 字符。模型返回的每个来源锚点都会在写入草稿前核验。
          </p>

          <button
            className="button button--primary button--wide"
            type="button"
            onClick={() =>
              onGenerate({
                baseUrl: baseUrl.trim(),
                model: model.trim(),
                apiKey,
              })
            }
            disabled={!canGenerate}
          >
            {busyLabel === '正在生成蓝图草稿'
              ? '正在生成…'
              : '生成蓝图草稿'}
          </button>
          {!consent ? (
            <p className="field__hint field__hint--warning">
              勾选外部发送确认后才能生成。
            </p>
          ) : null}
        </section>
      </div>
    </section>
  );
}
