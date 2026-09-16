import { layerOf } from "../domain/layers";
import { LayerGlyph } from "./LayerGlyph";
import type { CouncilSeatSpeech, CouncilSourceView } from "../ipc/commands";

const ROLE_LABEL: Record<string, string> = {
  answer: "独立作答",
  cross: "交叉质询",
  synthesis: "收敛裁决",
};

const STATUS_LABEL: Record<string, string> = {
  answered: "已作答",
  failed: "有失败",
  pending: "待发言",
};

/**
 * 把一段发言拆成事实与判断两类句子，展示层再落实一次区分。
 * 以「据资料」开头的句子视为引用事实，其余视为自身判断。
 */
export function speechSegments(content: string): readonly { readonly kind: "fact" | "judgment"; readonly text: string }[] {
  return content
    .split(/(?<=[。！？])/)
    .map((sentence) => sentence.trim())
    .filter((sentence) => sentence.length > 0)
    .map((sentence) => ({
      kind: sentence.startsWith("据资料") ? ("fact" as const) : ("judgment" as const),
      text: sentence,
    }));
}

/**
 * 逐席发言：按席位展开各轮全文，同时提供按轮次分组与表格两种等效视图。
 */
export function SeatSpeech({
  seats,
  sources = [],
  onRetry,
  onFollowUp,
  busy = false,
}: {
  readonly seats: readonly CouncilSeatSpeech[];
  readonly sources?: readonly CouncilSourceView[] | undefined;
  readonly onRetry?: ((masterId: string, round: number) => void) | undefined;
  readonly onFollowUp?:
    | ((masterId: string, round: number, text: string) => void)
    | undefined;
  readonly busy?: boolean;
}) {
  if (seats.length === 0) {
    return null;
  }

  const rounds = [...new Set(seats.flatMap((seat) => seat.rounds.map((round) => round.round)))].sort(
    (left, right) => left - right,
  );

  return (
    <section className="speech">
      <h2 className="section-head">逐席发言</h2>
      <ul className="speech__seats">
        {seats.map((seat) => {
          const layer = layerOf(seat.layer);
          const own = sources.filter((source) => source.masterId === seat.masterId);
          return (
            <li key={seat.masterId} className="speech__seat" data-layer={seat.layer}>
              <details className="speech__detail">
                <summary className="speech__summary">
                  <span className="speech__sigil" aria-hidden="true">
                    <LayerGlyph glyph={layer.glyph} size={13} />
                  </span>
                  <span className="speech__name">{seat.masterName}</span>
                  <span className="speech__layer">{layer.name}</span>
                  <span className="speech__status" data-status={seat.status}>
                    {STATUS_LABEL[seat.status] ?? seat.status}
                  </span>
                  <span className="speech__count mono">{seat.rounds.length} 轮</span>
                  {own.length > 0 ? (
                    <span className="speech__sources mono">
                      补充检索 {own.length} 条
                    </span>
                  ) : null}
                </summary>
                {seat.rounds.length === 0 ? (
                  <p className="speech__empty">该席位本轮尚无发言记录。</p>
                ) : (
                  <ol className="speech__rounds">
                    {seat.rounds.map((round) => (
                      <li key={`${round.round}-${round.role}`} className="speech__round">
                        <div className="speech__round-head">
                          <span className="speech__round-label mono">
                            第 {round.round} 轮 · {ROLE_LABEL[round.role] ?? round.role}
                          </span>
                          {round.status !== "ok" ? (
                            <span className="speech__error mono">
                              {round.errorCode ?? "调用失败"}
                            </span>
                          ) : null}
                          {round.status !== "ok" && onRetry ? (
                            <button
                              className="speech__retry"
                              type="button"
                              disabled={busy}
                              onClick={() => onRetry(seat.masterId, round.round)}
                            >
                              重试该轮
                            </button>
                          ) : null}
                        </div>
                        {round.content
                          ? speechSegments(round.content).map((segment, index) => (
                              <span
                                key={index}
                                className="speech__segment"
                                data-kind={segment.kind}
                              >
                                {segment.text}
                              </span>
                            ))
                          : null}
                        {onFollowUp && round.status === "ok" && round.content ? (
                          <button
                            className="speech__followup"
                            type="button"
                            onClick={() =>
                              onFollowUp(seat.masterId, round.round, round.content)
                            }
                          >
                            就这段追问
                          </button>
                        ) : null}
                      </li>
                    ))}
                  </ol>
                )}
              </details>
            </li>
          );
        })}
      </ul>

      <details className="speech__outline">
        <summary className="speech__outline-summary">按轮次分组的发言记录</summary>
        {rounds.map((round) => (
          <div key={round} className="speech__outline-round">
            <h3 className="speech__outline-head">第 {round} 轮</h3>
            <ul className="speech__outline-list">
              {seats
                .filter((seat) => seat.rounds.some((item) => item.round === round))
                .map((seat) => {
                  const item = seat.rounds.find((entry) => entry.round === round)!;
                  return (
                    <li key={seat.masterId} className="speech__outline-item">
                      <span className="speech__name">{seat.masterName}</span>
                      <span className="speech__outline-text">
                        {item.status === "ok" ? item.content : `未完成（${item.errorCode ?? "失败"}）`}
                      </span>
                    </li>
                  );
                })}
            </ul>
          </div>
        ))}
      </details>

      <details className="speech__table-wrap">
        <summary className="speech__outline-summary">表格等效视图</summary>
        <table className="speech__table" aria-label="逐席发言记录">
          <thead>
            <tr>
              <th scope="col">席位</th>
              <th scope="col">轮次</th>
              <th scope="col">角色</th>
              <th scope="col">状态</th>
              <th scope="col">发言</th>
            </tr>
          </thead>
          <tbody>
            {seats.flatMap((seat) =>
              seat.rounds.map((item) => (
                <tr key={`${seat.masterId}-${item.round}-${item.role}`}>
                  <td>{seat.masterName}</td>
                  <td className="mono">{item.round}</td>
                  <td>{ROLE_LABEL[item.role] ?? item.role}</td>
                  <td>{item.status === "ok" ? "已完成" : (item.errorCode ?? "失败")}</td>
                  <td>{item.content || "—"}</td>
                </tr>
              )),
            )}
          </tbody>
        </table>
      </details>
    </section>
  );
}
