import { useCallback, useEffect, useState } from "react";
import { RealmShell } from "./RealmShell";
import { useCommand, useCommands } from "../app/ipc";
import { ThemeToggle } from "../components/ThemeToggle";
import type { ThemeName } from "../app/theme";
import type { Preferences } from "../app/preferences";
import type {
  BackupOutcome,
  BackupView,
  CaptureAuditView,
  CaptureEventView,
  CaptureKindKey,
  CaptureSettingsView,
  CollisionOutcome,
  CompanionSettings,
  ConsolidationReport,
  ConsolidationRun,
  ConnectorCallView,
  ConnectorTestOutcome,
  ConnectorView,
  CostEstimate,
  CostSummary,
  DataEventView,
  DataScope,
  ExportOutcome,
  LlmCall,
  MasterSummary,
  ModelProbeOutcome,
  PlatformView,
  PrincipleSeal,
  RingOverview,
  SelfDraftDetail,
  SelfItemView,
  SelfReadiness,
  ThoughtRecord,
  TopicView,
  TuningItem,
} from "../ipc/commands";
import { LayerGlyph } from "../components/LayerGlyph";
import { layerOf } from "../domain/layers";
import {
  callPurposeLabel,
  callStatusLabel,
  captureExcerpt,
  captureKindLabel,
  consolidationModeLabel,
  connectorStatusLabel,
  formatBytes,
  formatDuration,
  formatMoney,
  formatTime,
  platformStatusLabel,
  queryModeLabel,
  readableError,
  sourceKindLabel,
} from "../domain/labels";

const CONNECTOR_KINDS: readonly { readonly kind: string; readonly label: string }[] = [
  { kind: "search", label: "搜索" },
  { kind: "page", label: "网页阅读" },
  { kind: "mcp", label: "外部工具" },
];

/** 各类型外部数据源地址的样例，避免用户不知道要填什么格式。 */
const CONNECTOR_PLACEHOLDERS: Record<string, string> = {
  search: "搜索服务地址，如 http://localhost:8080",
  page: "要读取的网页地址，如 https://example.com/post",
  mcp: "外部工具服务地址，如 https://example.com/mcp",
};

const CREDENTIAL_SCOPES: readonly { readonly scope: string; readonly label: string }[] = [
  { scope: "platform", label: "模型平台" },
  { scope: "connector", label: "外部数据源" },
];

/** 平台表单草稿。单价按元 / 千 token 填写，内核按百万分之一元记账。 */
type PlatformDraft = {
  code: string;
  displayName: string;
  endpoint: string;
  modelName: string;
  inputPrice: string;
  outputPrice: string;
  currency: string;
};

const EMPTY_PLATFORM_DRAFT: PlatformDraft = {
  code: "",
  displayName: "",
  endpoint: "",
  modelName: "",
  inputPrice: "",
  outputPrice: "",
  currency: "CNY",
};

/** 单价留空按零计；非数字或负数判为填写有误。 */
function parsePrice(value: string): number | null {
  const text = value.trim();
  if (!text) {
    return 0;
  }
  const amount = Number(text);
  if (!Number.isFinite(amount) || amount < 0) {
    return null;
  }
  return Math.round(amount * 1_000_000);
}

/** 已有单价回填成元的写法；零价不占位。 */
function priceToInput(micros: number): string {
  return micros > 0 ? String(micros / 1_000_000) : "";
}

/** 探针退出码对应的可读原因，便于照着结论去修。 */
const PROBE_EXIT_LABEL: Record<number, string> = {
  0: "连通正常",
  1: "模型不可用",
  2: "联网能力没开",
  3: "还没配置平台",
};

/** 体检清单的一项。required 为真表示它是跑通会诊的门槛。 */
type CheckupItem = {
  readonly key: string;
  readonly label: string;
  readonly state: string;
  readonly hint: string;
  readonly ok: boolean;
  readonly required: boolean;
  /** 「去配置」的落点：同页给分区 id，大师包给 "vault"（在另一个境界）。 */
  readonly target: string;
};

/** 数必备项里已就绪或未就绪的条数。 */
function countRequired(items: readonly CheckupItem[], ok: boolean): number {
  return items.filter((item) => item.required && item.ok === ok).length;
}

/** 历史回顾的合并时间轴：各来源的列表本身是截断的，这里只回顾最近发生的事。 */
type HistoryEntry = {
  readonly id: string;
  readonly at: string;
  readonly source: string;
  readonly detail: string;
};

const COST_POLICY_LABELS: Record<string, string> = {
  reject: "不发起这次会诊",
  reduce_rounds: "减少讨论轮数",
  reduce_seats: "减少参与人数",
};

function auditActionLabel(action: string): string {
  switch (action) {
    case "enable":
      return "开启";
    case "disable":
      return "关闭";
    case "pause":
      return "暂停";
    case "resume":
      return "恢复";
    default:
      return action;
  }
}

/**
 * 我：成长与设置。P1 阶段先接入运行信息，成长轨迹在 P5 接入。
 */
/** 「我」分两处看：成长是每天会用的内容，设置是低频的配置、用量与历史。 */
type SelfView = "growth" | "system";

/** 设置页的分区，顺序就是面板顺序；上面那排跳转按钮按这里生成。 */
const SETTING_SECTIONS: readonly { readonly id: string; readonly label: string }[] = [
  { id: "setting-checkup", label: "体检清单" },
  { id: "setting-history", label: "历史回顾" },
  { id: "setting-models", label: "联网与模型平台" },
  { id: "setting-sources", label: "外部数据源" },
  { id: "setting-usage", label: "用量" },
  { id: "setting-tuning", label: "调参" },
  { id: "setting-backup", label: "备份与恢复" },
  { id: "setting-data", label: "数据主权" },
  { id: "setting-appearance", label: "外观" },
  { id: "setting-runtime", label: "运行信息" },
];


function captureMaterialText(payload: unknown): string {
  if (typeof payload === "object" && payload !== null) {
    const record = payload as Record<string, unknown>;
    for (const key of ["text", "title", "path", "imageRef"]) {
      const value = record[key];
      if (typeof value === "string" && value.trim()) {
        return value.trim();
      }
    }
  }
  return "";
}

