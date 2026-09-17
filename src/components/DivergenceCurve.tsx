import type { CouncilRoundMetric } from "../ipc/commands";
import { divergenceModeLabel } from "../domain/labels";

const VIEW_W = 360;
const VIEW_H = 128;
const PAD_X = 34;
const PAD_TOP = 14;
const PAD_BOTTOM = 30;

/** 判定方式与回退标记合成一段可读文字。 */
function methodNote(metric: CouncilRoundMetric): string {
  return `${divergenceModeLabel(metric.method)}${metric.fellBack ? "（回退）" : ""}`;
}

/** 分歧度映射到图内坐标；1 在顶，0 在底。 */
function pointOf(index: number, total: number, divergence: number) {
  const usable = VIEW_W - PAD_X * 2;
  const x = total <= 1 ? VIEW_W / 2 : PAD_X + (usable * index) / (total - 1);
  const ratio = Math.min(1, Math.max(0, divergence));
  const y = PAD_TOP + (1 - ratio) * (VIEW_H - PAD_TOP - PAD_BOTTOM);
  return { x, y };
}

/**
 * 分歧曲线：折线加收敛阈值参考线，配一份表格作为等效视图。
 *
 * 颜色之外还有数值与文字标注，单轮质询时明确提示观测点不足。
 */
export function DivergenceCurve({
  metrics,
  threshold,
}: {
  readonly metrics: readonly CouncilRoundMetric[];
  readonly threshold: number | null;
}) {
  const rounds = [...metrics].sort((left, right) => left.round - right.round);
  if (rounds.length === 0) {
    return (
      <p className="council__note" data-tone="muted">
        这次没有留下可供对比的轮次数据。
      </p>
    );
  }

  const points = rounds.map((metric, index) =>
    pointOf(index, rounds.length, metric.divergence),
  );
  const first = rounds[0]!;
  const last = rounds[rounds.length - 1]!;
  const line = points.map((point) => `${point.x.toFixed(1)},${point.y.toFixed(1)}`).join(" ");
  const thresholdPoint =
    threshold === null ? null : pointOf(0, rounds.length, threshold).y;

  return (
    <div className="divergence">
      <svg
        className="divergence__chart"
        viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
        role="img"
        aria-label={`分歧变化图，从第 ${first.round} 轮到第 ${last.round} 轮，当前分歧程度 ${last.divergence.toFixed(2)}`}
      >
        {[0, 0.5, 1].map((tick) => {
          const { y } = pointOf(0, rounds.length, tick);
          return (
            <g key={tick}>
              <line
                className="divergence__grid"
                x1={PAD_X}
                x2={VIEW_W - PAD_X}
                y1={y}
                y2={y}
              />
              <text className="divergence__tick" x={PAD_X - 6} y={y + 3} textAnchor="end">
                {tick.toFixed(1)}
              </text>
            </g>
          );
        })}
        {thresholdPoint !== null ? (
          <>
            <line
              className="divergence__threshold"
              x1={PAD_X}
              x2={VIEW_W - PAD_X}
              y1={thresholdPoint}
              y2={thresholdPoint}
            />
            <text
              className="divergence__threshold-label"
              x={VIEW_W - PAD_X}
              y={thresholdPoint - 4}
              textAnchor="end"
            >
              收敛线 {threshold?.toFixed(2)}
            </text>
          </>
        ) : null}
        <polyline className="divergence__line" points={line} />
        {rounds.map((metric, index) => {
          const point = points[index]!;
          return (
            <circle
              key={metric.round}
              className="divergence__dot"
              data-converged={metric.converged ? "true" : "false"}
              cx={point.x}
              cy={point.y}
              r={4}
            >
              <title>
                第 {metric.round} 轮 · 分歧程度 {metric.divergence.toFixed(2)} ·{" "}
                {methodNote(metric)}
              </title>
            </circle>
          );
        })}
        {rounds.map((metric, index) => {
          const point = points[index]!;
          return (
            <text
              key={`label-${metric.round}`}
              className="divergence__axis"
              x={point.x}
              y={VIEW_H - 12}
              textAnchor="middle"
            >
              第 {metric.round} 轮
            </text>
          );
        })}
      </svg>
      <p className="divergence__caption">
        {rounds.length === 1
          ? "只有一轮，图上只有一个点，还看不出走势。"
          : `分歧程度从 ${first.divergence.toFixed(2)} 变到 ${last.divergence.toFixed(2)}，${last.converged ? "已经收敛" : "还没收敛"}。`}
        {rounds.some((metric) => metric.fellBack)
          ? ` 第 ${rounds
              .filter((metric) => metric.fellBack)
              .map((metric) => metric.round)
              .join("、")} 轮只能按用词判断，当时无法判断观点方向。`
          : ""}
      </p>
      <details className="divergence__table">
        <summary>逐轮数据表</summary>
        <table>
          <caption>每一轮的相似程度与分歧程度</caption>
          <thead>
            <tr>
              <th scope="col">轮次</th>
              <th scope="col">参与人数</th>
              <th scope="col">平均相似程度</th>
              <th scope="col">分歧程度</th>
              <th scope="col">判定方式</th>
              <th scope="col">是否收敛</th>
            </tr>
          </thead>
          <tbody>
            {rounds.map((metric) => (
              <tr key={metric.round}>
                <th scope="row">第 {metric.round} 轮</th>
                <td>{metric.participantCount}</td>
                <td className="mono">{metric.avgSimilarity.toFixed(2)}</td>
                <td className="mono">{metric.divergence.toFixed(2)}</td>
                <td>{methodNote(metric)}</td>
                <td>{metric.converged ? "是" : "否"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </div>
  );
}
