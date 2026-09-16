/**
 * 五个境界：观、会、炼、藏、我。
 * 它们不是五个页面，而是同一连续空间的五个视点。
 */

export const REALM_KEYS = ["observe", "council", "refine", "vault", "self"] as const;

export type RealmKey = (typeof REALM_KEYS)[number];

export interface RealmMeta {
  readonly key: RealmKey;
  /** 单字印记。 */
  readonly sigil: string;
  readonly title: string;
  readonly subtitle: string;
}

const REALM_LIST: readonly RealmMeta[] = [
  {
    key: "observe",
    sigil: "观",
    title: "思维星图",
    subtitle: "你的念头在这里连成网络，激活与衰减都在观察之中",
  },
  {
    key: "council",
    sigil: "会",
    title: "圆桌会诊",
    subtitle: "让骑士团从各层入场，隔离作答后再交叉质询",
  },
  {
    key: "refine",
    sigil: "炼",
    title: "蒸馏熔炉",
    subtitle: "把一位大师的语料炼成可调用的技能单元",
  },
  {
    key: "vault",
    sigil: "藏",
    title: "大师与资产",
    subtitle: "大师包、语料来源与知识地形的总账",
  },
  {
    key: "self",
    sigil: "我",
    title: "成长与设置",
    subtitle: "演化长河、个人原则与这台炉子的所有开关",
  },
];

export const REALMS: ReadonlyMap<RealmKey, RealmMeta> = new Map(
  REALM_LIST.map((realm) => [realm.key, realm]),
);

export const REALM_ORDER: readonly RealmKey[] = REALM_KEYS;

export function isRealmKey(value: unknown): value is RealmKey {
  return typeof value === "string" && (REALM_KEYS as readonly string[]).includes(value);
}

export function realmOf(key: RealmKey): RealmMeta {
  const meta = REALMS.get(key);
  if (!meta) {
    throw new Error(`未知境界：${key}`);
  }
  return meta;
}

export const DEFAULT_REALM: RealmKey = "observe";

/** 视域环与炉脊共享的顺序移动规则：在两端回绕。 */
export function stepRealm(current: RealmKey, delta: number): RealmKey {
  const index = REALM_ORDER.indexOf(current);
  if (index < 0) {
    return DEFAULT_REALM;
  }
  const total = REALM_ORDER.length;
  const next = (index + delta + total) % total;
  const key = REALM_ORDER[next];
  return key ?? DEFAULT_REALM;
}
