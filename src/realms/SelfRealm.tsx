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

const CAPTURE_LABELS: Record<string, string> = {
  clipboard_text: "剪贴板文本",
  clipboard_image: "剪贴板图片",
  window: "前台窗口",
  file: "文件活动",
};

const CONNECTOR_KINDS: readonly { readonly kind: string; readonly label: string }[] = [
  { kind: "search", label: "搜索" },
  { kind: "page", label: "网页阅读" },
  { kind: "mcp", label: "MCP 工具" },
];

const CONNECTOR_PURPOSE_LABELS: Record<string, string> = {
  council_background: "共享背景检索",
  council_seat_search: "席位补充检索",
  connector_test: "连通测试",
};

const CREDENTIAL_SCOPES: readonly { readonly scope: string; readonly label: string }[] = [
  { scope: "platform", label: "模型平台" },
  { scope: "connector", label: "连接器" },
];

const COST_POLICY_LABELS: Record<string, string> = {
  reject: "拒绝发起",
  reduce_rounds: "压缩轮次",
  reduce_seats: "压缩席位",
};

/** 整数微元换算成可读金额，1 元 = 1,000,000 微元。 */
function formatMoney(micros: number, currency: string): string {
  const unit = currency === "CNY" ? "元" : currency;
  return `${(micros / 1_000_000).toFixed(4)} ${unit}`;
}

/** 发送模式的中文说明，用于预演确认面板。 */
function queryModeLabel(mode: string): string {
  return mode === "question"
    ? "问句模式，发送脱敏后的问句"
    : "关键词模式，只发送抽取出的关键词";
}

function connectorPurposeLabel(purpose: string): string {
  return CONNECTOR_PURPOSE_LABELS[purpose] ?? purpose;
}

function captureLabel(kind: string): string {
  return CAPTURE_LABELS[kind] ?? kind;
}