export function SelfRealm({
  theme,
  onThemeChange,
  preferences,
  onPreferencesChange,
  view,
  onViewChange,
  onGoVault,
}: {
  readonly theme: ThemeName;
  readonly onThemeChange: (next: ThemeName) => void;
  readonly preferences: Preferences;
  readonly onPreferencesChange: (patch: Partial<Preferences>) => void;
  /** 外壳指定看哪一处（顶部「设置」按钮直进设置）；不传就自己管。 */
  readonly view?: SelfView;
  readonly onViewChange?: (next: SelfView) => void;
  /** 体检项的「去配置」可能要换境界（大师包在「藏」），由外壳给出口。 */
  readonly onGoVault?: () => void;
}) {
  const db = useCommand("db_status", {});
  const app = useCommand("app_info", {});
  const client = useCommands();
  const [innerView, setInnerView] = useState<SelfView>("growth");
  const selfView = view ?? innerView;
  function showView(next: SelfView) {
    setInnerView(next);
    onViewChange?.(next);
  }
  const [networking, setNetworking] = useState<boolean | null>(null);
  const [platforms, setPlatforms] = useState<readonly PlatformView[]>([]);
  const [platformDraft, setPlatformDraft] = useState<PlatformDraft>(EMPTY_PLATFORM_DRAFT);
  const [platformNote, setPlatformNote] = useState<string | null>(null);
  const [probe, setProbe] = useState<ModelProbeOutcome | null>(null);
  const [probeNote, setProbeNote] = useState<string | null>(null);
  const [calls, setCalls] = useState<readonly LlmCall[]>([]);
  const [records, setRecords] = useState<readonly ThoughtRecord[]>([]);
  const [chain, setChain] = useState<readonly ThoughtRecord[]>([]);
  const [chainTopic, setChainTopic] = useState<string | null>(null);
  const [runs, setRuns] = useState<readonly ConsolidationRun[]>([]);
  const [report, setReport] = useState<ConsolidationReport | null>(null);
  const [companion, setCompanion] = useState<CompanionSettings | null>(null);
  const [topics, setTopics] = useState<readonly TopicView[]>([]);
  const [principles, setPrinciples] = useState<readonly PrincipleSeal[]>([]);
  const [ring, setRing] = useState<RingOverview | null>(null);
  const [capture, setCapture] = useState<CaptureSettingsView | null>(null);
  const [captures, setCaptures] = useState<readonly CaptureEventView[]>([]);
  const [intakeDone, setIntakeDone] = useState<ReadonlySet<string>>(() => new Set());
  const [collideDraft, setCollideDraft] = useState("");
  const [collision, setCollision] = useState<CollisionOutcome | null>(null);
  const [captureAudit, setCaptureAudit] = useState<readonly CaptureAuditView[]>([]);
  const [captureKind, setCaptureKind] = useState<CaptureKindKey | "all">("all");
  const [watchRootDraft, setWatchRootDraft] = useState("");
  const [self, setSelf] = useState<SelfReadiness | null>(null);
  const [draft, setDraft] = useState<SelfDraftDetail | null>(null);
  const [dataScope, setDataScope] = useState<DataScope | null>(null);
  const [dataEvents, setDataEvents] = useState<readonly DataEventView[]>([]);
  const [exported, setExported] = useState<ExportOutcome | null>(null);
  const [purgeArmed, setPurgeArmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [mirrorNote, setMirrorNote] = useState<string | null>(null);
  const [dataNote, setDataNote] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [tuning, setTuning] = useState<readonly TuningItem[]>([]);
  const [tuningDraft, setTuningDraft] = useState<Record<string, string>>({});
  const [tuningNote, setTuningNote] = useState<string | null>(null);
  const [connectors, setConnectors] = useState<readonly ConnectorView[]>([]);
  const [connectorCalls, setConnectorCalls] = useState<readonly ConnectorCallView[]>([]);
  const [connectorKind, setConnectorKind] = useState<string>("search");
  const [connectorName, setConnectorName] = useState("");
  const [connectorEndpoint, setConnectorEndpoint] = useState("");
  const [connectorNote, setConnectorNote] = useState<string | null>(null);
  const [connectorProbe, setConnectorProbe] = useState<{
    readonly connectorId: string;
    readonly outcome: ConnectorTestOutcome;
  } | null>(null);
  const [cost, setCost] = useState<CostSummary | null>(null);
  const [estimate, setEstimate] = useState<CostEstimate | null>(null);
  const [backups, setBackups] = useState<readonly BackupView[]>([]);
  const [masters, setMasters] = useState<readonly MasterSummary[]>([]);
  const [backupNote, setBackupNote] = useState<string | null>(null);
  const [credentialScope, setCredentialScope] = useState("platform");
  const [credentialOwner, setCredentialOwner] = useState("");
  const [credentialSecret, setCredentialSecret] = useState("");
  const [credentialKnown, setCredentialKnown] = useState<Record<string, boolean>>({});
  const [credentialNote, setCredentialNote] = useState<string | null>(null);
  const [revokeTarget, setRevokeTarget] = useState<PrincipleSeal | null>(null);
  const [revokeReason, setRevokeReason] = useState("");
  const [revoked, setRevoked] = useState<
    readonly { readonly nodeId: string; readonly content: string; readonly reason: string; readonly at: string }[]
  >([]);

  const refresh = useCallback(async () => {
    const [enabled, platformList, recent, recordList, runList, companionState, topicList, sealList, overview, captureState, captureList, captureLog, selfState, draftDetail, scope, events, connectorList, connectorLog, costState, costEstimate, backupList, masterList] = await Promise.all([
      client.call("networking_get", {}),
      client.call("platform_list", {}),
      client.call("llm_calls", { limit: 5 }),
      client.call("records_list", { limit: 20 }),
      client.call("consolidate_runs", { limit: 5 }),
      client.call("companion_settings", {}),
      client.call("topics_list", { limit: 10 }),
      client.call("principles_list", { limit: 12 }),
      client.call("ring_overview", {}),
      client.call("capture_settings", {}),
      client.call("capture_events", { limit: 40 }),
      client.call("capture_audit", { limit: 12 }),
      client.call("self_readiness", {}),
      client.call("self_draft", {}),
      client.call("data_scope", {}),
      client.call("data_events", { limit: 12 }),
      client.call("connector_list", {}),
      client.call("connector_calls", { limit: 12 }),
      client.call("cost_summary", { days: 30 }),
      client.call("cost_estimate", {}),
      client.call("backup_list", { limit: 10 }),
      client.call("master_list", {}),
    ]);
    setNetworking(enabled);
    setPlatforms(platformList);
    setCalls(recent);
    setRecords(recordList);
    setRuns(runList);
    setCompanion(companionState);
    setTopics(topicList);
    setPrinciples(sealList);
    setRing(overview);
    setCapture(captureState);
    setCaptures(captureList);
    setCaptureAudit(captureLog);
    setSelf(selfState);
    setDraft(draftDetail);
    setDataScope(scope);
    setDataEvents(events);
    setConnectors(connectorList);
    setConnectorCalls(connectorLog);
    setCost(costState);
    setEstimate(costEstimate);
    setBackups(backupList);
    setMasters(masterList);

    // 体检清单要按平台代码逐个数密钥；密钥库不可用只是这一项未知，不能把整页读取带崩。
    const knownKeys = await Promise.all(
      platformList.map(
        async (platform) =>
          [
            `platform:${platform.code}`,
            await client
              .call("credential_status", { scope: "platform", ownerId: platform.code })
              .catch(() => false),
          ] as const,
      ),
    );
    setCredentialKnown((current) => ({ ...current, ...Object.fromEntries(knownKeys) }));
  }, [client]);

  useEffect(() => {
    void refresh().catch(() => setNote("模型平台配置读取失败"));
  }, [refresh]);

  useEffect(() => {
    let alive = true;
    client
      .call("tuning_get", {})
      .then((items) => {
        if (alive) {
          setTuning(items);
        }
      })
      .catch(() => {
        if (alive) {
          setTuningNote("调参读取失败");
        }
      });
    return () => {
      alive = false;
    };
  }, [client]);

  function draftOf(item: TuningItem): string {
    return tuningDraft[item.key] ?? item.value;
  }

  const [diagnosticsNote, setDiagnosticsNote] = useState<string | null>(null);

  /**
   * 诊断包：把体检与配置摘要写成 JSON 交给排查。
   * 密钥只记「有没有写入」，不写密钥本体。
   */
  async function exportDiagnostics() {
    setDiagnosticsNote(null);
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    const summary = {
      generatedAt: new Date().toISOString(),
      app: {
        version: app.data?.version ?? null,
        schemaVersion: db.data?.schemaVersion ?? null,
        journalMode: db.data?.journalMode ?? null,
      },
      networking,
      checkup: checkupItems(),
      masters: masters.length,
      backups: backups.filter((item) => item.present).length,
      platforms: platforms.map((item) => ({
        code: item.code,
        enabled: item.enabled,
        keyWritten: credentialKnown[`platform:${item.code}`] === true,
      })),
      capture: capture
        ? {
            paused: capture.paused,
            capabilities: capture.capabilities.map((item) => ({
              kind: item.kind,
              enabled: item.enabled,
              available: item.available,
            })),
          }
        : null,
      cost: cost
        ? {
            currency: cost.currency,
            todayMicros: cost.todayMicros,
            monthMicros: cost.monthMicros,
            dailyLimitMicros: cost.dailyLimitMicros,
            monthlyLimitMicros: cost.monthlyLimitMicros,
          }
        : null,
      recentCalls: calls.length,
    };
    try {
      const result = await client.call("diagnostics_write", {
        fileName: `diagnostics-${stamp}.json`,
        content: JSON.stringify(summary, null, 2),
      });
      setDiagnosticsNote(`已写出 ${result.bytes} 字节：${result.path}`);
    } catch (cause) {
      setDiagnosticsNote(cause instanceof Error ? cause.message : "诊断包导出失败");
    }
  }

  /** 体检项的「去配置」：同页分区滚过去，大师包在别的境界，交给外层换境界。 */
  function goToCheckupTarget(target: string) {
    if (target === "vault") {
      onGoVault?.();
      return;
    }
    document.getElementById(target)?.scrollIntoView?.({ block: "start" });
  }

  /** 体检清单：必备项决定能不能跑通会诊，其余只是提醒。 */
  function checkupItems(): readonly CheckupItem[] {
    const enabledPlatforms = platforms.filter((item) => item.enabled);
    const missingKeys = enabledPlatforms.filter(
      (item) => !credentialKnown[`platform:${item.code}`],
    );
    const openCaptures = capture
      ? capture.capabilities.filter((item) => item.enabled && item.available).length
      : 0;
    const presentBackups = backups.filter((item) => item.present).length;
    return [
      {
        key: "networking",
        label: "联网能力",
        ok: networking === true,
        required: true,
        state: networking === true ? "已开启" : "已关闭",
        hint:
          networking === true
            ? "每次提问都会留下记录，费用也按这里记账"
            : "关着的时候，模型与外部数据源都调不动",
        target: "setting-models",
      },
      {
        key: "platform",
        label: "模型平台",
        ok: enabledPlatforms.length > 0,
        required: true,
        state:
          platforms.length === 0
            ? "还没配置"
            : enabledPlatforms.length > 0
              ? `已启用 ${enabledPlatforms.length} 个`
              : "已配置，未启用",
        hint:
          platforms.length === 0
            ? "在下面「大模型」里填服务地址与模型名"
            : "确认要用的那个已经启用",
        target: "setting-models",
      },
      {
        key: "credential",
        label: "平台密钥",
        ok: enabledPlatforms.length > 0 && missingKeys.length === 0,
        required: true,
        state:
          enabledPlatforms.length === 0
            ? "等启用平台后再看"
            : missingKeys.length === 0
              ? "已写入"
              : `缺 ${missingKeys.map((item) => item.code).join("、")}`,
        hint: "密钥存系统密钥库，条目名按 thought-forge/platform/平台代码",
        target: "setting-models",
      },
      {
        key: "masters",
        label: "大师包",
        ok: masters.length > 0,
        required: true,
        state: masters.length > 0 ? `已装 ${masters.length} 位` : "未安装",
        hint:
          masters.length > 0
            ? "会诊的可选席位来自这里"
            : "去「藏」境界点「安装种子大师包」",
        target: "vault",
      },
      {
        key: "capture",
        label: "采集",
        ok: openCaptures > 0,
        required: false,
        state: capture
          ? capture.paused
            ? `暂停中（已开 ${openCaptures} 项）`
            : openCaptures > 0
              ? `已开 ${openCaptures} 项`
              : "全关"
          : "读取中",
        hint: "默认全关。要用再开，不用就关掉，少攒无关数据",
        target: "setting-usage",
      },
      {
        key: "backup",
        label: "备份",
        ok: presentBackups > 0,
        required: false,
        state: presentBackups > 0 ? `有 ${presentBackups} 份` : "还没有",
        hint: "动手实测前先备一份，出问题能退回来",
        target: "setting-backup",
      },
    ];
  }

  const checkup = checkupItems();
  const presentBackups = backups.filter((item) => item.present).length;
  const history: readonly HistoryEntry[] = [
    ...captures.map((item) => ({
      id: `capture-${item.id}`,
      at: item.occurredAt,
      source: "采集",
      detail: `${captureKindLabel(item.kind)} · ${item.sourceApp || "来源未知"}`,
    })),
    ...calls.map((item) => ({
      id: `call-${item.id}`,
      at: item.createdAt,
      source: "模型调用",
      detail: `${callPurposeLabel(item.purpose)} · ${callStatusLabel(item.status)} · ${
        item.modelName || item.platformCode
      }`,
    })),
    ...backups.map((item) => ({
      id: `backup-${item.id}`,
      at: item.createdAt,
      source: "备份",
      detail: `${formatBytes(item.sizeBytes)}${item.present ? "" : " · 文件已不在原处"}`,
    })),
    ...dataEvents.map((item) => ({
      id: `data-${item.id}`,
      at: item.createdAt,
      source: "数据留痕",
      detail: `${item.kindLabel} · ${item.rowCount} 条`,
    })),
  ]
    .sort((left, right) => (left.at < right.at ? 1 : left.at > right.at ? -1 : 0))
    .slice(0, 12);

  /** 只有越界或格式不对才算无效，交给用户明确看到问题再改。 */
  function invalidOf(item: TuningItem): boolean {
    const value = draftOf(item).trim();
    if (item.kind === "int" || item.kind === "float") {
      const parsed = Number(value);
      if (!Number.isFinite(parsed)) {
        return true;
      }
      return parsed < item.min || parsed > item.max;
    }
    if (item.kind === "bool") {
      return value !== "true" && value !== "false";
    }
    return value.length === 0;
  }

  const pendingChanges = tuning
    .filter((item) => draftOf(item).trim() !== item.value)
    .map((item) => [item.key, draftOf(item).trim()] as const);

  async function saveTuning() {
    if (pendingChanges.length === 0) {
      setTuningNote("没有需要保存的改动");
      return;
    }
    setTuningNote(null);
    try {
      const saved = await client.call("tuning_set", {
        values: pendingChanges.map(([key, value]) => [key, value] as const),
      });
      setTuning(saved);
      setTuningDraft({});
      setTuningNote("已保存");
    } catch (cause) {
      setTuningNote(cause instanceof Error ? cause.message : "调参保存失败");
    }
  }

  async function toggleNetworking(next: boolean) {
    setNote(null);
    try {
      setNetworking(await client.call("networking_set", { enabled: next }));
    } catch {
      setNote("联网能力切换失败");
    }
  }

  async function togglePlatform(platform: PlatformView) {
    setNote(null);
    try {
      await client.call("platform_enable", {
        code: platform.code,
        enabled: !platform.enabled,
      });
      setPlatforms(await client.call("platform_list", {}));
    } catch (cause) {
      setNote(cause instanceof Error ? cause.message : "平台配置更新失败");
    }
  }

  function patchPlatformDraft(patch: Partial<PlatformDraft>) {
    setPlatformDraft((current) => ({ ...current, ...patch }));
  }

  /** 编辑已有平台：把现值填进表单，再保存即覆盖同代码的记录。 */
  function editPlatform(platform: PlatformView) {
    setPlatformDraft({
      code: platform.code,
      displayName: platform.displayName,
      endpoint: platform.endpoint,
      modelName: platform.modelName,
      inputPrice: priceToInput(platform.inputPriceMicrosPer1k),
      outputPrice: priceToInput(platform.outputPriceMicrosPer1k),
      currency: platform.currency || "CNY",
    });
    setPlatformNote(null);
  }

  async function savePlatform() {
    const code = platformDraft.code.trim();
    const endpoint = platformDraft.endpoint.trim();
    const modelName = platformDraft.modelName.trim();
    if (!code) {
      setPlatformNote("先填平台代码，密钥条目按它命名");
      return;
    }
    if (!endpoint || !modelName) {
      setPlatformNote("服务地址与模型名都填上才能保存");
      return;
    }
    const inputPrice = parsePrice(platformDraft.inputPrice);
    const outputPrice = parsePrice(platformDraft.outputPrice);
    if (inputPrice === null || outputPrice === null) {
      setPlatformNote("单价只能填非负数字，不清楚就留空");
      return;
    }
    setPlatformNote(null);
    setBusy(true);
    try {
      const saved = await client.call("platform_upsert", {
        code,
        displayName: platformDraft.displayName.trim() || code,
        endpoint,
        modelName,
        inputPriceMicrosPer1k: inputPrice,
        outputPriceMicrosPer1k: outputPrice,
        currency: platformDraft.currency.trim() || "CNY",
      });
      setPlatforms((current) => [
        ...current.filter((item) => item.code !== saved.code),
        saved,
      ]);
      setPlatformNote(
        `已保存 ${saved.displayName}。密钥按代码 ${saved.code} 写进下面的「密钥」里。`,
      );
    } catch (cause) {
      setPlatformNote(cause instanceof Error ? cause.message : "平台没能保存");
    } finally {
      setBusy(false);
    }
  }

  /** 外壳自检：用一次最小调用判定当前平台是否真的能连上。 */
  async function probeModel() {
    setProbeNote(null);
    try {
      setProbe(await client.call("model_probe", {}));
    } catch (cause) {
      setProbe(null);
      setProbeNote(cause instanceof Error ? cause.message : "探针没能执行");
    }
  }

  /** 读取连接器配置与调用审计。 */
  async function reloadConnectors() {
    const [list, log] = await Promise.all([
      client.call("connector_list", {}),
      client.call("connector_calls", { limit: 12 }),
    ]);
    setConnectors(list);
    setConnectorCalls(log);
  }

  /** 保存连接器配置；搜索类会先做一次地址预检。 */
  async function saveConnector() {
    const name = connectorName.trim();
    const endpoint = connectorEndpoint.trim();
    if (!name) {
      setConnectorNote("先给这个外部数据源起个名字");
      return;
    }
    setConnectorNote(null);
    try {
      await client.call("connector_upsert", {
        kind: connectorKind,
        displayName: name,
        endpoint,
      });
      await reloadConnectors();
      setConnectorNote("已保存");
    } catch (cause) {
      setConnectorNote(cause instanceof Error ? cause.message : "没能保存");
    }
  }

  async function toggleConnector(connector: ConnectorView) {
    setConnectorNote(null);
    try {
      await client.call("connector_enable", {
        id: connector.id,
        enabled: !connector.enabled,
      });
      await reloadConnectors();
    } catch (cause) {
      setConnectorNote(cause instanceof Error ? cause.message : "没能切换");
    }
  }

  /** 测试是否能连上。预演开启时首次点击只拿到待发送内容，确认后才真正发出请求。 */
  async function testConnector(connector: ConnectorView, confirm?: string) {
    setConnectorNote(null);
    try {
      const outcome = await client.call("connector_test", {
        id: connector.id,
        ...(confirm ? { confirm } : {}),
      });
      if (outcome.pending) {
        setConnectorProbe({ connectorId: connector.id, outcome });
        return;
      }
      setConnectorProbe(null);
      const call = outcome.call;
      setConnectorNote(
        call?.status === "ok"
          ? `已连上 · 用时 ${formatDuration(call.latencyMs)}`
          : `没能连上 · ${
              call?.errorCode ? readableError(call.errorCode) : "没有拿到结果"
            }`,
      );
      await reloadConnectors();
    } catch (cause) {
      setConnectorNote(cause instanceof Error ? cause.message : "测试连接没能完成");
    }
  }

  /** 保存密钥：先存进系统密钥库，再登记引用名。 */
  async function saveCredential() {
    const owner = credentialOwner.trim();
    if (!owner) {
      setCredentialNote("先填写这条密钥属于谁，例如平台名或外部数据源名");
      return;
    }
    if (!credentialSecret) {
      setCredentialNote("密钥不能为空");
      return;
    }
    setCredentialNote(null);
    try {
      await client.call("credential_set", {
        scope: credentialScope,
        ownerId: owner,
        secret: credentialSecret,
      });
      setCredentialSecret("");
      setCredentialKnown((current) => ({
        ...current,
        [`${credentialScope}:${owner}`]: true,
      }));
      setCredentialNote("密钥已存进系统密钥库，本应用只记住它的名字");
    } catch (cause) {
      setCredentialNote(cause instanceof Error ? cause.message : "没能保存密钥");
    }
  }

  /** 查询某个归属在系统凭据库里是否已有密钥。 */
  async function checkCredential() {
    const owner = credentialOwner.trim();
    if (!owner) {
      setCredentialNote("先填写这条密钥属于谁");
      return;
    }
    setCredentialNote(null);
    try {
      const present = await client.call("credential_status", {
        scope: credentialScope,
        ownerId: owner,
      });
      setCredentialKnown((current) => ({
        ...current,
        [`${credentialScope}:${owner}`]: present,
      }));
      setCredentialNote(present ? "该归属已配置密钥" : "该归属还没有密钥");
    } catch (cause) {
      setCredentialNote(cause instanceof Error ? cause.message : "没能读取密钥状态");
    }
  }

  /** 创建一份备份，并刷新保留策略下的备份列表。 */
  async function createBackup() {
    setBackupNote(null);
    try {
      const created: BackupOutcome = await client.call("backup_create", {});
      setBackups(await client.call("backup_list", { limit: 10 }));
      setBackupNote(
        `已创建备份 · ${formatBytes(created.sizeBytes)} · 数据格式版本 ${created.schemaVersion}`,
      );
    } catch (cause) {
      setBackupNote(cause instanceof Error ? cause.message : "没能生成备份");
    }
  }

  /** 恢复前由内核先校验完整性，校验通过才替换数据文件。 */
  async function restoreBackup(backup: BackupView) {
    setBackupNote(null);
    try {
      await client.call("backup_restore", { path: backup.path });
      setBackupNote("备份已校验并恢复，重启应用后生效");
    } catch (cause) {
      setBackupNote(cause instanceof Error ? cause.message : "没能恢复数据");
    }
  }

  async function showChain(topicKey: string) {
    setNote(null);
    try {
      setChain(await client.call("records_compare", { topicKey }));
      setChainTopic(topicKey);
    } catch {
      setNote("演化链读取失败");
    }
  }

  async function decide(
    record: ThoughtRecord,
    adopted: boolean,
    reason: string,
  ) {
    setNote(null);
    try {
      await client.call("record_decision", {
        recordId: record.id,
        adopted,
        reason,
      });
      setRecords(await client.call("records_list", { limit: 20 }));
    } catch {
      setNote("记录结论失败");
    }
  }

  async function consolidate() {
    setNote(null);
    try {
      const result = await client.call("consolidate_trigger", { mode: "manual" });
      setReport(result);
      setRuns(await client.call("consolidate_runs", { limit: 5 }));
      setRing(await client.call("ring_overview", {}));
    } catch {
      setNote("固化未能完成");
    }
  }

  async function toggleCompanion(next: boolean) {
    setNote(null);
    try {
      setCompanion(await client.call("companion_enable", { enabled: next }));
    } catch {
      setNote("主动助学切换失败");
    }
  }

  async function changeLimit(next: number) {
    setNote(null);
    try {
      setCompanion(await client.call("companion_limit", { limit: next }));
    } catch (cause) {
      setNote(cause instanceof Error ? cause.message : "每日上限设置失败");
    }
  }

  async function toggleRule(key: "triggerOnRecord" | "triggerOnCapture") {
    if (!companion) {
      return;
    }
    setNote(null);
    try {
      setCompanion(
        await client.call("companion_rules", {
          rules: { ...companion.rules, [key]: !companion.rules[key] },
        }),
      );
    } catch {
      setNote("触发规则更新失败");
    }
  }

  async function promote() {
    setNote(null);
    try {
      const promoted = await client.call("promote_principles", {});
      setPrinciples(await client.call("principles_list", { limit: 12 }));
      setRing(await client.call("ring_overview", {}));
      setNote(
        promoted.length > 0 ? `新沉淀 ${promoted.length} 条原则` : "还没有连续采纳三次的议题",
      );
    } catch {
      setNote("原则提升失败");
    }
  }

  /** 撤销一条原则：撤下后不再进入会诊上下文，但内容与原因仍留痕可读。 */
  async function revokePrinciple() {
    if (!revokeTarget) {
      return;
    }
    setNote(null);
    try {
      await client.call("principle_revoke", {
        nodeId: revokeTarget.nodeId,
        reason: revokeReason.trim(),
      });
      setRevoked((current) => [
        {
          nodeId: revokeTarget.nodeId,
          content: revokeTarget.content,
          reason: revokeReason.trim(),
          at: new Date().toISOString(),
        },
        ...current,
      ]);
      setPrinciples(await client.call("principles_list", { limit: 12 }));
      setRing(await client.call("ring_overview", {}));
      setNote("原则已撤销，不再参与会诊；内容与原因仍可读");
      setRevokeTarget(null);
      setRevokeReason("");
    } catch {
      setNote("原则撤销失败");
    }
  }

  async function loadCaptures(kind: CaptureKindKey | "all") {
    setCaptureKind(kind);
    setCaptures(
      await client.call("capture_events", kind === "all" ? { limit: 40 } : { kind, limit: 40 }),
    );
  }

  async function toggleCapturePaused(next: boolean) {
    setNote(null);
    try {
      setCapture(await client.call("capture_set_paused", { paused: next }));
    } catch {
      setNote("采集暂停切换失败");
    }
  }

  async function toggleCapability(kind: CaptureKindKey, enabled: boolean) {
    setNote(null);
    try {
      setCapture(await client.call("capture_set_capability", { kind, enabled }));
      setCaptureAudit(await client.call("capture_audit", { limit: 12 }));
    } catch (cause) {
      setNote(cause instanceof Error ? cause.message : "采集能力切换失败");
    }
  }

  async function toggleRedaction(next: boolean) {
    setNote(null);
    try {
      setCapture(await client.call("capture_set_redaction", { enabled: next, terms: [] }));
    } catch {
      setNote("遮蔽敏感信息的开关没能切换");
    }
  }

  async function changeDedup(seconds: number) {
    setNote(null);
    try {
      setCapture(await client.call("capture_set_dedup", { seconds }));
    } catch {
      setNote("去重窗口设置失败");
    }
  }

  async function collectOnce() {
    setNote(null);
    try {
      const outcome = await client.call("capture_collect", {});
      setCaptures(await client.call("capture_events", { limit: 40 }));
      setNote(
        outcome.paused
          ? "已暂停，本轮未采集"
          : `本轮写入 ${outcome.written} 条，跳过 ${outcome.skippedDuplicate + outcome.skippedDisabled} 条`,
      );
    } catch {
      setNote("采集一轮失败");
    }
  }

  /** 提交关注目录。校验与监听重建都在命令层完成，这里只在成功后刷新视图。 */
  async function saveWatchRoots(paths: readonly string[]) {
    setNote(null);
    try {
      setCapture(await client.call("capture_set_watch_roots", { paths: [...paths] }));
      setCaptureAudit(await client.call("capture_audit", { limit: 12 }));
      setNote(paths.length > 0 ? `已监听 ${paths.length} 个目录` : "已清空关注目录");
    } catch (cause) {
      setNote(cause instanceof Error ? cause.message : "关注目录设置失败");
    }
  }

  async function removeCapture(eventId: string) {
    setNote(null);
    try {
      await client.call("capture_delete_event", { eventId });
      setCaptures(
        await client.call(
          "capture_events",
          captureKind === "all" ? { limit: 40 } : { kind: captureKind, limit: 40 },
        ),
      );
      setNote("已删除该条记录");
    } catch {
      setNote("删除采集记录失败");
    }
  }


  async function generateIntake(event: CaptureEventView) {
    if (intakeDone.has(event.id)) {
      setNote("这条已经生成过录入，不再重复");
      return;
    }
    setNote(null);
    try {
      const body = captureMaterialText(event.payload);
      await client.call("intake_create", {
        masterId: "capture-inbox",
        masterName: "采集转入",
        domain: "采集",
        materials: [
          {
            title: captureKindLabel(event.kind),
            kind: event.kind,
            sourceRef: event.id,
            text: body || "无正文",
          },
        ],
      });
      setIntakeDone((current) => {
        const next = new Set(current);
        next.add(event.id);
        return next;
      });
      setNote("已生成录入 · 去「炼」看");
    } catch (cause) {
      setNote(cause instanceof Error ? cause.message : "生成录入失败");
    }
  }

  async function runCollision() {
    const content = collideDraft.trim();
    if (!content) {
      setNote("请先写一段要对撞的内容");
      return;
    }
    setNote(null);
    try {
      const outcome = await client.call("companion_collide", {
        signal: {
          sourceKind: "manual",
          sourceRef: "companion-panel",
          content,
        },
      });
      setCollision(outcome);
      if (outcome.skipped) {
        setNote(
          outcome.skipped === "disabled"
            ? "主动助学已关闭，本次未对撞"
            : `本次未对撞：${outcome.skipped}`,
        );
        return;
      }
      setNote(`对撞产生 ${outcome.pushed} 条洞察，还可再推 ${outcome.remaining} 条`);
    } catch (cause) {
      setNote(cause instanceof Error ? cause.message : "对撞失败");
    }
  }

  async function startSelf() {
    setMirrorNote(null);
    setBusy(true);
    try {
      const started = await client.call("self_start", {});
      setDraft(started);
      setMirrorNote("铜镜已启动，逐条确认后即可安装");
    } catch {
      setMirrorNote("启动自我蒸馏失败，请确认记录数量与联网状态");
    } finally {
      setBusy(false);
    }
  }

  async function decideSelf(item: SelfItemView, accepted: boolean) {
    if (!draft) {
      return;
    }
    setMirrorNote(null);
    try {
      setDraft(
        await client.call("self_decide", { draftId: draft.draft.id, itemId: item.id, accepted }),
      );
    } catch {
      setMirrorNote("写入确认结果失败");
    }
  }

  async function installSelf() {
    if (!draft) {
      return;
    }
    setMirrorNote(null);
    setBusy(true);
    try {
      const outcome = await client.call("self_install", { draftId: draft.draft.id });
      setSelf(await client.call("self_readiness", {}));
      setDraft(await client.call("self_draft", { draftId: draft.draft.id }));
      setMirrorNote(`已安装「你」的席位，版本 v${outcome.version}，共 ${outcome.unitCount} 条`);
    } catch {
      setMirrorNote("安装失败，请先处理所有待确认条目并至少采纳一条");
    } finally {
      setBusy(false);
    }
  }

  async function toggleSelfSeat(next: boolean) {
    setMirrorNote(null);
    try {
      setSelf(await client.call("self_set_seat", { enabled: next }));
    } catch {
      setMirrorNote("席位开关切换失败");
    }
  }

  async function exportData() {
    setDataNote(null);
    try {
      const outcome = await client.call("data_export", {});
      setExported(outcome);
      setDataEvents(await client.call("data_events", { limit: 12 }));
      setDataNote(`已导出 ${outcome.rowCount} 条记录`);
    } catch {
      setDataNote("导出失败，请确认数据目录可写");
    }
  }

  async function purgeData() {
    if (!purgeArmed) {
      setPurgeArmed(true);
      setDataNote("再次点击以确认清除本机全部数据，此操作不可撤销");
      return;
    }
    setPurgeArmed(false);
    setDataNote(null);
    try {
      const outcome = await client.call("data_purge", { confirm: true });
      setExported(null);
      setPurgeArmed(false);
      await refresh();
      setDataEvents(await client.call("data_events", { limit: 12 }));
      setDataNote(`已清除 ${outcome.rowCount} 条记录，并留下了清除记录`);
    } catch {
      setDataNote("清除失败，已保留原有数据");
    }
  }

  return (
    <RealmShell realm="self">
      <div className="self-views" role="tablist" aria-label="我的视图">
        <button
          type="button"
          role="tab"
          aria-selected={selfView === "growth"}
          data-on={selfView === "growth"}
          onClick={() => showView("growth")}
        >
          成长
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={selfView === "system"}
          data-on={selfView === "system"}
          onClick={() => showView("system")}
        >
          设置
        </button>
      </div>
      {/* 设置一页放不下，给一排分区按钮，点了直接跳到对应面板。 */}
      {/* 这里用条件渲染而不是 hidden：导航自己设了 display: flex，会盖掉 hidden 的默认隐藏。 */}
      {selfView === "system" ? (
        <nav className="setting-nav" aria-label="设置分区">
          {SETTING_SECTIONS.map((section) => (
            <button
              key={section.id}
              type="button"
              aria-controls={section.id}
              onClick={() => {
                document.getElementById(section.id)?.scrollIntoView?.({ block: "start" });
              }}
            >
              {section.label}
            </button>
          ))}
        </nav>
      ) : null}
      <section className="panel" id="setting-checkup" hidden={selfView !== "system"}>
        <h2 className="section-head">体检清单</h2>
        <p className="setting-row__hint">
          必备 {countRequired(checkup, true) + countRequired(checkup, false)} 项里已就绪{" "}
          {countRequired(checkup, true)} 项
          {countRequired(checkup, false) === 0
            ? "，可以开始实测了。"
            : "，还差下面标红的那几项。"}
        </p>
        <ul className="checkups">
          {checkup.map((item) => (
            <li key={item.key} className="checkup" data-ok={item.ok}>
              <span className="checkup__label">{item.label}</span>
              <span className="checkup__state mono">{item.state}</span>
              <span className="checkup__hint">
                {item.hint}
                {item.required ? "" : "（可选，不算门槛）"}{" "}
                <button
                  className="checkup__go"
                  type="button"
                  aria-label={`去配置：${item.label}`}
                  onClick={() => goToCheckupTarget(item.target)}
                >
                  去配置
                </button>
              </span>
            </li>
          ))}
        </ul>

        <div className="setting-row">
          <span className="setting-row__label">诊断包</span>
          <button
            className="connector__test"
            type="button"
            onClick={() => void exportDiagnostics()}
          >
            导出诊断包
          </button>
        </div>
        <p className="setting-row__hint">
          写出一份 JSON：版本、数据格式、体检结论、平台与采集开关、费用与备份份数。
          密钥只记「有没有写入」，不写密钥本体；文件落在数据目录里。
        </p>
        {diagnosticsNote ? (
          <p className="setting-row__hint mono">{diagnosticsNote}</p>
        ) : null}

        <h3 className="section-head section-head--minor">使用概览</h3>
        {cost ? (
          <div className="cost-grid">
            <div className="cost-cell">
              <span className="cost-cell__label">今日花费</span>
              <span className="cost-cell__value mono">
                {formatMoney(cost.todayMicros, cost.currency)}
              </span>
              <span className="cost-cell__note mono">
                上限{" "}
                {cost.dailyLimitMicros > 0
                  ? formatMoney(cost.dailyLimitMicros, cost.currency)
                  : "不限"}
              </span>
            </div>
            <div className="cost-cell">
              <span className="cost-cell__label">本月花费</span>
              <span className="cost-cell__value mono">
                {formatMoney(cost.monthMicros, cost.currency)}
              </span>
              <span className="cost-cell__note mono">
                上限{" "}
                {cost.monthlyLimitMicros > 0
                  ? formatMoney(cost.monthlyLimitMicros, cost.currency)
                  : "不限"}
              </span>
            </div>
            <div className="cost-cell">
              <span className="cost-cell__label">已装大师</span>
              <span className="cost-cell__value mono">{masters.length} 位</span>
              <span className="cost-cell__note">会诊席位来自这里</span>
            </div>
            <div className="cost-cell">
              <span className="cost-cell__label">备份</span>
              <span className="cost-cell__value mono">{presentBackups} 份</span>
              <span className="cost-cell__note">完整明细在「数据与备份」</span>
            </div>
          </div>
        ) : (
          <p className="setting-row__hint">用量读取中。</p>
        )}
        {cost && !cost.priced ? (
          <p className="setting-row__hint">
            单价还没填，费用按零计。在下面「大模型」里填了单价，估算才会反映真实开销。
          </p>
        ) : null}
      </section>

      <section className="panel" id="setting-history" hidden={selfView !== "system"}>
        <h2 className="section-head">历史回顾</h2>
        <p className="setting-row__hint">
          最近发生过的事按时间倒序放一条流水：采集、模型调用、备份、数据留痕。
          每类完整的明细仍在各自面板里（模型调用记录、采集台、数据与备份、数据主权）。
        </p>
        {history.length === 0 ? (
          <p className="setting-row__hint">还没有可回顾的事。</p>
        ) : (
          <ul className="calls">
            {history.map((entry) => (
              <li key={entry.id} className="call">
                <span className="mono">{formatTime(entry.at)}</span>
                <span>{entry.source}</span>
                <span>{entry.detail}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel" hidden={selfView !== "growth"}>
        <h2 className="section-head">自我画像</h2>
        <p className="setting-row__hint">
          累计 {self?.required ?? 20} 条思考记录后解锁。系统会从你过去的记录里提炼一份初稿，
          逐条确认采纳后，「你」会成为之后每一次会诊的第七位与会者，用来对照现在的你与当时的你。
        </p>
        {self ? (
          <>
            <div className="setting-row">
              <span className="setting-row__label">解锁进度</span>
              <span className="mono">
                {self.recordCount} / {self.required}
              </span>
              <div
                className="mirror"
                role="meter"
                aria-label="自我画像解锁进度"
                aria-valuenow={Math.min(self.recordCount, self.required)}
                aria-valuemin={0}
                aria-valuemax={self.required}
                data-unlocked={self.unlocked}
              >
                <span
                  className="mirror__fill"
                  style={{
                    width: `${Math.min(100, Math.round((self.recordCount / self.required) * 100))}%`,
                  }}
                />
              </div>
            </div>
            <div className="setting-row">
              <span className="setting-row__label">提炼初稿</span>
              <button
                className="consolidate__go"
                type="button"
                disabled={!self.unlocked || busy}
                onClick={() => void startSelf()}
              >
                {busy ? "提炼中" : "开始生成"}
              </button>
              {!self.unlocked ? (
                <span className="setting-row__hint">
                  还差 {self.required - self.recordCount} 条记录
                </span>
              ) : null}
            </div>
            {self.installed ? (
              <div className="setting-row">
                <span className="setting-row__label">
                  参与会诊的你 · 已安装版本 {self.currentVersion}
                </span>
                <button
                  className="switch"
                  type="button"
                  role="switch"
                  aria-label="把你自己加入会诊"
                  aria-checked={self.seatEnabled}
                  data-on={self.seatEnabled}
                  onClick={() => void toggleSelfSeat(!self.seatEnabled)}
                >
                  {self.seatEnabled ? "已开启" : "已关闭"}
                </button>
              </div>
            ) : null}
          </>
        ) : (
          <p className="setting-row__hint">读取中</p>
        )}
        {draft ? (
          <>
            <h3 className="section-head section-head--minor">
              初稿 · 待确认 {draft.pendingCount} · 已采纳 {draft.acceptedCount} · 已剔除{" "}
              {draft.rejectedCount}
            </h3>
            {draft.draft.errorCode ? (
              <p className="setting-row__hint" data-tone="warn">
                上一次提炼没成功：{readableError(draft.draft.errorCode, draft.draft.note)}
              </p>
            ) : null}
            <ul className="self-items" aria-label="自我画像初稿">
              {draft.items.map((item) => (
                <li key={item.id} className="self-item" data-status={item.status}>
                  <div className="self-item__head">
                    <span
                      className="self-item__layer"
                      data-layer={item.layer}
                      title={layerOf(item.layer).colorName}
                    >
                      <LayerGlyph glyph={layerOf(item.layer).glyph} />
                      <span className="mono">{layerOf(item.layer).name}</span>
                    </span>
                    <span className="self-item__title">{item.title}</span>
                    <span className="self-item__status mono">{item.statusLabel}</span>
                  </div>
                  <dl className="kv">
                    <div className="kv__row">
                      <dt>触发条件</dt>
                      <dd>{item.triggerCondition}</dd>
                    </div>
                    <div className="kv__row">
                      <dt>做法</dt>
                      <dd>{item.steps.join("；")}</dd>
                    </div>
                    <div className="kv__row">
                      <dt>机制</dt>
                      <dd>{item.mechanism}</dd>
                    </div>
                    <div className="kv__row">
                      <dt>边界</dt>
                      <dd>{item.boundary}</dd>
                    </div>
                    {item.evidence.length > 0 ? (
                      <div className="kv__row">
                        <dt>来源</dt>
                        <dd className="prose">{item.evidence.join(" / ")}</dd>
                      </div>
                    ) : null}
                  </dl>
                  <div className="self-item__actions">
                    <button
                      type="button"
                      aria-pressed={item.status === "accepted"}
                      data-on={item.status === "accepted"}
                      onClick={() => void decideSelf(item, true)}
                    >
                      采纳
                    </button>
                    <button
                      type="button"
                      aria-pressed={item.status === "rejected"}
                      data-on={item.status === "rejected"}
                      onClick={() => void decideSelf(item, false)}
                    >
                      剔除
                    </button>
                  </div>
                </li>
              ))}
            </ul>
            <div className="setting-row">
              <span className="setting-row__label">安装成「你」的档案</span>
              <button
                className="consolidate__go"
                type="button"
                disabled={busy || draft.pendingCount > 0 || draft.acceptedCount === 0}
                onClick={() => void installSelf()}
              >
                安装并加入会诊
              </button>
              {draft.pendingCount > 0 ? (
                <span className="setting-row__hint">还有 {draft.pendingCount} 条待确认</span>
              ) : draft.acceptedCount === 0 ? (
                <span className="setting-row__hint">至少采纳一条才能安装</span>
              ) : null}
            </div>
          </>
        ) : null}
        {mirrorNote ? (
          <p className="setting-row__hint" data-tone="warn">
            {mirrorNote}
          </p>
        ) : null}
      </section>

      <section className="panel" id="setting-models" hidden={selfView !== "system"}>
        <h2 className="section-head">联网与模型平台</h2>
        <div className="setting-row">
          <span className="setting-row__label">联网能力</span>
          <button
            className="switch"
            type="button"
            role="switch"
            aria-label="联网能力"
            aria-checked={networking === true}
            data-on={networking === true}
            onClick={() => void toggleNetworking(networking !== true)}
          >
            {networking === true ? "已开启" : "已关闭"}
          </button>
        </div>
        <p className="setting-row__hint">
          默认关闭。开启后每次向模型提问都会留下记录。密钥不进数据库：写进系统密钥库，
          条目名是 <span className="mono">thought-forge/platform/平台代码</span>；
          系统密钥库不可用时退回环境变量 <span className="mono">THOUGHT_FORGE_API_KEY</span>。
        </p>
        <ul className="platforms">
          {platforms.map((platform) => (
            <li key={platform.code} className="platform" data-enabled={platform.enabled}>
              <div className="platform__head">
                <span className="platform__name">{platform.displayName}</span>
                <span className="platform__status">{platformStatusLabel(platform.status)}</span>
              </div>
              <span className="platform__endpoint mono">
                {platform.endpoint || "还没填服务地址"}
              </span>
              <div className="platform__actions">
                <button
                  className="platform__toggle"
                  type="button"
                  aria-pressed={platform.enabled}
                  onClick={() => void togglePlatform(platform)}
                >
                  {platform.enabled ? "停用" : "启用"}
                </button>
                <button
                  className="platform__toggle"
                  type="button"
                  onClick={() => editPlatform(platform)}
                >
                  编辑
                </button>
              </div>
            </li>
          ))}
        </ul>

        <h3 className="section-head section-head--minor">新增或修改平台</h3>
        <p className="setting-row__hint">
          服务地址要填到接口那一段（形如
          <span className="mono">https://api.deepseek.com/chat/completions</span>），
          只填域名会连不上。平台代码同时是密钥条目名，下面的「密钥」里归属要填同一个代码。
          单价按元 / 千 token 计，不清楚就留空，按零计。
        </p>
        <div className="setting-row">
          <label className="setting-row__label" htmlFor="platform-code">
            平台代码
          </label>
          <input
            id="platform-code"
            className="connector-input"
            value={platformDraft.code}
            placeholder="如 deepseek"
            onChange={(event) => patchPlatformDraft({ code: event.target.value })}
          />
        </div>
        <div className="setting-row">
          <label className="setting-row__label" htmlFor="platform-name">
            显示名
          </label>
          <input
            id="platform-name"
            className="connector-input"
            value={platformDraft.displayName}
            placeholder="留空就用平台代码"
            onChange={(event) => patchPlatformDraft({ displayName: event.target.value })}
          />
        </div>
        <div className="setting-row">
          <label className="setting-row__label" htmlFor="platform-endpoint">
            服务地址
          </label>
          <input
            id="platform-endpoint"
            className="connector-input"
            value={platformDraft.endpoint}
            placeholder="如 https://api.deepseek.com/chat/completions"
            onChange={(event) => patchPlatformDraft({ endpoint: event.target.value })}
          />
        </div>
        <div className="setting-row">
          <label className="setting-row__label" htmlFor="platform-model">
            模型名
          </label>
          <input
            id="platform-model"
            className="connector-input"
            value={platformDraft.modelName}
            placeholder="如 deepseek-chat"
            onChange={(event) => patchPlatformDraft({ modelName: event.target.value })}
          />
        </div>
        <div className="setting-row">
          <label className="setting-row__label" htmlFor="platform-input-price">
            输入单价
          </label>
          <input
            id="platform-input-price"
            className="connector-input"
            value={platformDraft.inputPrice}
            placeholder="元 / 千 token，可留空"
            onChange={(event) => patchPlatformDraft({ inputPrice: event.target.value })}
          />
        </div>
        <div className="setting-row">
          <label className="setting-row__label" htmlFor="platform-output-price">
            输出单价
          </label>
          <input
            id="platform-output-price"
            className="connector-input"
            value={platformDraft.outputPrice}
            placeholder="元 / 千 token，可留空"
            onChange={(event) => patchPlatformDraft({ outputPrice: event.target.value })}
          />
        </div>
        <div className="credential-actions">
          <button
            className="connector-save"
            type="button"
            disabled={busy}
            onClick={() => void savePlatform()}
          >
            保存平台
          </button>
          <button
            className="connector__test"
            type="button"
            onClick={() => void probeModel()}
          >
            测试连通
          </button>
        </div>
        {platformNote ? (
          <p className="setting-row__hint" data-tone="warn">
            {platformNote}
          </p>
        ) : null}
        {probeNote ? (
          <p className="setting-row__hint" data-tone="warn">
            {probeNote}
          </p>
        ) : null}
        {probe ? (
          <p className="setting-row__hint" data-tone={probe.ok ? undefined : "warn"}>
            {[
              `探针${PROBE_EXIT_LABEL[probe.exitCode] ?? `退出码 ${probe.exitCode}`}`,
              `${probe.platformCode || "未配置平台"} / ${probe.modelName || "未选模型"}`,
              `耗时 ${formatDuration(probe.latencyMs)}`,
              probe.callId ? `审计 ${probe.callId}` : "",
              probe.errorCode ? `错误码 ${probe.errorCode}` : "",
            ]
              .filter((part) => part !== "")
              .join("，")}
          </p>
        ) : null}
        {note ? (
          <p className="setting-row__hint" data-tone="warn">
            {note}
          </p>
        ) : null}

        <h3 className="section-head section-head--minor">密钥</h3>
        <p className="setting-row__hint">
          密钥存进系统密钥库，这里只记住它的名字，不会保存密钥本身。
          平台密钥的归属填平台代码，条目名就是 thought-forge/platform/平台代码。
        </p>
        <div className="setting-row">
          <label className="setting-row__label" htmlFor="credential-scope">
            范围
          </label>
          <select
            id="credential-scope"
            className="connector-input"
            value={credentialScope}
            onChange={(event) => setCredentialScope(event.target.value)}
          >
            {CREDENTIAL_SCOPES.map((item) => (
              <option key={item.scope} value={item.scope}>
                {item.label}
              </option>
            ))}
          </select>
        </div>
        <div className="setting-row">
          <label className="setting-row__label" htmlFor="credential-owner">
            归属
          </label>
          <input
            id="credential-owner"
            className="connector-input"
            value={credentialOwner}
            placeholder="平台代码或外部数据源名"
            onChange={(event) => setCredentialOwner(event.target.value)}
          />
        </div>
        <div className="setting-row">
          <label className="setting-row__label" htmlFor="credential-secret">
            密钥
          </label>
          <input
            id="credential-secret"
            className="connector-input"
            type="password"
            value={credentialSecret}
            placeholder="只在写入时出现，不落库"
            onChange={(event) => setCredentialSecret(event.target.value)}
          />
        </div>
        <div className="credential-actions">
          <button className="connector-save" type="button" onClick={() => void saveCredential()}>
            保存密钥
          </button>
          <button className="connector__test" type="button" onClick={() => void checkCredential()}>
            查询状态
          </button>
          {credentialOwner.trim() ? (
            <span className="credential-state mono">
              {credentialKnown[`${credentialScope}:${credentialOwner.trim()}`]
                ? "已配置"
                : "未配置"}
            </span>
          ) : null}
        </div>
        {credentialNote ? (
          <p className="setting-row__hint" data-tone="warn">
            {credentialNote}
          </p>
        ) : null}
      </section>

      <section className="panel" id="setting-sources" hidden={selfView !== "system"}>
        <h2 className="section-head">外部数据源</h2>
        <p className="setting-row__hint">
          联网搜索与网页阅读在这里逐项开启，默认全部关闭。每次搜索都会留下记录；
          外部工具服务必须能返回工具清单才可接入。
        </p>
        <div className="setting-row">
          <span className="setting-row__label">类型</span>
          <span className="connector-kinds" role="group" aria-label="外部数据源类型">
            {CONNECTOR_KINDS.map((item) => (
              <button
                key={item.kind}
                type="button"
                className="strategy"
                aria-pressed={connectorKind === item.kind}
                data-active={connectorKind === item.kind}
                onClick={() => setConnectorKind(item.kind)}
              >
                {item.label}
              </button>
            ))}
          </span>
        </div>
        <div className="setting-row">
          <label className="setting-row__label" htmlFor="connector-name">
            名称
          </label>
          <input
            id="connector-name"
            className="connector-input"
            value={connectorName}
            placeholder="例如：本地搜索"
            onChange={(event) => setConnectorName(event.target.value)}
          />
        </div>
        <div className="setting-row">
          <label className="setting-row__label" htmlFor="connector-endpoint">
            地址
          </label>
          <input
            id="connector-endpoint"
            className="connector-input"
            value={connectorEndpoint}
            placeholder={CONNECTOR_PLACEHOLDERS[connectorKind] ?? "https://..."}
            onChange={(event) => setConnectorEndpoint(event.target.value)}
          />
        </div>
        <button
          className="connector-save"
          type="button"
          onClick={() => void saveConnector()}
        >
          保存并试连
        </button>
        <ul className="connectors">
          {connectors.map((connector) => (
            <li
              key={connector.id}
              className="connector"
              data-enabled={connector.enabled}
              data-status={connector.status}
            >
              <div className="connector__head">
                <span className="connector__name">{connector.displayName}</span>
                <span className="connector__kind">{sourceKindLabel(connector.kind)}</span>
                <span className="connector__status">
                  {connectorStatusLabel(connector.status)}
                </span>
              </div>
              <span className="connector__endpoint mono">
                {connector.endpoint || "还没填地址"}
              </span>
              <div className="connector__actions">
                <button
                  className="switch"
                  type="button"
                  role="switch"
                  aria-label={connector.displayName}
                  aria-checked={connector.enabled}
                  data-on={connector.enabled}
                  onClick={() => void toggleConnector(connector)}
                >
                  {connector.enabled ? "已开启" : "已关闭"}
                </button>
                <button
                  className="connector__test"
                  type="button"
                  onClick={() => void testConnector(connector)}
                >
                  测试连接
                </button>
              </div>
            </li>
          ))}
        </ul>
        {connectorProbe ? (
          <div className="preflight" role="group" aria-label="发送前确认">
            <p className="preflight__title">发送前确认</p>
            <dl className="preflight__pairs">
              <dt>原始内容</dt>
              <dd className="mono">{connectorProbe.outcome.prepared?.original}</dd>
              <dt>实际发送</dt>
              <dd className="mono">{connectorProbe.outcome.prepared?.sent}</dd>
              <dt>发送模式</dt>
              <dd>{queryModeLabel(connectorProbe.outcome.prepared?.mode ?? "keyword")}</dd>
            </dl>
            {connectorProbe.outcome.prepared?.redacted ? (
              <p className="preflight__note">
                检测到敏感内容，已替换成掩码后才发送。
              </p>
            ) : null}
            <div className="preflight__actions">
              <button
                className="connector__test"
                type="button"
                onClick={() => {
                  const target = connectors.find(
                    (item) => item.id === connectorProbe.connectorId,
                  );
                  if (target) {
                    void testConnector(target, connectorProbe.outcome.fingerprint);
                  }
                }}
              >
                确认发送
              </button>
              <button
                className="connector__test"
                type="button"
                onClick={() => setConnectorProbe(null)}
              >
                取消
              </button>
            </div>
          </div>
        ) : null}
        {connectorNote ? (
          <p className="setting-row__hint" data-tone="warn">
            {connectorNote}
          </p>
        ) : null}
        <h3 className="section-head section-head--minor">外部数据源调用记录</h3>
        {connectorCalls.length === 0 ? (
          <p className="setting-row__hint">还没有外部数据源的调用记录。</p>
        ) : (
          <ul className="calls">
            {connectorCalls.map((call) => (
              <li key={call.id} className="call" data-status={call.status}>
                <span className="call__purpose">
                  {callPurposeLabel(call.purpose)}
                </span>
                <span className="call__meta">
                  {sourceKindLabel(call.kind)} · 发送 {call.querySent || "—"}
                  {call.redacted ? "（已遮蔽敏感信息）" : ""} · {call.resultCount} 条 · 用时{" "}
                  {formatDuration(call.latencyMs)}
                </span>
                <span className="call__status">
                  {call.errorCode
                    ? readableError(call.errorCode)
                    : callStatusLabel(call.status)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel" id="setting-usage" hidden={selfView !== "system"}>
        <h2 className="section-head">用量</h2>
        <p className="setting-row__hint">
          模型调用的流水与费用。外部数据源的调用次数跟着数据源配置放在「外部数据源」里。
        </p>
        <h3 className="section-head section-head--minor">模型调用</h3>
        {calls.length === 0 ? (
          <p className="setting-row__hint">还没有向模型提问的记录。</p>
        ) : (
          <ul className="calls">
            {calls.map((call) => (
              <li key={call.id} className="call" data-status={call.status}>
                <span className="call__purpose">{callPurposeLabel(call.purpose)}</span>
                <span className="call__meta">
                  {call.modelName || call.platformCode || "未指定平台"} · 用时{" "}
                  {formatDuration(call.latencyMs)} · 第 {call.attempt} 次
                </span>
                <span className="call__status">
                  {call.errorCode
                    ? readableError(call.errorCode)
                    : callStatusLabel(call.status)}
                </span>
              </li>
            ))}
          </ul>
        )}
        <h3 className="section-head section-head--minor">费用与配额</h3>
        <p className="setting-row__hint">
          每次模型与外部数据源的调用都会按实际用量记账，按自然日汇总。
          花到上限之后怎么办，在「调参」里设置。
        </p>
        {estimate && cost ? (
          <div className="cost-grid">
            <div className="cost-cell">
              <span className="cost-cell__label">单场估算</span>
              <span className="cost-cell__value mono">
                {formatMoney(estimate.costMicros, cost.currency)}
              </span>
              <span className="cost-cell__note">
                提问模型 {estimate.llmCalls} 次 · 搜索 {estimate.searchCalls} 次
              </span>
            </div>
            <div className="cost-cell">
              <span className="cost-cell__label">今日累计</span>
              <span className="cost-cell__value mono">
                {formatMoney(cost.todayMicros, cost.currency)}
              </span>
              <span className="cost-cell__note mono">
                上限{" "}
                {cost.dailyLimitMicros > 0
                  ? formatMoney(cost.dailyLimitMicros, cost.currency)
                  : "不限"}
              </span>
            </div>
            <div className="cost-cell">
              <span className="cost-cell__label">本月累计</span>
              <span className="cost-cell__value mono">
                {formatMoney(cost.monthMicros, cost.currency)}
              </span>
              <span className="cost-cell__note mono">
                上限{" "}
                {cost.monthlyLimitMicros > 0
                  ? formatMoney(cost.monthlyLimitMicros, cost.currency)
                  : "不限"}
              </span>
            </div>
            <div className="cost-cell">
              <span className="cost-cell__label">超限策略</span>
              <span className="cost-cell__value">
                {COST_POLICY_LABELS[cost.policy] ?? cost.policy}
              </span>
              <span className="cost-cell__note mono">{cost.currency}</span>
            </div>
          </div>
        ) : (
          <p className="setting-row__hint">费用汇总读取中。</p>
        )}
        {cost && !cost.priced ? (
          <p className="setting-row__hint">
            还没有配置任何单价，费用按零计。填写平台单价后估算才会反映真实开销。
          </p>
        ) : null}

      </section>

      <section className="panel" hidden={selfView !== "growth"}>
        <h2 className="section-head">采集台</h2>
        <p className="setting-row__hint">
          四类采集逐项开关，默认全部关闭。每次开启都需要你确认一次，并留下记录；
          全局暂停时不再读取、也不再写入。
        </p>
        {capture ? (
          <>
            <div className="setting-row">
              <span className="setting-row__label">全局暂停</span>
              <button
                className="switch"
                type="button"
                role="switch"
                aria-label="全局暂停"
                aria-checked={capture.paused}
                data-on={capture.paused}
                onClick={() => void toggleCapturePaused(!capture.paused)}
              >
                {capture.paused ? "已暂停" : "运行中"}
              </button>
            </div>
            <ul className="capture-caps">
              {capture.capabilities.map((capability) => (
                <li
                  key={capability.kind}
                  className="capture-cap"
                  data-enabled={capability.enabled}
                  data-available={capability.available}
                >
                  <span className="capture-cap__name">{capability.label}</span>
                  <span className="capture-cap__meta mono">
                    {capability.available
                      ? capability.consentedAt
                        ? `已同意 · ${formatTime(capability.consentedAt, { dateOnly: true })}`
                        : "尚未开启"
                      : capability.kind === "file"
                        ? "炉口闭合 · 未设置关注目录"
                        : "炉口闭合 · 系统权限被拒"}
                  </span>
                  <button
                    className="switch"
                    type="button"
                    role="switch"
                    aria-label={capability.label}
                    aria-checked={capability.enabled}
                    data-on={capability.enabled}
                    disabled={!capability.available}
                    onClick={() =>
                      void toggleCapability(capability.kind, !capability.enabled)
                    }
                  >
                    {capability.enabled ? "已开启" : "已关闭"}
                  </button>
                </li>
              ))}
            </ul>
            {/* 文件活动由关注目录决定，原因在下方区块里说明，不走系统授权的说法。 */}
            {capture.capabilities.some(
              (capability) => !capability.available && capability.kind !== "file",
            ) ? (
              <p className="setting-row__hint" data-tone="warn">
                炉口闭合的能力已被系统拒绝：请前往系统设置的隐私与安全页面授权，
                授权后回到这里重新开启。其他能力不受影响。
              </p>
            ) : null}
            <div className="capture-roots">
              <span className="capture-roots__head">
                <span className="setting-row__label">关注目录</span>
                {capture.watchRoots.length > 0 ? (
                  <span className="capture-roots__count mono">
                    {capture.watchRoots.length} 个目录
                  </span>
                ) : null}
              </span>
              {capture.watchRoots.length > 0 ? (
                <ul className="watch-roots" aria-label="关注目录">
                  {capture.watchRoots.map((root) => (
                    <li key={root} className="watch-root">
                      <span className="watch-root__path mono">{root}</span>
                      <button
                        type="button"
                        aria-label={`移除关注目录 ${root}`}
                        className="watch-root__remove"
                        onClick={() =>
                          void saveWatchRoots(
                            capture.watchRoots.filter((item) => item !== root),
                        )
                      }
                    >
                      移除
                    </button>
                  </li>
                  ))}
                </ul>
              ) : (
                <span className="capture-roots__empty">
                  还没有关注目录，文件活动因此不可用。
                </span>
              )}
              <div className="capture-roots__add">
                <input
                  type="text"
                  className="mono"
                  aria-label="新增关注目录"
                  placeholder="粘贴一个绝对路径，例如 D:\\notes"
                  value={watchRootDraft}
                  onChange={(event) => setWatchRootDraft(event.target.value)}
                />
                <button
                  type="button"
                  className="capture-roots__submit"
                  disabled={watchRootDraft.trim().length === 0}
                  onClick={() => {
                    void saveWatchRoots([...capture.watchRoots, watchRootDraft.trim()]);
                    setWatchRootDraft("");
                  }}
                >
                  添加
                </button>
              </div>
              <span className="setting-row__hint">
                只接受已存在的绝对路径，添加后立即生效，不需要重启。嵌套的目录会被合并，
                避免同一文件重复上报。文件正文不会被读取，只记录路径与事件类型。
              </span>
            </div>
            <div className="setting-row">
              <span className="setting-row__label">遮蔽敏感信息</span>
              <button
                className="switch"
                type="button"
                role="switch"
                aria-label="遮蔽敏感信息"
                aria-checked={capture.redactionEnabled}
                data-on={capture.redactionEnabled}
                onClick={() => void toggleRedaction(!capture.redactionEnabled)}
              >
                {capture.redactionEnabled
                  ? `已开启 · ${capture.redactionTerms} 条自定义词条`
                  : "已关闭"}
              </button>
            </div>
            <div className="setting-row">
              <span className="setting-row__label">去重窗口</span>
              <div className="adopt-row">
                {[60, 300, 900].map((value) => (
                  <button
                    key={value}
                    type="button"
                    aria-pressed={capture.dedupSeconds === value}
                    onClick={() => void changeDedup(value)}
                  >
                    {value < 60 ? `${value} 秒` : `${value / 60} 分钟`}
                  </button>
                ))}
              </div>
            </div>
            <div className="setting-row">
              <span className="setting-row__label">采集一轮</span>
              <button
                className="consolidate__go"
                type="button"
                onClick={() => void collectOnce()}
              >
                立即采集
              </button>
            </div>
            <div className="setting-row">
              <span className="setting-row__label">原始记录</span>
              <div className="adopt-row">
                <button
                  type="button"
                  aria-pressed={captureKind === "all"}
                  onClick={() => void loadCaptures("all")}
                >
                  全部
                </button>
                {capture.capabilities.map((capability) => (
                  <button
                    key={capability.kind}
                    type="button"
                    aria-pressed={captureKind === capability.kind}
                    onClick={() => void loadCaptures(capability.kind)}
                  >
                    {captureKindLabel(capability.kind)}
                  </button>
                ))}
              </div>
            </div>
            {captures.length === 0 ? (
              <p className="setting-row__hint">还没有采集记录。</p>
            ) : (
              <ul className="captures">
                {captures.map((event) => (
                  <li key={event.id} className="capture-row" data-redacted={event.redacted}>
                    <span className="capture-row__kind mono">
                      {captureKindLabel(event.kind)}
                    </span>
                    <span className="capture-row__time mono">
                      {formatTime(event.occurredAt)}
                    </span>
                    <span className="capture-row__excerpt">
                      {captureExcerpt(event.payload) || "无正文"}
                    </span>
                    {event.redacted ? (
                      <span className="capture-row__tag">已遮蔽敏感信息</span>
                    ) : null}
                    <button
                      className="capture-row__del"
                      type="button"
                      onClick={() => void generateIntake(event)}
                    >
                      生成录入
                    </button>
                    <button
                      className="capture-row__del"
                      type="button"
                      onClick={() => void removeCapture(event.id)}
                    >
                      删除
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <h3 className="section-head section-head--minor">开关记录</h3>
            {captureAudit.length === 0 ? (
              <p className="setting-row__hint">还没有开启记录。</p>
            ) : (
              <ul className="capture-audit">
                {captureAudit.map((entry) => (
                  <li key={entry.id} className="capture-audit__row">
                    <span>{formatTime(entry.createdAt)}</span>
                    <span>{captureKindLabel(entry.kind)}</span>
                    <span>{auditActionLabel(entry.action)}</span>
                  </li>
                ))}
              </ul>
            )}
          </>
        ) : (
          <p className="setting-row__hint">读取中</p>
        )}
      </section>

      <section className="panel" hidden={selfView !== "growth"}>
        <h2 className="section-head">成长轨迹</h2>
        <p className="setting-row__hint">
          同一议题的结论按时间串成演化链，采纳与否都留档。
        </p>
        {records.length === 0 ? (
          <p className="setting-row__hint">还没有思考记录。会诊收敛后会自动落成一条。</p>
        ) : (
          <ul className="records">
            {records.map((record) => (
              <li key={record.id} className="record" data-adopted={record.adopted}>
                <div className="record__head">
                  <span className="record__question">{record.question}</span>
                  <span className="record__time">
                    {formatTime(record.createdAt, { dateOnly: true })}
                  </span>
                </div>
                <p className="record__conclusion prose">{record.conclusion}</p>
                <div className="record__actions">
                  <button
                    className="record__chain"
                    type="button"
                    onClick={() => void showChain(record.topicKey)}
                  >
                    演化链
                  </button>
                  <button
                    className="record__adopt"
                    type="button"
                    aria-pressed={record.adopted}
                    data-on={record.adopted}
                    onClick={() => void decide(record, !record.adopted, record.reason)}
                  >
                    {record.adopted ? "已采纳" : "采纳"}
                  </button>
                  {record.reason ? (
                    <span className="record__reason">理由：{record.reason}</span>
                  ) : null}
                </div>
              </li>
            ))}
          </ul>
        )}
        {chain.length > 0 ? (
          <div className="chain">
            <span className="chain__topic">{chainTopic}</span>
            <ol className="chain__list" aria-label="演化链">
              {chain.map((record) => (
                <li key={record.id} className="chain__item" data-adopted={record.adopted}>
                  <span className="chain__time">
                    {formatTime(record.createdAt, { dateOnly: true })}
                  </span>
                  <span className="chain__text">{record.conclusion}</span>
                </li>
              ))}
            </ol>
          </div>
        ) : null}
      </section>

      <section className="panel" hidden={selfView !== "growth"}>
        <h2 className="section-head">记忆固化</h2>
        <p className="setting-row__hint">
          把最近一起被想起来的念头连得更紧，让久未被碰到的连接慢慢变淡，
          合并重复的念头，并找出彼此矛盾的地方。
        </p>
        <button className="consolidate__go" type="button" onClick={() => void consolidate()}>
          立即固化
        </button>
        {report ? (
          <dl className="kv">
            <div className="kv__row">
              <dt>加深了的连接</dt>
              <dd className="mono">{report.strengthenedCount}</dd>
            </div>
            <div className="kv__row">
              <dt>变淡了的连接</dt>
              <dd className="mono">{report.decayedCount}</dd>
            </div>
            <div className="kv__row">
              <dt>合并的重复念头</dt>
              <dd className="mono">{report.mergedCount}</dd>
            </div>
            <div className="kv__row">
              <dt>发现的矛盾</dt>
              <dd className="mono">{report.conflictCount}</dd>
            </div>
          </dl>
        ) : null}
        {runs.length > 0 ? (
          <ul className="runs">
            {runs.map((run) => (
              <li key={run.id} className="run">
                <span className="run__mode">{consolidationModeLabel(run.mode)}</span>
                <span className="run__time">{formatTime(run.startedAt)}</span>
                <span className="run__counts">
                  加深 {run.strengthenedCount} · 变淡 {run.decayedCount} · 合并{" "}
                  {run.mergedCount} · 矛盾 {run.conflictCount}
                </span>
              </li>
            ))}
          </ul>
        ) : null}
      </section>

      <section className="panel" id="setting-tuning" hidden={selfView !== "system"}>
        <h2 className="section-head">调参</h2>
        <p className="setting-row__hint">
          这些数字原本写死在程序里，现在交给你。越界或格式不对的取值整批不会生效。
        </p>
        {tuning.length === 0 ? (
          <p className="setting-row__hint">{tuningNote ?? "读取中"}</p>
        ) : (
          <>
            {[...new Set(tuning.map((item) => item.group))].map((group) => (
              <div key={group} className="tuning__group">
                <h3 className="setting-row__label">{group}</h3>
                <ul className="tuning__list">
                  {tuning
                    .filter((item) => item.group === group)
                    .map((item) => {
                      const invalid = invalidOf(item);
                      return (
                        <li
                          key={item.key}
                          className="tuning__row"
                          data-customized={item.customized}
                        >
                          <label className="tuning__label" htmlFor={`tuning-${item.key}`}>
                            {item.label}
                            <span className="tuning__key mono">{item.key}</span>
                          </label>
                          {item.kind === "bool" ? (
                            <select
                              id={`tuning-${item.key}`}
                              className="tuning__input mono"
                              aria-label={item.label}
                              value={draftOf(item)}
                              onChange={(event) =>
                                setTuningDraft((current) => ({
                                  ...current,
                                  [item.key]: event.target.value,
                                }))
                              }
                            >
                              <option value="true">开启</option>
                              <option value="false">关闭</option>
                            </select>
                          ) : (
                            <input
                              id={`tuning-${item.key}`}
                              className="tuning__input mono"
                              aria-label={item.label}
                              aria-invalid={invalid}
                              value={draftOf(item)}
                              onChange={(event) =>
                                setTuningDraft((current) => ({
                                  ...current,
                                  [item.key]: event.target.value,
                                }))
                              }
                            />
                          )}
                          <span className="tuning__meta mono">
                            {item.kind === "bool"
                              ? `默认 ${item.defaultValue === "true" ? "开启" : "关闭"}`
                              : `默认 ${item.defaultValue} · 范围 ${item.min}–${item.max}${item.unit ? ` ${item.unit}` : ""}`}
                          </span>
                          <span className="tuning__note">{item.note}</span>
                          {invalid ? (
                            <span className="tuning__invalid" role="alert">
                              取值超出允许范围
                            </span>
                          ) : null}
                        </li>
                      );
                    })}
                </ul>
              </div>
            ))}
            <div className="tuning__actions">
              <button
                className="consolidate__go"
                type="button"
                onClick={() => void saveTuning()}
                disabled={pendingChanges.length === 0 || tuning.some(invalidOf)}
              >
                保存全部
              </button>
              <span className="setting-row__hint">
                {tuning.some(invalidOf)
                  ? "有取值越界，先改回范围内"
                  : pendingChanges.length > 0
                    ? `${pendingChanges.length} 项待保存`
                    : tuningNote ?? "没有改动"}
              </span>
            </div>
          </>
        )}
      </section>

      <section className="panel" hidden={selfView !== "growth"}>
        <h2 className="section-head">主动助学</h2>
        <p className="setting-row__hint">
          默认关闭。开启后助理会把新念头与网络中的判断、框架做一次轻量碰撞，
          只在发现真实的关联、冲突或盲区时，安静地点亮画布边缘的一簇余烬。
        </p>
        <div className="setting-row">
          <span className="setting-row__label">主动推送</span>
          <button
            className="switch"
            type="button"
            role="switch"
            aria-label="主动助学"
            aria-checked={companion?.enabled === true}
            data-on={companion?.enabled === true}
            onClick={() => void toggleCompanion(companion?.enabled !== true)}
          >
            {companion?.enabled === true ? "已开启" : "已关闭"}
          </button>
        </div>
        {companion ? (
          <>
            <div className="setting-row">
              <span className="setting-row__label">每日上限</span>
              <span className="mono">
                {companion.usedToday} / {companion.dailyLimit}
              </span>
              <div className="adopt-row">
                {[3, 5, 10].map((value) => (
                  <button
                    key={value}
                    type="button"
                    aria-pressed={companion.dailyLimit === value}
                    onClick={() => void changeLimit(value)}
                  >
                    {value} 条
                  </button>
                ))}
              </div>
            </div>
            <div className="setting-row">
              <span className="setting-row__label">记下新想法时顺手碰一碰</span>
              <button
                className="switch"
                type="button"
                role="switch"
                aria-label="记下新想法时触发"
                aria-checked={companion.rules.triggerOnRecord}
                data-on={companion.rules.triggerOnRecord}
                onClick={() => void toggleRule("triggerOnRecord")}
              >
                {companion.rules.triggerOnRecord ? "已开启" : "已关闭"}
              </button>
            </div>
            <div className="setting-row">
              <span className="setting-row__label">采集到新内容时顺手碰一碰</span>
              <button
                className="switch"
                type="button"
                role="switch"
                aria-label="采集到新内容时触发"
                aria-checked={companion.rules.triggerOnCapture}
                data-on={companion.rules.triggerOnCapture}
                onClick={() => void toggleRule("triggerOnCapture")}
              >
                {companion.rules.triggerOnCapture ? "已开启" : "已关闭"}
              </button>
            </div>
            <div className="setting-row">
              <span className="setting-row__label">对撞</span>
              <textarea
                className="corpus-search__input"
                rows={3}
                value={collideDraft}
                onChange={(event) => setCollideDraft(event.target.value)}
                placeholder="把一段想法丢进来，和已有判断碰一碰"
                aria-label="对撞内容"
              />
              <button
                className="consolidate__go"
                type="button"
                onClick={() => void runCollision()}
              >
                发起对撞
              </button>
            </div>
            {collision ? (
              <ul className="capture-audit" aria-label="对撞结果">
                {collision.generated.length === 0 ? (
                  <li>没有新的洞察</li>
                ) : (
                  collision.generated.map((insight) => (
                    <li key={insight.id}>{insight.title}</li>
                  ))
                )}
              </ul>
            ) : null}
          </>
        ) : (
          <p className="setting-row__hint">读取中</p>
        )}
      </section>

      <section className="panel" hidden={selfView !== "growth"}>
        <h2 className="section-head">演化长河</h2>
        <p className="setting-row__hint">
          同一议题的多次结论沿一条长河排布，点选可并列看到当时的结论与现在的结论。
        </p>
        {topics.length === 0 ? (
          <p className="setting-row__hint">还没有形成议题。</p>
        ) : (
          <ul className="river" aria-label="议题长河">
            {topics.map((topic) => (
              <li key={topic.topicKey} className="river__node">
                <span className="river__time mono">
                  {topic.recordCount} 次判断 · 采纳 {topic.adoptedCount}
                </span>
                <span className="river__text prose">{topic.latestConclusion}</span>
                <button
                  className="record__chain"
                  type="button"
                  onClick={() => void showChain(topic.topicKey)}
                >
                  看演化链
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel" hidden={selfView !== "growth"}>
        <h2 className="section-head">个人原则</h2>
        <p className="setting-row__hint">
          被连续采纳三次的结论升格为原则，以印章形态陈列，可回溯到它从哪些判断演化而来。
        </p>
        <button className="consolidate__go" type="button" onClick={() => void promote()}>
          沉淀原则
        </button>
        {principles.length === 0 ? (
          <p className="setting-row__hint">还没有沉淀出原则。</p>
        ) : (
          <ul className="seals">
            {principles.map((seal) => (
              <li key={seal.nodeId} className="seal">
                <p className="seal__content">{seal.content}</p>
                <p className="seal__meta">
                  采纳 {seal.adoptedCount} 次 · 唤醒度 {seal.activation.toFixed(2)} ·{" "}
                  {seal.layers.map((layer) => layerOf(layer).name).join(" / ") ||
                    "未标注层次"}
                </p>
                <button
                  className="seal__revoke"
                  type="button"
                  onClick={() => {
                    setRevokeTarget(seal);
                    setRevokeReason("");
                  }}
                >
                  撤销
                </button>
              </li>
            ))}
          </ul>
        )}
        {revokeTarget ? (
          <div className="seal__confirm" role="alertdialog" aria-label="撤销原则确认">
            <p className="seal__confirm-text">
              撤销后这条原则不再进入会诊上下文：{revokeTarget.content}
            </p>
            <label className="seal__confirm-label" htmlFor="seal-revoke-reason">
              撤销原因
            </label>
            <input
              id="seal-revoke-reason"
              className="seal__confirm-input"
              value={revokeReason}
              placeholder="写下为什么不再认同这条原则"
              onChange={(event) => setRevokeReason(event.target.value)}
            />
            <div className="seal__confirm-actions">
              <button
                className="seal__confirm-go"
                type="button"
                onClick={() => void revokePrinciple()}
              >
                确认撤销
              </button>
              <button
                className="seal__confirm-cancel"
                type="button"
                onClick={() => {
                  setRevokeTarget(null);
                  setRevokeReason("");
                }}
              >
                取消
              </button>
            </div>
          </div>
        ) : null}
        {revoked.length > 0 ? (
          <div className="seal__revoked">
            <h3 className="section-head">已撤销原则</h3>
            <ul className="seals" aria-label="已撤销原则">
              {revoked.map((item) => (
                <li key={item.nodeId} className="seal" data-revoked="true">
                  <p className="seal__content">{item.content}</p>
                  <p className="seal__meta">
                    撤销于 {formatTime(item.at)} · 原因：{item.reason || "未填写"}
                  </p>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </section>

      <section className="panel" hidden={selfView !== "growth"}>
        <h2 className="section-head">年轮概览</h2>
        <div className="ring">
          <div className="ring__cell">
            <span className="ring__value mono">{ring?.principleCount ?? 0}</span>
            <span className="ring__label">已沉淀原则</span>
          </div>
          <div className="ring__cell">
            <span className="ring__value mono">{ring?.newEdgeCount ?? 0}</span>
            <span className="ring__label">近七天新增的连接</span>
          </div>
          <div className="ring__cell">
            <span className="ring__value mono">{ring?.topNodes.length ?? 0}</span>
            <span className="ring__label">最常被想起的念头</span>
          </div>
          <div className="ring__cell">
            <span className="ring__value mono">{ring?.fastestDomains[0]?.domain ?? "—"}</span>
            <span className="ring__label">增长最快领域</span>
          </div>
        </div>
      </section>

      <section className="panel" id="setting-backup" hidden={selfView !== "system"}>
        <h2 className="section-head">备份与恢复</h2>
        <p className="setting-row__hint">
          备份会产出一份完整的数据文件，恢复前先校验完整性；数据格式升级前会自动备份一次，
          超出保留份数的旧备份会标记为已移除。
        </p>
        <button className="connector-save" type="button" onClick={() => void createBackup()}>
          立即备份
        </button>
        {backupNote ? (
          <p className="setting-row__hint" data-tone="warn">
            {backupNote}
          </p>
        ) : null}
        {backups.length === 0 ? (
          <p className="setting-row__hint">还没有备份记录。</p>
        ) : (
          <ul className="backups">
            {backups.map((backup) => (
              <li key={backup.id} className="backup" data-present={backup.present}>
                <div className="backup__head">
                  <span className="backup__kind">
                    {backup.kind === "pre_migration" ? "升级前自动备份" : "手动备份"}
                  </span>
                  <span className="backup__time">{formatTime(backup.createdAt)}</span>
                </div>
                <span className="backup__path mono">{backup.path}</span>
                <span className="backup__meta">
                  {formatBytes(backup.sizeBytes)} · 数据格式版本 {backup.schemaVersion}
                </span>
                <button
                  className="connector__test"
                  type="button"
                  disabled={!backup.present}
                  onClick={() => void restoreBackup(backup)}
                >
                  {backup.present ? "校验并恢复" : "已移除"}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel" id="setting-data" hidden={selfView !== "system"}>
        <h2 className="section-head">数据主权</h2>
        <p className="setting-row__hint">
          数据默认只存本机。导出会生成一份可以直接打开的数据文件，不会改动任何内容；
          清除会删除本机全部数据，需要二次确认，并在清除后留下一条无法撤销的记录。
        </p>
        {dataScope ? (
          <>
            <div className="setting-row">
              <span className="setting-row__label">本机数据量</span>
              <span>共 {dataScope.rowCount} 条记录</span>
            </div>
            <ul className="data-tables">
              {dataScope.tables
                .filter((table) => table.rows > 0)
                .map((table) => (
                  <li key={table.table} className="data-table">
                    <span className="data-table__label">{table.label}</span>
                    <span className="data-table__rows mono">{table.rows}</span>
                  </li>
                ))}
            </ul>
            <div className="setting-row">
              <span className="setting-row__label">导出数据</span>
              <button className="consolidate__go" type="button" onClick={() => void exportData()}>
                导出数据文件
              </button>
            </div>
            {exported ? (
              <p className="setting-row__hint">
                已导出 {exported.rowCount} 条记录 · {formatBytes(exported.bytes)}
                <br />
                <span className="mono">{exported.path}</span>
              </p>
            ) : null}
            <div className="setting-row">
              <span className="setting-row__label">清除本机数据</span>
              <button
                className="data-purge"
                type="button"
                aria-pressed={purgeArmed}
                data-armed={purgeArmed}
                onClick={() => void purgeData()}
              >
                {purgeArmed ? "确认清除，不可撤销" : "清除全部数据"}
              </button>
            </div>
            {dataNote ? (
              <p className="setting-row__hint" data-tone="warn">
                {dataNote}
              </p>
            ) : null}
            <h3 className="section-head section-head--minor">导出与清除记录</h3>
            {dataEvents.length === 0 ? (
              <p className="setting-row__hint">还没有导出或清除记录。</p>
            ) : (
              <ul className="data-events">
                {dataEvents.map((event) => (
                  <li key={event.id} className="data-event" data-kind={event.kind}>
                    <span>{formatTime(event.createdAt)}</span>
                    <span>{event.kindLabel}</span>
                    <span>{event.rowCount} 条记录</span>
                    {event.location ? (
                      <span className="mono data-event__path">{event.location}</span>
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
          </>
        ) : (
          <p className="setting-row__hint">读取中</p>
        )}
      </section>

      <section className="panel" id="setting-appearance" hidden={selfView !== "system"}>
        <h2 className="section-head">外观</h2>
        <div className="setting-row">
          <span className="setting-row__label">主题</span>
          <ThemeToggle theme={theme} onChange={onThemeChange} />
        </div>
        <p className="setting-row__hint">
          换主题只换配色，不会打乱布局，也不会重新加载画布。
        </p>
        <div className="setting-row">
          <span className="setting-row__label">降低动态效果</span>
          <button
            className="switch"
            type="button"
            role="switch"
            aria-label="降低动态效果"
            aria-checked={preferences.reduceMotion}
            data-on={preferences.reduceMotion}
            onClick={() =>
              onPreferencesChange({ reduceMotion: !preferences.reduceMotion })
            }
          >
            {preferences.reduceMotion ? "已开启" : "已关闭"}
          </button>
        </div>
        <p className="setting-row__hint">
          开启后不再飘动粒子、不再有背景呼吸感，状态与位置的变化照常显示。
        </p>
        <div className="setting-row">
          <span className="setting-row__label">高对比模式</span>
          <button
            className="switch"
            type="button"
            role="switch"
            aria-label="高对比模式"
            aria-checked={preferences.highContrast}
            data-on={preferences.highContrast}
            onClick={() =>
              onPreferencesChange({ highContrast: !preferences.highContrast })
            }
          >
            {preferences.highContrast ? "已开启" : "已关闭"}
          </button>
        </div>
        <p className="setting-row__hint">
          开启后文字与边线更清晰，每一层靠不同形状区分，不靠颜色分辨。
        </p>
      </section>

      <section className="panel" id="setting-runtime" hidden={selfView !== "system"}>
        <h2 className="section-head">运行信息</h2>
        <dl className="kv">
          <div className="kv__row">
            <dt>版本</dt>
            <dd className="mono">{app.data?.version ?? "读取中"}</dd>
          </div>
          <div className="kv__row">
            <dt>数据格式版本</dt>
            <dd className="mono">{db.data?.schemaVersion ?? "读取中"}</dd>
          </div>
          <div className="kv__row">
            <dt>数据写入方式</dt>
            <dd className="mono">{db.data?.journalMode ?? "读取中"}</dd>
          </div>
          <div className="kv__row">
            <dt>数据位置</dt>
            <dd className="mono">{db.data?.path ?? "读取中"}</dd>
          </div>
        </dl>
        {db.error ? (
          <p className="setting-row__hint" data-tone="warn">
            数据还没准备好：{db.error.message}
          </p>
        ) : null}
      </section>
    </RealmShell>
  );
}
