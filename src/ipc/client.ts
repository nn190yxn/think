/**
 * 类型化 command 客户端。把 CommandResult 解包成数据或抛出 CommandInvocationError，
 * 让业务代码不必到处判断 ok 字段。
 */

import {
  CommandInvocationError,
  isCommandErrorCode,
  isCommandResult,
  type CommandErrorCode,
} from "./protocol";
import type { CommandName, CommandRequest, CommandResponse } from "./commands";
import {
  DEMO_ASSET_ROOTS,
  DEMO_ASSET_SKILLS,
  DEMO_BACKUPS,
  DEMO_CALLS,
  DEMO_CAPTURE_AUDIT,
  DEMO_CAPTURE_EVENTS,
  DEMO_CAPTURE_SETTINGS,
  DEMO_CAPTURE_SUMMARIES,
  DEMO_COLLISION,
  DEMO_COMPANION,
  DEMO_CONSOLIDATION,
  DEMO_CONNECTOR_CALLS,
  DEMO_CONNECTORS,
  DEMO_COST_DAYS,
  DEMO_COVERAGE,
  DEMO_CORPUS,
  DEMO_CONCLUSION,
  DEMO_DATA_EVENTS,
  DEMO_DATA_SCOPE,
  DEMO_DISCOVERY,
  DEMO_DISTILL_DETAIL,
  DEMO_DISTILL_JOB,
  DEMO_ECHO_HITS,
  DEMO_GRAPH,
  DEMO_INTAKE_JOB,
  DEMO_INSIGHTS,
  DEMO_KB_DOCUMENTS,
  DEMO_KB_OVERVIEW,
  DEMO_KB_SOURCES,
  DEMO_NODE_DETAIL,
  DEMO_PLATFORMS,
  DEMO_PROBE,
  DEMO_POOL,
  DEMO_PRINCIPLES,
  DEMO_PROMPT_VERSION,
  DEMO_RECORDS,
  DEMO_RING,
  DEMO_RUNS,
  DEMO_SEED_REPORT,
  DEMO_SELF_DETAIL,
  DEMO_SELF_READINESS,
  DEMO_SESSION,
  DEMO_SESSIONS,
  DEMO_SIGNALS,
  DEMO_SUMMARIES,
  DEMO_TOPICS,
  DEMO_TUNING,
  demoDetail,
  demoSearchHits,
  demoSeatSpeech,
  demoSelection,
} from "./demoData";
import type {
  AssetScanOutcome,
  AssetRootView,
  AssetSummary,
  BackupOutcome,
  BackupView,
  CaptureEventView,
  CaptureSettingsView,
  CompanionSettings,
  ConnectorCallView,
  ConnectorView,
  CouncilConclusionView,
  CouncilSeatSpeech,
  CouncilSessionView,
  CouncilStrategy,
  CostEstimate,
  CostSummary,
  CredentialRefView,
  DataEventView,
  DiscoverySettings,
  DistillJobView,
  FollowUpAnchor,
  Insight,
  IntakeJobView,
  KbDocumentView,
  KbSourceView,
  NodeKind,
  PreparedQueryView,
  SelfDraftDetail,
  SelfReadiness,
  SkillDependencyView,
  SkillDetail,
  SkillView,
  SignalView,
  SearchOutcome,
  ConnectorTestOutcome,
  TuningItem,
} from "./commands";

export interface CommandTransport {
  invoke(name: string, request: unknown): Promise<unknown>;
}

export interface CommandClient {
  call<K extends CommandName>(
    name: K,
    request: CommandRequest<K>,
  ): Promise<CommandResponse<K>>;
}

function toInvocationError(cause: unknown): CommandInvocationError {
  if (cause instanceof CommandInvocationError) {
    return cause;
  }
  if (typeof cause === "string") {
    // Rust 侧返回 Err(String) 时，字符串本身可能就是错误码。
    if (isCommandErrorCode(cause)) {
      return new CommandInvocationError(cause, cause);
    }
    return new CommandInvocationError("E_UNKNOWN", cause);
  }
  if (typeof cause === "object" && cause !== null) {
    const candidate = cause as { code?: unknown; message?: unknown; detail?: unknown };
    const code: CommandErrorCode = isCommandErrorCode(candidate.code)
      ? candidate.code
      : "E_UNKNOWN";
    const message =
      typeof candidate.message === "string" ? candidate.message : "命令执行失败";
    const detail = typeof candidate.detail === "string" ? candidate.detail : undefined;
    return new CommandInvocationError(code, message, detail);
  }
  return new CommandInvocationError("E_UNKNOWN", "命令执行失败");
}

export function createCommandClient(transport: CommandTransport): CommandClient {
  return {
    async call<K extends CommandName>(name: K, request: CommandRequest<K>) {
      let raw: unknown;
      try {
        raw = await transport.invoke(name, request);
      } catch (cause) {
        throw toInvocationError(cause);
      }

      if (!isCommandResult(raw)) {
        throw new CommandInvocationError(
          "E_MALFORMED_RESPONSE",
          `命令 ${name} 返回了不符合约定的数据`,
        );
      }
      if (!raw.ok) {
        throw new CommandInvocationError(raw.code, raw.message, raw.detail);
      }
      return raw.data as CommandResponse<K>;
    },
  };
}

/** 预览模式下主动助学设置与洞察可变更，用模块级状态模拟一次性会话。 */
let demoCompanion: CompanionSettings = { ...DEMO_COMPANION };
let demoInsights: Insight[] = [...DEMO_INSIGHTS];
/** 预览模式下蒸馏与入库任务可推进，用模块级状态模拟一次性会话。 */
let demoDistill: DistillJobView = { ...DEMO_DISTILL_JOB };
let demoIntake: IntakeJobView = { ...DEMO_INTAKE_JOB };
let demoSignals: SignalView[] = [...DEMO_SIGNALS];
let demoDiscovery: DiscoverySettings = { ...DEMO_DISCOVERY };
/** 预览模式下采集台与知识地形可变更，用模块级状态模拟一次性会话。 */
let demoCapture: CaptureSettingsView = {
  ...DEMO_CAPTURE_SETTINGS,
  capabilities: DEMO_CAPTURE_SETTINGS.capabilities.map((item) => ({ ...item })),
  watchRoots: [...DEMO_CAPTURE_SETTINGS.watchRoots],
};
let demoCaptureEvents: CaptureEventView[] = [...DEMO_CAPTURE_EVENTS];
let demoKbSources: KbSourceView[] = DEMO_KB_SOURCES.map((source) => ({ ...source }));
let demoKbDocuments: KbDocumentView[] = [...DEMO_KB_DOCUMENTS];
/** 预览模式下自我蒸馏与数据主权可推进，用模块级状态模拟一次性会话。 */
let demoSelfReadiness: SelfReadiness = { ...DEMO_SELF_READINESS };
let demoSelfDetail: SelfDraftDetail | null = {
  ...DEMO_SELF_DETAIL,
  draft: { ...DEMO_SELF_DETAIL.draft },
  items: DEMO_SELF_DETAIL.items.map((item) => ({ ...item })),
};
let demoDataEvents: DataEventView[] = [...DEMO_DATA_EVENTS];
/** 预览模式下资产根目录与 Skill 索引可变更，用模块级状态模拟一次性会话。 */
let demoAssetRoots: AssetRootView[] = DEMO_ASSET_ROOTS.map((root) => ({ ...root }));
let demoAssetSkills: SkillView[] = DEMO_ASSET_SKILLS.map((skill) => ({
  ...skill,
  tags: [...skill.tags],
}));
/** 预览模式下调参可写入，用模块级状态模拟一次性会话。 */
let demoTuning: TuningItem[] = DEMO_TUNING.map((item) => ({ ...item }));
/** 预览模式下会诊会话与追问可推进，用模块级状态模拟一次性会话。 */
let demoSessions: CouncilSessionView[] = [...DEMO_SESSIONS];
/** 预览模式下连接器配置与调用审计可变更，用模块级状态模拟一次性会话。 */
let demoConnectors: ConnectorView[] = DEMO_CONNECTORS.map((item) => ({ ...item }));
let demoConnectorCalls: ConnectorCallView[] = [...DEMO_CONNECTOR_CALLS];
/** 预览模式下成本、备份与凭据可变更，用模块级状态模拟一次性会话。 */
let demoCostDays = DEMO_COST_DAYS.map((item) => ({ ...item }));
let demoBackups: BackupView[] = DEMO_BACKUPS.map((item) => ({ ...item }));
const demoCredentials = new Set<string>();