function payloadExcerpt(payload: unknown): string {
  if (typeof payload === "object" && payload !== null) {
    const record = payload as Record<string, unknown>;
    for (const key of ["text", "title", "path", "imageRef"]) {
      const value = record[key];
      if (typeof value === "string" && value) {
        return value.slice(0, 80);
      }
    }
  }
  return "";
}

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
export function SelfRealm({
  theme,
  onThemeChange,
  preferences,
  onPreferencesChange,
}: {
  readonly theme: ThemeName;
  readonly onThemeChange: (next: ThemeName) => void;
  readonly preferences: Preferences;
  readonly onPreferencesChange: (patch: Partial<Preferences>) => void;
}) {
  const db = useCommand("db_status", {});
  const app = useCommand("app_info", {});
  const client = useCommands();
  const [networking, setNetworking] = useState<boolean | null>(null);
  const [platforms, setPlatforms] = useState<readonly PlatformView[]>([]);
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
  const [captureAudit, setCaptureAudit] = useState<readonly CaptureAuditView[]>([]);
  const [captureKind, setCaptureKind] = useState<CaptureKindKey | "all">("all");
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
  const [costNote, setCostNote] = useState<string | null>(null);
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
    const [enabled, platformList, recent, recordList, runList, companionState, topicList, sealList, overview, captureState, captureList, captureLog, selfState, draftDetail, scope, events, connectorList, connectorLog, costState, costEstimate, backupList] = await Promise.all([
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
      setConnectorNote("先给连接器起一个名字");
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
      setConnectorNote("连接器已保存");
    } catch (cause) {
      setConnectorNote(cause instanceof Error ? cause.message : "连接器保存失败");
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
      setConnectorNote(cause instanceof Error ? cause.message : "连接器切换失败");
    }
  }

  /**
   * 连通测试。预演开启时首次点击只拿到待发送内容与指纹，确认后才真正发出请求。
   */
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
          ? `连通测试通过 · ${call.latencyMs}ms`
          : `连通测试失败 · ${call?.errorCode ?? "未获得结果"}`,
      );
      await reloadConnectors();
    } catch (cause) {
      setConnectorNote(cause instanceof Error ? cause.message : "连通测试失败");
    }
  }

  /** 保存凭据：先写入系统凭据库，再由内核登记引用名。 */
  async function saveCredential() {
    const owner = credentialOwner.trim();
    if (!owner) {
      setCredentialNote("先填写归属标识，例如平台代码或连接器名称");
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
      setCredentialNote("密钥已写入系统凭据库，数据库只保留引用名");
    } catch (cause) {
      setCredentialNote(cause instanceof Error ? cause.message : "密钥写入失败");
    }
  }

  /** 查询某个归属在系统凭据库里是否已有密钥。 */
  async function checkCredential() {
    const owner = credentialOwner.trim();
    if (!owner) {
      setCredentialNote("先填写归属标识");
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
      setCredentialNote(cause instanceof Error ? cause.message : "凭据状态读取失败");
    }
  }

  /** 创建一份备份，并刷新保留策略下的备份列表。 */
  async function createBackup() {
    setCostNote(null);
    try {
      const created: BackupOutcome = await client.call("backup_create", {});
      setBackups(await client.call("backup_list", { limit: 10 }));
      setCostNote(
        `已创建备份 · ${(created.sizeBytes / 1024 / 1024).toFixed(1)} MB · 版本 ${created.schemaVersion}`,
      );
    } catch (cause) {
      setCostNote(cause instanceof Error ? cause.message : "备份创建失败");
    }
  }

  /** 恢复前由内核先校验完整性，校验通过才替换数据文件。 */
  async function restoreBackup(backup: BackupView) {
    setCostNote(null);
    try {
      await client.call("backup_restore", { path: backup.path });
      setCostNote("备份已校验并恢复，重启应用后生效");
    } catch (cause) {
      setCostNote(cause instanceof Error ? cause.message : "恢复失败");
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
      setNote("脱敏开关切换失败");
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
      setDataNote(`已导出 ${outcome.rowCount} 行到 ${outcome.path}`);
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
      setDataNote(`已清除 ${outcome.rowCount} 行，清除记录已留痕`);
    } catch {
      setDataNote("清除失败，已保留原有数据");
    }
  }

  return (
    <RealmShell realm="self">
      <section className="panel">
        <h2 className="section-head">外观</h2>
        <div className="setting-row">
          <span className="setting-row__label">主题</span>
          <ThemeToggle theme={theme} onChange={onThemeChange} />
        </div>
        <p className="setting-row__hint">
          切换只改令牌，不动布局，也不重载画布。
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
          开启后取消粒子与背景呼吸，保留状态色变与位置变化。
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
          开启后提高文字与描边对比度，层次由几何标记区分，不依赖颜色。
        </p>
      </section>

      <section className="panel">
        <h2 className="section-head">运行信息</h2>
        <dl className="kv">
          <div className="kv__row">
            <dt>版本</dt>
            <dd className="mono">{app.data?.version ?? "读取中"}</dd>
          </div>
          <div className="kv__row">
            <dt>数据结构版本</dt>
            <dd className="mono">{db.data?.schemaVersion ?? "读取中"}</dd>
          </div>
          <div className="kv__row">
            <dt>日志模式</dt>
            <dd className="mono">{db.data?.journalMode ?? "读取中"}</dd>
          </div>
          <div className="kv__row">
            <dt>数据位置</dt>
            <dd className="mono">{db.data?.path ?? "读取中"}</dd>
          </div>
        </dl>
        {db.error ? (
          <p className="setting-row__hint" data-tone="warn">
            数据结构未就绪：{db.error.message}
          </p>
        ) : null}
      </section>

      <section className="panel">
        <h2 className="section-head">铜镜 · 自我蒸馏</h2>
        <p className="setting-row__hint">
          累计 {self?.required ?? 20} 条思考记录后解锁。铜镜从你自己的历史记录蒸出一份初稿，
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
                aria-label="自我蒸馏解锁进度"
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
              <span className="setting-row__label">蒸出初稿</span>
              <button
                className="consolidate__go"
                type="button"
                disabled={!self.unlocked || busy}
                onClick={() => void startSelf()}
              >
                {busy ? "蒸馏中" : "启动铜镜"}
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
                  会诊席位 · 已安装 v{self.currentVersion}
                </span>
                <button
                  className="switch"
                  type="button"
                  role="switch"
                  aria-label="自我席位"
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
                上一次蒸馏未成功（{draft.draft.errorCode}）：{draft.draft.note}
              </p>
            ) : null}
            <ul className="self-items" aria-label="自我蒸馏初稿">
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
              <span className="setting-row__label">安装为我的大师包</span>
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

      <section className="panel">
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
          默认关闭。开启后每次模型调用都会写入审计，密钥由环境变量
          <span className="mono"> THOUGHT_FORGE_API_KEY </span>
          提供，不入库。
        </p>
        <ul className="platforms">
          {platforms.map((platform) => (
            <li key={platform.code} className="platform" data-enabled={platform.enabled}>
              <div className="platform__head">
                <span className="platform__name">{platform.displayName}</span>
                <span className="platform__status mono">{platform.status}</span>
              </div>
              <span className="platform__endpoint mono">{platform.endpoint || "未配置端点"}</span>
              <button
                className="platform__toggle"
                type="button"
                aria-pressed={platform.enabled}
                onClick={() => void togglePlatform(platform)}
              >
                {platform.enabled ? "停用" : "启用"}
              </button>
            </li>
          ))}
        </ul>
        {note ? (
          <p className="setting-row__hint" data-tone="warn">
            {note}
          </p>
        ) : null}
      </section>

      <section className="panel">
        <h2 className="section-head">连接器</h2>
        <p className="setting-row__hint">
          外部检索逐项开启，默认全部关闭。每次检索都会写入调用审计；MCP 服务器必须能返回工具能力声明才可接入。
        </p>
        <div className="setting-row">
          <span className="setting-row__label">类型</span>
          <span className="connector-kinds" role="group" aria-label="连接器类型">
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
            placeholder="例如：本地检索"
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
            placeholder="https://..."
            onChange={(event) => setConnectorEndpoint(event.target.value)}
          />
        </div>
        <button
          className="connector-save"
          type="button"
          onClick={() => void saveConnector()}
        >
          保存并预检
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
                <span className="connector__kind mono">{connector.kindLabel}</span>
                <span className="connector__status mono">{connector.status}</span>
              </div>
              <span className="connector__endpoint mono">
                {connector.endpoint || "未配置地址"}
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
                  连通测试
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
                命中脱敏规则，原始串里的敏感内容已按掩码替换后才进入发送串。
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
        <h3 className="section-head section-head--minor">连接器调用审计</h3>
        {connectorCalls.length === 0 ? (
          <p className="setting-row__hint">还没有连接器调用记录。</p>
        ) : (
          <ul className="calls">
            {connectorCalls.map((call) => (
              <li key={call.id} className="call" data-status={call.status}>
                <span className="call__purpose mono">
                  {connectorPurposeLabel(call.purpose)}
                </span>
                <span className="call__meta mono">
                  {call.kindLabel} · 发送 {call.querySent || "—"}
                  {call.redacted ? "（已脱敏）" : ""} · {call.resultCount} 条 ·{" "}
                  {call.latencyMs}ms
                </span>
                <span className="call__status mono">
                  {call.errorCode ?? call.status}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2 className="section-head">调用审计</h2>
        {calls.length === 0 ? (
          <p className="setting-row__hint">还没有模型调用记录。</p>
        ) : (
          <ul className="calls">
            {calls.map((call) => (
              <li key={call.id} className="call" data-status={call.status}>
                <span className="call__purpose mono">{call.purpose}</span>
                <span className="call__meta mono">
                  {call.modelName || call.platformCode || "未指定平台"} · {call.latencyMs}ms · 第{" "}
                  {call.attempt} 次
                </span>
                <span className="call__status mono">
                  {call.errorCode ?? call.status}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2 className="section-head">成本与配额</h2>
        <p className="setting-row__hint">
          每次模型与连接器调用都按整数微元记账，按 UTC 日汇总。上限与超限策略在「调参」里配置。
        </p>
        {estimate && cost ? (
          <div className="cost-grid">
            <div className="cost-cell">
              <span className="cost-cell__label">单场估算</span>
              <span className="cost-cell__value mono">
                {formatMoney(estimate.costMicros, cost.currency)}
              </span>
              <span className="cost-cell__note mono">
                {estimate.llmCalls} 次模型 · {estimate.searchCalls} 次检索
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
        {costNote ? (
          <p className="setting-row__hint" data-tone="warn">
            {costNote}
          </p>
        ) : null}

        <h3 className="section-head section-head--minor">凭据</h3>
        <p className="setting-row__hint">
          密钥写入操作系统凭据库，数据库只保存引用名，任何字段都不会出现密钥正文。
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
            placeholder="平台代码或连接器名称"
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
            写入凭据库
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

        <h3 className="section-head section-head--minor">备份与恢复</h3>
        <p className="setting-row__hint">
          备份产出完整数据文件，恢复前先做完整性校验；迁移前会自动创建一份
          pre_migration 备份，超出保留份数的旧备份只标记移除。
        </p>
        <button className="connector-save" type="button" onClick={() => void createBackup()}>
          立即备份
        </button>
        {backups.length === 0 ? (
          <p className="setting-row__hint">还没有备份记录。</p>
        ) : (
          <ul className="backups">
            {backups.map((backup) => (
              <li key={backup.id} className="backup" data-present={backup.present}>
                <div className="backup__head">
                  <span className="backup__kind mono">{backup.kind}</span>
                  <span className="backup__time mono">{backup.createdAt}</span>
                </div>
                <span className="backup__path mono">{backup.path}</span>
                <span className="backup__meta mono">
                  {(backup.sizeBytes / 1024 / 1024).toFixed(1)} MB · 版本 {backup.schemaVersion} ·{" "}
                  {backup.checksum.slice(0, 12)}
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

      <section className="panel">
        <h2 className="section-head">采集台</h2>
        <p className="setting-row__hint">
          四类采集逐项开关，默认全部关闭。每次开启都要一次显式确认并写入审计；
          全局暂停时不轮询、不写入。
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
                        ? `已同意 · ${capability.consentedAt.slice(0, 10)}`
                        : "尚未开启"
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
            {capture.capabilities.some((capability) => !capability.available) ? (
              <p className="setting-row__hint" data-tone="warn">
                炉口闭合的能力已被系统拒绝：请前往系统设置的隐私与安全页面授权，
                授权后回到这里重新开启。其他能力不受影响。
              </p>
            ) : null}
            <div className="setting-row">
              <span className="setting-row__label">脱敏</span>
              <button
                className="switch"
                type="button"
                role="switch"
                aria-label="脱敏"
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
                    {captureLabel(capability.kind)}
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
                      {captureLabel(event.kind)}
                    </span>
                    <span className="capture-row__time mono">
                      {event.occurredAt.slice(0, 16).replace("T", " ")}
                    </span>
                    <span className="capture-row__excerpt">
                      {payloadExcerpt(event.payload) || "无正文"}
                    </span>
                    {event.redacted ? (
                      <span className="capture-row__tag">已脱敏</span>
                    ) : null}
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
            <h3 className="section-head section-head--minor">开启审计</h3>
            {captureAudit.length === 0 ? (
              <p className="setting-row__hint">还没有开启记录。</p>
            ) : (
              <ul className="capture-audit">
                {captureAudit.map((entry) => (
                  <li key={entry.id} className="capture-audit__row">
                    <span className="mono">
                      {entry.createdAt.slice(0, 16).replace("T", " ")}
                    </span>
                    <span>{captureLabel(entry.kind)}</span>
                    <span className="mono">{auditActionLabel(entry.action)}</span>
                  </li>
                ))}
              </ul>
            )}
          </>
        ) : (
          <p className="setting-row__hint">读取中</p>
        )}
      </section>

      <section className="panel">
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
                  <span className="record__time mono">{record.createdAt.slice(0, 10)}</span>
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
                  <span className="chain__time mono">{record.createdAt.slice(0, 10)}</span>
                  <span className="chain__text">{record.conclusion}</span>
                </li>
              ))}
            </ol>
          </div>
        ) : null}
      </section>

      <section className="panel">
        <h2 className="section-head">记忆固化</h2>
        <p className="setting-row__hint">
          强化刚被共同唤起的连线，衰减陈旧的，合并重复节点，识别矛盾。
        </p>
        <button className="consolidate__go" type="button" onClick={() => void consolidate()}>
          立即固化
        </button>
        {report ? (
          <dl className="kv">
            <div className="kv__row">
              <dt>强化连线</dt>
              <dd className="mono">{report.strengthenedCount}</dd>
            </div>
            <div className="kv__row">
              <dt>衰减连线</dt>
              <dd className="mono">{report.decayedCount}</dd>
            </div>
            <div className="kv__row">
              <dt>合并节点</dt>
              <dd className="mono">{report.mergedCount}</dd>
            </div>
            <div className="kv__row">
              <dt>识别冲突</dt>
              <dd className="mono">{report.conflictCount}</dd>
            </div>
          </dl>
        ) : null}
        {runs.length > 0 ? (
          <ul className="runs">
            {runs.map((run) => (
              <li key={run.id} className="run">
                <span className="run__mode mono">{run.mode}</span>
                <span className="run__time mono">{run.startedAt.slice(0, 16).replace("T", " ")}</span>
                <span className="run__counts mono">
                  +{run.strengthenedCount} / -{run.decayedCount} / 并{run.mergedCount} / 冲
                  {run.conflictCount}
                </span>
              </li>
            ))}
          </ul>
        ) : null}
      </section>

      <section className="panel">
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

      <section className="panel">
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
              <span className="setting-row__label">思考记录落库时碰撞</span>
              <button
                className="switch"
                type="button"
                role="switch"
                aria-label="思考记录触发"
                aria-checked={companion.rules.triggerOnRecord}
                data-on={companion.rules.triggerOnRecord}
                onClick={() => void toggleRule("triggerOnRecord")}
              >
                {companion.rules.triggerOnRecord ? "已开启" : "已关闭"}
              </button>
            </div>
            <div className="setting-row">
              <span className="setting-row__label">采集内容落库时碰撞</span>
              <button
                className="switch"
                type="button"
                role="switch"
                aria-label="采集触发"
                aria-checked={companion.rules.triggerOnCapture}
                data-on={companion.rules.triggerOnCapture}
                onClick={() => void toggleRule("triggerOnCapture")}
              >
                {companion.rules.triggerOnCapture ? "已开启" : "已关闭"}
              </button>
            </div>
          </>
        ) : (
          <p className="setting-row__hint">读取中</p>
        )}
      </section>

      <section className="panel">
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

      <section className="panel">
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
                <p className="seal__meta mono">
                  采纳 {seal.adoptedCount} 次 · 激活 {seal.activation.toFixed(2)} ·{" "}
                  {seal.layers.join("/") || "未标注层次"}
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
                  <p className="seal__meta mono">
                    撤销于 {item.at} · 原因：{item.reason || "未填写"}
                  </p>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </section>

      <section className="panel">
        <h2 className="section-head">年轮概览</h2>
        <div className="ring">
          <div className="ring__cell">
            <span className="ring__value mono">{ring?.principleCount ?? 0}</span>
            <span className="ring__label">已沉淀原则</span>
          </div>
          <div className="ring__cell">
            <span className="ring__value mono">{ring?.newEdgeCount ?? 0}</span>
            <span className="ring__label">近七天新增连线</span>
          </div>
          <div className="ring__cell">
            <span className="ring__value mono">{ring?.topNodes.length ?? 0}</span>
            <span className="ring__label">活跃认知节点</span>
          </div>
          <div className="ring__cell">
            <span className="ring__value mono">{ring?.fastestDomains[0]?.domain ?? "—"}</span>
            <span className="ring__label">增长最快领域</span>
          </div>
        </div>
      </section>

      <section className="panel">
        <h2 className="section-head">数据主权</h2>
        <p className="setting-row__hint">
          数据默认只存本机。导出是一份可读的 JSON 归档，只读不改动；清除会删除本机全部数据，
          需要二次确认，并在清除后留下一条不可回滚的痕迹。
        </p>
        {dataScope ? (
          <>
            <div className="setting-row">
              <span className="setting-row__label">本机数据范围</span>
              <span className="mono">
                {dataScope.tableCount} 张表 · {dataScope.rowCount} 行
              </span>
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
              <span className="setting-row__label">导出归档</span>
              <button className="consolidate__go" type="button" onClick={() => void exportData()}>
                导出为 JSON
              </button>
            </div>
            {exported ? (
              <p className="setting-row__hint">
                已导出 {exported.rowCount} 行 · {exported.tableCount} 张表 ·{" "}
                {(exported.bytes / 1024).toFixed(1)} KB
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
            <h3 className="section-head section-head--minor">数据变动留痕</h3>
            {dataEvents.length === 0 ? (
              <p className="setting-row__hint">还没有导出或清除记录。</p>
            ) : (
              <ul className="data-events">
                {dataEvents.map((event) => (
                  <li key={event.id} className="data-event" data-kind={event.kind}>
                    <span className="mono">
                      {event.createdAt.slice(0, 16).replace("T", " ")}
                    </span>
                    <span>{event.kindLabel}</span>
                    <span className="mono">
                      {event.tableCount} 表 / {event.rowCount} 行
                    </span>
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
    </RealmShell>
  );
}
