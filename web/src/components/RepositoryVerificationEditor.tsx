import { open } from '@tauri-apps/plugin-dialog';
import { useEffect, useRef, useState } from 'react';

import type {
  Repository,
  RepositoryVerificationCommand,
  RepositoryVerificationView,
} from '../api/types';

interface RepositoryVerificationEditorProps {
  repositories: Repository[];
  configurations: RepositoryVerificationView[];
  busy: boolean;
  onSave: (
    repositoryKey: string,
    commands: RepositoryVerificationCommand[],
  ) => Promise<boolean>;
}

interface DraftCommand {
  rowId: number;
  name: string;
  executable: string;
  argsText: string;
  timeoutSeconds: number;
}

function draftRows(commands: RepositoryVerificationCommand[]): DraftCommand[] {
  return commands.map((command, index) => ({
    rowId: index + 1,
    name: command.name,
    executable: command.executable,
    argsText: command.args.join('\n'),
    timeoutSeconds: command.timeoutSeconds,
  }));
}

function blankRow(rowId: number): DraftCommand {
  return {
    rowId,
    name: '',
    executable: '',
    argsText: '',
    timeoutSeconds: 300,
  };
}

export function RepositoryVerificationEditor({
  repositories,
  configurations,
  busy,
  onSave,
}: RepositoryVerificationEditorProps) {
  const [selectedKey, setSelectedKey] = useState(repositories[0]?.key ?? '');
  const [rows, setRows] = useState<DraftCommand[]>([]);
  const [dirty, setDirty] = useState(false);
  const nextRowId = useRef(1);
  const configuration = configurations.find(
    (item) => item.repositoryKey === selectedKey,
  );

  useEffect(() => {
    const commands = configuration?.commands ?? [];
    nextRowId.current = commands.length + 1;
    setRows(draftRows(commands));
    setDirty(false);
  }, [selectedKey, configuration?.commands]);

  useEffect(() => {
    if (
      repositories.length > 0 &&
      !repositories.some((repository) => repository.key === selectedKey)
    ) {
      setSelectedKey(repositories[0].key);
    }
  }, [repositories, selectedKey]);

  function updateRow(rowId: number, update: Partial<DraftCommand>) {
    setRows((current) =>
      current.map((row) => (row.rowId === rowId ? { ...row, ...update } : row)),
    );
    setDirty(true);
  }

  function addRow() {
    const rowId = nextRowId.current;
    nextRowId.current += 1;
    setRows((current) => [...current, blankRow(rowId)]);
    setDirty(true);
  }

  function deleteRow(rowId: number) {
    setRows((current) => current.filter((row) => row.rowId !== rowId));
    setDirty(true);
  }

  function moveRow(index: number, offset: -1 | 1) {
    setRows((current) => {
      const target = index + offset;
      if (target < 0 || target >= current.length) {
        return current;
      }
      const next = [...current];
      [next[index], next[target]] = [next[target], next[index]];
      return next;
    });
    setDirty(true);
  }

  async function selectExecutable(rowId: number) {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Windows 可执行文件', extensions: ['exe'] }],
    });
    if (typeof selected === 'string') {
      updateRow(rowId, { executable: selected });
    }
  }

  async function save() {
    const commands = rows.map<RepositoryVerificationCommand>((row, index) => ({
      name: row.name.trim(),
      executable: row.executable.trim(),
      args: row.argsText
        .split(/\r?\n/)
        .map((argument) => argument.trim())
        .filter((argument) => argument.length > 0),
      timeoutSeconds: row.timeoutSeconds,
      position: index,
    }));
    if (await onSave(selectedKey, commands)) {
      setDirty(false);
    }
  }

  if (repositories.length === 0) {
    return null;
  }

  return (
    <section
      className="integration-subsection verification-editor"
      aria-labelledby="verification-editor-title"
    >
      <div className="panel__head">
        <div>
          <p className="panel__eyebrow">PHASE 07 / COMMAND VERIFICATION</p>
          <h3 id="verification-editor-title">仓库验证门禁</h3>
        </div>
        <span className="count-badge">{rows.length} / 8</span>
      </div>
      <p className="field__hint">
        Codex 上报完成后，ColdStart 会在仓库根目录直接执行这些 .exe，不经过
        Shell。全部通过后任务才会完成；失败时保留租约供修复重试。
      </p>

      <div className="verification-editor__repositories">
        {repositories.map((repository) => {
          const count =
            configurations.find((item) => item.repositoryKey === repository.key)
              ?.commands.length ?? 0;
          return (
            <button
              className={
                repository.key === selectedKey
                  ? 'verification-repository verification-repository--active'
                  : 'verification-repository'
              }
              type="button"
              key={repository.id}
              onClick={() => setSelectedKey(repository.key)}
              aria-pressed={repository.key === selectedKey}
            >
              <code>{repository.key}</code>
              <span>{count ? `${count} 条` : '不可领取'}</span>
            </button>
          );
        })}
      </div>

      <label className="field verification-editor__selector">
        <span>配置仓库</span>
        <select
          value={selectedKey}
          onChange={(event) => setSelectedKey(event.target.value)}
          disabled={busy}
        >
          {repositories.map((repository) => (
            <option value={repository.key} key={repository.id}>
              {repository.name} · {repository.key}
            </option>
          ))}
        </select>
      </label>

      {rows.length === 0 ? (
        <div className="verification-editor__warning" role="status">
          <strong>该仓库没有验证命令</strong>
          <p>Codex 无法领取此仓库的编码任务。至少添加一条可直接启动的 .exe 命令。</p>
        </div>
      ) : null}

      <div className="verification-command-list">
        {rows.map((row, index) => (
          <article className="verification-command" key={row.rowId}>
            <header>
              <strong>命令 {index + 1}</strong>
              <div className="verification-command__order">
                <button
                  className="button button--quiet"
                  type="button"
                  onClick={() => moveRow(index, -1)}
                  disabled={busy || index === 0}
                  aria-label={`上移命令 ${index + 1}`}
                >
                  上移
                </button>
                <button
                  className="button button--quiet"
                  type="button"
                  onClick={() => moveRow(index, 1)}
                  disabled={busy || index === rows.length - 1}
                  aria-label={`下移命令 ${index + 1}`}
                >
                  下移
                </button>
                <button
                  className="button button--quiet button--danger"
                  type="button"
                  onClick={() => deleteRow(row.rowId)}
                  disabled={busy}
                  aria-label={`删除命令 ${index + 1}`}
                >
                  删除
                </button>
              </div>
            </header>
            <div className="verification-command__grid">
              <label className="field">
                <span>名称</span>
                <input
                  value={row.name}
                  onChange={(event) =>
                    updateRow(row.rowId, { name: event.target.value })
                  }
                  placeholder="测试或构建"
                  aria-label={`验证命令名称 ${index + 1}`}
                  disabled={busy}
                />
              </label>
              <label className="field">
                <span>超时（秒）</span>
                <input
                  type="number"
                  min={10}
                  max={900}
                  value={row.timeoutSeconds}
                  onChange={(event) =>
                    updateRow(row.rowId, {
                      timeoutSeconds: Number(event.target.value),
                    })
                  }
                  aria-label={`验证命令超时 ${index + 1}`}
                  disabled={busy}
                />
              </label>
              <label className="field verification-command__wide">
                <span>可执行文件（绝对路径）</span>
                <div className="verification-command__path">
                  <input
                    value={row.executable}
                    onChange={(event) =>
                      updateRow(row.rowId, { executable: event.target.value })
                    }
                    placeholder={String.raw`D:\tools\gradle.exe`}
                    aria-label={`验证命令可执行文件 ${index + 1}`}
                    disabled={busy}
                  />
                  <button
                    className="button button--secondary"
                    type="button"
                    onClick={() => void selectExecutable(row.rowId)}
                    disabled={busy}
                    aria-label={`选择可执行文件 ${index + 1}`}
                  >
                    选择 .exe
                  </button>
                </div>
              </label>
              <label className="field verification-command__wide">
                <span>参数（每行一个，不经过 Shell）</span>
                <textarea
                  value={row.argsText}
                  onChange={(event) =>
                    updateRow(row.rowId, { argsText: event.target.value })
                  }
                  placeholder={'test\n--no-daemon'}
                  rows={3}
                  aria-label={`验证命令参数 ${index + 1}`}
                  disabled={busy}
                />
              </label>
            </div>
          </article>
        ))}
      </div>

      <div className="view-actions integration-form__actions">
        <button
          className="button button--secondary"
          type="button"
          onClick={addRow}
          disabled={busy || rows.length >= 8}
        >
          新增验证命令
        </button>
        <button
          className="button button--primary"
          type="button"
          onClick={() => void save()}
          disabled={busy || !dirty || !selectedKey}
        >
          保存验证命令
        </button>
      </div>
    </section>
  );
}
