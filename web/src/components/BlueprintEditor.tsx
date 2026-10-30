import type {
  ArtifactMetadata,
  BlueprintContent,
  DocumentAnchor,
  ModuleDefinition,
  PlatformKind,
  RiskSeverity,
  SourceReference,
} from '../api/types';
import { createEntityId } from '../blueprintFactory';

interface BlueprintEditorProps {
  content: BlueprintContent;
  artifacts: ArtifactMetadata[];
  sourceAnchors: Record<string, DocumentAnchor[]>;
  readOnly: boolean;
  onChange: (content: BlueprintContent) => void;
}

const platformKindOptions: Array<{ value: PlatformKind; label: string }> = [
  { value: 'web', label: 'Web 前台' },
  { value: 'admin', label: '管理后台' },
  { value: 'server', label: '服务端' },
  { value: 'ios', label: 'iOS App' },
  { value: 'android', label: 'Android App' },
  { value: 'mini_program', label: '小程序' },
];

const severityOptions: Array<{ value: RiskSeverity; label: string }> = [
  { value: 'low', label: '低' },
  { value: 'medium', label: '中' },
  { value: 'high', label: '高' },
];

function replaceAt<T>(items: T[], index: number, value: T) {
  return items.map((item, itemIndex) => (itemIndex === index ? value : item));
}

function removeAt<T>(items: T[], index: number) {
  return items.filter((_, itemIndex) => itemIndex !== index);
}

