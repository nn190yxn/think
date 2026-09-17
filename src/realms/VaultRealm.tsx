import { useCallback, useEffect, useState, type FormEvent } from "react";
import { LAYER_KEYS, layerDepth, layerOf } from "../domain/layers";
import { LayerGlyph } from "../components/LayerGlyph";
import { RealmShell } from "./RealmShell";
import { useCommand, useCommands } from "../app/ipc";
import type { CommandInvocationError } from "../ipc/protocol";
import {
  formatTime,
  masterStatusLabel,
  roleLabel,
  sourceKindLabel,
} from "../domain/labels";
import type {
  AssetRootView,
  AssetSummary,
  CorpusSearchHit,
  KbDocumentView,
  KbSearchHit,
  KbSourceView,
  KnowledgeOverview,
  MasterDetail,
  SkillView,
} from "../ipc/commands";

type VaultView = "masters" | "archive" | "assets" | "terrain";

/**
 * 藏：大师与资产。层次覆盖矩阵是选角的地基，空缺一眼可见；
 * 大师架子、版本历史与语料溯源都在这里，作为会诊的备料场。
 */
export function VaultRealm() {
  const client = useCommands();
  const matrix = useCommand("coverage_matrix", {});
  const masters = useCommand("master_list", {});
  const [view, setView] = useState<VaultView>("masters");
  const [selected, setSelected] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const coverage = matrix.data;

  async function run(label: string, action: () => Promise<unknown>) {
    setBusy(true);
    try {
      await action();
      setStatus(label);
      window.setTimeout(() => setStatus(null), 2400);
    } catch (cause) {
      const error = cause as CommandInvocationError;
      setStatus(`${label}失败：${error.message}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <RealmShell realm="vault">
      <div className="vault-views" role="tablist" aria-label="藏境界视图">
        <button
          type="button"
          role="tab"
          aria-selected={view === "masters"}
          data-on={view === "masters"}
          onClick={() => setView("masters")}
        >
          大师架子
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={view === "archive"}
          data-on={view === "archive"}
          onClick={() => setView("archive")}
        >
          大师档案
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={view === "assets"}
          data-on={view === "assets"}
          onClick={() => setView("assets")}
        >
          资产图谱
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={view === "terrain"}
          data-on={view === "terrain"}
          onClick={() => setView("terrain")}
        >
          知识地形
        </button>
      </div>

      {view === "terrain" ? (
        <KnowledgeTerrain />
      ) : view === "assets" ? (
        <AssetAtlas />
      ) : view === "archive" ? (
        selected ? (
          <MasterArchive key={selected} masterId={selected} onStatus={setStatus} />
        ) : (
          <section className="vault-block">
            <h2 className="section-head">大师档案</h2>
            <p className="vault-note">先选一位大师，再看他的完整档案。</p>
            <ul className="masters">
              {(masters.data ?? []).map((master) => (
                <li key={master.id} className="masters__row">
                  <button
                    type="button"
                    className="masters__pick"
                    onClick={() => setSelected(master.id)}
                  >
                    <span className="masters__glyphs">
                      {master.layers.map((layer) => (
                        <span key={layer} data-layer={layer} className="masters__glyph">
                          <LayerGlyph glyph={layerOf(layer).glyph} size={12} />
                        </span>
                      ))}
                    </span>
                    <span className="masters__name">{master.name}</span>
                    <span className="masters__domain">{master.domain}</span>
                    <span className="masters__meta">
                      v{master.currentVersion} · {master.unitCount} 单元
                    </span>
                    <span className="masters__open">查看档案</span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )
      ) : (
        <>
      <section className="vault-block">
        <h2 className="section-head">层次覆盖</h2>
        {matrix.error ? (
          <p className="vault-error">覆盖矩阵读取失败：{matrix.error.message}</p>
        ) : null}
        <ul className="coverage">
          {LAYER_KEYS.map((key) => {
            const layer = layerOf(key);
            const entry = coverage?.layers.find((item) => item.layer === key);
            const masterCount = entry?.masterCount ?? 0;
            return (
              <li
                key={key}
                className="coverage__cell"
                data-layer={key}
                data-empty={masterCount === 0}
              >
                <span className="coverage__glyph">
                  <LayerGlyph glyph={layer.glyph} size={14} />
                </span>
                <span className="coverage__name">
                  {layer.name} · {layer.colorName}
                </span>
                <span className="coverage__count mono">{masterCount}</span>
                <span className="coverage__hint">
                  {masterCount === 0
                    ? "空缺"
                    : `${entry?.unitCount ?? 0} 个单元`}
                </span>
              </li>
            );
          })}
        </ul>
        {coverage && coverage.suggestions.length > 0 ? (
          <ul className="vault-suggestions">
            {coverage.suggestions.map((text) => (
              <li key={text}>{text}</li>
            ))}
          </ul>
        ) : (
          <p className="council__note">
            六层各取一位，选角的层次覆盖约束即可满足。
          </p>
        )}
      </section>

      <section className="vault-block">
        <div className="vault-bar">
          <h2 className="section-head">
            大师架子
            <span className="mono vault-count">{masters.data?.length ?? 0}</span>
          </h2>
          <button
            type="button"
            className="vault-action"
            disabled={busy}
            onClick={() =>
              run("已安装种子大师包", () => client.call("seed_packs_install", {}))
            }
          >
            安装种子大师包
          </button>
        </div>
        <ul className="masters">
          {(masters.data ?? []).map((master) => (
            <li key={master.id} className="masters__row" data-status={master.status}>
              <button
                type="button"
                className="masters__pick"
                aria-pressed={selected === master.id}
                onClick={() => {
                  setSelected(master.id);
                  setView("archive");
                }}
              >
                <span className="masters__glyphs">
                  {master.layers.map((layer) => (
                    <span key={layer} data-layer={layer} className="masters__glyph">
                      <LayerGlyph glyph={layerOf(layer).glyph} size={12} />
                    </span>
                  ))}
                </span>
                <span className="masters__name">{master.name}</span>
                <span className="masters__domain">{master.domain}</span>
                <span className="masters__meta mono">
                  v{master.currentVersion} · {master.unitCount} 单元
                </span>
                <span className="masters__open">查看档案</span>
              </button>
            </li>
          ))}
          {masters.data && masters.data.length === 0 ? (
            <li className="vault-empty">
              还没有大师包。安装种子包，或从蒸馏熔炉炼出第一位。
            </li>
          ) : null}
        </ul>
      </section>

      <CorpusSearch />
        </>
      )}

      {status ? (
        <p className="vault-status" role="status">
          {status}
        </p>
      ) : null}
    </RealmShell>
  );
}

/**
 * 一位大师的完整档案：他是谁、看重什么、有哪几条技能、版本如何迭代、
 * 在会诊里说过什么、材料从哪来。档案随每次蒸馏与版本更新一起演化。
 */
function MasterArchive({
  masterId,
  onStatus,
}: {
  readonly masterId: string;
  readonly onStatus: (message: string) => void;
}) {
  const client = useCommands();
  const detail = useCommand("master_detail", { masterId });
  const history = useCommand("master_history", { masterId, limit: 50 });
  const corpus = useCommand("corpus_list", { masterId });
  const [flagging, setFlagging] = useState<string | null>(null);
  const [flagReason, setFlagReason] = useState("");

  if (detail.error) {
    return <p className="vault-error">读取失败：{detail.error.message}</p>;
  }
  if (!detail.data) {
    return <p className="vault-note">载入中…</p>;
  }

  const master: MasterDetail = detail.data;
  const profile = layerDepth(master.units);
  const covered = profile.filter((entry) => entry.count > 0);
  const deepest = covered.reduce((max, entry) => Math.max(max, entry.count), 0);

  async function flagUnit(unitId: string) {
    const reason = flagReason.trim();
    if (!reason) {
      return;
    }
    try {
      await client.call("master_flag_unit", { unitId, reason });
      setFlagging(null);
      setFlagReason("");
      onStatus("已标记这条技能的问题");
    } catch (cause) {
      onStatus(`标记失败：${(cause as CommandInvocationError).message}`);
    }
  }

  return (
    <div className="master-archive">
      <section className="vault-block">
        <div className="master-archive__head">
          <div>
            <h2 className="section-head">{master.name}</h2>
            <p className="master-archive__domain">
              {master.domain} ·{" "}
              {profile
                .filter((entry) => entry.count > 0)
                .map((entry) => layerOf(entry.key).name)
                .join(" / ")}{" "}
              ·{" "}
              {masterStatusLabel(master.status)}
            </p>
          </div>
          <span className="master-archive__version mono">v{master.currentVersion}</span>
        </div>
        <p className="master-panel__summary">{master.summary}</p>
        <dl className="master-panel__meta">
          <div>
            <dt>看重什么</dt>
            <dd>{master.style}</dd>
          </div>
          <div>
            <dt>容易忽略</dt>
            <dd>{master.blindSpots}</dd>
          </div>
        </dl>
      </section>

      <section className="vault-block">
        <h2 className="section-head">
          六题档案
          <span className="mono vault-count">
            {covered.length} / {LAYER_KEYS.length}
          </span>
        </h2>
        <p className="vault-note">
          六题是所有大师共同面对的问题。题目下有几条技能，就在这一题上有几分积累；
          空着的题也是信息，代表这一问上他还没有形成自己的框架。
        </p>
        <ul className="profile">
          {profile.map((entry) => {
            const meta = layerOf(entry.key);
            const ratio = deepest > 0 ? (entry.count / deepest) * 100 : 0;
            return (
              <li
                key={entry.key}
                className="profile__row"
                data-layer={entry.key}
                data-empty={entry.count === 0}
              >
                <div className="profile__line">
                  <span
                    className="profile__glyph"
                    style={{ color: `var(--layer-${entry.key})` }}
                  >
                    <LayerGlyph glyph={meta.glyph} size={13} />
                  </span>
                  <span className="profile__name">{meta.name}</span>
                  <span className="profile__question">{meta.question}</span>
                  <span className="profile__bar" aria-hidden="true">
                    <span
                      className="profile__fill"
                      style={{
                        width: `${ratio}%`,
                        background: `var(--layer-${entry.key})`,
                      }}
                    />
                  </span>
                  <span className="profile__count mono">{entry.count}</span>
                </div>
                <p className="profile__units">
                  {entry.count === 0
                    ? "这一题还没有积累"
                    : entry.titles.join("、")}
                </p>
              </li>
            );
          })}
        </ul>
      </section>

      <section className="vault-block">
        <h2 className="section-head">
          技能
          <span className="mono vault-count">{master.units.length}</span>
        </h2>
        <ul className="units">
        {master.units.map((unit) => {
          const layer = layerOf(unit.layer);
          return (
            <li key={unit.id} className="unit" data-layer={unit.layer}>
              <header className="unit__head">
                <span className="unit__glyph">
                  <LayerGlyph glyph={layer.glyph} size={12} />
                </span>
                <h3 className="unit__title">{unit.title}</h3>
                <span className="unit__layer">
                  {layer.name} · {layer.question}
                </span>
                {unit.flaggedReason ? (
                  <span className="unit__flag">已标记：{unit.flaggedReason}</span>
                ) : null}
              </header>
              <dl className="unit__body">
                <div>
                  <dt>触发条件</dt>
                  <dd>{unit.triggerCondition}</dd>
                </div>
                <div>
                  <dt>执行步骤</dt>
                  <dd>
                    <ol className="unit__steps">
                      {unit.steps.map((step) => (
                        <li key={step}>{step}</li>
                      ))}
                    </ol>
                  </dd>
                </div>
                <div>
                  <dt>作用机制</dt>
                  <dd>{unit.mechanism}</dd>
                </div>
                <div>
                  <dt>适用边界</dt>
                  <dd>{unit.boundary}</dd>
                </div>
              </dl>
              <ul className="citations">
                {unit.citations.map((citation) => (
                  <li
                    key={`${citation.location}-${citation.excerpt}`}
                    className="citation"
                    data-missing={!citation.available}
                  >
                    <span className="citation__excerpt">「{citation.excerpt}」</span>
                    <span className="citation__location mono">
                      {citation.location}
                    </span>
                    {citation.available ? null : (
                      <span className="citation__missing">来源缺失</span>
                    )}
                  </li>
                ))}
              </ul>
              {flagging === unit.id ? (
                <div className="unit__flag-form">
                  <input
                    className="corpus-search__input"
                    value={flagReason}
                    placeholder="这条技能哪里有问题？写一句原因"
                    aria-label={`标记问题原因：${unit.title}`}
                    onChange={(event) => setFlagReason(event.target.value)}
                  />
                  <button
                    type="button"
                    className="vault-action"
                    onClick={() => void flagUnit(unit.id)}
                  >
                    确认标记
                  </button>
                  <button
                    type="button"
                    className="vault-action vault-action--quiet"
                    onClick={() => {
                      setFlagging(null);
                      setFlagReason("");
                    }}
                  >
                    取消
                  </button>
                </div>
              ) : (
                <button
                  type="button"
                  className="vault-action vault-action--quiet"
                  onClick={() => {
                    setFlagging(unit.id);
                    setFlagReason("");
                  }}
                >
                  标记问题
                </button>
              )}
            </li>
          );
        })}
        </ul>
      </section>

      <section className="vault-block">
        <h2 className="section-head">认知迭代</h2>
        <ol className="versions">
        {master.versions.map((version) => (
          <li
            key={version.version}
            className="version"
            data-current={version.version === master.currentVersion}
          >
            <span className="version__no mono">v{version.version}</span>
            <span className="version__note">{version.note || "无备注"}</span>
            <span className="version__diff">
              新增 {version.diff.added.length} · 更新 {version.diff.updated.length} ·
              保留 {version.diff.carried}
            </span>
            <span className="version__time">{formatTime(version.createdAt)}</span>
            {version.version === master.currentVersion ? (
              <span className="version__tag">当前</span>
            ) : (
              <button
                type="button"
                className="vault-action vault-action--quiet"
                onClick={async () => {
                  await client.call("master_revert", {
                    masterId,
                    version: version.version,
                  });
                  onStatus(`已回退到 v${version.version}`);
                }}
              >
                回退
              </button>
            )}
          </li>
        ))}
        </ol>
      </section>

      <section className="vault-block">
        <h2 className="section-head">观点轨迹</h2>
        {history.error ? (
          <p className="vault-error">读取失败：{history.error.message}</p>
        ) : history.data && history.data.length > 0 ? (
          <ol className="track">
            {history.data.map((entry) => (
              <li key={`${entry.sessionId}-${entry.round}-${entry.role}`} className="track__row">
                <span className="track__time">{formatTime(entry.createdAt)}</span>
                <span className="track__role">
                  {roleLabel(entry.role)}
                  {entry.masterVersion ? ` · v${entry.masterVersion}` : ""}
                </span>
                <span className="track__question">{entry.question}</span>
                <p className="track__content prose">{entry.content}</p>
              </li>
            ))}
          </ol>
        ) : (
          <p className="vault-note">这位大师还没有在会诊里发言过。</p>
        )}
        <p className="vault-note">
          只记录他在哪一场、以哪个版本说过什么，方便对照结论是怎么变过来的。
        </p>
      </section>

      <section className="vault-block">
        <h2 className="section-head">材料来源</h2>
        {corpus.data && corpus.data.length > 0 ? (
          <ul className="corpus-hits">
            {corpus.data.map((item) => (
              <li key={item.id} className="corpus-hit" data-missing={!item.available}>
                <span className="corpus-hit__title">{item.title}</span>
                <span className="corpus-hit__ref">{item.locationHint || item.sourceRef}</span>
                <span className="corpus-hit__mode">
                  {sourceKindLabel(item.sourceKind)}
                </span>
                {item.available ? null : (
                  <span className="citation__missing">来源缺失</span>
                )}
              </li>
            ))}
          </ul>
        ) : (
          <p className="vault-note">还没有登记这位大师的材料来源。</p>
        )}
      </section>
    </div>
  );
}

/** 材料检索：短查询改按文件名匹配，结果标注命中方式与来源。 */
function CorpusSearch() {
  const client = useCommands();
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<readonly CorpusSearchHit[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const text = query.trim();
    if (!text) {
      setHits(null);
      return;
    }
    try {
      const result = await client.call("corpus_search", { query: text });
      setHits(result);
      setError(null);
    } catch (cause) {
      setError((cause as CommandInvocationError).message);
    }
  }

  return (
    <section className="vault-block">
      <h2 className="section-head">材料检索</h2>
      <form className="corpus-search" onSubmit={submit}>
        <input
          className="corpus-search__input"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="按标题或来源查材料"
          aria-label="检索材料"
        />
        <button type="submit" className="vault-action">
          查找
        </button>
      </form>
      {error ? <p className="vault-error">查找失败：{error}</p> : null}
      {hits ? (
        <ul className="corpus-hits">
          {hits.map((hit) => (
            <li key={hit.item.id} className="corpus-hit">
              <span className="corpus-hit__title">{hit.item.title}</span>
              <span className="corpus-hit__ref mono">{hit.item.sourceRef}</span>
            <span className="corpus-hit__mode">
                {hit.matchedBy === "fts" ? "内容命中" : "文件名命中"}
            </span>
            </li>
          ))}
          {hits.length === 0 ? (
            <li className="vault-empty">没有命中，换个词试试。</li>
          ) : null}
        </ul>
      ) : null}
    </section>
  );
}

/**
 * 知识地形：来源只管登记与扫描，索引只读元数据，不复制正文。
 * 主题以面积映射文档量、颜色映射领域；年轮以圈层厚度映射每月新增。
 */
function KnowledgeTerrain() {
  const client = useCommands();
  const [overview, setOverview] = useState<KnowledgeOverview | null>(null);
  const [sources, setSources] = useState<readonly KbSourceView[]>([]);
  const [documents, setDocuments] = useState<readonly KbDocumentView[]>([]);
  const [path, setPath] = useState("");
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<readonly KbSearchHit[] | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    const [nextOverview, nextSources, nextDocuments] = await Promise.all([
      client.call("kb_overview", {}),
      client.call("kb_sources", {}),
      client.call("kb_documents", {}),
    ]);
    setOverview(nextOverview);
    setSources(nextSources);
    setDocuments(nextDocuments);
  }, [client]);

  useEffect(() => {
    void load().catch(() => setNote("知识地形读取失败"));
  }, [load]);

  async function run(label: string, action: () => Promise<unknown>) {
    setBusy(true);
    setNote(null);
    try {
      await action();
      await load();
      setNote(label);
    } catch (cause) {
      setNote(cause instanceof Error ? cause.message : `${label}失败`);
    } finally {
      setBusy(false);
    }
  }

  async function addSource(event: FormEvent) {
    event.preventDefault();
    const value = path.trim();
    if (!value) {
      return;
    }
    await run("已登记来源", async () => {
      await client.call("kb_add_source", { path: value });
      setPath("");
    });
  }

  async function search(event: FormEvent) {
    event.preventDefault();
    const text = query.trim();
    if (!text) {
      setHits(null);
      return;
    }
    setNote(null);
    try {
      setHits(await client.call("kb_search", { query: text }));
    } catch (cause) {
      setNote(cause instanceof Error ? cause.message : "查找没能完成");
    }
  }

  // 领域到颜色的稳定映射：同一领域始终落在同一个色相上。
  const domainIndex = new Map<string, number>();
  for (const stat of overview?.domains ?? []) {
    domainIndex.set(stat.domain, domainIndex.size);
  }
  const domainOfTopic = new Map<string, string>();
  for (const doc of documents) {
    if (!domainOfTopic.has(doc.topicName)) {
      domainOfTopic.set(doc.topicName, doc.domain || "未归类");
    }
  }
  const maxTopicCount = Math.max(1, ...(overview?.topics ?? []).map((topic) => topic.docCount));
  const maxRingTotal = Math.max(1, ...(overview?.rings ?? []).map((ring) => ring.total));

  return (
    <section className="vault-block terrain">
      <h2 className="section-head">知识地形</h2>
      <p className="vault-note">
        只登记文档的路径、大小与时间，正文留在原处。来源离线时既有索引保持可读。
      </p>

      <dl className="terrain__stats">
        <div>
          <dt>来源</dt>
          <dd className="mono">
            {overview?.availableSources ?? 0} / {overview?.sourceCount ?? 0}
          </dd>
        </div>
        <div>
          <dt>文档</dt>
          <dd className="mono">{overview?.docCount ?? 0}</dd>
        </div>
        <div>
          <dt>主题</dt>
          <dd className="mono">{overview?.topicCount ?? 0}</dd>
        </div>
        <div>
          <dt>领域</dt>
          <dd className="mono">{overview?.domains.length ?? 0}</dd>
        </div>
      </dl>

      <form className="corpus-search" onSubmit={addSource}>
        <input
          className="corpus-search__input"
          value={path}
          onChange={(event) => setPath(event.target.value)}
          placeholder="登记一个文件夹，例如 D:/笔记"
          aria-label="知识库来源路径"
        />
        <button type="submit" className="vault-action" disabled={busy}>
          登记
        </button>
      </form>

      <ul className="terrain__sources">
        {sources.map((source) => (
          <li key={source.id} className="terrain__source" data-available={source.available}>
            <span className="terrain__source-path mono">{source.path}</span>
            <span className="terrain__source-meta mono">
              {source.available ? "在线" : "离线"} · {source.docCount} 篇
            </span>
            <button
              type="button"
              className="vault-action vault-action--quiet"
              disabled={busy}
              onClick={() =>
                run(source.available ? "扫描完成" : "来源离线，索引已保留", () =>
                  client.call("kb_scan", { sourceId: source.id }),
                )
              }
            >
              扫描
            </button>
            <button
              type="button"
              className="vault-action vault-action--quiet"
              disabled={busy}
              onClick={() => run("已移除来源", () => client.call("kb_remove_source", { sourceId: source.id }))}
            >
              移除
            </button>
          </li>
        ))}
        {sources.length === 0 ? (
          <li className="vault-empty">还没有登记来源。先加一个文件夹。</li>
        ) : null}
      </ul>

      <h3 className="section-head section-head--minor">主题分布</h3>
      <ul className="terrain__topics">
        {(overview?.topics ?? []).map((topic) => {
          const domain = domainOfTopic.get(topic.displayName) ?? "未归类";
          const index = (domainIndex.get(domain) ?? 0) % 6;
          return (
            <li key={topic.id} className="terrain__topic" data-color={index}>
              <span className="terrain__topic-name">{topic.displayName}</span>
              <span className="terrain__topic-domain">{domain}</span>
              <span
                className="terrain__topic-bar"
                style={{ width: `${(topic.docCount / maxTopicCount) * 100}%` }}
                aria-hidden="true"
              />
              <span className="terrain__topic-count mono">{topic.docCount}</span>
            </li>
          );
        })}
        {(overview?.topics.length ?? 0) === 0 ? (
          <li className="vault-empty">还没有主题。扫描一个来源后再看。</li>
        ) : null}
      </ul>

      <h3 className="section-head section-head--minor">年轮</h3>
      <ul className="terrain__rings" aria-label="知识增长年轮">
        {(overview?.rings ?? []).map((ring) => (
          <li key={ring.period} className="terrain__ring">
            <span className="terrain__ring-period mono">{ring.period}</span>
            <span
              className="terrain__ring-bar"
              style={{ width: `${(ring.total / maxRingTotal) * 100}%` }}
              aria-hidden="true"
            >
              <span
                className="terrain__ring-added"
                style={{ width: `${(ring.added / Math.max(1, ring.total)) * 100}%` }}
              />
            </span>
            <span className="terrain__ring-count mono">
              +{ring.added} / {ring.total}
            </span>
          </li>
        ))}
        {(overview?.rings.length ?? 0) === 0 ? (
          <li className="vault-empty">还没有可按月聚合的文档。</li>
        ) : null}
      </ul>

      <h3 className="section-head section-head--minor">按主题查找</h3>
      <form className="corpus-search" onSubmit={search}>
        <input
          className="corpus-search__input"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="优先查找正文内容，关键词太短时改按文件名"
          aria-label="查找知识库"
        />
        <button type="submit" className="vault-action">
          查找
        </button>
      </form>
      {hits ? (
        <ul className="corpus-hits">
          {hits.map((hit) => (
            <li key={hit.document.id} className="corpus-hit">
              <span className="corpus-hit__title">{hit.document.normalizedName}</span>
              <span className="corpus-hit__ref mono">{hit.document.path}</span>
              <span className="corpus-hit__mode">
                {hit.matchedBy === "fts" ? "内容命中" : "文件名命中"}
              </span>
            </li>
          ))}
          {hits.length === 0 ? (
            <li className="vault-empty">没有命中，换个词试试。</li>
          ) : null}
        </ul>
      ) : null}

      {note ? (
        <p className="vault-status" role="status">
          {note}
        </p>
      ) : null}
    </section>
  );
}

/**
 * 资产图谱：Skill 与已接入 AI 平台的总览。扫描只读清单，
 * 缺清单或清单损坏的 Skill 保留为裂纹陶片并标注待修复。
 */
function AssetAtlas() {
  const client = useCommands();
  const [summary, setSummary] = useState<AssetSummary | null>(null);
  const [roots, setRoots] = useState<readonly AssetRootView[]>([]);
  const [skills, setSkills] = useState<readonly SkillView[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [path, setPath] = useState("");
  const [query, setQuery] = useState("");
  const [onlyRepair, setOnlyRepair] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    const [nextSummary, nextRoots, nextSkills] = await Promise.all([
      client.call("asset_summary", {}),
      client.call("asset_roots", {}),
      client.call("asset_skills", {}),
    ]);
    setSummary(nextSummary);
    setRoots(nextRoots);
    setSkills(nextSkills);
  }, [client]);

  useEffect(() => {
    void load().catch(() => setNote("资产图谱读取失败"));
  }, [load]);

  async function run(label: string, action: () => Promise<unknown>) {
    setBusy(true);
    setNote(null);
    try {
      await action();
      await load();
      setNote(label);
    } catch (cause) {
      setNote(cause instanceof Error ? cause.message : `${label}失败`);
    } finally {
      setBusy(false);
    }
  }

  async function addRoot(event: FormEvent) {
    event.preventDefault();
    const value = path.trim();
    if (!value) {
      return;
    }
    await run("已登记 Skill 根目录", async () => {
      await client.call("asset_add_root", { path: value });
      setPath("");
    });
  }

  // 分类到颜色的稳定映射：分类顺序决定色相，同一分类始终同色。
  const categoryIndex = new Map<string, number>();
  for (const stat of summary?.categories ?? []) {
    categoryIndex.set(stat.category, categoryIndex.size);
  }
  const visible = skills.filter((skill) => {
    if (onlyRepair && !skill.needsRepair) {
      return false;
    }
    const text = query.trim();
    if (!text) {
      return true;
    }
    return (
      skill.name.includes(text) ||
      skill.description.includes(text) ||
      skill.path.includes(text)
    );
  });
  const enabledRatio =
    summary && summary.skillCount > 0
      ? Math.round((summary.enabledCount / summary.skillCount) * 100)
      : 0;
  const maxCategory = Math.max(1, ...(summary?.categories ?? []).map((stat) => stat.skillCount));

  return (
    <section className="vault-block assets">
      <h2 className="section-head">资产图谱</h2>
      <p className="vault-note">
        只读取 Skill 的清单与元信息，正文留在原处。根目录离线时既有索引保持可读。
      </p>

      <dl className="terrain__stats">
        <div>
          <dt>Skill 总数</dt>
          <dd className="mono">{summary?.skillCount ?? 0}</dd>
        </div>
        <div>
          <dt>覆盖领域</dt>
          <dd className="mono">{summary?.categories.length ?? 0}</dd>
        </div>
        <div>
          <dt>启用比例</dt>
          <dd className="mono">{enabledRatio}%</dd>
        </div>
        <div>
          <dt>近 30 天</dt>
          <dd className="mono">
            +{summary?.recentAdded ?? 0} / -{summary?.recentRemoved ?? 0}
          </dd>
        </div>
      </dl>

      <form className="corpus-search" onSubmit={addRoot}>
        <input
          className="corpus-search__input"
          value={path}
          onChange={(event) => setPath(event.target.value)}
          placeholder="登记一个 Skill 目录，例如 D:/Skills"
          aria-label="Skill 根目录路径"
        />
        <button type="submit" className="vault-action" disabled={busy}>
          登记
        </button>
      </form>

      <ul className="terrain__sources">
        {roots.map((root) => (
          <li key={root.id} className="terrain__source" data-available={root.available}>
            <span className="terrain__source-path mono">{root.path}</span>
            <span className="terrain__source-meta mono">
              {root.available ? "在线" : "离线"} · {root.skillCount} 个 Skill
            </span>
            <button
              type="button"
              className="vault-action vault-action--quiet"
              disabled={busy}
              onClick={() =>
                run(root.available ? "扫描完成" : "根目录离线，索引已保留", () =>
                  client.call("asset_scan", { rootId: root.id }),
                )
              }
            >
              扫描
            </button>
            <button
              type="button"
              className="vault-action vault-action--quiet"
              disabled={busy}
              onClick={() => run("已移除根目录", () => client.call("asset_remove_root", { rootId: root.id }))}
            >
              移除
            </button>
          </li>
        ))}
        {roots.length === 0 ? (
          <li className="vault-empty">还没有登记 Skill 目录。先加一个文件夹。</li>
        ) : null}
      </ul>

      <h3 className="section-head section-head--minor">分类分布</h3>
      <ul className="assets__categories">
        {(summary?.categories ?? []).map((stat) => (
          <li key={stat.category} className="assets__category">
            <span className="assets__category-name">{stat.category}</span>
            <span
              className="assets__category-bar"
              style={{ width: `${(stat.skillCount / maxCategory) * 100}%` }}
              aria-hidden="true"
            />
            <span className="mono assets__category-count">{stat.skillCount}</span>
            <span className="assets__category-ratio mono">
              启用 {stat.enabledCount}/{stat.skillCount}
            </span>
          </li>
        ))}
        {(summary?.categories.length ?? 0) === 0 ? (
          <li className="vault-empty">还没有 Skill。先登记目录再扫描。</li>
        ) : null}
      </ul>

      <h3 className="section-head section-head--minor">AI 能力</h3>
      <ul className="assets__platforms">
        {(summary?.platforms ?? []).map((platform) => (
          <li key={platform.code} className="assets__platform" data-on={platform.enabled}>
            <span className="assets__platform-name">{platform.displayName}</span>
            <span className="assets__platform-model mono">{platform.modelName || "未填模型"}</span>
            <span className="assets__platform-status">{platformStatusLabel(platform.status)}</span>
          </li>
        ))}
        {(summary?.platforms.length ?? 0) === 0 ? (
          <li className="vault-empty">还没有接入模型平台。</li>
        ) : null}
      </ul>

      <div className="vault-bar">
        <h3 className="section-head section-head--minor">Skill 一览</h3>
        <label className="assets__filter">
          <input
            type="checkbox"
            checked={onlyRepair}
            onChange={(event) => setOnlyRepair(event.target.checked)}
          />
          只看待修复
        </label>
      </div>
      <form className="corpus-search" onSubmit={(event) => event.preventDefault()}>
        <input
          className="corpus-search__input"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="按名称、描述或路径过滤"
          aria-label="过滤 Skill"
        />
      </form>

      <ul className="assets__mosaic" aria-label="Skill 资产图谱">
        {visible.map((skill) => {
          const category = skill.category || "未归类";
          const index = (categoryIndex.get(category) ?? 0) % 6;
          return (
            <li key={skill.id}>
              <button
                type="button"
                className="asset-tile"
                data-category={index}
                data-repair={skill.needsRepair}
                data-missing={skill.missing}
                data-on={selected === skill.id}
                aria-pressed={selected === skill.id}
                onClick={() => setSelected(selected === skill.id ? null : skill.id)}
              >
                <span className="asset-tile__name">{skill.name}</span>
                <span className="asset-tile__category">{category}</span>
                <span className="asset-tile__meta mono">
                  {skill.enabled ? "启用" : "停用"}
                  {skill.needsRepair ? " · 待修复" : ""}
                  {skill.missing ? " · 不可读" : ""}
                </span>
              </button>
            </li>
          );
        })}
        {visible.length === 0 ? (
          <li className="vault-empty">
            {skills.length === 0 ? "还没有 Skill 记录。" : "没有符合条件的 Skill。"}
          </li>
        ) : null}
      </ul>

      {selected ? <SkillPanel key={selected} skillId={selected} /> : null}

      {note ? (
        <p className="vault-status" role="status">
          {note}
        </p>
      ) : null}
    </section>
  );
}

/** 单个 Skill 的展开面板：来源、路径、依赖与清单原文。 */
function SkillPanel({ skillId }: { readonly skillId: string }) {
  const detail = useCommand("asset_skill_detail", { skillId });

  if (detail.error) {
    return <p className="vault-error">读取失败：{detail.error.message}</p>;
  }
  if (!detail.data) {
    return <p className="vault-note">载入中…</p>;
  }

  const { skill, dependencies, manifestExcerpt } = detail.data;

  return (
    <div className="asset-panel">
      <dl className="master-panel__meta">
        <div>
          <dt>路径</dt>
          <dd className="mono">{skill.path}</dd>
        </div>
        <div>
          <dt>清单</dt>
          <dd>{skill.source || "缺清单"}</dd>
        </div>
        <div>
          <dt>版本</dt>
          <dd className="mono">{skill.version || "—"}</dd>
        </div>
        <div>
          <dt>最近改动</dt>
          <dd className="mono">{skill.modifiedAt ?? "—"}</dd>
        </div>
      </dl>
      {skill.tags.length > 0 ? (
        <p className="asset-panel__tags">标签：{skill.tags.join(" · ")}</p>
      ) : null}
      {skill.needsRepair ? (
        <p className="vault-error">待修复：{repairLabel(skill.repairReason)}</p>
      ) : null}
      {skill.missing ? (
        <p className="vault-error">根目录离线，索引保留但文件当前不可读。</p>
      ) : null}

      <h4 className="section-head section-head--minor">依赖项</h4>
      {dependencies.length > 0 ? (
        <ul className="citations">
          {dependencies.map((dependency) => (
            <li key={dependency.name} className="citation">
              <span className="citation__excerpt">{dependency.name}</span>
              <span className="citation__location mono">{dependency.version || "未标版本"}</span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="council__note">无外部依赖。</p>
      )}

      <details className="asset-panel__manifest">
        <summary>清单原文</summary>
        <pre className="mono">{manifestExcerpt || "清单不可读"}</pre>
      </details>
    </div>
  );
}

function platformStatusLabel(status: string): string {
  if (status === "ready") {
    return "已接入";
  }
  if (status === "disabled") {
    return "已配置未启用";
  }
  return "未配置";
}

function repairLabel(reason: string): string {
  if (reason === "manifest_missing") {
    return "缺少 manifest.json 或 SKILL.md";
  }
  if (reason === "frontmatter_missing") {
    return "SKILL.md 缺少 YAML 头";
  }
  if (reason === "name_missing") {
    return "清单缺少 name 字段";
  }
  if (reason.startsWith("manifest_invalid")) {
    return "清单格式无效";
  }
  return reason || "清单待修复";
}
