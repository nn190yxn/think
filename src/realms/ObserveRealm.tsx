import { useCallback, useEffect, useMemo, useState } from "react";
import type { KeyboardEvent, ReactNode, WheelEvent } from "react";
import { RealmShell } from "./RealmShell";
import { useCommand, useCommands } from "../app/ipc";
import { LayerGlyph } from "../components/LayerGlyph";
import { layerOf } from "../domain/layers";
import { nodeSourceLabel } from "../domain/labels";
import type {
  EdgeRelation,
  GraphEdge,
  GraphNode,
  NodeDetail,
  NodeKind,
} from "../ipc/commands";

/** 三级视点：星云看聚类，星图看节点与连线，心核看单点的全部连接与来源。 */
const VIEW_LEVELS = ["nebula", "graph", "core"] as const;
type ViewLevel = (typeof VIEW_LEVELS)[number];

const VIEW_LABELS: Record<ViewLevel, string> = {
  nebula: "星云",
  graph: "星图",
  core: "心核",
};

/** 连线颜色映射关系类型，与图例、正文保持同一套语义。 */
const RELATION_COLORS: Record<EdgeRelation, string> = {
  supports: "var(--text-1)",
  conflicts: "var(--layer-qi)",
  derives: "var(--layer-fa)",
  analogous: "var(--layer-dao)",
  applies: "var(--layer-shu)",
};

const RELATION_NAMES: Record<EdgeRelation, string> = {
  supports: "支持",
  conflicts: "冲突",
  derives: "衍生",
  analogous: "类比",
  applies: "应用",
};

const KIND_NAMES: Record<NodeKind, string> = {
  idea: "念头",
  judgment: "判断",
  framework: "框架",
  principle: "原则",
  question: "问题",
  evidence: "证据",
};

/** 等效视图：同一份图数据，图形、列表、大纲三种读法。 */
const VIEW_MODES = ["graph", "list", "outline"] as const;
type ViewMode = (typeof VIEW_MODES)[number];

const VIEW_MODE_LABELS: Record<ViewMode, string> = {
  graph: "图形视图",
  list: "列表视图",
  outline: "大纲视图",
};

const VIEW_W = 1000;
const VIEW_H = 600;
/** 力导向只对激活度最高的一批节点做迭代，避免大图谱卡住主线程。 */
const SIMULATION_LIMIT = 220;
const SIMULATION_STEPS = 160;

interface Point {
  readonly x: number;
  readonly y: number;
}

/** 星云视点的社区区域：成员在画布上的质心。 */
interface CommunityRegion extends Point {
  readonly id: string;
  readonly label: string;
  readonly count: number;
}

function hash(text: string): number {
  let value = 2166136261;
  for (let index = 0; index < text.length; index += 1) {
    value ^= text.charCodeAt(index);
    value = Math.imul(value, 16777619);
  }
  return (value >>> 0) / 4294967295;
}

/**
 * 确定性力导向布局：先按节点 id 撒点，再用网格加速的排斥力与连线弹簧迭代。
 * 不用随机数，因此同一张图每次渲染得到同一布局，测试与截图都稳定。
 */