/** 预览模式的费用汇总，上限与策略取自当前调参。 */
function demoCostSummary(days: number): CostSummary {
  const slice = demoCostDays.slice(0, Math.max(1, days));
  const today = demoCostDays[0]?.costMicros ?? 0;
  const month = demoCostDays.reduce((sum, item) => sum + item.costMicros, 0);
  return {
    days: slice,
    todayMicros: today,
    monthMicros: month,
    dailyLimitMicros: Number(demoTuningValue("cost.daily_limit_micros", "0")),
    monthlyLimitMicros: Number(demoTuningValue("cost.monthly_limit_micros", "0")),
    policy: demoTuningValue("cost.over_limit_policy", "reject"),
    currency: DEMO_PLATFORMS.find((item) => item.enabled)?.currency ?? "CNY",
    priced: DEMO_PLATFORMS.some(
      (item) => item.inputPriceMicrosPer1k > 0 || item.outputPriceMicrosPer1k > 0,
    ),
  };
}

/** 预览模式的费用估算，公式与内核保持一致：每席每轮一次调用，另加一次收敛裁决。 */
function demoCostEstimate(seats: number, rounds: number, searches: number): CostEstimate {
  const llmCalls = seats * rounds + 1;
  const tokens = llmCalls * 1200;
  return {
    llmCalls,
    searchCalls: searches,
    tokens,
    costMicros: llmCalls * 4000 + searches * 2000,
    platformCode: DEMO_PLATFORMS.find((item) => item.enabled)?.code ?? "",
    priced: DEMO_PLATFORMS.some(
      (item) => item.inputPriceMicrosPer1k > 0 || item.outputPriceMicrosPer1k > 0,
    ),
  };
}

/** 预览模式的调参取值读取，缺省回落到给定默认值。 */
function demoTuningValue(key: string, fallback: string): string {
  return demoTuning.find((item) => item.key === key)?.value ?? fallback;
}

/** 关键词模式演示用的停用词，与内核侧口径保持同一意图。 */
const DEMO_STOPWORDS = [
  "的",
  "了",
  "吗",
  "呢",
  "要不要",
  "是不是",
  "如何",
  "怎么",
  "什么",
  "应该",
  "可以",
  "要不要",
  "这个",
  "那个",
];

/**
 * 预览模式的发送前准备：先按简单规则脱敏，再按模式决定实际发送串。
 * 目的是让界面上的预演确认面板有真实可核对的内容，而非照搬内核实现。
 */
export function demoPrepareQuery(question: string, mode: string): PreparedQueryView {
  const original = question.trim();
  const masked = original.replace(
    /[\w.+-]+@[\w-]+(\.[\w-]+)+/g,
    "[已脱敏]",
  );
  const redacted = masked !== original;
  if (mode === "question") {
    return { original, sent: masked, redacted, mode };
  }
  const terms = masked
    .split(/[\s，。？！、；：,?!;:]+/)
    .filter((term) => term && !DEMO_STOPWORDS.includes(term));
  return { original, sent: terms.join(" ") || masked, redacted, mode };
}

/** 预览模式的发送串指纹，只用于界面上的两阶段确认回路。 */
export function demoFingerprint(sent: string): string {
  let hash = 5381;
  for (const ch of sent) {
    hash = (hash * 33 + (ch.codePointAt(0) ?? 0)) >>> 0;
  }
  return `demo-${hash.toString(16)}`;
}

/** 演示用依赖表，仅覆盖示例 Skill。 */
const DEMO_SKILL_DEPS: Record<string, readonly SkillDependencyView[]> = {
  "skill-asset-1": [
    { name: "typo-check", version: "0.3", kind: "" },
    { name: "rag", version: "", kind: "" },
  ],
};

/** 预览模式的资产总览由当前根基与 Skill 索引即时汇总。 */
function demoAssetSummary(): AssetSummary {
  const categories = new Map<string, { skillCount: number; enabledCount: number }>();
  let enabledCount = 0;
  for (const skill of demoAssetSkills) {
    const category = skill.category || "未归类";
    const entry = categories.get(category) ?? { skillCount: 0, enabledCount: 0 };
    entry.skillCount += 1;
    if (skill.enabled) {
      entry.enabledCount += 1;
      enabledCount += 1;
    }
    categories.set(category, entry);
  }
  return {
    rootCount: demoAssetRoots.length,
    availableRoots: demoAssetRoots.filter((root) => root.available).length,
    skillCount: demoAssetSkills.length,
    enabledCount,
    disabledCount: demoAssetSkills.length - enabledCount,
    needsRepairCount: demoAssetSkills.filter((skill) => skill.needsRepair).length,
    recentAdded: 3,
    recentRemoved: 1,
    categories: [...categories.entries()]
      .map(([category, stat]) => ({ category, ...stat }))
      .sort((left, right) => right.skillCount - left.skillCount || left.category.localeCompare(right.category)),
    platforms: DEMO_PLATFORMS.map((platform) => ({
      code: platform.code,
      displayName: platform.displayName,
      modelName: platform.modelName,
      enabled: platform.enabled,
      status: platform.status,
    })),
  };
}

/** 预览模式的知识地形总览由当前来源与文档即时汇总。 */
/** 预览模式下的连接器类型名。 */
function demoKindLabel(kind: string): string {
  if (kind === "search") {
    return "搜索";
  }
  if (kind === "page") {
    return "网页阅读";
  }
  if (kind === "mcp") {
    return "MCP 工具";
  }
  return "未知";
}

/** 预览模式的知识地形总览由当前来源与文档即时汇总。 */
function demoOverview() {
  const domains = new Map<string, number>();
  const topics = new Map<string, { docCount: number; latestModifiedAt: string | null }>();
  for (const doc of demoKbDocuments) {
    const domain = doc.domain || "未归类";
    domains.set(domain, (domains.get(domain) ?? 0) + 1);
    const key = doc.topicId ?? doc.topicName;
    const entry = topics.get(key);
    const latest = doc.modifiedAt ?? doc.createdAt;
    if (entry) {
      entry.docCount += 1;
      if (latest && (!entry.latestModifiedAt || latest > entry.latestModifiedAt)) {
        entry.latestModifiedAt = latest;
      }
    } else {
      topics.set(key, { docCount: 1, latestModifiedAt: latest });
    }
  }
  return {
    sourceCount: demoKbSources.length,
    availableSources: demoKbSources.filter((source) => source.available).length,
    docCount: demoKbDocuments.length,
    topicCount: topics.size,
    domains: [...domains.entries()]
      .map(([domain, docCount]) => ({ domain, docCount }))
      .sort((a, b) => b.docCount - a.docCount),
    topics: [...topics.entries()].map(([id, value]) => ({
      id,
      displayName:
        demoKbDocuments.find((doc) => (doc.topicId ?? doc.topicName) === id)?.topicName ?? id,
      docCount: value.docCount,
      latestModifiedAt: value.latestModifiedAt,
    })),
    rings: DEMO_KB_OVERVIEW.rings,
  };
}

