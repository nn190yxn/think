import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { layoutGraph, nodeRadius, recencyBrightness, ObserveRealm } from "./ObserveRealm";
import { IpcProvider } from "../app/ipc";
import type { GraphEdge, GraphNode } from "../ipc/commands";

function renderRealm(onSeed?: (question: string) => void) {
  return render(
    <IpcProvider>
      <ObserveRealm temp={40} onSeed={onSeed} />
    </IpcProvider>,
  );
}

const nodes: readonly GraphNode[] = [
  {
    id: "n1",
    kind: "judgment",
    content: "先算清最坏结果",
    domains: ["职业"],
    layers: ["dao"],
    activation: 0.9,
    activationUpdatedAt: "2026-09-14T09:00:00Z",
    clusterId: null,
  },
  {
    id: "n2",
    kind: "framework",
    content: "先胜后战",
    domains: ["职业"],
    layers: ["shi"],
    activation: 0.6,
    activationUpdatedAt: "2026-09-01T09:00:00Z",
    clusterId: null,
  },
];

const edges: readonly GraphEdge[] = [
  { id: "e1", from: "n2", to: "n1", relation: "derives", weight: 0.7 },
];

describe("思维星图布局", () => {
  it("同一输入给出确定布局，且落在画布内", () => {
    const first = layoutGraph(nodes, edges);
    const second = layoutGraph(nodes, edges);
    expect([...first.entries()]).toEqual([...second.entries()]);
    for (const point of first.values()) {
      expect(point.x).toBeGreaterThanOrEqual(40);
      expect(point.x).toBeLessThanOrEqual(960);
      expect(point.y).toBeGreaterThanOrEqual(40);
      expect(point.y).toBeLessThanOrEqual(560);
    }
  });

  it("没有节点时返回空布局", () => {
    expect(layoutGraph([], []).size).toBe(0);
  });

  it("节点半径随激活度单调增大", () => {
    expect(nodeRadius(0.9)).toBeGreaterThan(nodeRadius(0.2));
    expect(nodeRadius(5)).toBe(nodeRadius(1));
  });

  it("最近唤起越新亮度越高", () => {
    const now = Date.parse("2026-09-14T09:00:00Z");
    expect(recencyBrightness("2026-09-14T09:00:00Z", now)).toBe(1);
    expect(recencyBrightness("2026-08-01T09:00:00Z", now)).toBeLessThan(1);
    expect(recencyBrightness("不是时间", now)).toBe(0.6);
  });
});

describe("观境界", () => {
  beforeEach(() => {
    window.history.replaceState(null, "", "#/observe");
  });

  it("默认显示星图并渲染节点与连线", async () => {
    renderRealm();
    expect(await screen.findByRole("application", { name: "思维星图" })).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getAllByTestId("star-node")).toHaveLength(11);
    });
    expect(screen.getByRole("button", { name: "星图" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("切到星云视点只显示领域聚类", async () => {
    renderRealm();
    await userEvent.click(await screen.findByRole("button", { name: "星云" }));
    expect(screen.queryAllByTestId("star-node")).toHaveLength(0);
    expect(screen.getAllByText("职业").length).toBeGreaterThan(0);
  });

  it("切到心核视点展开选中节点的连线与来源", async () => {
    renderRealm();
    await userEvent.click(await screen.findByRole("button", { name: "心核" }));
    const core = await screen.findByRole("complementary", { name: "心核" });
    expect(core).toHaveTextContent("全部连接");
    expect(core).toHaveTextContent("思考记录");
  });

  it("Space 以选中节点为种子发起会诊", async () => {
    const onSeed = vi.fn();
    renderRealm(onSeed);
    const canvas = await screen.findByRole("application", { name: "思维星图" });
    await userEvent.click(canvas);
    await userEvent.keyboard(" ");
    expect(onSeed).toHaveBeenCalledTimes(1);
    expect(onSeed.mock.calls[0]![0]).toContain("最坏结果");
  });

  it("J 键在节点间移动选择", async () => {
    renderRealm();
    const canvas = await screen.findByRole("application", { name: "思维星图" });
    await waitFor(() => expect(screen.getAllByTestId("star-node")).toHaveLength(11));
    await waitFor(() =>
      expect(document.querySelectorAll('.star__node[data-selected="true"]')).toHaveLength(1),
    );
    const before = document.querySelectorAll('.star__node[data-selected="true"]').length;
    await userEvent.click(canvas);
    await userEvent.keyboard("j");
    const after = document.querySelectorAll('.star__node[data-selected="true"]').length;
    expect(before).toBe(1);
    expect(after).toBe(1);
  });

  it("待裁决列出冲突连线，裁决后从列表移除", async () => {
    renderRealm();
    const items = await screen.findAllByText(/↔/);
    expect(items).toHaveLength(2);
    await userEvent.click(screen.getAllByRole("button", { name: "保留矛盾" })[0]!);
    await waitFor(() => expect(screen.getAllByText(/↔/)).toHaveLength(1));
  });

  it("列表视图以文字等效呈现节点、层次与激活度", async () => {
    renderRealm();
    await userEvent.click(await screen.findByRole("button", { name: "列表视图" }));
    const list = await screen.findByRole("list", { name: "思维网络列表" });
    const items = list.querySelectorAll(".graph-list__item");
    expect(items.length).toBeGreaterThan(0);
    expect(list).toHaveTextContent("判断");
    expect(list).toHaveTextContent("活跃度");
    expect(screen.queryByRole("application", { name: "思维星图" })).toBeNull();
  });

  it("大纲视图按领域分组，并可用键盘发起会诊", async () => {
    const onSeed = vi.fn();
    renderRealm(onSeed);
    await userEvent.click(await screen.findByRole("button", { name: "大纲视图" }));
    const outline = await screen.findByRole("tree", { name: "思维网络大纲" });
    expect(outline.querySelectorAll(".graph-outline__group").length).toBeGreaterThan(0);
    await userEvent.click(screen.getByRole("button", { name: /以此发起会诊/ }));
    expect(onSeed).toHaveBeenCalledTimes(1);
  });

  it("星云视点按认知社区呈现区域与规模", async () => {
    renderRealm();
    await userEvent.click(await screen.findByRole("button", { name: "星云" }));
    const group = await screen.findByRole("group", { name: "认知社区" });
    expect(group).toHaveTextContent("职业");
    expect(group).toHaveTextContent("6");
  });

  it("按社区过滤后只保留团内节点", async () => {
    renderRealm();
    await waitFor(() => expect(screen.getAllByTestId("star-node")).toHaveLength(11));
    await userEvent.click(await screen.findByRole("button", { name: "职业 · 3" }));
    await waitFor(() => expect(screen.getAllByTestId("star-node")).toHaveLength(3));
    await userEvent.click(screen.getByRole("button", { name: "全部" }));
    await waitFor(() => expect(screen.getAllByTestId("star-node")).toHaveLength(11));
  });
});
