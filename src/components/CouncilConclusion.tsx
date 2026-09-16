import { DivergenceCurve } from "./DivergenceCurve";
import { SeatSpeech } from "./SeatSpeech";
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

/** 微元转展示金额：保留两位小数，币种前缀按记账币种给出。 */
function formatMoney(micros: number, currency: string): string {
  const amount = (micros / 1_000_000).toFixed(2);
  return currency === "CNY" ? `¥${amount}` : `${amount} ${currency}`;
}

function sourcesBlock(sources: readonly CouncilSourceView[]) {
  if (sources.length === 0) {
    return <p className="conclusion__empty">本次会诊未启用外部检索。</p>;
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
            <span className="conclusion__source-time mono">{source.fetchedAt}</span>
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
              <summary>展开正文快照</summary>
              <p>正文已按上限截断后保存在本地快照中。</p>
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
          <p className="conclusion__parent mono">
            追问会话 · 母会话 {session.parentSessionId}
          </p>
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
        <h3 className="section-head">二 · 分歧与未决</h3>
        {session.divergences.length === 0 ? (
          <p className="conclusion__empty">本次会诊没有记录未决分歧。</p>
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
        <h3 className="section-head">三 · 收敛过程</h3>
        {view.metrics.length === 0 ? (
          <p className="conclusion__empty">本次会诊尚未产生质询轮指标。</p>
        ) : (
          <>
            <DivergenceCurve metrics={view.metrics} threshold={threshold} />
            {single ? (
              <p className="conclusion__note">
                只有一个质询轮，曲线仅呈现单点，轮次尚不足以观察收敛趋势。
              </p>
            ) : null}
          </>
        )}
      </section>

      <section className="conclusion__section">
        <h3 className="section-head">四 · 逐席依据</h3>
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
        <h3 className="section-head">五 · 外部来源</h3>
        {sourcesBlock(view.sources)}
      </section>

      <section className="conclusion__section">
        <h3 className="section-head">六 · 演化链与追问</h3>
        {view.history.length === 0 ? (
          <p className="conclusion__empty">同一主题下还没有更早的结论。</p>
        ) : (
          <ol className="conclusion__history">
            {view.history.map((item) => (
              <li key={item.id} className="conclusion__history-item">
                <span className="conclusion__history-time mono">{item.createdAt}</span>
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
        <p className="conclusion__prompt-version mono">
          提示词版本 {view.promptVersion || "未记录"} · 会诊回看可据此复现当时的提问模板
        </p>
        <p className="conclusion__cost mono">
          本次调用 {view.llmCalls} 次模型 · {view.searchCalls} 次检索 · 费用估算{" "}
          {formatMoney(view.costMicros, view.currency)}
          {view.priced ? "" : "（未配置单价，按零计）"}
        </p>
      </section>
    </article>
  );
}
