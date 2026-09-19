import { useEffect, useMemo, useRef, useState } from "react";
import { LAYER_KEYS, layerOf, type LayerKey } from "../domain/layers";
import { LayerGlyph } from "./LayerGlyph";
import type { MasterSummary } from "../ipc/commands";

/** 第一阶段名册规划容量；超出只提示，不拦截安装。 */
export const ROSTER_CAPACITY = 20;

const EMPTY_HINT = "还没招募 · 去「藏」装种子包，或在「炼」蒸馏一位";

function isCjk(ch: string): boolean {
  const code = ch.codePointAt(0) ?? 0;
  return code >= 0x3400 && code <= 0x9fff;
}

/** 与内核 scoring::tokens 对齐：中文取相邻二字，英文取整词。 */
export function tokenize(text: string): Set<string> {
  const normalized = text.toLowerCase();
  const out = new Set<string>();
  const cjkRun: string[] = [];
  let word = "";

  const flushWord = () => {
    if ([...word].length >= 2) {
      out.add(word);
    }
    word = "";
  };
  const flushCjk = () => {
    for (let index = 0; index + 1 < cjkRun.length; index += 1) {
      out.add(`${cjkRun[index]}${cjkRun[index + 1]}`);
    }
    cjkRun.length = 0;
  };

  for (const ch of normalized) {
    if (isCjk(ch)) {
      flushWord();
      cjkRun.push(ch);
    } else if (/[a-z0-9]/i.test(ch)) {
      flushCjk();
      word += ch;
    } else {
      flushWord();
      flushCjk();
    }
  }
  flushWord();
  flushCjk();
  return out;
}

export function overlapCount(left: Set<string>, right: Set<string>): number {
  let count = 0;
  for (const token of left) {
    if (right.has(token)) {
      count += 1;
    }
  }
  return count;
}

export function layerUnitCount(master: MasterSummary, layer: LayerKey): number {
  return master.layerProfile.find((entry) => entry.layer === layer)?.unitCount ?? 0;
}

export function layerFit(
  master: MasterSummary,
  layer: LayerKey,
  question: string,
): { readonly units: number; readonly overlap: number } {
  const profile = master.layerProfile.find((entry) => entry.layer === layer);
  const titles = profile?.unitTitles.join(" ") ?? "";
  return {
    units: profile?.unitCount ?? 0,
    overlap: overlapCount(tokenize(titles), tokenize(question)),
  };
}

/**
 * 落座层：优先有料且尚未覆盖的最深一题；并列按道法术气器势。
 * 有料的题都被占了就退回他最深的一题；完全没单元则退回声明层。
 */
export function deepestLayer(
  master: MasterSummary,
  covered: ReadonlySet<LayerKey>,
): LayerKey {
  for (const pass of [0, 1] as const) {
    let best: { layer: LayerKey; count: number } | null = null;
    for (const layer of LAYER_KEYS) {
      if (pass === 0 && covered.has(layer)) {
        continue;
      }
      const count = layerUnitCount(master, layer);
      if (count === 0) {
        continue;
      }
      if (!best || count > best.count) {
        best = { layer, count };
      }
    }
    if (best) {
      return best.layer;
    }
  }
  return master.layers[0] ?? "dao";
}

export function predictedSeats(
  roster: readonly MasterSummary[],
  pinned: readonly string[],
): ReadonlyMap<string, LayerKey> {
  const byId = new Map(roster.map((master) => [master.id, master]));
  const covered = new Set<LayerKey>();
  const assigned = new Map<string, LayerKey>();
  for (const id of pinned) {
    const master = byId.get(id);
    if (!master) {
      continue;
    }
    const layer = deepestLayer(master, covered);
    covered.add(layer);
    assigned.set(id, layer);
  }
  return assigned;
}