/** 浏览器开发模式下没有 IPC，用固定应答保证界面可用。 */
export const stubTransport: CommandTransport = {
  async invoke(name, request) {
    const payload = (request ?? {}) as Record<string, unknown>;
    switch (name) {
      case "app_info":
        return {
          ok: true,
          data: { name: "思想熔炉", version: "0.1.0", schemaVersion: 1 },
        };
      case "db_status":
        return {
          ok: true,
          data: {
            path: "memory://prototype",
            schemaVersion: 1,
            journalMode: "memory",
            foreignKeys: true,
          },
        };
      case "furnace_snapshot":
        return {
          ok: true,
          data: {
            activeNodes: DEMO_GRAPH.nodes.filter((node) => node.activation >= 0.3).length,
            totalNodes: DEMO_GRAPH.nodes.length,
            recentCaptures: 12,
            recentCouncils: 2,
            computedAt: new Date().toISOString(),
          },
        };
      case "master_list": {
        const domain = typeof payload.domain === "string" ? payload.domain : undefined;
        const layer = typeof payload.layer === "string" ? payload.layer : undefined;
        const list = DEMO_SUMMARIES.filter(
          (master) =>
            (!domain || master.domain === domain) &&
            (!layer || master.layers.includes(layer as never)),
        );
        return { ok: true, data: list };
      }
      case "master_detail": {
        const detail = demoDetail(String(payload.masterId ?? ""));
        if (!detail) {
          return { ok: false, code: "E_NOT_FOUND", message: "大师不存在" };
        }
        return { ok: true, data: detail };
      }
      case "master_versions": {
        const detail = demoDetail(String(payload.masterId ?? ""));
        return { ok: true, data: detail?.versions ?? [] };
      }
      case "master_revert":
        return { ok: true, data: Number(payload.version ?? 1) };
      case "master_flag_unit":
        return { ok: true, data: null };
      case "master_install":
        return {
          ok: true,
          data: {
            masterId: "installed-demo",
            version: 1,
            unitCount: 0,
            corpusCount: 0,
            created: true,
            diff: { added: [], updated: [], carried: 0, sourceRefs: [] },
          },
        };
      case "master_validate":
        return { ok: true, data: { valid: true } };
      case "coverage_matrix":
        return { ok: true, data: DEMO_COVERAGE };
      case "master_domains":
        return { ok: true, data: DEMO_SUMMARIES.map((master) => master.domain) };
      case "corpus_list":
        return { ok: true, data: DEMO_CORPUS };
      case "corpus_search": {
        const query = String(payload.query ?? "").trim();
        if (!query) {
          return { ok: true, data: [] };
        }
        const hits = DEMO_CORPUS.filter(
          (item) => item.title.includes(query) || item.normalizedName.includes(query),
        ).map((item) => ({ item, matchedBy: "like" as const }));
        return { ok: true, data: hits };
      }
      case "seed_packs_install":
        return { ok: true, data: DEMO_SEED_REPORT };
      case "settings_get":
        return { ok: true, data: null };
      case "settings_set":
        return { ok: true, data: null };
      case "networking_get":
        return { ok: true, data: false };
      case "networking_set":
        return { ok: true, data: Boolean(payload.enabled) };
      case "platform_list":
        return { ok: true, data: DEMO_PLATFORMS };
      case "platform_upsert": {
        const code = String(payload.code ?? "demo");
        return {
          ok: true,
          data: {
            id: `platform-${code}`,
            code,
            displayName: String(payload.displayName ?? code),
            endpoint: String(payload.endpoint ?? ""),
            modelName: String(payload.modelName ?? ""),
            enabled: false,
            status: "disabled",
            createdAt: new Date().toISOString(),
            updatedAt: new Date().toISOString(),
          },
        };
      }
      case "platform_enable": {
        const code = String(payload.code ?? "demo");
        const base = DEMO_PLATFORMS.find((platform) => platform.code === code);
        return {
          ok: true,
          data: {
            id: base?.id ?? `platform-${code}`,
            code,
            displayName: base?.displayName ?? code,
            endpoint: base?.endpoint ?? "",
            modelName: base?.modelName ?? "",
            enabled: Boolean(payload.enabled),
            status: payload.enabled ? "ready" : "disabled",
            createdAt: base?.createdAt ?? new Date().toISOString(),
            updatedAt: new Date().toISOString(),
          },
        };
      }
      case "llm_calls":
        return { ok: true, data: DEMO_CALLS };
      case "council_candidates":
        return { ok: true, data: DEMO_POOL };
      case "council_create":
        return {
          ok: true,
          data: {
            ...DEMO_SESSION.session,
            selfSeatIncluded: payload.includeSelf !== false,
          },
        };
      case "council_select":
      case "council_rotate": {
        const strategy = (payload.strategy ?? "steady") as CouncilStrategy;
        const pinned = Array.isArray(payload.pinned) ? (payload.pinned as string[]) : [];
        return { ok: true, data: demoSelection(strategy, pinned) };
      }
      case "council_run":
        return {
          ok: true,
          data: {
            sessionId: String(payload.sessionId ?? DEMO_SESSION.session.id),
            rotation: 1,
            answered: 6,
            failed: 0,
            conclusion: DEMO_SESSION.session.conclusion,
            divergences: DEMO_SESSION.session.divergences,
            rounds: DEMO_SESSION.metrics.length + 1,
            metrics: DEMO_SESSION.metrics,
          },
        };
      case "council_sessions":
        return { ok: true, data: demoSessions };
      case "council_session": {
        const sessionId = String(payload.sessionId ?? DEMO_SESSION.session.id);
        if (sessionId === DEMO_SESSION.session.id) {
          return { ok: true, data: DEMO_SESSION };
        }
        const session = demoSessions.find((item) => item.id === sessionId);
        if (!session) {
          return { ok: false, code: "E_NOT_FOUND", message: `会诊 ${sessionId}` };
        }
        return { ok: true, data: { session, panels: [], turns: [], metrics: [] } };
      }
      case "council_turns": {
        const sessionId = String(payload.sessionId ?? DEMO_SESSION.session.id);
        const rotation =
          typeof payload.rotation === "number"
            ? payload.rotation
            : DEMO_SESSION.panels[DEMO_SESSION.panels.length - 1]!.rotation;
        const data =
          sessionId === DEMO_SESSION.session.id ? demoSeatSpeech(rotation) : [];
        return { ok: true, data };
      }
      case "council_retry_seat": {
        const rotation = Number(payload.rotation ?? 1);
        const masterId = String(payload.masterId ?? "");
        const round = Number(payload.round ?? 1);
        const seats: CouncilSeatSpeech[] = demoSeatSpeech(rotation).map((seat) => {
          if (seat.masterId !== masterId) {
            return seat;
          }
          const rounds = seat.rounds.map((item) =>
            item.round === round && item.status !== "ok"
              ? {
                  ...item,
                  status: "ok",
                  errorCode: null,
                  content: `${seat.masterName}的补答：先算清代价，再谈勇气与时机。`,
                }
              : item,
          );
          const status = rounds.some((item) => item.status !== "ok")
            ? "failed"
            : "answered";
          return { ...seat, rounds, status };
        });
        return { ok: true, data: seats };
      }
      case "council_followup": {
        const anchor = (payload.anchor ?? {
          kind: "conclusion",
          text: "",
        }) as FollowUpAnchor;
        const question = String(payload.question ?? "");
        const parentId = String(payload.parentSessionId ?? DEMO_SESSION.session.id);
        const parent =
          demoSessions.find((item) => item.id === parentId) ?? DEMO_SESSION.session;
        const now = new Date().toISOString();
        const session: CouncilSessionView = {
          ...DEMO_SESSION.session,
          id: `council-demo-followup-${demoSessions.length}`,
          question,
          status: "draft",
          conclusion: "",
          divergences: [],
          rotationCount: 0,
          turnCount: 0,
          parentSessionId: parent.id,
          anchorKind: anchor.kind,
          anchorText: anchor.text,
          anchorMasterId: anchor.masterId ?? null,
          anchorRound: anchor.round ?? null,
          anchorTruncated: anchor.text.length > 2000,
          panelInherited: payload.inheritPanel !== false,
          createdAt: now,
          updatedAt: now,
        };
        demoSessions = [session, ...demoSessions];
        return { ok: true, data: session };
      }
      case "council_conclusion": {
        const sessionId = String(payload.sessionId ?? DEMO_SESSION.session.id);
        if (sessionId === DEMO_SESSION.session.id) {
          return { ok: true, data: DEMO_CONCLUSION };
        }
        const session = demoSessions.find((item) => item.id === sessionId);
        if (!session) {
          return { ok: false, code: "E_NOT_FOUND", message: `会诊 ${sessionId}` };
        }
        const view: CouncilConclusionView = {
          session,
          metrics: [],
          speeches: [],
          sources: [],
          history: [DEMO_SESSION.session],
          stanceChanges: [],
          promptVersion: DEMO_PROMPT_VERSION,
          llmCalls: 0,
          searchCalls: 0,
          costMicros: 0,
          currency: "CNY",
          priced: false,
        };
        return { ok: true, data: view };
      }
      case "council_sources": {
        const sessionId = String(payload.sessionId ?? DEMO_SESSION.session.id);
        if (sessionId !== DEMO_SESSION.session.id) {
          return { ok: true, data: [] };
        }
        const rotation =
          typeof payload.rotation === "number"
            ? payload.rotation
            : DEMO_SESSION.panels[DEMO_SESSION.panels.length - 1]!.rotation;
        // round 为 0 的条目是共享背景，其余按传入轮次取本次快照。
        const data = DEMO_CONCLUSION.sources.filter(
          (source) => source.round === 0 || source.round === rotation,
        );
        return { ok: true, data };
      }
      case "council_cancel": {
        const sessionId = String(payload.sessionId ?? DEMO_SESSION.session.id);
        const session = demoSessions.find((item) => item.id === sessionId);
        if (!session) {
          return { ok: false, code: "E_NOT_FOUND", message: `会诊 ${sessionId}` };
        }
        const view = { ...session, cancelRequested: true };
        demoSessions = demoSessions.map((item) => (item.id === sessionId ? view : item));
        return { ok: true, data: view };
      }
      case "council_recoverable":
        return {
          ok: true,
          data: demoSessions.filter((item) => item.status === "running"),
        };
      case "echo_check":
        return { ok: true, data: DEMO_ECHO_HITS };
      case "principle_revoke": {
        const nodeId = String(payload.nodeId ?? "");
        if (!nodeId) {
          return { ok: false, code: "E_INVALID_INPUT", message: "缺少 nodeId" };
        }
        return { ok: true, data: true };
      }
      case "council_search": {
        const searchable = demoConnectors.some(
          (item) => item.kind === "search" && item.enabled,
        );
        if (!searchable) {
          return {
            ok: false,
            code: "E_NETWORK_OFF",
            message: "尚未启用任何搜索连接器",
          };
        }
        const query = String(payload.query ?? "");
        if (!query.trim()) {
          return { ok: false, code: "E_INVALID_INPUT", message: "检索问句不能为空" };
        }
        const prepared = demoPrepareQuery(
          query,
          demoTuningValue("connector.query_mode", "keyword"),
        );
        const fingerprint = demoFingerprint(prepared.sent);
        const confirm =
          typeof payload.confirm === "string" ? payload.confirm : undefined;
        if (confirm !== undefined && confirm !== fingerprint) {
          return {
            ok: false,
            code: "E_INVALID_INPUT",
            message: "确认内容与当前待发送内容不一致，请重新预览后再确认",
          };
        }
        const preflight = demoTuningValue("connector.preflight", "true") === "true";
        if (confirm === undefined && preflight) {
          const outcome: SearchOutcome = {
            pending: true,
            fingerprint,
            prepared,
            hits: [],
          };
          return { ok: true, data: outcome };
        }
        const outcome: SearchOutcome = {
          pending: false,
          fingerprint,
          prepared,
          hits: demoSearchHits(prepared.sent),
        };
        return { ok: true, data: outcome };
      }
      case "connector_list":
        return { ok: true, data: demoConnectors };
      case "connector_upsert": {
        const kind = String(payload.kind ?? "");
        const displayName = String(payload.displayName ?? "").trim();
        const endpoint = String(payload.endpoint ?? "").trim();
        if (!kind || !displayName) {
          return {
            ok: false,
            code: "E_INVALID_INPUT",
            message: "连接器类型与名称不能为空",
          };
        }
        const existing =
          typeof payload.id === "string"
            ? demoConnectors.find((item) => item.id === payload.id)
            : undefined;
        const id = existing?.id ?? `connector-demo-${demoConnectors.length + 1}`;
        const enabled = existing?.enabled ?? false;
        const now = new Date().toISOString();
        const view: ConnectorView = {
          id,
          kind,
          kindLabel: demoKindLabel(kind),
          displayName,
          endpoint,
          config: payload.config ?? {},
          enabled,
          status: endpoint ? (enabled ? "ready" : "disabled") : "unconfigured",
          createdAt: existing?.createdAt ?? now,
          updatedAt: now,
        };
        demoConnectors = existing
          ? demoConnectors.map((item) => (item.id === id ? view : item))
          : [...demoConnectors, view];
        return { ok: true, data: view };
      }
      case "connector_enable": {
        const id = String(payload.id ?? "");
        const target = demoConnectors.find((item) => item.id === id);
        if (!target) {
          return { ok: false, code: "E_NOT_FOUND", message: `连接器 ${id}` };
        }
        const enabled = payload.enabled === true;
        const updated: ConnectorView = {
          ...target,
          enabled,
          status: target.endpoint ? (enabled ? "ready" : "disabled") : "unconfigured",
          updatedAt: new Date().toISOString(),
        };
        demoConnectors = demoConnectors.map((item) =>
          item.id === id ? updated : item,
        );
        return { ok: true, data: updated };
      }
      case "connector_test": {
        const id = String(payload.id ?? "");
        const target = demoConnectors.find((item) => item.id === id);
        if (!target) {
          return { ok: false, code: "E_NOT_FOUND", message: `连接器 ${id}` };
        }
        // 网页阅读类发送的是地址本身；检索类走发送前准备与两阶段确认。
        const prepared =
          target.kind === "search"
            ? demoPrepareQuery(
                "连接测试 检索",
                demoTuningValue("connector.query_mode", "keyword"),
              )
            : target.kind === "page"
              ? {
                  original: target.endpoint,
                  sent: target.endpoint,
                  redacted: false,
                  mode: "question",
                }
              : null;
        const fingerprint = prepared ? demoFingerprint(prepared.sent) : "";
        const confirm =
          typeof payload.confirm === "string" ? payload.confirm : undefined;
        if (confirm !== undefined && confirm !== fingerprint) {
          return {
            ok: false,
            code: "E_INVALID_INPUT",
            message: "确认内容与当前待发送内容不一致，请重新预览后再确认",
          };
        }
        if (
          confirm === undefined &&
          prepared &&
          demoTuningValue("connector.preflight", "true") === "true"
        ) {
          const pending: ConnectorTestOutcome = {
            pending: true,
            fingerprint,
            prepared,
            call: null,
          };
          return { ok: true, data: pending };
        }
        const call: ConnectorCallView = {
          id: `connector-call-${demoConnectorCalls.length + 1}`,
          connectorId: target.id,
          kind: target.kind,
          kindLabel: target.kindLabel,
          purpose: "connector_test",
          sessionId: null,
          query: prepared?.original ?? target.endpoint,
          queryOriginal: prepared?.original ?? target.endpoint,
          querySent: prepared?.sent ?? target.endpoint,
          redacted: prepared?.redacted ?? false,
          resultCount: target.endpoint ? 1 : 0,
          latencyMs: 88,
          status: target.endpoint ? "ok" : "failed",
          errorCode: target.endpoint ? null : "E_INVALID_INPUT",
          createdAt: new Date().toISOString(),
        };
        demoConnectorCalls = [call, ...demoConnectorCalls];
        const outcome: ConnectorTestOutcome = {
          pending: false,
          fingerprint,
          prepared,
          call,
        };
        return { ok: true, data: outcome };
      }
      case "connector_calls": {
        const limit = typeof payload.limit === "number" ? payload.limit : 50;
        return { ok: true, data: demoConnectorCalls.slice(0, limit) };
      }
      case "master_history":
        return { ok: true, data: [] };
      case "tuning_get":
        return { ok: true, data: demoTuning };
      case "tuning_set": {
        const values = Array.isArray(payload.values) ? payload.values : [];
        const next = demoTuning.map((item) => ({ ...item }));
        for (const pair of values) {
          if (!Array.isArray(pair) || pair.length !== 2) {
            return { ok: false, code: "E_INVALID_INPUT", message: "调参取值必须是键值对" };
          }
          const [key, raw] = pair as [string, string];
          const item = next.find((candidate) => candidate.key === key);
          if (!item) {
            return { ok: false, code: "E_INVALID_INPUT", message: `未知调参项：${key}` };
          }
          const normalized = raw.trim();
          Object.assign(item, {
            value: normalized,
            customized: normalized !== item.defaultValue,
          });
        }
        demoTuning = next;
        return { ok: true, data: demoTuning };
      }
      case "network_upsert_node": {
        const content = String(payload.content ?? "");
        return {
          ok: true,
          data: {
            nodeId: `node-demo-${content.length}`,
            outcome: "created",
          },
        };
      }
      case "network_link":
        return {
          ok: true,
          data: {
            edgeId: "edge-demo-new",
            created: true,
            weight: Number(payload.weight ?? 0.5),
          },
        };
      case "network_activate": {
        const nodeIds = Array.isArray(payload.nodeIds) ? payload.nodeIds : [];
        const propagated = Math.min(2, nodeIds.length);
        return {
          ok: true,
          data: {
            activated: nodeIds.length,
            propagated,
            propagatedFar: propagated > 0 ? 1 : 0,
            hops: propagated > 0 ? 2 : 1,
          },
        };
      }
      case "network_decay":
        return { ok: true, data: 0 };
      case "network_node": {
        const nodeId = String(payload.nodeId ?? "");
        if (nodeId === DEMO_NODE_DETAIL.node.id) {
          return { ok: true, data: DEMO_NODE_DETAIL };
        }
        const node = DEMO_GRAPH.nodes.find((item) => item.id === nodeId);
        if (!node) {
          return { ok: false, code: "E_NOT_FOUND", message: "节点不存在" };
        }
        return {
          ok: true,
          data: {
            node: {
              ...node,
              normalizedContent: node.content.replace(/\s+/g, ""),
              sourceKind: "manual",
              sourceRef: "demo",
              version: 1,
              createdAt: node.activationUpdatedAt,
            },
            links: DEMO_NODE_DETAIL.links.filter(
              (link) => link.peerId === nodeId || link.edgeId.startsWith("edge-"),
            ),
            activations: [],
          },
        };
      }
      case "network_graph": {
        const layer = typeof payload.layer === "string" ? payload.layer : undefined;
        const kind = typeof payload.kind === "string" ? (payload.kind as NodeKind) : undefined;
        const domain = typeof payload.domain === "string" ? payload.domain : undefined;
        const clusterId = typeof payload.clusterId === "string" ? payload.clusterId : undefined;
        const minActivation =
          typeof payload.minActivation === "number" ? payload.minActivation : undefined;
        const nodes = DEMO_GRAPH.nodes.filter(
          (node) =>
            (!layer || node.layers.includes(layer as never)) &&
            (!kind || node.kind === kind) &&
            (!domain || node.domains.includes(domain)) &&
            (!clusterId || node.clusterId === clusterId) &&
            (minActivation === undefined || node.activation >= minActivation),
        );
        const visible = new Set(nodes.map((node) => node.id));
        return {
          ok: true,
          data: {
            nodes,
            edges: DEMO_GRAPH.edges.filter(
              (edge) => visible.has(edge.from) && visible.has(edge.to),
            ),
            clusters: DEMO_GRAPH.clusters.filter(
              (cluster) =>
                (!clusterId || cluster.id === clusterId) &&
                cluster.memberIds.every((id) => visible.has(id)),
            ),
            totalNodes: DEMO_GRAPH.totalNodes,
            truncated: nodes.length < DEMO_GRAPH.totalNodes,
          },
        };
      }
      case "network_resolve_conflict":
        return { ok: true, data: "insight-demo-decision" };
      case "network_record_session":
        return {
          ok: true,
          data: {
            recordId: "record-demo-2",
            judgmentId: "node-judgment-1",
            frameworkIds: ["node-framework-sunzi", "node-framework-munger"],
            divergenceIds: ["node-question-1"],
            linkedPrior: ["node-judgment-2"],
            activated: 4,
          },
        };
      case "records_list":
        return { ok: true, data: DEMO_RECORDS };
      case "records_compare": {
        const topicKey = String(payload.topicKey ?? "");
        return {
          ok: true,
          data: DEMO_RECORDS.filter((record) => record.topicKey === topicKey),
        };
      }
      case "record_decision":
        return { ok: true, data: true };
      case "consolidate_trigger":
      case "consolidate_report":
        return { ok: true, data: DEMO_CONSOLIDATION };
      case "consolidate_runs":
        return { ok: true, data: DEMO_RUNS };
      case "companion_settings":
        return { ok: true, data: demoCompanion };
      case "companion_enable":
        demoCompanion = { ...demoCompanion, enabled: Boolean(payload.enabled) };
        return { ok: true, data: demoCompanion };
      case "companion_limit":
        demoCompanion = {
          ...demoCompanion,
          dailyLimit: Number(payload.limit ?? demoCompanion.dailyLimit),
        };
        return { ok: true, data: demoCompanion };
      case "companion_rules":
        demoCompanion = {
          ...demoCompanion,
          rules: (payload.rules as CompanionSettings["rules"]) ?? demoCompanion.rules,
        };
        return { ok: true, data: demoCompanion };
      case "companion_collide": {
        if (!demoCompanion.enabled) {
          return {
            ok: true,
            data: { generated: [], pushed: 0, skipped: "disabled", remaining: demoCompanion.dailyLimit },
          };
        }
        return { ok: true, data: DEMO_COLLISION };
      }
      case "insights_list": {
        const kind = typeof payload.kind === "string" ? payload.kind : undefined;
        const status = typeof payload.status === "string" ? payload.status : undefined;
        const source = typeof payload.source === "string" ? payload.source : undefined;
        return {
          ok: true,
          data: demoInsights.filter(
            (insight) =>
              (!kind || insight.kind === kind) &&
              (!status || insight.status === status) &&
              (!source || insight.source === source),
          ),
        };
      }
      case "insight_mark": {
        const insightId = String(payload.insightId ?? "");
        const action = String(payload.action ?? "");
        const status =
          action === "adopt" ? "adopted" : action === "ignore" ? "ignored" : "converted";
        const target = demoInsights.find((insight) => insight.id === insightId);
        if (!target) {
          return { ok: false, code: "E_NOT_FOUND", message: "洞察不存在" };
        }
        const updated: Insight = {
          ...target,
          action,
          status,
          reason: String(payload.reason ?? ""),
        };
        demoInsights = demoInsights.map((insight) =>
          insight.id === insightId ? updated : insight,
        );
        return { ok: true, data: updated };
      }
      case "insight_convert":
        return { ok: true, data: DEMO_SESSION.session };
      case "topics_list":
        return { ok: true, data: DEMO_TOPICS };
      case "principles_list":
        return { ok: true, data: DEMO_PRINCIPLES };
      case "promote_principles":
        return { ok: true, data: [] };
      case "ring_overview":
        return { ok: true, data: DEMO_RING };
      case "distill_start":
      case "distill_from_intake": {
        demoDistill = {
          ...demoDistill,
          sourceKind: name === "distill_start" ? "manual" : demoDistill.sourceKind,
          state: "awaiting_confirmation",
          stage: "extract",
          stageName: "五路提取",
        };
        return { ok: true, data: demoDistill };
      }
      case "distill_list":
        return { ok: true, data: [demoDistill] };
      case "distill_detail":
        return { ok: true, data: { job: demoDistill, draft: DEMO_DISTILL_DETAIL.draft } };
      case "distill_confirm":
      case "distill_resume": {
        demoDistill = {
          ...demoDistill,
          state: "done",
          stage: "done",
          stageName: "完成",
          modelCalls: name === "distill_confirm" ? 9 : demoDistill.modelCalls,
        };
        return { ok: true, data: demoDistill };
      }
      case "intake_create": {
        const materials = Array.isArray(payload.materials) ? payload.materials.length : 0;
        demoIntake = {
          ...demoIntake,
          mode: "manual",
          state: "confirmed",
          materialCount: materials,
          acceptedCount: materials,
          rejectedCount: 0,
        };
        return { ok: true, data: demoIntake };
      }
      case "intake_list":
        return { ok: true, data: [demoIntake] };
      case "intake_preview":
        return { ok: true, data: demoSignals };
      case "intake_confirm": {
        const accepted = new Set(
          Array.isArray(payload.acceptedIds) ? (payload.acceptedIds as string[]) : [],
        );
        const rejected = new Set(
          Array.isArray(payload.rejectedIds) ? (payload.rejectedIds as string[]) : [],
        );
        demoSignals = demoSignals.map((signal) =>
          accepted.has(signal.id)
            ? { ...signal, status: "accepted" }
            : rejected.has(signal.id)
              ? { ...signal, status: "rejected" }
              : signal,
        );
        const acceptedCount = demoSignals.filter((signal) => signal.status === "accepted").length;
        const rejectedCount = demoSignals.filter((signal) => signal.status === "rejected").length;
        demoIntake = {
          ...demoIntake,
          state: acceptedCount > 0 ? "confirmed" : "rejected",
          acceptedCount,
          rejectedCount,
        };
        return { ok: true, data: demoIntake };
      }
      case "discovery_settings":
        return { ok: true, data: demoDiscovery };
      case "discovery_enable":
        demoDiscovery = { ...demoDiscovery, enabled: Boolean(payload.enabled) };
        return { ok: true, data: demoDiscovery };
      case "discovery_schedule":
        demoDiscovery = { ...demoDiscovery, schedule: payload.schedule ?? {} };
        return { ok: true, data: demoDiscovery };
      case "discovery_run":
        if (!demoDiscovery.enabled) {
          return {
            ok: true,
            data: { triggered: false, reason: "disabled", discovered: 0, saved: 0, pending: 0 },
          };
        }
        return {
          ok: true,
          data: {
            triggered: true,
            discovered: demoSignals.length,
            saved: 0,
            pending: demoSignals.length,
            jobId: demoIntake.id,
          },
        };
      case "capture_settings":
        return { ok: true, data: demoCapture };
      case "capture_set_capability": {
        const kind = String(payload.kind ?? "");
        const enabled = Boolean(payload.enabled);
        demoCapture = {
          ...demoCapture,
          capabilities: demoCapture.capabilities.map((item) =>
            item.kind === kind
              ? {
                  ...item,
                  enabled,
                  consentedAt: enabled ? new Date().toISOString() : item.consentedAt,
                }
              : item,
          ),
        };
        return { ok: true, data: demoCapture };
      }
      case "capture_set_paused":
        demoCapture = { ...demoCapture, paused: Boolean(payload.paused) };
        return { ok: true, data: demoCapture };
      case "capture_set_redaction": {
        const terms = Array.isArray(payload.terms) ? payload.terms.length : 0;
        demoCapture = {
          ...demoCapture,
          redactionEnabled: Boolean(payload.enabled),
          redactionTerms: terms,
        };
        return { ok: true, data: demoCapture };
      }
      case "capture_set_dedup":
        demoCapture = {
          ...demoCapture,
          dedupSeconds: Number(payload.seconds ?? demoCapture.dedupSeconds),
        };
        return { ok: true, data: demoCapture };
      case "capture_set_watch_roots": {
        // 与外壳一致：只翻转可用性，不动用户已给出的开启同意（同意与否有其审计含义）。
        const paths = Array.isArray(payload.paths)
          ? payload.paths.map((item) => String(item))
          : [];
        demoCapture = {
          ...demoCapture,
          watchRoots: paths,
          capabilities: demoCapture.capabilities.map((item) =>
            item.kind === "file"
              ? { ...item, available: paths.length > 0 }
              : item,
          ),
        };
        return { ok: true, data: demoCapture };
      }
      case "capture_collect": {
        if (demoCapture.paused) {
          return {
            ok: true,
            data: {
              paused: true,
              polled: 0,
              written: 0,
              skippedDisabled: 0,
              skippedDuplicate: 0,
              redacted: 0,
              dropped: 0,
              errors: 0,
            },
          };
        }
        const enabled = demoCapture.capabilities.filter((item) => item.enabled).length;
        return {
          ok: true,
          data: {
            paused: false,
            polled: enabled,
            written: enabled,
            skippedDisabled: demoCapture.capabilities.length - enabled,
            skippedDuplicate: 0,
            redacted: 0,
            dropped: 0,
            errors: 0,
          },
        };
      }
      case "capture_events": {
        const kind = typeof payload.kind === "string" ? payload.kind : undefined;
        const limit = typeof payload.limit === "number" ? payload.limit : 100;
        return {
          ok: true,
          data: demoCaptureEvents
            .filter((event) => !kind || event.kind === kind)
            .slice(0, limit),
        };
      }
      case "capture_summaries": {
        const eventId = String(payload.eventId ?? "");
        return {
          ok: true,
          data: DEMO_CAPTURE_SUMMARIES.filter((summary) => summary.eventId === eventId),
        };
      }
      case "capture_delete_event": {
        const eventId = String(payload.eventId ?? "");
        const before = demoCaptureEvents.length;
        demoCaptureEvents = demoCaptureEvents.filter((event) => event.id !== eventId);
        return { ok: true, data: demoCaptureEvents.length < before };
      }
      case "capture_audit":
        return { ok: true, data: DEMO_CAPTURE_AUDIT };
      case "kb_sources":
        return { ok: true, data: demoKbSources };
      case "kb_add_source": {
        const path = String(payload.path ?? "").trim();
        const existing = demoKbSources.find((source) => source.path === path);
        if (existing) {
          return { ok: true, data: existing };
        }
        const created: KbSourceView = {
          id: `kb-src-demo-${demoKbSources.length + 1}`,
          path,
          available: true,
          paused: false,
          lastScanAt: null,
          lastSuccessAt: null,
          docCount: 0,
        };
        demoKbSources = [...demoKbSources, created];
        return { ok: true, data: created };
      }
      case "kb_remove_source": {
        const sourceId = String(payload.sourceId ?? "");
        const before = demoKbSources.length;
        demoKbSources = demoKbSources.filter((source) => source.id !== sourceId);
        demoKbDocuments = demoKbDocuments.filter((doc) => doc.sourceId !== sourceId);
        return { ok: true, data: demoKbSources.length < before };
      }
      case "kb_scan": {
        const target = typeof payload.sourceId === "string" ? payload.sourceId : undefined;
        const outcomes = demoKbSources
          .filter((source) => !target || source.id === target)
          .map((source) => {
            const scanned = source.docCount;
            return {
              sourceId: source.id,
              available: source.available,
              scanned: source.available ? scanned : 0,
              added: 0,
              updated: source.available ? scanned : 0,
              removed: 0,
              skipped: 0,
              reason: source.available ? null : "source_unavailable",
            };
          });
        return { ok: true, data: outcomes };
      }
      case "kb_documents": {
        const sourceId = typeof payload.sourceId === "string" ? payload.sourceId : undefined;
        const domain = typeof payload.domain === "string" ? payload.domain : undefined;
        const topicId = typeof payload.topicId === "string" ? payload.topicId : undefined;
        const limit = typeof payload.limit === "number" ? payload.limit : 200;
        return {
          ok: true,
          data: demoKbDocuments
            .filter(
              (doc) =>
                (!sourceId || doc.sourceId === sourceId) &&
                (!domain || doc.domain === domain) &&
                (!topicId || doc.topicId === topicId),
            )
            .slice(0, limit),
        };
      }
      case "kb_search": {
        const query = String(payload.query ?? "").trim();
        if (!query) {
          return { ok: true, data: [] };
        }
        const matches = demoKbDocuments.filter(
          (doc) =>
            doc.normalizedName.includes(query) ||
            doc.topicName.includes(query) ||
            doc.path.includes(query),
        );
        return {
          ok: true,
          data: matches.map((document) => ({
            document,
            matchedBy: query.length >= 3 ? "fts" : "like",
          })),
        };
      }
      case "kb_overview":
        return { ok: true, data: demoOverview() };
      case "self_readiness":
        return { ok: true, data: demoSelfReadiness };
      case "self_draft":
        return { ok: true, data: demoSelfDetail };
      case "self_start": {
        demoSelfDetail = {
          ...DEMO_SELF_DETAIL,
          draft: { ...DEMO_SELF_DETAIL.draft },
          items: DEMO_SELF_DETAIL.items.map((item) => ({
            ...item,
            status: "pending",
            statusLabel: "待确认",
          })),
          pendingCount: DEMO_SELF_DETAIL.items.length,
          acceptedCount: 0,
          rejectedCount: 0,
        };
        demoSelfReadiness = { ...demoSelfReadiness, latestDraft: demoSelfDetail.draft };
        return { ok: true, data: demoSelfDetail };
      }
      case "self_decide": {
        if (!demoSelfDetail) {
          return { ok: false, code: "E_NOT_FOUND", message: "还没有自我蒸馏草稿" };
        }
        const itemId = String(payload.itemId ?? "");
        const accepted = payload.accepted === true;
        const items = demoSelfDetail.items.map((item) =>
          item.id === itemId
            ? {
                ...item,
                status: accepted ? "accepted" : "rejected",
                statusLabel: accepted ? "已采纳" : "已剔除",
              }
            : item,
        );
        demoSelfDetail = {
          ...demoSelfDetail,
          items,
          pendingCount: items.filter((item) => item.status === "pending").length,
          acceptedCount: items.filter((item) => item.status === "accepted").length,
          rejectedCount: items.filter((item) => item.status === "rejected").length,
        };
        return { ok: true, data: demoSelfDetail };
      }
      case "self_install": {
        if (!demoSelfDetail) {
          return { ok: false, code: "E_NOT_FOUND", message: "还没有自我蒸馏草稿" };
        }
        const version = demoSelfReadiness.currentVersion + 1;
        const acceptedCount = demoSelfDetail.acceptedCount;
        demoSelfDetail = {
          ...demoSelfDetail,
          draft: { ...demoSelfDetail.draft, status: "installed", note: `已安装为我 v${version}` },
        };
        demoSelfReadiness = {
          ...demoSelfReadiness,
          installed: true,
          seatEnabled: true,
          currentVersion: version,
          latestDraft: demoSelfDetail.draft,
        };
        return {
          ok: true,
          data: {
            masterId: "self",
            version,
            unitCount: acceptedCount,
            corpusCount: 1,
            created: version === 1,
            diff: { added: [], updated: [], carried: 0, sourceRefs: [] },
          },
        };
      }
      case "self_set_seat": {
        const enabled = payload.enabled === true;
        demoSelfReadiness = { ...demoSelfReadiness, seatEnabled: enabled };
        return { ok: true, data: demoSelfReadiness };
      }
      case "data_scope":
        return { ok: true, data: DEMO_DATA_SCOPE };
      case "data_export": {
        const path =
          typeof payload.path === "string" && payload.path.trim()
            ? payload.path
            : "D:/思想熔炉/exports/forge-archive-demo.json";
        const event: DataEventView = {
          id: `data-demo-export-${demoDataEvents.length + 1}`,
          kind: "export",
          kindLabel: "导出",
          scope: "all",
          tableCount: DEMO_DATA_SCOPE.tableCount,
          rowCount: DEMO_DATA_SCOPE.rowCount,
          location: path,
          createdAt: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
        };
        demoDataEvents = [event, ...demoDataEvents];
        return {
          ok: true,
          data: {
            path,
            tableCount: DEMO_DATA_SCOPE.tableCount,
            rowCount: DEMO_DATA_SCOPE.rowCount,
            bytes: 262_144,
            createdAt: event.createdAt,
          },
        };
      }
      case "data_purge": {
        if (payload.confirm !== true) {
          return { ok: false, code: "E_INVALID_INPUT", message: "清除数据需要显式确认" };
        }
        const createdAt = new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
        demoDataEvents = [
          {
            id: `data-demo-purge-${demoDataEvents.length + 1}`,
            kind: "purge",
            kindLabel: "清除",
            scope: "all",
            tableCount: DEMO_DATA_SCOPE.tableCount,
            rowCount: DEMO_DATA_SCOPE.rowCount,
            location: "",
            createdAt,
          },
        ];
        demoSelfDetail = null;
        demoSelfReadiness = {
          ...DEMO_SELF_READINESS,
          recordCount: 0,
          installed: false,
          seatEnabled: false,
          currentVersion: 0,
          latestDraft: null,
        };
        return {
          ok: true,
          data: {
            scope: "all",
            tableCount: DEMO_DATA_SCOPE.tableCount,
            rowCount: DEMO_DATA_SCOPE.rowCount,
            createdAt,
          },
        };
      }
      case "data_events":
        return { ok: true, data: demoDataEvents };
      case "asset_roots":
        return { ok: true, data: demoAssetRoots };
      case "asset_add_root": {
        const path = String(payload.path ?? "").trim();
        if (!path) {
          return { ok: false, code: "E_INVALID_INPUT", message: "Skill 根目录路径不能为空" };
        }
        const existing = demoAssetRoots.find((root) => root.path === path);
        if (existing) {
          return { ok: true, data: existing };
        }
        const created: AssetRootView = {
          id: `asset-root-demo-${demoAssetRoots.length + 1}`,
          path,
          available: true,
          lastScanAt: null,
          skillCount: 0,
        };
        demoAssetRoots = [...demoAssetRoots, created];
        return { ok: true, data: created };
      }
      case "asset_remove_root": {
        const rootId = String(payload.rootId ?? "");
        const before = demoAssetRoots.length;
        demoAssetRoots = demoAssetRoots.filter((root) => root.id !== rootId);
        demoAssetSkills = demoAssetSkills.filter((skill) => skill.rootId !== rootId);
        return { ok: true, data: demoAssetRoots.length < before };
      }
      case "asset_scan": {
        const target = typeof payload.rootId === "string" ? payload.rootId : undefined;
        const outcomes: AssetScanOutcome[] = demoAssetRoots
          .filter((root) => !target || root.id === target)
          .map((root) => {
            const skills = demoAssetSkills.filter((skill) => skill.rootId === root.id);
            return {
              rootId: root.id,
              rootPath: root.path,
              available: root.available,
              scanned: root.available ? skills.length : 0,
              added: 0,
              updated: root.available ? skills.length : 0,
              removed: 0,
              needsRepair: root.available
                ? skills.filter((skill) => skill.needsRepair).length
                : 0,
              reason: root.available ? null : "root_unavailable",
            };
          });
        return { ok: true, data: outcomes };
      }
      case "asset_skills": {
        const rootId = typeof payload.rootId === "string" ? payload.rootId : undefined;
        const category = typeof payload.category === "string" ? payload.category : undefined;
        const tag = typeof payload.tag === "string" ? payload.tag : undefined;
        const enabled = typeof payload.enabled === "boolean" ? payload.enabled : undefined;
        const needsRepair =
          typeof payload.needsRepair === "boolean" ? payload.needsRepair : undefined;
        const query = typeof payload.query === "string" ? payload.query.trim() : "";
        const limit = typeof payload.limit === "number" ? payload.limit : 200;
        return {
          ok: true,
          data: demoAssetSkills
            .filter(
              (skill) =>
                (!rootId || skill.rootId === rootId) &&
                (!category || skill.category === category) &&
                (!tag || skill.tags.includes(tag)) &&
                (enabled === undefined || skill.enabled === enabled) &&
                (needsRepair === undefined || skill.needsRepair === needsRepair) &&
                (!query ||
                  skill.name.includes(query) ||
                  skill.description.includes(query) ||
                  skill.path.includes(query)),
            )
            .slice(0, limit),
        };
      }
      case "asset_skill_detail": {
        const skillId = String(payload.skillId ?? "");
        const skill = demoAssetSkills.find((item) => item.id === skillId);
        if (!skill) {
          return { ok: false, code: "E_NOT_FOUND", message: `Skill 不存在：${skillId}` };
        }
        const detail: SkillDetail = {
          skill,
          dependencies: DEMO_SKILL_DEPS[skill.id] ?? [],
          manifestExcerpt: skill.needsRepair
            ? ""
            : JSON.stringify(
                {
                  name: skill.name,
                  description: skill.description,
                  category: skill.category,
                  version: skill.version,
                },
                null,
                2,
              ),
        };
        return { ok: true, data: detail };
      }
      case "asset_summary":
        return { ok: true, data: demoAssetSummary() };
      case "cost_summary":
        return { ok: true, data: demoCostSummary(Number(payload.days ?? 30)) };
      case "cost_estimate": {
        const seats = Number(payload.seats ?? 6);
        const rounds = Number(payload.rounds ?? demoTuningValue("council.max_rounds", "3"));
        const searches = Number(payload.searches ?? seats + 1);
        return { ok: true, data: demoCostEstimate(seats, rounds, searches) };
      }
      case "backup_list":
        return {
          ok: true,
          data: demoBackups.slice(0, Math.max(1, Number(payload.limit ?? 50))),
        };
      case "backup_create": {
        const kind = String(payload.kind ?? "manual");
        const now = new Date().toISOString();
        const backup: BackupView = {
          id: `backup-demo-${demoBackups.length + 1}`,
          path: `/data/thought-forge/backups/thought-forge-${kind}-${now.replace(/[:.]/g, "")}.sqlite3`,
          sizeBytes: 6_400_000,
          checksum: "demo0000000000000000000000000000",
          schemaVersion: 14,
          kind,
          present: true,
          createdAt: now,
        };
        demoBackups = [backup, ...demoBackups];
        const outcome: BackupOutcome = {
          path: backup.path,
          sizeBytes: backup.sizeBytes,
          checksum: backup.checksum,
          schemaVersion: backup.schemaVersion,
          kind: backup.kind,
          createdAt: backup.createdAt,
        };
        return { ok: true, data: outcome };
      }
      case "backup_restore": {
        const path = String(payload.path ?? "");
        const backup = demoBackups.find((item) => item.path === path);
        if (!backup) {
          return { ok: false, code: "E_NOT_FOUND", message: `备份文件不存在：${path}` };
        }
        if (!backup.present) {
          return { ok: false, code: "E_INVALID_INPUT", message: "备份已按保留策略移除" };
        }
        const outcome: BackupOutcome = {
          path: backup.path,
          sizeBytes: backup.sizeBytes,
          checksum: backup.checksum,
          schemaVersion: backup.schemaVersion,
          kind: backup.kind,
          createdAt: backup.createdAt,
        };
        return { ok: true, data: outcome };
      }
      case "credential_set": {
        const scope = String(payload.scope ?? "");
        const ownerId = String(payload.ownerId ?? "");
        const secret = String(payload.secret ?? "");
        if (!ownerId || !secret) {
          return { ok: false, code: "E_INVALID_INPUT", message: "归属标识与密钥都不能为空" };
        }
        const refName = `thought-forge/${scope}/${ownerId}`;
        demoCredentials.add(refName);
        const now = new Date().toISOString();
        const view: CredentialRefView = {
          id: `cred-demo-${demoCredentials.size}`,
          refName,
          scope,
          ownerId,
          createdAt: now,
          updatedAt: now,
        };
        return { ok: true, data: view };
      }
      case "credential_status": {
        const refName = `thought-forge/${String(payload.scope ?? "")}/${String(payload.ownerId ?? "")}`;
        return { ok: true, data: demoCredentials.has(refName) };
      }
      case "model_probe":
        return { ok: true, data: DEMO_PROBE };
      default:
        return {
          ok: false,
          code: "E_NOT_FOUND",
          message: `未注册的命令：${name}`,
        };
    }
  },
};
