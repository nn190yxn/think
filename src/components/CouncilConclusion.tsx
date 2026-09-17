import { DivergenceCurve } from "./DivergenceCurve";
import { SeatSpeech } from "./SeatSpeech";
import { formatMoney, formatTime } from "../domain/labels";
import type {
  CouncilConclusionView,
  CouncilSourceView,
  FollowUpAnchor,
} from "../ipc/commands";

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
          <ul className="conclusion__divergences">
            {session.divergences.map((item) => (
              <li key={item} className="conclusion__divergence">
                <span className="conclusion__divergence-text">{item}</span>
                <button
                  className="conclusion__ask"
                  type="button"
                  onClick={() =>
                    onFollowUp({ kind: "divergence", text: item })
                  }
                >
                  追问
                </button>
              </li>
            ))}
          </ul>
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