export function previewLayer(
  master: MasterSummary,
  roster: readonly MasterSummary[],
  pinned: readonly string[],
): LayerKey {
  const others = pinned.filter((id) => id !== master.id);
  const covered = new Set<LayerKey>();
  const byId = new Map(roster.map((item) => [item.id, item]));
  for (const id of others) {
    const item = byId.get(id);
    if (!item) {
      continue;
    }
    covered.add(deepestLayer(item, covered));
  }
  return deepestLayer(master, covered);
}

export function sortRoster(
  roster: readonly MasterSummary[],
  layer: LayerKey,
  question: string,
): MasterSummary[] {
  return [...roster].sort((left, right) => {
    const leftUnits = layerUnitCount(left, layer);
    const rightUnits = layerUnitCount(right, layer);
    const leftReady = leftUnits > 0 ? 1 : 0;
    const rightReady = rightUnits > 0 ? 1 : 0;
    if (leftReady !== rightReady) {
      return rightReady - leftReady;
    }
    if (leftUnits !== rightUnits) {
      return rightUnits - leftUnits;
    }
    const leftFit = layerFit(left, layer, question);
    const rightFit = layerFit(right, layer, question);
    if (leftFit.overlap !== rightFit.overlap) {
      return rightFit.overlap - leftFit.overlap;
    }
    return left.name.localeCompare(right.name, "zh");
  });
}

