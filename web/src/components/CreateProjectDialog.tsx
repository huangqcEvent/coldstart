import {
  useEffect,
  useId,
  useRef,
  useState,
  type FormEvent,
} from 'react';

interface CreateProjectDialogProps {
  submitting: boolean;
  onCancel: () => void;
  onPickDirectory: () => Promise<string | null>;
  onCreate: (name: string, path: string) => void;
}

export function CreateProjectDialog({
  submitting,
  onCancel,
  onPickDirectory,
  onCreate,
}: CreateProjectDialogProps) {
  const titleId = useId();
  const nameId = useId();
  const pathId = useId();
  const nameInputRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [isPicking, setIsPicking] = useState(false);

  useEffect(() => {
    nameInputRef.current?.focus();
  }, []);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape' && !submitting) {
        onCancel();
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onCancel, submitting]);

  async function handlePickDirectory() {
    setIsPicking(true);
    try {
      const selected = await onPickDirectory();
      if (selected !== null) {
        setPath(selected);
      }
    } finally {
      setIsPicking(false);
    }
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!name.trim() || !path || submitting || isPicking) {
      return;
    }
    onCreate(name.trim(), path);
  }

  const canSubmit = Boolean(name.trim() && path) && !submitting && !isPicking;

  return (
    <div className="dialog-backdrop">
      <section
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <div className="dialog__head">
          <div>
            <p className="eyebrow">新建项目</p>
            <h2 id={titleId}>新建项目</h2>
          </div>
          <span className="dialog__step">01</span>
        </div>

        <form className="dialog__form" onSubmit={handleSubmit}>
          <div className="field">
            <label htmlFor={nameId}>项目名称</label>
            <input
              ref={nameInputRef}
              id={nameId}
              type="text"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="例如：冻土实验室"
              autoComplete="off"
              disabled={submitting}
            />
          </div>

          <div className="field">
            <span className="field__label" id={pathId}>
              项目主目录
            </span>
            <div className="directory-picker">
              <output
                className={path ? 'directory-picker__value' : 'directory-picker__empty'}
                aria-labelledby={pathId}
              >
                {path || '尚未选择目录'}
              </output>
              <button
                className="button button--ghost"
                type="button"
                onClick={() => void handlePickDirectory()}
                disabled={submitting || isPicking}
              >
                {isPicking ? '选择中…' : '选择目录'}
              </button>
            </div>
            <p className="field__hint">
              选择一个空目录，ColdStart 将在其中初始化项目清单与本地状态。
            </p>
          </div>

          {submitting ? (
            <p className="dialog__progress" aria-live="polite">
              正在创建项目…
            </p>
          ) : null}

          <div className="dialog__actions">
            <button
              className="button button--quiet"
              type="button"
              onClick={onCancel}
              disabled={submitting}
            >
              取消
            </button>
            <button
              className="button button--primary"
              type="submit"
              disabled={!canSubmit}
            >
              {submitting ? '创建中…' : '创建项目'}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}