export function layoutGraph(
  nodes: readonly GraphNode[],
  edges: readonly GraphEdge[],
): ReadonlyMap<string, Point> {
  const positions = new Map<string, Point>();
  nodes.forEach((node) => {
    const seed = hash(node.id);
    const angle = seed * Math.PI * 2;
    const radius = 150 + seed * 90;
    positions.set(node.id, {
      x: VIEW_W / 2 + Math.cos(angle) * radius,
      y: VIEW_H / 2 + Math.sin(angle) * radius * 0.6,
    });
  });

  const simulated = nodes.slice(0, SIMULATION_LIMIT);
  if (simulated.length < 2) {
    return positions;
  }

  const springs = edges
    .filter((edge) => positions.has(edge.from) && positions.has(edge.to))
    .map((edge) => ({ from: edge.from, to: edge.to, weight: edge.weight }));

  const cell = 90;
  for (let step = 0; step < SIMULATION_STEPS; step += 1) {
    const cooling = 0.6 * (1 - step / SIMULATION_STEPS);

    // 网格分桶后只在相邻格子内计算排斥力，把 O(n²) 降到近似线性。
    const buckets = new Map<string, number[]>();
    simulated.forEach((node, index) => {
      const point = positions.get(node.id)!;
      const key = `${Math.floor(point.x / cell)}:${Math.floor(point.y / cell)}`;
      const bucket = buckets.get(key);
      if (bucket) {
        bucket.push(index);
      } else {
        buckets.set(key, [index]);
      }
    });

    simulated.forEach((node, index) => {
      const point = positions.get(node.id)!;
      let dx = 0;
      let dy = 0;
      const cx = Math.floor(point.x / cell);
      const cy = Math.floor(point.y / cell);
      for (let gx = cx - 1; gx <= cx + 1; gx += 1) {
        for (let gy = cy - 1; gy <= cy + 1; gy += 1) {
          for (const other of buckets.get(`${gx}:${gy}`) ?? []) {
            if (other === index) {
              continue;
            }
            const peer = positions.get(simulated[other]!.id)!;
            let ox = point.x - peer.x;
            let oy = point.y - peer.y;
            const distance = Math.hypot(ox, oy) || 0.01;
            if (distance > 150) {
              continue;
            }
            const strength = ((150 - distance) / distance) * 0.18 * cooling;
            ox *= strength;
            oy *= strength;
            dx += ox;
            dy += oy;
          }
        }
      }
      if (dx !== 0 || dy !== 0) {
        positions.set(node.id, { x: point.x + dx, y: point.y + dy });
      }
    });

    for (const spring of springs) {
      const a = positions.get(spring.from)!;
      const b = positions.get(spring.to)!;
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const distance = Math.hypot(dx, dy) || 0.01;
      const ideal = 200 - spring.weight * 60;
      const force = ((distance - ideal) / distance) * 0.08 * cooling;
      const offsetX = dx * force;
      const offsetY = dy * force;
      positions.set(spring.from, { x: a.x + offsetX, y: a.y + offsetY });
      positions.set(spring.to, { x: b.x - offsetX, y: b.y - offsetY });
    }

    simulated.forEach((node) => {
      const point = positions.get(node.id)!;
      positions.set(node.id, {
        x: point.x + (VIEW_W / 2 - point.x) * 0.01 * cooling,
        y: point.y + (VIEW_H / 2 - point.y) * 0.012 * cooling,
      });
    });
  }

  positions.forEach((point, id) => {
    positions.set(id, {
      x: Math.min(VIEW_W - 40, Math.max(40, point.x)),
      y: Math.min(VIEW_H - 40, Math.max(40, point.y)),
    });
  });
  return positions;
}

/** 距离最近唤起时间的亮度：越近越亮，最暗不低于 0.35。 */
export function recencyBrightness(updatedAt: string, now: number = Date.now()): number {
  const stamped = Date.parse(updatedAt);
  if (Number.isNaN(stamped)) {
    return 0.6;
  }
  const days = Math.max(0, now - stamped) / 86_400_000;
  return Math.max(0.35, Math.min(1, 1 - days / 30));
}

export function nodeRadius(activation: number): number {
  return 7 + Math.min(1, Math.max(0, activation)) * 12;
}

function polar(radius: number, degrees: number): Point {
  const radians = (degrees * Math.PI) / 180;
  return { x: Math.cos(radians) * radius, y: Math.sin(radians) * radius };
}

function hexPoints(radius: number): string {
  return Array.from({ length: 6 }, (_, index) => {
    const point = polar(radius, index * 60 - 90);
    return `${point.x.toFixed(2)},${point.y.toFixed(2)}`;
  }).join(" ");
}

/** 问号缺口：一段留白约 80 度的圆弧。 */
function openCirclePath(radius: number): string {
  const start = polar(radius, 40);
  const end = polar(radius, 320);
  return `M ${start.x.toFixed(2)} ${start.y.toFixed(2)} A ${radius} ${radius} 0 1 1 ${end.x.toFixed(2)} ${end.y.toFixed(2)}`;
}

