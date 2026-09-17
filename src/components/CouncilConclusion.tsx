import { DivergenceCurve } from "./DivergenceCurve";
import { SeatSpeech } from "./SeatSpeech";
import { LayerGlyph } from "./LayerGlyph";
import { formatMoney, formatTime, stanceChangeLabel } from "../domain/labels";
import { LAYER_KEYS, layerOf } from "../domain/layers";
import type {
  CouncilConclusionView,
  CouncilSourceView,
  CouncilStanceChange,
  DivergenceView,
  FollowUpAnchor,
} from "../ipc/commands";
import type { LayerKey } from "../domain/layers";

/** 按题归拢分歧，题序固定为道法术气器势，避免同一场会诊顺序漂移。 */
function groupByLayer(
  divergences: readonly DivergenceView[],
): readonly { layer: LayerKey; items: readonly DivergenceView[] }[] {
  return LAYER_KEYS.map((layer) => ({
    layer,
    items: divergences.filter((item) => item.layer === layer),
  })).filter((group) => group.items.length > 0);
}

function sentences(text: string): readonly string[] {
  return text
    .split(/(?<=[。！？])/)
    .map((sentence) => sentence.trim())
    .filter((sentence) => sentence.length > 0);
}

function sourcesBlock(sources: readonly CouncilSourceView[]) {
  if (sources.length === 0) {
    return <p className="conclusion__empty">这次会诊没有用到外部资料。</p>;
  }
  return (
    <ul className="conclusion__sources">
      {sources.map((source) => (
        <li key={source.id} className="conclusion__source">
          <div className="conclusion__source-head">
            <span className="conclusion__source-title">{source.title}</span>
            {source.flagged ? (
              <span className="conclusion__source-flag">
                可疑指令，仅作资料
              </span>
            ) : null}
            <span className="conclusion__source-time">{formatTime(source.fetchedAt)}</span>
          </div>
          <p className="conclusion__source-snippet">{source.snippet}</p>
          <a
            className="conclusion__source-url"
            href={source.url}
            target="_blank"
            rel="noreferrer"
          >
            {source.url}
          </a>
          {source.hasBody ? (
            <details className="conclusion__source-body">
              <summary>展开留存的正文</summary>
              <p>正文太长时只保留前面一段，存在本机。</p>
            </details>
          ) : null}
        </li>
      ))}
    </ul>
  );
}

