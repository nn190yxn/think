import { useCallback, useEffect, useMemo, useState } from "react";
import type { CSSProperties } from "react";
import { useCommands } from "../app/ipc";
import type { CouncilSessionView, Insight, InsightKindKey } from "../ipc/commands";

/** 洞察分色：关联赭金、冲突朱砂、盲区玄青，与层次令牌同源。 */
const KIND_TOKEN: Record<InsightKindKey, string> = {
  relation: "var(--layer-shu)",
  conflict: "var(--layer-qi)",
  blindspot: "var(--layer-dao)",
};

const KIND_NAMES: Record<InsightKindKey, string> = {
  relation: "关联",
  conflict: "冲突",
  blindspot: "盲区",
};

/** 余烬随时间变暗：三天后最低保留三成亮度，不催促也不熄灭。 */
export function emberFade(createdAt: string, now: number = Date.now()): number {
  const created = Date.parse(createdAt);
  if (Number.isNaN(created)) {
    return 1;
  }
  const hours = Math.max(0, (now - created) / 3_600_000);
  return Math.max(0.3, 1 - hours / 72);
}

/**
 * 余烬：主动助理的推送入口。
 *
 * 不做通知中心，只在画布边缘安静地亮着一簇余烬。点开后洞察以漂浮卡片进场，
 * 每张卡片可采纳、忽略或转为会诊。转为会诊会把洞察直接送入圆桌。
 */
export function EmberLayer({
  onOpenCouncil,
}: {
  readonly onOpenCouncil?: ((session: CouncilSessionView) => void) | undefined;
}) {
  const client = useCommands();
  const [insights, setInsights] = useState<readonly Insight[]>([]);
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const pending = await client.call("insights_list", { status: "new", limit: 20 });
      setInsights(pending);
    } catch {
      // 余烬失败保持静默，不打扰用户。
    }
  }, [client]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const ordered = useMemo(
    () =>
      [...insights].sort(
        (a, b) => Date.parse(b.createdAt) - Date.parse(a.createdAt),
      ),
    [insights],
  );

  async function dispose(insight: Insight, action: "adopt" | "ignore") {
    setNote(null);
    try {
      await client.call("insight_mark", { insightId: insight.id, action });
      setInsights((current) => current.filter((item) => item.id !== insight.id));
    } catch {
      setNote("处置失败，稍后再试");
    }
  }

  async function convert(insight: Insight) {
    setNote(null);
    try {
      const session = await client.call("insight_convert", { insightId: insight.id });
      setInsights((current) => current.filter((item) => item.id !== insight.id));
      setOpen(false);
      onOpenCouncil?.(session);
    } catch {
      setNote("转入会诊失败");
    }
  }

  if (insights.length === 0) {
    return null;
  }

  return (
    <div className="ember" data-open={open}>
      <button
        className="ember__cluster"
        type="button"
        aria-expanded={open}
        aria-label={`余烬，${insights.length} 条待看洞察`}
        onClick={() => setOpen((current) => !current)}
      >
        <span className="ember__glow" aria-hidden="true" />
        <span className="ember__count mono">{insights.length}</span>
      </button>
      {open ? (
        <div className="ember__tray" role="group" aria-label="待看洞察">
          {note ? (
            <p className="ember__note" data-tone="warn">
              {note}
            </p>
          ) : null}
          <ul className="ember__cards">
            {ordered.map((insight) => (
              <li
                key={insight.id}
                className="ember__card"
                data-kind={insight.kind}
                style={
                  {
                    "--ember-color": KIND_TOKEN[insight.kind],
                    "--ember-fade": String(emberFade(insight.createdAt)),
                  } as CSSProperties
                }
              >
                <div className="ember__head">
                  <span className="ember__kind">{KIND_NAMES[insight.kind]}</span>
                  <span className="ember__time mono">{insight.createdAt.slice(0, 10)}</span>
                </div>
                <p className="ember__title">{insight.title}</p>
                <p className="ember__summary prose">{insight.summary}</p>
                <p className="ember__link mono">
                  {insight.relatedNodeIds.length > 0
                    ? `关联 ${insight.relatedNodeIds.length} 个节点`
                    : "尚无关联节点"}
                  {insight.relatedMasterIds.length > 0
                    ? ` · 参考 ${insight.relatedMasterIds.length} 位大师`
                    : ""}
                </p>
                <div className="ember__actions">
                  <button type="button" onClick={() => void dispose(insight, "adopt")}>
                    采纳
                  </button>
                  <button type="button" onClick={() => void dispose(insight, "ignore")}>
                    忽略
                  </button>
                  <button
                    className="ember__to-council"
                    type="button"
                    onClick={() => void convert(insight)}
                  >
                    转为会诊
                  </button>
                </div>
              </li>
            ))}
          </ul>
          <p className="ember__hint">余烬会随时间变暗，不会催促。</p>
        </div>
      ) : null}
    </div>
  );
}