function nodeShape(kind: NodeKind, radius: number): ReactNode {
  switch (kind) {
    case "judgment":
      return <rect x={-radius} y={-radius} width={radius * 2} height={radius * 2} rx={1.5} />;
    case "framework":
      return <polygon points={hexPoints(radius)} />;
    case "principle":
      return (
        <>
          <rect x={-radius} y={-radius} width={radius * 2} height={radius * 2} rx={2} />
          <rect
            className="star__seal"
            x={-radius * 0.42}
            y={-radius * 0.42}
            width={radius * 0.84}
            height={radius * 0.84}
          />
        </>
      );
    case "question":
      return (
        <>
          <path d={openCirclePath(radius)} />
          <circle className="star__tick" cy={-radius * 0.4} r={radius * 0.16} />
        </>
      );
    case "evidence":
      return (
        <polygon points={`0,${-radius} ${radius},0 0,${radius} ${-radius},0`} />
      );
    case "idea":
    default:
      return <circle r={radius} />;
  }
}

/** 心核视图按需读取节点详情，取不到时退回图谱里的摘要。 */
function useNodeDetail(nodeId: string | null): NodeDetail | null {
  const client = useCommands();
  const [detail, setDetail] = useState<NodeDetail | null>(null);

  useEffect(() => {
    if (!nodeId) {
      setDetail(null);
      return;
    }
    let alive = true;
    client
      .call("network_node", { nodeId })
      .then((data) => {
        if (alive) {
          setDetail(data);
        }
      })
      .catch(() => {
        if (alive) {
          setDetail(null);
        }
      });
    return () => {
      alive = false;
    };
  }, [client, nodeId]);

  return detail;
}

/**
 * 观：思维星图常驻画布。
 *
 * 节点按力导向布局聚成星云，视点用滚轮或按钮在星云、星图、心核三级间过渡；
 * `J`/`K` 移动选择，`Enter` 进入心核，`Space` 以该节点为种子发起会诊。
 */
