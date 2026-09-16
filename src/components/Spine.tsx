import type { CSSProperties } from "react";
import { REALM_ORDER, realmOf, type RealmKey } from "../domain/realms";

export interface SpineEntry {
  readonly key: RealmKey;
  /** 0 到 1 的近期活跃度，用于印记下方的光带。 */
  readonly activity: number;
  /** 该境界是否有未处理的新内容。 */
  readonly hasEmber: boolean;
}

const DEFAULT_ACTIVITY: Record<RealmKey, number> = {
  observe: 0.72,
  council: 0.38,
  refine: 0.12,
  vault: 0.24,
  self: 0.08,
};

const DEFAULT_EMBER: Record<RealmKey, boolean> = {
  observe: true,
  council: false,
  refine: false,
  vault: false,
  self: false,
};

export function defaultSpineEntries(): readonly SpineEntry[] {
  return REALM_ORDER.map((key) => ({
    key,
    activity: DEFAULT_ACTIVITY[key],
    hasEmber: DEFAULT_EMBER[key],
  }));
}

/**
 * 炉脊：一条 72 像素宽的窄脊，悬停展开到 240 像素显示文字。
 * 它不是功能列表，而是五个视点的印记所在。
 */
export function Spine({
  entries,
  current,
  onSelect,
}: {
  readonly entries: readonly SpineEntry[];
  readonly current: RealmKey;
  readonly onSelect: (next: RealmKey) => void;
}) {
  return (
    <nav className="spine" aria-label="境界">
      <div className="spine__mark" aria-hidden="true">
        炉
      </div>
      <ul className="spine__list">
        {entries.map((entry) => {
          const realm = realmOf(entry.key);
          const active = entry.key === current;
          return (
            <li key={entry.key}>
              <button
                type="button"
                className="spine__item"
                aria-current={active ? "page" : undefined}
                data-active={active}
                onClick={() => onSelect(entry.key)}
              >
                <span className="spine__sigil" aria-hidden="true">
                  {realm.sigil}
                </span>
                <span className="spine__text">
                  <span className="spine__title">{realm.title}</span>
                  <span className="spine__subtitle">{realm.subtitle}</span>
                </span>
                <span
                  className="spine__ribbon"
                  style={{ "--activity": entry.activity } as CSSProperties}
                />
                {entry.hasEmber ? <span className="spine__ember" aria-hidden="true" /> : null}
                {entry.hasEmber ? <span className="visually-hidden">有新内容</span> : null}
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
