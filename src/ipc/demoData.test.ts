import { describe, expect, it } from "vitest";
import { LAYER_KEYS } from "../domain/layers";
import {
  DEMO_CONCLUSION,
  DEMO_CONNECTOR_CALLS,
  DEMO_CONNECTORS,
  DEMO_COVERAGE,
  DEMO_GRAPH,
  DEMO_MASTERS,
  DEMO_POOL,
  DEMO_RECORDS,
  DEMO_SESSION,
  DEMO_SUMMARIES,
  DEMO_TUNING,
  demoDetail,
  demoSearchHits,
  demoSeatSpeech,
  demoSelection,
} from "./demoData";

describe("预览数据集", () => {
  it("六层各至少一位大师，覆盖矩阵无空缺", () => {
    expect(DEMO_COVERAGE.suggestions).toEqual([]);
    for (const key of LAYER_KEYS) {
      const entry = DEMO_COVERAGE.layers.find((item) => item.layer === key);
      expect(entry?.masterCount, `${key} 层应有大师`).toBeGreaterThan(0);
      expect(entry?.unitCount, `${key} 层应有单元`).toBeGreaterThan(0);
    }
  });

  it("摘要与详情一一对应，且每个单元都有来源标注", () => {
    expect(DEMO_SUMMARIES).toHaveLength(DEMO_MASTERS.length);
    for (const summary of DEMO_SUMMARIES) {
      const detail = demoDetail(summary.id);
      expect(detail?.units).toHaveLength(summary.unitCount);
      for (const unit of detail?.units ?? []) {
        expect(unit.citations.length).toBeGreaterThan(0);
        expect(unit.citations[0]?.available).toBe(true);
      }
    }
  });

  it("未知大师返回空", () => {
    expect(demoDetail("nobody")).toBeNull();
  });

  it("三套阵容都满足六层覆盖，且席位大师确实声明了该层次", () => {
    for (const strategy of ["steady", "clash", "serendipity"] as const) {
      const selection = demoSelection(strategy);
      expect(selection.layers, `${strategy} 应覆盖六层`).toHaveLength(6);
      expect(selection.gaps).toEqual([]);
      for (const seat of selection.seats) {
        expect(seat.layers, `${seat.name} 应声明层次 ${seat.layer}`).toContain(seat.layer);
      }
    }
  });

  it("候选池评分齐全", () => {
    expect(DEMO_POOL.candidates).toHaveLength(DEMO_MASTERS.length);
    for (const candidate of DEMO_POOL.candidates) {
      expect(candidate.relevance).toBeGreaterThan(0);
      expect(candidate.opposition).toBeGreaterThan(0);
      expect(candidate.domainDistance).toBeGreaterThan(0);
    }
  });

  it("星图连线两端都在节点集合内，且无自环", () => {
    const ids = new Set(DEMO_GRAPH.nodes.map((node) => node.id));
    expect(ids.size).toBe(DEMO_GRAPH.nodes.length);
    for (const edge of DEMO_GRAPH.edges) {
      expect(ids.has(edge.from), `连线 ${edge.id} 起点应存在`).toBe(true);
      expect(ids.has(edge.to), `连线 ${edge.id} 终点应存在`).toBe(true);
      expect(edge.from).not.toBe(edge.to);
    }
  });

  it("星图节点的激活度与层次都落在取值范围内", () => {
    for (const node of DEMO_GRAPH.nodes) {
      expect(node.activation).toBeGreaterThanOrEqual(0);
      expect(node.activation).toBeLessThanOrEqual(1);
      for (const layer of node.layers) {
        expect(LAYER_KEYS).toContain(layer);
      }
    }
  });

  it("社区成员都在节点集合内，且成员数与节点归属一致", () => {
    const byId = new Map(DEMO_GRAPH.nodes.map((node) => [node.id, node]));
    for (const cluster of DEMO_GRAPH.clusters) {
      expect(cluster.memberIds.length).toBe(cluster.memberCount);
      for (const id of cluster.memberIds) {
        expect(byId.get(id)?.clusterId).toBe(cluster.id);
      }
    }
    const assigned = DEMO_GRAPH.nodes.filter((node) => node.clusterId !== null);
    const members = DEMO_GRAPH.clusters.flatMap((cluster) => cluster.memberIds);
    expect(new Set(members)).toEqual(new Set(assigned.map((node) => node.id)));
  });

  it("思考记录按主题成链，时间正序", () => {
    const topicKey = DEMO_RECORDS[0]!.topicKey;
    const chain = DEMO_RECORDS.filter((record) => record.topicKey === topicKey).sort((a, b) =>
      a.createdAt.localeCompare(b.createdAt),
    );
    expect(chain.length).toBeGreaterThanOrEqual(2);
    for (let index = 1; index < chain.length; index += 1) {
      expect(chain[index - 1]!.createdAt <= chain[index]!.createdAt).toBe(true);
    }
  });

  it("逐席发言按席位归组并合成状态", () => {
    const seats = demoSeatSpeech(1);
    expect(seats).toHaveLength(6);
    const failed = seats.filter((seat) => seat.status === "failed");
    expect(failed).toHaveLength(1);
    for (const seat of seats) {
      expect(seat.masterName.length).toBeGreaterThan(0);
      expect(seat.rounds.every((round) => round.round >= 1)).toBe(true);
    }
  });

  it("结论详情页六段数据齐全", () => {
    expect(DEMO_CONCLUSION.session.id).toBe(DEMO_SESSION.session.id);
    expect(DEMO_CONCLUSION.metrics.length).toBeGreaterThan(0);
    expect(DEMO_CONCLUSION.speeches.length).toBe(6);
    expect(DEMO_CONCLUSION.sources.length).toBeGreaterThan(0);
    expect(DEMO_CONCLUSION.history.length).toBeGreaterThan(0);
    expect(DEMO_CONCLUSION.stanceChanges.length).toBeGreaterThan(0);
    for (const item of DEMO_CONCLUSION.stanceChanges) {
      expect(item.change.length).toBeGreaterThan(0);
      if (item.change === "new" || item.change === "dropped") {
        expect(item.similarity).toBe(0);
      }
    }
  });

  it("连接器配置的类型、状态与启用条件一致", () => {
    expect(DEMO_CONNECTORS.length).toBeGreaterThan(0);
    for (const connector of DEMO_CONNECTORS) {
      expect(["search", "page", "mcp"]).toContain(connector.kind);
      expect(connector.kindLabel.length).toBeGreaterThan(0);
      if (!connector.endpoint) {
        expect(connector.status).toBe("unconfigured");
      } else {
        expect(connector.status).toBe(connector.enabled ? "ready" : "disabled");
      }
    }
  });

  it("连接器调用审计覆盖共享背景、席位检索与失败降级", () => {
    const purposes = new Set(DEMO_CONNECTOR_CALLS.map((call) => call.purpose));
    expect(purposes).toEqual(
      new Set(["council_background", "council_seat_search", "connector_test"]),
    );
    const failed = DEMO_CONNECTOR_CALLS.filter((call) => call.status === "failed");
    expect(failed.length).toBeGreaterThan(0);
    expect(failed.every((call) => call.errorCode !== null)).toBe(true);
  });

  it("手动检索按问句返回带时间标注的结果", () => {
    expect(demoSearchHits("  ")).toEqual([]);
    const hits = demoSearchHits("要不要换赛道");
    expect(hits.length).toBeGreaterThan(0);
    expect(hits.every((hit) => hit.title.includes("要不要换赛道"))).toBe(true);
  });

  it("调参演示数据与内核声明的二十八项一致", () => {
    expect(DEMO_TUNING).toHaveLength(28);
    const keys = DEMO_TUNING.map((item) => item.key);
    expect(new Set(keys).size).toBe(keys.length);
    for (const item of DEMO_TUNING) {
      expect(item.value).toBe(item.defaultValue);
      expect(item.customized).toBe(false);
    }
    expect(keys).toContain("connector.max_results");
    expect(keys).toContain("council.seat_search");
    expect(keys).toContain("connector.preflight");
    expect(keys).toContain("council.divergence_mode");
    expect(keys).toContain("cost.daily_limit_micros");
    expect(keys).toContain("cost.over_limit_policy");
    expect(keys).toContain("backup.keep_count");
  });
});
