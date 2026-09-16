import { describe, expect, it } from "vitest";
import { FURNACE_WEIGHTS, ambientGlow, computeFurnaceTemp, temperatureBand } from "./furnace";

describe("炉温", () => {
  it("空输入为冷炉", () => {
    const temp = computeFurnaceTemp({
      activeNodes: 0,
      totalNodes: 0,
      recentCaptures: 0,
      recentCouncils: 0,
    });
    expect(temp).toBe(0);
    expect(temperatureBand(temp).key).toBe("cold");
  });

  it("权重之和为 1", () => {
    const total =
      FURNACE_WEIGHTS.activation + FURNACE_WEIGHTS.capture + FURNACE_WEIGHTS.council;
    expect(total).toBeCloseTo(1, 10);
  });

  it("饱和输入不会超过 100", () => {
    const temp = computeFurnaceTemp({
      activeNodes: 10_000,
      totalNodes: 10_000,
      recentCaptures: 10_000,
      recentCouncils: 10_000,
    });
    expect(temp).toBe(100);
  });

  it("激活节点多于总数时不超过总数", () => {
    const temp = computeFurnaceTemp({
      activeNodes: 999,
      totalNodes: 10,
      recentCaptures: 0,
      recentCouncils: 0,
    });
    const capped = computeFurnaceTemp({
      activeNodes: 10,
      totalNodes: 10,
      recentCaptures: 0,
      recentCouncils: 0,
    });
    expect(temp).toBe(capped);
  });

  it("负数与非法输入按零处理", () => {
    const temp = computeFurnaceTemp({
      activeNodes: -5,
      totalNodes: -1,
      recentCaptures: Number.NaN,
      recentCouncils: -3,
    });
    expect(temp).toBe(0);
  });

  it("炉温随活跃度单调不减", () => {
    const low = computeFurnaceTemp({
      activeNodes: 5,
      totalNodes: 50,
      recentCaptures: 2,
      recentCouncils: 0,
    });
    const high = computeFurnaceTemp({
      activeNodes: 25,
      totalNodes: 50,
      recentCaptures: 20,
      recentCouncils: 2,
    });
    expect(high).toBeGreaterThan(low);
  });

  it("分段边界按 25 / 55 / 80 划分", () => {
    expect(temperatureBand(0).key).toBe("cold");
    expect(temperatureBand(24).key).toBe("cold");
    expect(temperatureBand(25).key).toBe("warm");
    expect(temperatureBand(54).key).toBe("warm");
    expect(temperatureBand(55).key).toBe("hot");
    expect(temperatureBand(79).key).toBe("hot");
    expect(temperatureBand(80).key).toBe("blazing");
    expect(temperatureBand(100).key).toBe("blazing");
  });

  it("环境光随炉温上升且落在合理区间", () => {
    expect(ambientGlow(0)).toBeCloseTo(0.12, 5);
    expect(ambientGlow(100)).toBeCloseTo(1, 5);
    expect(ambientGlow(50)).toBeGreaterThan(ambientGlow(10));
    expect(ambientGlow(140)).toBeCloseTo(1, 5);
    expect(ambientGlow(-20)).toBeCloseTo(0.12, 5);
  });
});