/** 立场变化：每题一条，说明这一轮和上一轮相比是延续、调整还是转向。 */
function stanceChangesBlock(changes: readonly CouncilStanceChange[]) {
  return (
    <div className="conclusion__stances">
      <h4 className="conclusion__stances-head">每题与上一轮相比</h4>
      <ul className="conclusion__stance-list">
        {changes.map((item) => {
          const meta = layerOf(item.layer);
          const compared =
            item.change !== "new" && item.change !== "dropped";
          return (
            <li
              key={item.layer}
              className="conclusion__stance"
              data-change={item.change}
            >
              <div className="conclusion__stance-head">
                <span aria-hidden="true">
                  <LayerGlyph glyph={meta.glyph} size={12} />
                </span>
                <span className="conclusion__stance-layer">
                  {meta.name} · {meta.question}
                </span>
                <span className="conclusion__stance-change">
                  {stanceChangeLabel(item.change)}
                </span>
                {compared ? (
                  <span className="conclusion__stance-similarity">
                    用词重合 {Math.round(item.similarity * 100)}%
                  </span>
                ) : null}
              </div>
              {item.summary ? (
                <p className="conclusion__stance-text">{item.summary}</p>
              ) : (
                <p className="conclusion__stance-text conclusion__empty">
                  这一轮没有人站上这一题。
                </p>
              )}
              {item.previousSummary ? (
                <p className="conclusion__stance-previous">
                  上一轮
                  {item.previousMasterName
                    ? `（${item.previousMasterName}）`
                    : ""}
                  ：{item.previousSummary}
                </p>
              ) : null}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/**
 * 会诊结论详情页：结论要点、分歧与未决、收敛过程、逐席依据、外部来源、演化链六段。
 */
export function CouncilConclusion({
  view,
  threshold,
  onFollowUp,
}: {
  readonly view: CouncilConclusionView;
  readonly threshold: number | null;
  readonly onFollowUp: (anchor: FollowUpAnchor) => void;
}) {
  const { session } = view;
  const single = view.metrics.length <= 1;

  return (
    <article className="conclusion" aria-label="会诊结论详情">
      <header className="conclusion__head">
        <h2 className="section-head">结论详情</h2>
        <p className="prose conclusion__question">{session.question}</p>
        {session.parentSessionId ? (
          <p className="conclusion__parent">这场是由之前一次会诊追问出来的。</p>
        ) : null}
      </header>

      <section className="conclusion__section">
        <h3 className="section-head">一 · 结论要点</h3>
        {session.conclusion ? (
          <ul className="conclusion__points">
            {sentences(session.conclusion).map((sentence, index) => (
              <li key={index} className="conclusion__point">
                <span className="conclusion__point-text">{sentence}</span>
                <button
                  className="conclusion__ask"
                  type="button"
                  onClick={() =>
                    onFollowUp({ kind: "conclusion", text: sentence })
                  }
                >
                  追问
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="conclusion__empty">这场会诊还没有收敛出结论。</p>
        )}
      </section>

      <section className="conclusion__section">
        <h3 className="section-head">二 · 还没谈拢的分歧</h3>
        {session.divergences.length === 0 ? (
          <p className="conclusion__empty">这次没有留下未决的分歧。</p>
        ) : (
          <>
            <div className="conclusion__divergences">
              {groupByLayer(session.divergences).map((group) => {
                const meta = layerOf(group.layer);
                return (
                  <section
                    key={group.layer}
                    className="conclusion__divergence-group"
                    aria-label={`${meta.name} · ${meta.question}`}
                  >
                    <h4 className="conclusion__divergence-head">
                      <span aria-hidden="true">
                        <LayerGlyph glyph={meta.glyph} size={13} />
                      </span>
                      <span>
                        {meta.name} · {meta.question}
                      </span>
                      <span className="conclusion__divergence-count">
                        {group.items.length} 条
                      </span>
                    </h4>
                    <ul className="conclusion__divergence-list">
                      {group.items.map((item) => (
                        <li key={item.text} className="conclusion__divergence">
                          <span className="conclusion__divergence-text">
                            {item.text}
                          </span>
                          <button
                            className="conclusion__ask"
                            type="button"
                            onClick={() =>
                              onFollowUp({ kind: "divergence", text: item.text })
                            }
                          >
                            追问
                          </button>
                        </li>
                      ))}
                    </ul>
                  </section>
                );
              })}
            </div>
            <details className="conclusion__divergence-table-wrap">
              <summary>分歧清单（表格视图）</summary>
              <table
                className="conclusion__divergence-table"
                aria-label="还没谈拢的分歧"
              >
                <thead>
                  <tr>
                    <th scope="col">题</th>
                    <th scope="col">分歧</th>
                  </tr>
                </thead>
                <tbody>
                  {session.divergences.map((item) => {
                    const meta = layerOf(item.layer);
                    return (
                      <tr key={item.text}>
                        <td>
                          {meta.name} · {meta.question}
                        </td>
                        <td>{item.text}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </details>
          </>
        )}
      </section>

      <section className="conclusion__section">
        <h3 className="section-head">三 · 分歧的变化</h3>
        {view.metrics.length === 0 ? (
          <p className="conclusion__empty">还没有可供对比的轮次数据。</p>
        ) : (
          <>
            <DivergenceCurve metrics={view.metrics} threshold={threshold} />
            {single ? (
              <p className="conclusion__note">
                目前只有一轮，曲线上只有一个点，还看不出分歧是在收敛还是扩大。
              </p>
            ) : null}
          </>
        )}
      </section>

      <section className="conclusion__section">
        <h3 className="section-head">四 · 各位大师的依据</h3>
        {view.speeches.length === 0 ? (
          <p className="conclusion__empty">尚无逐席发言记录。</p>
        ) : (
          <SeatSpeech
            seats={view.speeches}
            sources={view.sources}
            onFollowUp={(masterId, round, text) =>
              onFollowUp({ kind: "answer", text, masterId, round })
            }
          />
        )}
      </section>

      <section className="conclusion__section">
        <h3 className="section-head">五 · 用到的外部资料</h3>
        {sourcesBlock(view.sources)}
      </section>

      <section className="conclusion__section">
        <h3 className="section-head">六 · 前后几次结论</h3>
        {view.stanceChanges.length > 0 ? stanceChangesBlock(view.stanceChanges) : null}
        {view.history.length === 0 ? (
          <p className="conclusion__empty">同一主题下还没有更早的结论。</p>
        ) : (
          <ol className="conclusion__history">
            {view.history.map((item) => (
              <li key={item.id} className="conclusion__history-item">
                <span className="conclusion__history-time">{formatTime(item.createdAt)}</span>
                <p className="conclusion__history-text">{item.conclusion || item.question}</p>
                <button
                  className="conclusion__ask"
                  type="button"
                  onClick={() =>
                    onFollowUp({
                      kind: "conclusion",
                      text: item.conclusion || item.question,
                    })
                  }
                >
                  追问
                </button>
              </li>
            ))}
          </ol>
        )}
        <p className="conclusion__prompt-version">
          提问模板 {view.promptVersion || "未记录"}，回看时可据此还原当时的问法。
        </p>
        <p className="conclusion__cost">
          本次提问模型 {view.llmCalls} 次 · 查资料 {view.searchCalls} 次 · 费用约{" "}
          {formatMoney(view.costMicros, view.currency)}
          {view.priced ? "" : "（还没填单价，暂按 0 计）"}
        </p>
      </section>
    </article>
  );
}