export function ObserveRealm({
  temp,
  onSeed,
}: {
  readonly temp: number;
  readonly onSeed?: ((question: string) => void) | undefined;
}) {
  const graph = useCommand("network_graph", {});
  const snapshot = useCommand("furnace_snapshot", {});
  const client = useCommands();
  const [level, setLevel] = useState<ViewLevel>("graph");
  const [view, setView] = useState<ViewMode>("graph");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [resolved, setResolved] = useState<readonly string[]>([]);
  const [note, setNote] = useState<string | null>(null);

  const [clusterFilter, setClusterFilter] = useState<string | null>(null);
  const allNodes = useMemo(() => graph.data?.nodes ?? [], [graph.data]);
  const allEdges = useMemo(() => graph.data?.edges ?? [], [graph.data]);
  const communities = useMemo(() => graph.data?.clusters ?? [], [graph.data]);
  const nodes = useMemo(
    () =>
      clusterFilter
        ? allNodes.filter((node) => node.clusterId === clusterFilter)
        : allNodes,
    [allNodes, clusterFilter],
  );
  const edges = useMemo(() => {
    const visible = new Set(nodes.map((node) => node.id));
    return allEdges.filter((edge) => visible.has(edge.from) && visible.has(edge.to));
  }, [allEdges, nodes]);
  const positions = useMemo(() => layoutGraph(nodes, edges), [nodes, edges]);

  // 固化会重算社区归属，旧筛选可能在新的图里已不存在。
  useEffect(() => {
    if (clusterFilter && !communities.some((cluster) => cluster.id === clusterFilter)) {
      setClusterFilter(null);
    }
  }, [clusterFilter, communities]);

  useEffect(() => {
    if (!selectedId && nodes.length > 0) {
      setSelectedId(nodes[0]!.id);
    }
  }, [nodes, selectedId]);

  const selected = useMemo(
    () => nodes.find((node) => node.id === selectedId) ?? null,
    [nodes, selectedId],
  );
  const detail = useNodeDetail(level === "core" ? selectedId : null);

  const neighbours = useMemo(() => {
    if (!selectedId) {
      return new Set<string>();
    }
    const ids = new Set<string>();
    edges.forEach((edge) => {
      if (edge.from === selectedId) {
        ids.add(edge.to);
      }
      if (edge.to === selectedId) {
        ids.add(edge.from);
      }
    });
    return ids;
  }, [edges, selectedId]);

  const conflicts = useMemo(
    () =>
      edges.filter(
        (edge) => edge.relation === "conflicts" && !resolved.includes(edge.id),
      ),
    [edges, resolved],
  );

  const nodeLabel = useCallback(
    (id: string) => nodes.find((node) => node.id === id)?.content ?? id,
    [nodes],
  );

  async function resolve(edge: GraphEdge, decision: "keep" | "drop") {
    setNote(null);
    try {
      await client.call("network_resolve_conflict", {
        edgeId: edge.id,
        decision,
        reason: decision === "keep" ? "两者在不同条件下都成立" : "暂以其中一条为准",
      });
      setResolved((current) => [...current, edge.id]);
    } catch {
      setNote("裁决未能写入");
    }
  }

  // 社区来自固化阶段，这里把成员落到画布上求质心，作为星云视点的区域。
  const regions = useMemo<readonly CommunityRegion[]>(() => {
    const visible = new Set(nodes.map((node) => node.id));
    return communities
      .map((cluster) => {
        const points = cluster.memberIds
          .filter((id) => visible.has(id))
          .map((id) => positions.get(id))
          .filter((point): point is Point => Boolean(point));
        if (points.length === 0) {
          return null;
        }
        return {
          id: cluster.id,
          label: cluster.label,
          count: points.length,
          x: points.reduce((sum, point) => sum + point.x, 0) / points.length,
          y: points.reduce((sum, point) => sum + point.y, 0) / points.length,
        };
      })
      .filter((region): region is CommunityRegion => region !== null);
  }, [communities, nodes, positions]);

  const degree = useMemo(() => {
    const counts = new Map<string, number>();
    edges.forEach((edge) => {
      counts.set(edge.from, (counts.get(edge.from) ?? 0) + 1);
      counts.set(edge.to, (counts.get(edge.to) ?? 0) + 1);
    });
    return counts;
  }, [edges]);

  const communityLabel = useMemo(() => {
    const labels = new Map<string, string>();
    communities.forEach((cluster) => labels.set(cluster.id, cluster.label));
    return (node: GraphNode) =>
      node.clusterId ? labels.get(node.clusterId) ?? "未归类" : "未归类";
  }, [communities]);

  const outline = useMemo(() => {
    const groups = new Map<string, { label: string; nodes: GraphNode[] }>();
    nodes.forEach((node) => {
      const key = node.clusterId ?? "__unclassified__";
      const current = groups.get(key);
      if (current) {
        current.nodes.push(node);
      } else {
        groups.set(key, { label: communityLabel(node), nodes: [node] });
      }
    });
    return [...groups.entries()];
  }, [communityLabel, nodes]);

  const move = useCallback(
    (delta: number) => {
      if (nodes.length === 0) {
        return;
      }
      const current = nodes.findIndex((node) => node.id === selectedId);
      const next = (current + delta + nodes.length) % nodes.length;
      setSelectedId(nodes[next]!.id);
    },
    [nodes, selectedId],
  );

  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLDivElement>) => {
      if (event.key === "j" || event.key === "J") {
        move(1);
      } else if (event.key === "k" || event.key === "K") {
        move(-1);
      } else if (event.key === "Enter" && selected) {
        setLevel("core");
      } else if (event.key === " " && selected && onSeed) {
        event.preventDefault();
        onSeed(selected.content);
      } else if (event.key === "Escape") {
        setLevel("graph");
      }
    },
    [move, onSeed, selected],
  );

  const onWheel = useCallback((event: WheelEvent<HTMLDivElement>) => {
    // 只有画布获得焦点后才响应滚轮，避免页面滚动经过时误换视点。
    if (document.activeElement !== event.currentTarget) {
      return;
    }
    if (Math.abs(event.deltaY) < 24) {
      return;
    }
    event.preventDefault();
    setLevel((current) => {
      const index = VIEW_LEVELS.indexOf(current);
      const next = event.deltaY < 0 ? index + 1 : index - 1;
      return VIEW_LEVELS[Math.min(VIEW_LEVELS.length - 1, Math.max(0, next))]!;
    });
  }, []);

  const status = selected
    ? `已选「${selected.content}」· ${KIND_NAMES[selected.kind]} · 活跃度 ${selected.activation.toFixed(2)}`
    : "网络里还没有念头";
  const recentCaptures = snapshot.data?.recentCaptures ?? 0;

  return (
    <RealmShell realm="observe">
      <div className="star" data-level={level}>
        <div className="star__toolbar">
          <div className="star__levels" role="group" aria-label="视点">
            {VIEW_LEVELS.map((key) => (
              <button
                key={key}
                type="button"
                className="star__level"
                aria-pressed={level === key}
                onClick={() => setLevel(key)}
              >
                {VIEW_LABELS[key]}
              </button>
            ))}
          </div>
          <div className="star__views" role="group" aria-label="等效视图">
            {VIEW_MODES.map((key) => (
              <button
                key={key}
                type="button"
                className="star__view"
                aria-pressed={view === key}
                onClick={() => setView(key)}
              >
                {VIEW_MODE_LABELS[key]}
              </button>
            ))}
          </div>
          {communities.length > 0 ? (
            <div className="star__clusters" role="group" aria-label="认知社区">
              <span className="star__cluster-title">社区</span>
              <button
                type="button"
                className="star__cluster-filter"
                aria-pressed={clusterFilter === null}
                onClick={() => setClusterFilter(null)}
              >
                全部
              </button>
              {communities.map((cluster) => (
                <button
                  key={cluster.id}
                  type="button"
                  className="star__cluster-filter"
                  aria-pressed={clusterFilter === cluster.id}
                  onClick={() => setClusterFilter(cluster.id)}
                >
                  {cluster.label} · {cluster.memberCount}
                </button>
              ))}
            </div>
          ) : null}
          <ul className="star__legend" aria-label="连接的含义">
            {(Object.keys(RELATION_COLORS) as EdgeRelation[]).map((relation) => (
              <li key={relation} className="star__legend-item">
                <span
                  className="star__legend-swatch"
                  style={{ background: RELATION_COLORS[relation] }}
                  aria-hidden="true"
                />
                {RELATION_NAMES[relation]}
              </li>
            ))}
          </ul>
          <span className="star__hint">
            J K 移动 · Enter 入心核 · Space 发起会诊 · 点一下画布后滚轮换视点
          </span>
        </div>

        {view === "graph" ? (
        <div
          className="star__canvas"
          tabIndex={0}
          role="application"
          aria-label="思维星图"
          onKeyDown={onKeyDown}
          onWheel={onWheel}
        >
          <svg
            className="star__svg"
            viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
            role="img"
            aria-label={`思维星图，共 ${nodes.length} 个念头、${edges.length} 条连接`}
          >
            <g className="star__halo" opacity={0.2 + temp / 160}>
              <circle cx={VIEW_W / 2} cy={VIEW_H / 2} r={210} />
            </g>

            {level === "nebula"
              ? regions.map((region) => (
                  <g
                    key={region.id}
                    className="star__cluster"
                    transform={`translate(${region.x} ${region.y})`}
                  >
                    <circle r={24 + region.count * 3.5} />
                    <text className="star__cluster-label" y={4}>
                      {region.label}
                    </text>
                    <text className="star__cluster-count" y={22}>
                      {region.count}
                    </text>
                  </g>
                ))
              : null}

            {level !== "nebula"
              ? edges.map((edge) => {
                  const from = positions.get(edge.from);
                  const to = positions.get(edge.to);
                  if (!from || !to) {
                    return null;
                  }
                  const focused =
                    level === "graph" ||
                    (selectedId !== null && (edge.from === selectedId || edge.to === selectedId));
                  return (
                    <line
                      key={edge.id}
                      className="star__edge"
                      data-relation={edge.relation}
                      data-focused={focused}
                      x1={from.x}
                      y1={from.y}
                      x2={to.x}
                      y2={to.y}
                      stroke={RELATION_COLORS[edge.relation]}
                      strokeWidth={0.8 + edge.weight * 3.2}
                    />
                  );
                })
              : null}

            {level !== "nebula"
              ? nodes.map((node) => {
                  const point = positions.get(node.id);
                  if (!point) {
                    return null;
                  }
                  const radius = nodeRadius(node.activation);
                  const isSelected = node.id === selectedId;
                  const focused =
                    level === "graph" ||
                    isSelected ||
                    (selectedId !== null && neighbours.has(node.id));
                  const layer = node.layers[0];
                  return (
                    <g
                      key={node.id}
                      className="star__node"
                      data-testid="star-node"
                      data-kind={node.kind}
                      data-selected={isSelected}
                      data-focused={focused}
                      role="button"
                      tabIndex={-1}
                      aria-label={`${KIND_NAMES[node.kind]}：${node.content}`}
                      aria-pressed={isSelected}
                      transform={`translate(${point.x} ${point.y})`}
                      onClick={() => setSelectedId(node.id)}
                      onDoubleClick={() => setLevel("core")}
                      data-layer={layer}
                    >
                      <g
                        className="star__shape"
                        style={{
                          fill: layer ? `var(--layer-${layer})` : "var(--ash)",
                          opacity: focused ? recencyBrightness(node.activationUpdatedAt) : 0.18,
                        }}
                      >
                        {nodeShape(node.kind, radius)}
                      </g>
                      <circle
                        className="star__ring"
                        r={radius + 5}
                        opacity={isSelected ? 0.9 : 0}
                      />
                      {level === "graph" && (isSelected || node.activation >= 0.6) ? (
                        <text className="star__node-label" y={radius + 14}>
                          {node.content.length > 12
                            ? `${node.content.slice(0, 12)}…`
                            : node.content}
                        </text>
                      ) : null}
                    </g>
                  );
                })
              : null}
          </svg>

          {level === "core" ? (
            <aside className="star__core" aria-label="心核">
              {selected ? (
                <>
                  <p className="star__core-content">{selected.content}</p>
                  <p className="star__core-meta">
                    {KIND_NAMES[selected.kind]}
                    {selected.layers[0] ? ` · ${selected.layers.join(" ")}` : ""} · 活跃度{" "}
                    {selected.activation.toFixed(2)}
                  </p>
                  <h2 className="star__core-title">全部连接</h2>
                  <ul className="star__links">
                    {(detail?.links ??
                      edges
                        .filter(
                          (edge) => edge.from === selected.id || edge.to === selected.id,
                        )
                        .map((edge) => ({
                          edgeId: edge.id,
                          relation: edge.relation,
                          weight: edge.weight,
                          status: "active",
                          direction:
                            edge.from === selected.id ? ("out" as const) : ("in" as const),
                          peerId: edge.from === selected.id ? edge.to : edge.from,
                          peerKind: "idea" as NodeKind,
                          peerContent:
                            nodes.find(
                              (node) =>
                                node.id === (edge.from === selected.id ? edge.to : edge.from),
                            )?.content ?? "",
                        }))
                    ).map((link) => (
                      <li key={link.edgeId} className="star__link">
                        <span
                          className="star__link-dot"
                          data-relation={link.relation}
                          style={{ background: RELATION_COLORS[link.relation] }}
                          aria-hidden="true"
                        />
                        <span className="star__link-relation">
                          {RELATION_NAMES[link.relation]} · {link.weight.toFixed(2)}
                        </span>
                        <span className="star__link-peer">{link.peerContent}</span>
                      </li>
                    ))}
                  </ul>
                  {detail ? (
                    <>
                      <h2 className="star__core-title">来源</h2>
                      <p className="star__core-source mono">
                        {nodeSourceLabel(detail.node.sourceKind)} · {detail.node.sourceRef}
                      </p>
                      <p className="star__core-meta">
                        {detail.activations.length} 次唤醒记录
                      </p>
                    </>
                  ) : null}
                  {onSeed ? (
                    <button
                      type="button"
                      className="star__seed"
                      onClick={() => onSeed(selected.content)}
                    >
                      以此发起会诊
                    </button>
                  ) : null}
                </>
              ) : (
                <p className="star__core-meta">还没有选中的念头</p>
              )}
            </aside>
          ) : null}

          <p className="star__status" aria-live="polite">
            {status}
          </p>
          {graph.data?.truncated ? (
            <p className="star__note">
              只显示最活跃的 {nodes.length} / {graph.data.totalNodes} 个念头
            </p>
          ) : null}
        </div>
        ) : (
          <div className="star__equivalent">
            {nodes.length === 0 ? (
              <p className="star__note">网络里还没有念头。</p>
            ) : view === "list" ? (
              <ul className="graph-list" aria-label="思维网络列表">
                {nodes.map((node) => (
                  <li
                    key={node.id}
                    className="graph-list__item"
                    data-selected={node.id === selectedId}
                    data-layer={node.layers[0]}
                  >
                    <button
                      type="button"
                      className="graph-list__select"
                      aria-pressed={node.id === selectedId}
                      onClick={() => setSelectedId(node.id)}
                    >
                      <span className="graph-list__kind">{KIND_NAMES[node.kind]}</span>
                      <span className="graph-list__content">{node.content}</span>
                    </button>
                    <span className="graph-list__meta mono">
                      {node.layers.length > 0
                        ? node.layers.map((key) => layerOf(key).name).join(" ")
                        : "未标层次"}{" "}
                      · {communityLabel(node)} · 活跃度 {node.activation.toFixed(2)} · 连接{" "}
                      {degree.get(node.id) ?? 0}
                    </span>
                  </li>
                ))}
              </ul>
            ) : (
              <div className="graph-outline" role="tree" aria-label="思维网络大纲">
                {outline.map(([key, group]) => (
                  <div key={key} className="graph-outline__group" role="treeitem" aria-expanded="true">
                    <span className="graph-outline__domain">
                      {group.label}
                      <span className="mono"> · {group.nodes.length}</span>
                    </span>
                    <ul role="group" className="graph-outline__nodes">
                      {group.nodes.map((node) => (
                        <li
                          key={node.id}
                          className="graph-outline__node"
                          role="treeitem"
                          aria-selected={node.id === selectedId}
                          data-layer={node.layers[0]}
                        >
                          {node.layers[0] ? (
                            <span className="graph-outline__glyph" data-layer={node.layers[0]}>
                              <LayerGlyph glyph={layerOf(node.layers[0]).glyph} />
                            </span>
                          ) : null}
                          <button
                            type="button"
                            className="graph-outline__select"
                            onClick={() => setSelectedId(node.id)}
                          >
                            {node.content}
                          </button>
                          <span className="graph-outline__meta mono">
                            {KIND_NAMES[node.kind]} · 活跃度 {node.activation.toFixed(2)}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </div>
                ))}
              </div>
            )}

            {selected ? (
              <div className="graph-equivalent__actions">
                <span className="graph-equivalent__selected">
                  已选「{selected.content}」
                </span>
                <button
                  type="button"
                  onClick={() => {
                    setView("graph");
                    setLevel("core");
                  }}
                >
                  入心核
                </button>
                {onSeed ? (
                  <button
                    type="button"
                    className="star__seed"
                    onClick={() => onSeed(selected.content)}
                  >
                    以此发起会诊
                  </button>
                ) : null}
              </div>
            ) : null}
          </div>
        )}
      </div>

      <div className="tally-row">
        <Tally label="念头" value={nodes.length} hint="已经入网" />
        <Tally label="连接" value={edges.length} hint="彼此的关系" />
        <Tally label="新入炉" value={recentCaptures} hint="近期采集" />
        <Tally label="待裁决" value={conflicts.length} hint="矛盾待定" />
      </div>

      <section className="panel">
        <h2 className="section-head">待裁决</h2>
        {conflicts.length === 0 ? (
          <p className="setting-row__hint">没有尚未裁决的矛盾，网络自洽。</p>
        ) : (
          <ul className="adjudicate">
            {conflicts.map((edge) => (
              <li key={edge.id} className="adjudicate__item">
                <span className="adjudicate__pair">
                  「{nodeLabel(edge.from)}」 ↔ 「{nodeLabel(edge.to)}」
                </span>
                <span className="adjudicate__weight">分量 {edge.weight.toFixed(2)}</span>
                <button
                  className="adjudicate__keep"
                  type="button"
                  onClick={() => void resolve(edge, "keep")}
                >
                  保留矛盾
                </button>
                <button
                  className="adjudicate__drop"
                  type="button"
                  onClick={() => void resolve(edge, "drop")}
                >
                  舍弃一侧
                </button>
              </li>
            ))}
          </ul>
        )}
        {note ? (
          <p className="setting-row__hint" data-tone="warn">
            {note}
          </p>
        ) : null}
      </section>
    </RealmShell>
  );
}

function Tally({
  label,
  value,
  hint,
}: {
  readonly label: string;
  readonly value: number;
  readonly hint: string;
}) {
  return (
    <div className="tally" data-metric={label}>
      <span className="tally__label">{label}</span>
      <span className="tally__value mono">{value}</span>
      <span className="tally__hint">{hint}</span>
    </div>
  );
}
