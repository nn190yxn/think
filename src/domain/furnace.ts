/**
 * 炉温：全系统认知活跃度的合成指标，取值 0 到 100。
 * 它同时驱动界面环境光强度，冷炉暗淡、热炉温暖。
 */

export interface FurnaceInputs {
  /** 近期待处理与已激活节点的数量。 */
  readonly activeNodes: number;
  /** 已有节点总数，用作活跃度的参照基准。 */
  readonly totalNodes: number;
  /** 近期采集条数。 */
  readonly recentCaptures: number;
  /** 近期会诊场次。 */
  readonly recentCouncils: number;
}

/** 三项输入的权重之和为 1。 */
export const FURNACE_WEIGHTS = {
  activation: 0.5,
  capture: 0.3,
  council: 0.2,
} as const;

/** 各项达到满值所需的量级，避免需要无限增长才升温。 */
const SATURATION = {
  activeNodes: 60,
  capture: 40,
  council: 6,
} as const;

function ratio(value: number, saturation: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    return 0;
  }
  return Math.min(1, value / saturation);
}

/**
 * 活跃度需要相对规模看待：小库里的 20 个活跃节点比大库里的 20 个更热。
 */
export function computeFurnaceTemp(inputs: FurnaceInputs): number {
  const total = Math.max(0, inputs.totalNodes);
  const active = Math.max(0, Math.min(inputs.activeNodes, total > 0 ? total : inputs.activeNodes));
  const activationShare = total > 0 ? active / total : 0;
  const activationScore = ratio(active, SATURATION.activeNodes) * 0.5 + activationShare * 0.5;

  const raw =
    activationScore * FURNACE_WEIGHTS.activation +
    ratio(inputs.recentCaptures, SATURATION.capture) * FURNACE_WEIGHTS.capture +
    ratio(inputs.recentCouncils, SATURATION.council) * FURNACE_WEIGHTS.council;

  return Math.round(Math.max(0, Math.min(1, raw)) * 100);
}

export interface TemperatureBand {
  readonly key: "cold" | "warm" | "hot" | "blazing";
  readonly label: string;
  readonly min: number;
}

const BANDS: readonly TemperatureBand[] = [
  { key: "cold", label: "冷炉", min: 0 },
  { key: "warm", label: "回温", min: 25 },
  { key: "hot", label: "走火", min: 55 },
  { key: "blazing", label: "烧透", min: 80 },
];

export function temperatureBand(temp: number): TemperatureBand {
  const clamped = Math.max(0, Math.min(100, temp));
  let result = BANDS[0];
  for (const band of BANDS) {
    if (clamped >= band.min) {
      result = band;
    }
  }
  // BANDS 非空，此处必得一项。
  return result ?? { key: "cold", label: "冷炉", min: 0 };
}

/**
 * 环境光强度 0 到 1。暗色主题按线性映射，明色主题由 --ambient 再压到四成。
 */
export function ambientGlow(temp: number): number {
  const clamped = Math.max(0, Math.min(100, temp));
  return 0.12 + (clamped / 100) * 0.88;
}
