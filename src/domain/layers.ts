/**
 * 层次模型：道、法、术、气、器、势。
 * 这是大师分类与选角覆盖的基准轴，顺序即从抽象到具体再到期势。
 */

export const LAYER_KEYS = ["dao", "fa", "shu", "qi", "tool", "shi"] as const;

export type LayerKey = (typeof LAYER_KEYS)[number];

/** 几何标记，供高对比模式与色觉障碍用户在不依赖颜色时区分层次。 */
export type LayerGlyph = "dot" | "square" | "triangle" | "wave" | "cross" | "arrow";

export interface LayerMeta {
  readonly key: LayerKey;
  /** 单字名，用于徽记与标签。 */
  readonly name: string;
  /** 色名，不随主题变化。 */
  readonly colorName: string;
  /** 该层次回答的核心问题。 */
  readonly question: string;
  readonly glyph: LayerGlyph;
}

const LAYER_LIST: readonly LayerMeta[] = [
  {
    key: "dao",
    name: "道",
    colorName: "玄青",
    question: "什么值得做",
    glyph: "dot",
  },
  {
    key: "fa",
    name: "法",
    colorName: "松绿",
    question: "规律是什么",
    glyph: "square",
  },
  {
    key: "shu",
    name: "术",
    colorName: "赭金",
    question: "具体怎么做",
    glyph: "triangle",
  },
  {
    key: "qi",
    name: "气",
    colorName: "朱砂",
    question: "靠什么心力度过",
    glyph: "wave",
  },
  {
    key: "tool",
    name: "器",
    colorName: "藤紫",
    question: "用什么载体放大",
    glyph: "cross",
  },
  {
    key: "shi",
    name: "势",
    colorName: "天青",
    question: "现在是不是时候",
    glyph: "arrow",
  },
];

export const LAYERS: ReadonlyMap<LayerKey, LayerMeta> = new Map(
  LAYER_LIST.map((layer) => [layer.key, layer]),
);

export function isLayerKey(value: unknown): value is LayerKey {
  return typeof value === "string" && (LAYER_KEYS as readonly string[]).includes(value);
}

export function layerOf(key: LayerKey): LayerMeta {
  const meta = LAYERS.get(key);
  if (!meta) {
    // 键来自联合类型，运行期不可达；保留分支以覆盖数据越界的情况。
    throw new Error(`未知层次：${key}`);
  }
  return meta;
}

/** 层次全覆盖是骑士团选角的硬约束。 */
export function coversAllLayers(layers: readonly LayerKey[]): boolean {
  const seen = new Set(layers);
  return LAYER_KEYS.every((key) => seen.has(key));
}