function SourceReferences({
  references,
  artifacts,
  sourceAnchors,
}: {
  references: SourceReference[];
  artifacts: ArtifactMetadata[];
  sourceAnchors: Record<string, DocumentAnchor[]>;
}) {
  const artifactsById = new Map(artifacts.map((artifact) => [artifact.id, artifact]));

  return (
    <div className="source-references" aria-label="来源引用">
      <span className="source-references__label">来源引用</span>
      {references.length === 0 ? (
        <span className="source-references__empty">暂无来源锚点</span>
      ) : (
        <ul>
          {references.map((reference) => {
            const artifact = artifactsById.get(reference.artifactId);
            const anchor = sourceAnchors[reference.artifactId]?.find(
              (item) => item.id === reference.anchorId,
            );
            return (
              <li
                key={`${reference.artifactId}:${reference.anchorId}`}
                title={reference.quote ?? anchor?.text ?? undefined}
              >
                <span>{artifact?.fileName ?? reference.artifactId}</span>
                <span aria-hidden="true">·</span>
                <span>{anchor?.label ?? reference.anchorId}</span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

function SectionHeading({
  index,
  title,
  hint,
  readOnly,
  addLabel,
  onAdd,
}: {
  index: string;
  title: string;
  hint: string;
  readOnly: boolean;
  addLabel?: string;
  onAdd?: () => void;
}) {
  return (
    <header className="editor-section__head">
      <div>
        <p className="panel__eyebrow">{index}</p>
        <h2>{title}</h2>
        <p>{hint}</p>
      </div>
      {!readOnly && addLabel && onAdd ? (
        <button className="button button--ghost" type="button" onClick={onAdd}>
          {addLabel}
        </button>
      ) : null}
    </header>
  );
}

function ScopeSection({
  title,
  index,
  hint,
  scopeName,
  items,
  artifacts,
  sourceAnchors,
  readOnly,
  onChange,
}: {
  title: string;
  index: string;
  hint: string;
  scopeName: string;
  items: BlueprintContent['inScope'];
  artifacts: ArtifactMetadata[];
  sourceAnchors: Record<string, DocumentAnchor[]>;
  readOnly: boolean;
  onChange: (items: BlueprintContent['inScope']) => void;
}) {
  return (
    <section className="editor-section">
      <SectionHeading
        index={index}
        title={title}
        hint={hint}
        readOnly={readOnly}
        addLabel={`新增${title}条目`}
        onAdd={() =>
          onChange([...items, { id: createEntityId(scopeName), text: '', sources: [] }])
        }
      />
      <div className="editor-list">
        {items.length === 0 ? (
          <p className="editor-empty">当前没有条目。</p>
        ) : (
          items.map((item, itemIndex) => (
            <article className="editor-card" key={item.id}>
              <label className="field">
                <span>{title}条目 {itemIndex + 1}</span>
                <textarea
                  value={item.text}
                  aria-label={`${title}条目 ${itemIndex + 1}`}
                  onChange={(event) =>
                    onChange(replaceAt(items, itemIndex, { ...item, text: event.target.value }))
                  }
                  disabled={readOnly}
                  rows={3}
                />
              </label>
              <SourceReferences
                references={item.sources}
                artifacts={artifacts}
                sourceAnchors={sourceAnchors}
              />
              {!readOnly ? (
                <button
                  className="button button--quiet editor-card__delete"
                  type="button"
                  onClick={() => onChange(removeAt(items, itemIndex))}
                >
                  删除{title}条目 {itemIndex + 1}
                </button>
              ) : null}
            </article>
          ))
        )}
      </div>
    </section>
  );
}

export function BlueprintEditor({
  content,
  artifacts,
  sourceAnchors,
  readOnly,
  onChange,
}: BlueprintEditorProps) {
  function updateModule(index: number, module: ModuleDefinition) {
    onChange({ ...content, modules: replaceAt(content.modules, index, module) });
  }

  function deletePlatform(platformId: string, platformIndex: number) {
    onChange({
      ...content,
      platforms: removeAt(content.platforms, platformIndex),
      modules: content.modules.map((module) => ({
        ...module,
        platforms: module.platforms.filter((id) => id !== platformId),
      })),
    });
  }

  function deleteModule(moduleId: string, moduleIndex: number) {
    onChange({
      ...content,
      modules: content.modules
        .filter((_, index) => index !== moduleIndex)
        .map((module) => ({
          ...module,
          dependsOn: module.dependsOn.filter((id) => id !== moduleId),
        })),
    });
  }

  return (
    <div className="blueprint-editor">
      <section className="editor-section editor-section--goal">
        <header className="editor-section__head">
          <div>
            <p className="panel__eyebrow">01 / PRODUCT INTENT</p>
            <h2>产品目标</h2>
            <p>用一句话说明系统要解决的问题与期望结果。</p>
          </div>
        </header>
        <label className="field">
          <span>产品目标</span>
          <textarea
            value={content.productGoal.text}
            aria-label="产品目标"
            onChange={(event) =>
              onChange({
                ...content,
                productGoal: { ...content.productGoal, text: event.target.value },
              })
            }
            disabled={readOnly}
            rows={4}
          />
        </label>
        <SourceReferences
          references={content.productGoal.sources}
          artifacts={artifacts}
          sourceAnchors={sourceAnchors}
        />
      </section>

      <ScopeSection
        title="范围内"
        index="02 / IN SCOPE"
        hint="明确本期一定交付的业务边界。"
        scopeName="scope_in"
        items={content.inScope}
        artifacts={artifacts}
        sourceAnchors={sourceAnchors}
        readOnly={readOnly}
        onChange={(inScope) => onChange({ ...content, inScope })}
      />
      <ScopeSection
        title="范围外"
        index="03 / OUT OF SCOPE"
        hint="提前写清不做什么，防止后续范围漂移。"
        scopeName="scope_out"
        items={content.outOfScope}
        artifacts={artifacts}
        sourceAnchors={sourceAnchors}
        readOnly={readOnly}
        onChange={(outOfScope) => onChange({ ...content, outOfScope })}
      />

      <section className="editor-section">
        <SectionHeading
          index="04 / ROLES"
          title="角色"
          hint="描述谁会使用系统，以及其职责边界。"
          readOnly={readOnly}
          addLabel="新增角色"
          onAdd={() =>
            onChange({
              ...content,
              roles: [
                ...content.roles,
                { id: createEntityId('role'), name: '', description: '', sources: [] },
              ],
            })
          }
        />
        <div className="editor-list">
          {content.roles.length === 0 ? <p className="editor-empty">当前没有角色。</p> : null}
          {content.roles.map((role, index) => (
            <article className="editor-card" key={role.id}>
              <div className="editor-card__grid">
                <label className="field">
                  <span>角色名称 {index + 1}</span>
                  <input
                    value={role.name}
                    aria-label={`角色名称 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        roles: replaceAt(content.roles, index, { ...role, name: event.target.value }),
                      })
                    }
                    disabled={readOnly}
                  />
                </label>
                <label className="field">
                  <span>角色说明 {index + 1}</span>
                  <textarea
                    value={role.description}
                    aria-label={`角色说明 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        roles: replaceAt(content.roles, index, {
                          ...role,
                          description: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                    rows={2}
                  />
                </label>
              </div>
              <SourceReferences references={role.sources} artifacts={artifacts} sourceAnchors={sourceAnchors} />
              {!readOnly ? (
                <button
                  className="button button--quiet editor-card__delete"
                  type="button"
                  onClick={() => onChange({ ...content, roles: removeAt(content.roles, index) })}
                >
                  删除角色 {index + 1}
                </button>
              ) : null}
            </article>
          ))}
        </div>
      </section>

      <section className="editor-section">
        <SectionHeading
          index="05 / PLATFORMS"
          title="平台矩阵"
          hint="平台实体 ID 会作为模块适用范围的稳定引用。"
          readOnly={readOnly}
          addLabel="新增平台"
          onAdd={() =>
            onChange({
              ...content,
              platforms: [
                ...content.platforms,
                {
                  id: createEntityId('platform'),
                  name: '新平台',
                  kind: 'web',
                  description: '',
                  sources: [],
                },
              ],
            })
          }
        />
        <div className="editor-list">
          {content.platforms.length === 0 ? <p className="editor-empty">至少需要一个平台。</p> : null}
          {content.platforms.map((platform, index) => (
            <article className="editor-card" key={platform.id}>
              <div className="editor-card__grid editor-card__grid--platform">
                <label className="field">
                  <span>平台名称 {index + 1}</span>
                  <input
                    value={platform.name}
                    aria-label={`平台名称 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        platforms: replaceAt(content.platforms, index, {
                          ...platform,
                          name: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                  />
                </label>
                <label className="field">
                  <span>平台类型 {index + 1}</span>
                  <select
                    value={platform.kind}
                    aria-label={`平台类型 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        platforms: replaceAt(content.platforms, index, {
                          ...platform,
                          kind: event.target.value as PlatformKind,
                        }),
                      })
                    }
                    disabled={readOnly}
                  >
                    {platformKindOptions.map((option) => (
                      <option value={option.value} key={option.value}>{option.label}</option>
                    ))}
                  </select>
                </label>
                <label className="field">
                  <span>平台说明 {index + 1}</span>
                  <textarea
                    value={platform.description}
                    aria-label={`平台说明 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        platforms: replaceAt(content.platforms, index, {
                          ...platform,
                          description: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                    rows={2}
                  />
                </label>
              </div>
              <code className="entity-id">ID · {platform.id}</code>
              <SourceReferences references={platform.sources} artifacts={artifacts} sourceAnchors={sourceAnchors} />
              {!readOnly ? (
                <button
                  className="button button--quiet editor-card__delete"
                  type="button"
                  onClick={() => deletePlatform(platform.id, index)}
                >
                  删除平台 {index + 1}
                </button>
              ) : null}
            </article>
          ))}
        </div>
      </section>

      <section className="editor-section">
        <SectionHeading
          index="06 / MODULES"
          title="模块与依赖"
          hint="模块平台与前置模块都通过稳定实体 ID 关联。"
          readOnly={readOnly}
          addLabel="新增模块"
          onAdd={() =>
            onChange({
              ...content,
              modules: [
                ...content.modules,
                {
                  id: createEntityId('module'),
                  name: '',
                  description: '',
                  platforms: [],
                  dependsOn: [],
                  sources: [],
                },
              ],
            })
          }
        />
        <div className="editor-list">
          {content.modules.length === 0 ? <p className="editor-empty">至少需要一个模块。</p> : null}
          {content.modules.map((module, index) => (
            <article className="editor-card editor-card--module" key={module.id}>
              <div className="editor-card__grid">
                <label className="field">
                  <span>模块名称 {index + 1}</span>
                  <input
                    value={module.name}
                    aria-label={`模块名称 ${index + 1}`}
                    onChange={(event) => updateModule(index, { ...module, name: event.target.value })}
                    disabled={readOnly}
                  />
                </label>
                <label className="field">
                  <span>模块说明 {index + 1}</span>
                  <textarea
                    value={module.description}
                    aria-label={`模块说明 ${index + 1}`}
                    onChange={(event) =>
                      updateModule(index, { ...module, description: event.target.value })
                    }
                    disabled={readOnly}
                    rows={2}
                  />
                </label>
              </div>
              <fieldset className="choice-grid">
                <legend>适用平台</legend>
                {content.platforms.map((platform) => (
                  <label key={platform.id}>
                    <input
                      type="checkbox"
                      checked={module.platforms.includes(platform.id)}
                      onChange={(event) => {
                        const platforms = event.target.checked
                          ? [...module.platforms, platform.id]
                          : module.platforms.filter((id) => id !== platform.id);
                        updateModule(index, { ...module, platforms });
                      }}
                      disabled={readOnly}
                    />
                    <span>{platform.name || platform.id}</span>
                  </label>
                ))}
              </fieldset>
              <fieldset className="choice-grid">
                <legend>依赖模块</legend>
                {content.modules.filter((candidate) => candidate.id !== module.id).length === 0 ? (
                  <span className="choice-grid__empty">暂无其他模块可依赖</span>
                ) : null}
                {content.modules
                  .filter((candidate) => candidate.id !== module.id)
                  .map((candidate) => (
                    <label key={candidate.id}>
                      <input
                        type="checkbox"
                        checked={module.dependsOn.includes(candidate.id)}
                        onChange={(event) => {
                          const dependsOn = event.target.checked
                            ? [...module.dependsOn, candidate.id]
                            : module.dependsOn.filter((id) => id !== candidate.id);
                          updateModule(index, { ...module, dependsOn });
                        }}
                        disabled={readOnly}
                      />
                      <span>{candidate.name || candidate.id}</span>
                    </label>
                  ))}
              </fieldset>
              <code className="entity-id">ID · {module.id}</code>
              <SourceReferences references={module.sources} artifacts={artifacts} sourceAnchors={sourceAnchors} />
              {!readOnly ? (
                <button
                  className="button button--quiet editor-card__delete"
                  type="button"
                  onClick={() => deleteModule(module.id, index)}
                >
                  删除模块 {index + 1}
                </button>
              ) : null}
            </article>
          ))}
        </div>
      </section>

      <section className="editor-section">
        <SectionHeading
          index="07 / TECHNOLOGY"
          title="技术栈"
          hint="记录技术选择与理由，后续计划会沿用这些约束。"
          readOnly={readOnly}
          addLabel="新增技术栈"
          onAdd={() =>
            onChange({
              ...content,
              technologies: [
                ...content.technologies,
                { id: createEntityId('technology'), category: '', name: '', reason: '', sources: [] },
              ],
            })
          }
        />
        <div className="editor-list">
          {content.technologies.length === 0 ? <p className="editor-empty">当前没有技术栈决策。</p> : null}
          {content.technologies.map((technology, index) => (
            <article className="editor-card" key={technology.id}>
              <div className="editor-card__grid editor-card__grid--technology">
                <label className="field">
                  <span>技术类别 {index + 1}</span>
                  <input
                    value={technology.category}
                    aria-label={`技术类别 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        technologies: replaceAt(content.technologies, index, {
                          ...technology,
                          category: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                  />
                </label>
                <label className="field">
                  <span>技术栈名称 {index + 1}</span>
                  <input
                    value={technology.name}
                    aria-label={`技术栈名称 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        technologies: replaceAt(content.technologies, index, {
                          ...technology,
                          name: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                  />
                </label>
                <label className="field">
                  <span>选择理由 {index + 1}</span>
                  <textarea
                    value={technology.reason}
                    aria-label={`选择理由 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        technologies: replaceAt(content.technologies, index, {
                          ...technology,
                          reason: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                    rows={2}
                  />
                </label>
              </div>
              <SourceReferences references={technology.sources} artifacts={artifacts} sourceAnchors={sourceAnchors} />
              {!readOnly ? (
                <button
                  className="button button--quiet editor-card__delete"
                  type="button"
                  onClick={() =>
                    onChange({ ...content, technologies: removeAt(content.technologies, index) })
                  }
                >
                  删除技术栈 {index + 1}
                </button>
              ) : null}
            </article>
          ))}
        </div>
      </section>

      <section className="editor-section">
        <SectionHeading
          index="08 / CAPABILITIES"
          title="三方能力"
          hint="记录 OSS、短信、支付等外部能力及服务商。"
          readOnly={readOnly}
          addLabel="新增三方能力"
          onAdd={() =>
            onChange({
              ...content,
              capabilities: [
                ...content.capabilities,
                {
                  id: createEntityId('capability'),
                  name: '',
                  provider: '',
                  description: '',
                  sources: [],
                },
              ],
            })
          }
        />
        <div className="editor-list">
          {content.capabilities.length === 0 ? <p className="editor-empty">当前没有三方能力。</p> : null}
          {content.capabilities.map((capability, index) => (
            <article className="editor-card" key={capability.id}>
              <div className="editor-card__grid editor-card__grid--technology">
                <label className="field">
                  <span>能力名称 {index + 1}</span>
                  <input
                    value={capability.name}
                    aria-label={`能力名称 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        capabilities: replaceAt(content.capabilities, index, {
                          ...capability,
                          name: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                  />
                </label>
                <label className="field">
                  <span>服务商 {index + 1}</span>
                  <input
                    value={capability.provider}
                    aria-label={`服务商 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        capabilities: replaceAt(content.capabilities, index, {
                          ...capability,
                          provider: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                  />
                </label>
                <label className="field">
                  <span>能力说明 {index + 1}</span>
                  <textarea
                    value={capability.description}
                    aria-label={`能力说明 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        capabilities: replaceAt(content.capabilities, index, {
                          ...capability,
                          description: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                    rows={2}
                  />
                </label>
              </div>
              <SourceReferences references={capability.sources} artifacts={artifacts} sourceAnchors={sourceAnchors} />
              {!readOnly ? (
                <button
                  className="button button--quiet editor-card__delete"
                  type="button"
                  onClick={() =>
                    onChange({ ...content, capabilities: removeAt(content.capabilities, index) })
                  }
                >
                  删除三方能力 {index + 1}
                </button>
              ) : null}
            </article>
          ))}
        </div>
      </section>

      <section className="editor-section">
        <SectionHeading
          index="09 / RELEASE GATES"
          title="发布前置条件"
          hint="把提审、证书、备案、法务等长周期事项提前纳入计划。"
          readOnly={readOnly}
          addLabel="新增发布条件"
          onAdd={() =>
            onChange({
              ...content,
              releasePrerequisites: [
                ...content.releasePrerequisites,
                {
                  id: createEntityId('release'),
                  name: '',
                  category: '',
                  description: '',
                  leadTimeDays: null,
                  blocking: true,
                  sources: [],
                },
              ],
            })
          }
        />
        <div className="editor-list">
          {content.releasePrerequisites.length === 0 ? (
            <p className="editor-empty">当前没有发布前置条件。</p>
          ) : null}
          {content.releasePrerequisites.map((prerequisite, index) => (
            <article className="editor-card" key={prerequisite.id}>
              <div className="editor-card__grid editor-card__grid--technology">
                <label className="field">
                  <span>前置条件名称 {index + 1}</span>
                  <input
                    value={prerequisite.name}
                    aria-label={`前置条件名称 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        releasePrerequisites: replaceAt(content.releasePrerequisites, index, {
                          ...prerequisite,
                          name: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                  />
                </label>
                <label className="field">
                  <span>前置条件类别 {index + 1}</span>
                  <input
                    value={prerequisite.category}
                    aria-label={`前置条件类别 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        releasePrerequisites: replaceAt(content.releasePrerequisites, index, {
                          ...prerequisite,
                          category: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                  />
                </label>
                <label className="field">
                  <span>前置条件说明 {index + 1}</span>
                  <textarea
                    value={prerequisite.description}
                    aria-label={`前置条件说明 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        releasePrerequisites: replaceAt(content.releasePrerequisites, index, {
                          ...prerequisite,
                          description: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                    rows={2}
                  />
                </label>
                <label className="field">
                  <span>预计周期（天） {index + 1}</span>
                  <input
                    type="number"
                    min="0"
                    step="1"
                    value={prerequisite.leadTimeDays ?? ''}
                    aria-label={`预计周期（天） ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        releasePrerequisites: replaceAt(content.releasePrerequisites, index, {
                          ...prerequisite,
                          leadTimeDays:
                            event.target.value === ''
                              ? null
                              : Math.max(0, Math.trunc(Number(event.target.value))),
                        }),
                      })
                    }
                    disabled={readOnly}
                  />
                </label>
              </div>
              <label className="consent-row consent-row--compact">
                <input
                  type="checkbox"
                  checked={prerequisite.blocking}
                  onChange={(event) =>
                    onChange({
                      ...content,
                      releasePrerequisites: replaceAt(content.releasePrerequisites, index, {
                        ...prerequisite,
                        blocking: event.target.checked,
                      }),
                    })
                  }
                  disabled={readOnly}
                />
                <span>阻塞发布</span>
              </label>
              <SourceReferences
                references={prerequisite.sources}
                artifacts={artifacts}
                sourceAnchors={sourceAnchors}
              />
              {!readOnly ? (
                <button
                  className="button button--quiet editor-card__delete"
                  type="button"
                  onClick={() =>
                    onChange({
                      ...content,
                      releasePrerequisites: removeAt(content.releasePrerequisites, index),
                    })
                  }
                >
                  删除发布条件 {index + 1}
                </button>
              ) : null}
            </article>
          ))}
        </div>
      </section>

      <section className="editor-section">
        <SectionHeading
          index="10 / ASSUMPTIONS"
          title="假设"
          hint="记录当前成立但未来可能变化的前提。"
          readOnly={readOnly}
          addLabel="新增假设"
          onAdd={() =>
            onChange({
              ...content,
              assumptions: [
                ...content.assumptions,
                { id: createEntityId('assumption'), text: '', sources: [] },
              ],
            })
          }
        />
        <div className="editor-list">
          {content.assumptions.length === 0 ? <p className="editor-empty">当前没有假设。</p> : null}
          {content.assumptions.map((assumption, index) => (
            <article className="editor-card" key={assumption.id}>
              <label className="field">
                <span>假设 {index + 1}</span>
                <textarea
                  value={assumption.text}
                  aria-label={`假设 ${index + 1}`}
                  onChange={(event) =>
                    onChange({
                      ...content,
                      assumptions: replaceAt(content.assumptions, index, {
                        ...assumption,
                        text: event.target.value,
                      }),
                    })
                  }
                  disabled={readOnly}
                  rows={2}
                />
              </label>
              <SourceReferences references={assumption.sources} artifacts={artifacts} sourceAnchors={sourceAnchors} />
              {!readOnly ? (
                <button
                  className="button button--quiet editor-card__delete"
                  type="button"
                  onClick={() =>
                    onChange({ ...content, assumptions: removeAt(content.assumptions, index) })
                  }
                >
                  删除假设 {index + 1}
                </button>
              ) : null}
            </article>
          ))}
        </div>
      </section>

      <section className="editor-section">
        <SectionHeading
          index="11 / RISKS"
          title="风险"
          hint="用严重程度标记需要优先验证或规避的问题。"
          readOnly={readOnly}
          addLabel="新增风险"
          onAdd={() =>
            onChange({
              ...content,
              risks: [
                ...content.risks,
                {
                  id: createEntityId('risk'),
                  text: '',
                  severity: 'medium',
                  sources: [],
                },
              ],
            })
          }
        />
        <div className="editor-list">
          {content.risks.length === 0 ? <p className="editor-empty">当前没有风险。</p> : null}
          {content.risks.map((risk, index) => (
            <article className="editor-card" key={risk.id}>
              <div className="editor-card__grid editor-card__grid--risk">
                <label className="field">
                  <span>风险描述 {index + 1}</span>
                  <textarea
                    value={risk.text}
                    aria-label={`风险描述 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        risks: replaceAt(content.risks, index, {
                          ...risk,
                          text: event.target.value,
                        }),
                      })
                    }
                    disabled={readOnly}
                    rows={2}
                  />
                </label>
                <label className="field">
                  <span>严重程度 {index + 1}</span>
                  <select
                    value={risk.severity}
                    aria-label={`严重程度 ${index + 1}`}
                    onChange={(event) =>
                      onChange({
                        ...content,
                        risks: replaceAt(content.risks, index, {
                          ...risk,
                          severity: event.target.value as RiskSeverity,
                        }),
                      })
                    }
                    disabled={readOnly}
                  >
                    {severityOptions.map((option) => (
                      <option value={option.value} key={option.value}>{option.label}</option>
                    ))}
                  </select>
                </label>
              </div>
              <SourceReferences references={risk.sources} artifacts={artifacts} sourceAnchors={sourceAnchors} />
              {!readOnly ? (
                <button
                  className="button button--quiet editor-card__delete"
                  type="button"
                  onClick={() => onChange({ ...content, risks: removeAt(content.risks, index) })}
                >
                  删除风险 {index + 1}
                </button>
              ) : null}
            </article>
          ))}
        </div>
      </section>
    </div>
  );
}
