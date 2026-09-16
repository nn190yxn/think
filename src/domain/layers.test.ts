import { describe, expect, it } from "vitest";
import { LAYERS, LAYER_KEYS, coversAllLayers, isLayerKey, layerOf } from "./layers";

describe("层次模型", () => {
  it("恰好六层且顺序固定", () => {
    expect(LAYER_KEYS).toEqual(["dao", "fa", "shu", "qi", "tool", "shi"]);
    expect(LAYERS.size).toBe(6);
  });

  it("每层都有名称、色名、问题与几何标记", () => {
    for (const key of LAYER_KEYS) {
      const layer = layerOf(key);
      expect(layer.name).toHaveLength(1);
      expect(layer.colorName.length).toBeGreaterThan(0);
      expect(layer.question.length).toBeGreaterThan(0);
      expect(layer.glyph).toBeTruthy();
    }
  });

  it("几何标记互不重复，色觉障碍下也能区分层次", () => {
    const glyphs = LAYER_KEYS.map((key) => layerOf(key).glyph);
    expect(new Set(glyphs).size).toBe(glyphs.length);
  });

  it("势层回答时机问题", () => {
    expect(layerOf("shi").question).toContain("时候");
  });

  it("校验层次键", () => {
    expect(isLayerKey("dao")).toBe(true);
    expect(isLayerKey("liu")).toBe(false);
    expect(isLayerKey(6)).toBe(false);
  });

  it("只有覆盖全部六层才算全覆盖", () => {
    expect(coversAllLayers([...LAYER_KEYS])).toBe(true);
    expect(coversAllLayers(["dao", "fa", "shu", "qi", "tool"])).toBe(false);
    expect(coversAllLayers([])).toBe(false);
  });
});