export function SeatPicker({
  layer,
  question,
  roster,
  pinned,
  onConfirm,
  onClose,
}: {
  readonly layer: LayerKey;
  readonly question: string;
  readonly roster: readonly MasterSummary[];
  readonly pinned: readonly string[];
  readonly onConfirm: (masterId: string) => void;
  readonly onClose: () => void;
}) {
  const meta = layerOf(layer);
  const panelRef = useRef<HTMLDivElement | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [view, setView] = useState<"bars" | "list">("bars");

  const ordered = useMemo(
    () => sortRoster(roster, layer, question),
    [roster, layer, question],
  );
  const vacancies = Math.max(0, ROSTER_CAPACITY - roster.length);
  const coverage = useMemo(
    () =>
      LAYER_KEYS.map((key) => ({
        key,
        count: roster.filter((master) => layerUnitCount(master, key) > 0).length,
      })),
    [roster],
  );

  useEffect(() => {
    panelRef.current?.focus();
  }, []);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  function confirm() {
    if (!selected) {
      return;
    }
    onConfirm(selected);
  }

  return (
    <div className="picker">
      <button
        className="picker__scrim"
        type="button"
        aria-label="关闭点将面板"
        onClick={onClose}
      />
      <div
        ref={panelRef}
        className="picker__panel"
        role="dialog"
        aria-modal="true"
        aria-label="选角"
        tabIndex={-1}
      >
        <header className="picker__head">
          <h2 className="picker__title">
            {meta.name} · {meta.question}
          </h2>
          <p className="picker__coverage" aria-label="各题有料人数">
            {coverage.map((item) => (
              <span key={item.key} data-layer={item.key}>
                {layerOf(item.key).name} {item.count}
              </span>
            ))}
          </p>
        </header>
        <div className="picker__views" role="group" aria-label="等效视图">
          <button
            type="button"
            className="picker__view"
            aria-pressed={view === "bars"}
            onClick={() => setView("bars")}
          >
            条形
          </button>
          <button
            type="button"
            className="picker__view"
            aria-pressed={view === "list"}
            onClick={() => setView("list")}
          >
            列表
          </button>
        </div>
        {view === "bars" ? (
          <ul className="picker__grid">
            {ordered.map((master) => (
              <li key={master.id}>
                <MasterCard
                  master={master}
                  layer={layer}
                  question={question}
                  roster={roster}
                  pinned={pinned}
                  selected={selected === master.id}
                  onSelect={() => setSelected(master.id)}
                />
              </li>
            ))}
            {Array.from({ length: vacancies }, (_, index) => (
              <li key={`vacancy-${index}`}>
                <div className="picker__vacancy" aria-disabled="true">
                  {EMPTY_HINT}
                </div>
              </li>
            ))}
          </ul>
        ) : (
          <>
            <table className="picker__table" aria-label="六题积累">
              <thead>
                <tr>
                  <th>姓名</th>
                  <th>领域</th>
                  {LAYER_KEYS.map((key) => (
                    <th key={key}>{layerOf(key).name}</th>
                  ))}
                  <th>本席料</th>
                  <th>重合</th>
                  <th>会坐</th>
                </tr>
              </thead>
              <tbody>
                {ordered.map((master) => {
                  const fit = layerFit(master, layer, question);
                  const willSit = previewLayer(master, roster, pinned);
                  const will = layerOf(willSit);
                  return (
                    <tr
                      key={master.id}
                      data-master={master.id}
                      data-selected={selected === master.id ? "true" : "false"}
                    >
                      <td>
                        <button
                          type="button"
                          className="picker__row-select"
                          aria-pressed={selected === master.id}
                          onClick={() => setSelected(master.id)}
                        >
                          {master.name}
                        </button>
                      </td>
                      <td>{master.domain}</td>
                      {master.layerProfile.map((entry) => (
                        <td key={entry.layer} className="mono">
                          {entry.unitCount}
                        </td>
                      ))}
                      <td className="mono">{fit.units}</td>
                      <td className="mono">{fit.overlap}</td>
                      <td>
                        {will.name} · {will.question}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
            {vacancies > 0 ? (
              <ul className="picker__vacancies" aria-label="未招募空位">
                {Array.from({ length: vacancies }, (_, index) => (
                  <li key={`vacancy-list-${index}`} className="picker__vacancy">
                    {EMPTY_HINT}
                  </li>
                ))}
              </ul>
            ) : null}
          </>
        )}
        <div className="picker__actions">
          <button
            className="picker__confirm"
            type="button"
            disabled={!selected}
            onClick={confirm}
          >
            确认入席
          </button>
          <button className="picker__cancel" type="button" onClick={onClose}>
            取消
          </button>
        </div>
      </div>
    </div>
  );
}

function MasterCard({
  master,
  layer,
  question,
  roster,
  pinned,
  selected,
  onSelect,
}: {
  readonly master: MasterSummary;
  readonly layer: LayerKey;
  readonly question: string;
  readonly roster: readonly MasterSummary[];
  readonly pinned: readonly string[];
  readonly selected: boolean;
  readonly onSelect: () => void;
}) {
  const fit = layerFit(master, layer, question);
  const willSit = previewLayer(master, roster, pinned);
  const will = layerOf(willSit);
  const ready = fit.units > 0;
  return (
    <button
      type="button"
      className="picker__card"
      data-master={master.id}
      data-tier={ready ? "ready" : "fallback"}
      aria-pressed={selected}
      aria-label={`${master.name} · ${master.domain}`}
      onClick={onSelect}
    >
      <span className="picker__name">{master.name}</span>
      <span className="picker__domain">{master.domain}</span>
      <span className="picker__bar" aria-hidden="true">
        {master.layerProfile.map((entry) => (
          <span
            key={entry.layer}
            className="picker__cell"
            data-layer={entry.layer}
            data-active={entry.layer === layer ? "true" : "false"}
            data-filled={entry.unitCount > 0 ? "true" : "false"}
            title={`${layerOf(entry.layer).name} ${entry.unitCount}`}
          >
            <LayerGlyph glyph={layerOf(entry.layer).glyph} size={8} />
          </span>
        ))}
      </span>
      <span className="picker__fit">
        这一题 {fit.units} 条料 · 与本题重合 {fit.overlap} 处
      </span>
      <span className="picker__will">
        {willSit === layer
          ? "他会坐本题"
          : `他会坐：${will.name} · ${will.question}`}
      </span>
    </button>
  );
}
