import type { CSSProperties } from "react";
import { ambientGlow, temperatureBand } from "../domain/furnace";

const SEGMENTS = 12;

/**
 * 炉温是招牌元素：竖向 gauge 加一个度数。
 * 它同时决定界面环境光强度，冷炉暗淡、热炉温暖。
 * 离线时炉温环外再加一圈虚线，外部能力不可用这件事一眼可见。
 */
export function FurnaceTemp({
  temp,
  offline = false,
}: {
  readonly temp: number;
  readonly offline?: boolean;
}) {
  const band = temperatureBand(temp);
  const lit = Math.round((Math.max(0, Math.min(100, temp)) / 100) * SEGMENTS);

  return (
    <aside className="furnace" aria-label="炉温" data-offline={offline}>
      <span className="furnace__ring" data-offline={offline} aria-hidden="true" />
      <div
        className="furnace__gauge"
        role="meter"
        aria-valuenow={temp}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={`炉温 ${temp} 度，${band.label}`}
      >
        {Array.from({ length: SEGMENTS }, (_, index) => {
          const fromBottom = SEGMENTS - index;
          return (
            <span
              key={index}
              className="furnace__segment"
              data-lit={fromBottom <= lit}
              style={{ "--heat": fromBottom / SEGMENTS } as CSSProperties}
            />
          );
        })}
      </div>
      <div className="furnace__reading">
        <span className="furnace__value mono">{temp}°</span>
        <span className="furnace__band">{band.label}</span>
      </div>
      <span className="visually-hidden">
        环境光强度 {Math.round(ambientGlow(temp) * 100)}%
      </span>
      {offline ? (
        <span className="furnace__offline" role="status">
          离线运行 · 已装大师、记录与图谱仍可读可搜
        </span>
      ) : null}
    </aside>
  );
}
