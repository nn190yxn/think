/**
 * command 注册表：命令名到请求响应类型的唯一映射来源。
 * 前后端签名以这里为准，Rust 侧命令名必须与此一致。
 */

import type { LayerKey } from "../domain/layers";

/** 需要持久化的设置项。 */
/** 通用设置键。采集的关注目录用 JSON 字符串数组表达。 */
export type SettingKey = "theme" | "capture.watch_roots";

export interface AppInfo {
  readonly name: string;
  readonly version: string;
  readonly schemaVersion: number;
}

export interface DbStatus {
  readonly path: string;
  readonly schemaVersion: number;
  readonly journalMode: string;
  readonly foreignKeys: boolean;
}

export interface FurnaceSnapshot {
  readonly activeNodes: number;
  readonly totalNodes: number;
  readonly recentCaptures: number;
  readonly recentCouncils: number;
  readonly computedAt: string;
}

export interface MasterSummary {
  readonly id: string;
  readonly name: string;
  readonly domain: string;
  readonly layers: readonly LayerKey[];
  readonly status: string;
  readonly currentVersion: number;
  readonly unitCount: number;
  /** 六题积累：按道法术气器势给出每题单元数，空缺题计数为零。 */
  readonly layerProfile: readonly LayerProfile[];
  readonly installedAt: string;
  readonly updatedAt: string;
}

export interface CitationView {
  readonly corpusItemId?: string;
  readonly excerpt: string;
  readonly location: string;
  readonly available: boolean;
}

export interface MasterUnitView {
  readonly id: string;
  readonly title: string;
  readonly layer: LayerKey;
  readonly triggerCondition: string;
  readonly steps: readonly string[];
  readonly mechanism: string;
  readonly boundary: string;
  readonly flaggedReason?: string;
  readonly citations: readonly CitationView[];
}

export interface VersionDiff {
  readonly added: readonly string[];
  readonly updated: readonly string[];
  readonly carried: number;
  readonly sourceRefs: readonly string[];
}

export interface VersionView {
  readonly version: number;
  readonly unitCount: number;
  readonly note: string;
  readonly createdAt: string;
  readonly diff: VersionDiff;
}

export interface MasterDetail {
  readonly id: string;
  readonly name: string;
  readonly domain: string;
  readonly layers: readonly LayerKey[];
  readonly status: string;
  readonly currentVersion: number;
  readonly summary: string;
  readonly style: string;
  readonly blindSpots: string;
  readonly units: readonly MasterUnitView[];
  readonly versions: readonly VersionView[];
  /** 六题档案：按道法术气器势顺序给出每题上的积累深浅。 */
  readonly layerProfile: readonly LayerProfile[];
}

export interface LayerProfile {
  readonly layer: LayerKey;
  readonly name: string;
  readonly question: string;
  readonly unitCount: number;
  readonly unitTitles: readonly string[];
}

export interface InstallOutcome {
  readonly masterId: string;
  readonly version: number;
  readonly unitCount: number;
  readonly corpusCount: number;
  readonly created: boolean;
  readonly diff: VersionDiff;
}

export interface LayerCoverage {
  readonly layer: LayerKey;
  readonly name: string;
  readonly masterCount: number;
  readonly unitCount: number;
  readonly masters: readonly string[];
}

export interface DomainCoverage {
  readonly domain: string;
  readonly masterCount: number;
  readonly presentLayers: readonly LayerKey[];
  readonly missingLayers: readonly LayerKey[];
}

export interface CoverageMatrix {
  readonly layers: readonly LayerCoverage[];
  readonly domains: readonly DomainCoverage[];
  readonly suggestions: readonly string[];
  readonly masterCount: number;
}

export interface CorpusItemView {
  readonly id: string;
  readonly sourceKind: string;
  readonly sourceRef: string;
  readonly title: string;
  readonly normalizedName: string;
  readonly locationHint: string;
  readonly contentHash: string;
  readonly byteSize: number;
  readonly available: boolean;
  readonly masterIds: readonly string[];
  readonly registeredAt: string;
}

export interface CorpusSearchHit {
  readonly item: CorpusItemView;
  readonly matchedBy: "fts" | "like";
}

export interface SeedFailure {
  readonly pack: string;
  readonly message: string;
}

export interface SeedInstallReport {
  readonly root: string;
  readonly installed: readonly InstallOutcome[];
  readonly failures: readonly SeedFailure[];
}

/** 轮换策略：稳妥看相关度，碰撞看对立度，意外看领域距离。 */
export type CouncilStrategy = "steady" | "clash" | "serendipity";

export interface CouncilCandidate {
  readonly masterId: string;
  readonly name: string;
  readonly domain: string;
  readonly layers: readonly LayerKey[];
  readonly relevance: number;
  readonly opposition: number;
  readonly domainDistance: number;
}

export interface CandidatePool {
  readonly question: string;
  readonly domains: readonly string[];
  readonly topicTokens: readonly string[];
  readonly candidates: readonly CouncilCandidate[];
  readonly missingLayers: readonly LayerKey[];
}

export interface CouncilSeat {
  readonly masterId: string;
  readonly name: string;
  readonly domain: string;
  readonly layers: readonly LayerKey[];
  readonly layer: LayerKey;
  readonly score: number;
  readonly pinned: boolean;
}

export interface CouncilSelection {
  readonly strategy: CouncilStrategy;
  readonly size: number;
  readonly seats: readonly CouncilSeat[];
  readonly layers: readonly LayerKey[];
  readonly gaps: readonly LayerKey[];
}

export interface CouncilPanel {
  readonly rotation: number;
  readonly strategy: CouncilStrategy;
  readonly masterIds: readonly string[];
  readonly pinnedIds: readonly string[];
  /** 与 masterIds 同序的席位指派；历史阵容可能为空数组。 */
  readonly seats: readonly CouncilSeatRef[];
  readonly layers: readonly LayerKey[];
  readonly gaps: readonly LayerKey[];
  readonly createdAt: string;
}

/** 席位被指派到的题。 */
export interface CouncilSeatRef {
  readonly masterId: string;
  readonly layer: LayerKey;
}

export type CouncilTurnRole = "answer" | "cross" | "synthesis";

/** 一个质询轮的相似度与分歧指标。 */
export interface CouncilRoundMetric {
  readonly sessionId: string;
  readonly panelRotation: number;
  readonly round: number;
  readonly participantCount: number;
  readonly avgSimilarity: number;
  readonly minSimilarity: number;
  readonly divergence: number;
  readonly converged: boolean;
  /** 本轮分歧判定方式：lexical / polarity / hybrid。 */
  readonly method: string;
  /** 极性判定不可用而回退到词面判定时为真。 */
  readonly fellBack: boolean;
  readonly createdAt: string;
}

export interface CouncilTurn {
  readonly id: string;
  readonly round: number;
  readonly panelRotation: number;
  readonly role: string;
  readonly masterId: string | null;
  readonly masterVersion: number | null;
  readonly content: string;
  readonly citations: readonly string[];
  readonly status: string;
  readonly errorCode: string | null;
  readonly createdAt: string;
}

/** 一条还没谈拢的地方：分歧落在哪一题，以及分歧本身。 */
export interface DivergenceView {
  readonly layer: LayerKey;
  readonly text: string;
}

/** 一个席位在自己那一题上的立场摘要。 */
export interface CouncilStance {
  readonly masterId: string;
  readonly masterName: string;
  readonly layer: LayerKey;
  readonly summary: string;
}

/** 同一题与上一次同主题会诊相比的立场变化。 */
export interface CouncilStanceChange {
  readonly layer: LayerKey;
  readonly masterId: string;
  readonly masterName: string;
  readonly previousMasterName: string | null;
  /** 本轮立场摘要；停谈时为空。 */
  readonly summary: string;
  /** 上一轮立场摘要；新谈时为空。 */
  readonly previousSummary: string | null;
  /** 两轮摘要的用词重合度，0 到 1。 */
  readonly similarity: number;
  /** same 延续、adjusted 调整、shifted 转向、new 新谈、dropped 停谈。 */
  readonly change: string;
}

export interface CouncilSessionView {
  readonly id: string;
  readonly question: string;
  readonly domains: readonly string[];
  readonly layers: readonly LayerKey[];
  readonly strategy: CouncilStrategy;
  readonly status: string;
  readonly conclusion: string;
  readonly divergences: readonly DivergenceView[];
  readonly rotationCount: number;
  readonly turnCount: number;
  /** 追问会话的母会话标识；普通会诊为 null。 */
  readonly parentSessionId: string | null;
  readonly anchorKind: string;
  readonly anchorText: string;
  readonly anchorMasterId: string | null;
  readonly anchorRound: number | null;
  readonly anchorTruncated: boolean;
  readonly panelInherited: boolean;
  /** 本次会诊是否包含「你」的席位。 */
  readonly selfSeatIncluded: boolean;
  /** 是否已请求取消；取消在当前轮次结束时生效。 */
  readonly cancelRequested: boolean;
  readonly cancelledAt: string | null;
  /** 最近一次心跳时刻，用于判定会话是否中断。 */
  readonly heartbeatAt: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
}

/** 一条与既有个人原则高度重合的命中。 */
export interface EchoHit {
  readonly nodeId: string;
  readonly content: string;
  readonly overlap: number;
}

export type FollowUpAnchorKind = "conclusion" | "answer" | "critique" | "divergence";

/** 追问锚点：指向母会话里被追问的那段判断。 */
export interface FollowUpAnchor {
  readonly kind: FollowUpAnchorKind;
  readonly text: string;
  readonly masterId?: string | null;
  readonly round?: number | null;
}

/** 一个席位在某一轮的发言。 */
export interface CouncilRoundSpeech {
  readonly round: number;
  readonly role: string;
  readonly content: string;
  readonly status: string;
  readonly errorCode: string | null;
  readonly createdAt: string;
}

/** 逐席发言：席位状态与该席位各轮发言。 */
export interface CouncilSeatSpeech {
  readonly masterId: string;
  readonly masterName: string;
  readonly layer: LayerKey;
  readonly status: string;
  readonly rounds: readonly CouncilRoundSpeech[];
}

/** 检索快照视图。masterId 为 null 表示共享背景。 */
export interface CouncilSourceView {
  readonly id: string;
  readonly kind: string;
  readonly title: string;
  readonly url: string;
  readonly snippet: string;
  readonly publishedAt: string | null;
  readonly fetchedAt: string;
  readonly hasBody: boolean;
  /** 命中注入特征，仅作资料看待。 */
  readonly flagged: boolean;
  readonly masterId: string | null;
  readonly round: number;
}

/** 会诊结论详情页的六段数据。 */
export interface CouncilConclusionView {
  readonly session: CouncilSessionView;
  readonly metrics: readonly CouncilRoundMetric[];
  readonly speeches: readonly CouncilSeatSpeech[];
  readonly sources: readonly CouncilSourceView[];
  readonly history: readonly CouncilSessionView[];
  /** 每题立场与上一次同主题会诊相比的变化；没有可比记录时为空。 */
  readonly stanceChanges: readonly CouncilStanceChange[];
  /** 生成该结论所用的提示词模板版本，空串表示未记录。 */
  readonly promptVersion: string;
  /** 本场会诊已记录的模型调用次数。 */
  readonly llmCalls: number;
  /** 本场会诊已记录的检索调用次数。 */
  readonly searchCalls: number;
  /** 按本场调用次数与当前单价估算的费用，整数微元。 */
  readonly costMicros: number;
  readonly currency: string;
  /** 是否至少配置了一项非零单价；为假时费用按零计。 */
  readonly priced: boolean;
}

export interface CouncilSessionDetail {
  readonly session: CouncilSessionView;
  readonly panels: readonly CouncilPanel[];
  readonly turns: readonly CouncilTurn[];
  readonly metrics: readonly CouncilRoundMetric[];
}

export interface CouncilOutcome {
  readonly sessionId: string;
  readonly rotation: number;
  readonly answered: number;
  readonly failed: number;
  readonly conclusion: string;
  readonly divergences: readonly DivergenceView[];
  readonly rounds: number;
  readonly metrics: readonly CouncilRoundMetric[];
}

/** 一次搜索命中的一条结果。 */
export interface SearchHit {
  readonly title: string;
  readonly url: string;
  readonly snippet: string;
  readonly publishedAt: string | null;
}

/** 连接器配置视图。密钥不入库，config 只放非敏感参数。 */
export interface ConnectorView {
  readonly id: string;
  readonly kind: string;
  readonly kindLabel: string;
  readonly displayName: string;
  readonly endpoint: string;
  readonly config: unknown;
  readonly enabled: boolean;
  readonly status: string;
  readonly createdAt: string;
  readonly updatedAt: string;
}

/** 连接器调用审计。 */
export interface ConnectorCallView {
  readonly id: string;
  readonly connectorId: string | null;
  readonly kind: string;
  readonly kindLabel: string;
  readonly purpose: string;
  readonly sessionId: string | null;
  readonly query: string;
  /** 用户提出的原始问句。 */
  readonly queryOriginal: string;
  /** 脱敏与模式转换后实际发出的串。 */
  readonly querySent: string;
  readonly redacted: boolean;
  readonly resultCount: number;
  readonly latencyMs: number;
  readonly status: string;
  readonly errorCode: string | null;
  readonly createdAt: string;
}

/** 检索前的预演结果：pending 为真时尚未发出请求。 */
export interface PreparedQueryView {
  readonly original: string;
  readonly sent: string;
  readonly redacted: boolean;
  readonly mode: string;
}

export interface SearchOutcome {
  readonly pending: boolean;
  readonly fingerprint: string;
  readonly prepared: PreparedQueryView | null;
  readonly hits: readonly SearchHit[];
}

export interface ConnectorTestOutcome {
  readonly pending: boolean;
  readonly fingerprint: string;
  readonly prepared: PreparedQueryView | null;
  readonly call: ConnectorCallView | null;
}

/** 可调参数的当前取值及其边界。 */
export interface TuningItem {
  readonly key: string;
  readonly label: string;
  readonly group: string;
  readonly unit: string;
  readonly kind: "int" | "float" | "bool" | "text";
  readonly value: string;
  readonly defaultValue: string;
  readonly min: number;
  readonly max: number;
  readonly note: string;
  readonly customized: boolean;
}

export interface MasterHistoryEntry {
  readonly sessionId: string;
  readonly question: string;
  readonly round: number;
  readonly panelRotation: number;
  readonly role: string;
  readonly masterVersion: number | null;
  readonly content: string;
  readonly createdAt: string;
}

export interface PlatformView {
  readonly id: string;
  readonly code: string;
  readonly displayName: string;
  readonly endpoint: string;
  readonly modelName: string;
  readonly inputPriceMicrosPer1k: number;
  readonly outputPriceMicrosPer1k: number;
  readonly currency: string;
  readonly enabled: boolean;
  readonly status: string;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface CostDayView {
  readonly day: string;
  readonly calls: number;
  readonly tokens: number;
  readonly costMicros: number;
}

export interface CostSummary {
  readonly days: readonly CostDayView[];
  readonly todayMicros: number;
  readonly monthMicros: number;
  readonly dailyLimitMicros: number;
  readonly monthlyLimitMicros: number;
  readonly policy: string;
  readonly currency: string;
  readonly priced: boolean;
}

export interface CostEstimate {
  readonly llmCalls: number;
  readonly searchCalls: number;
  readonly tokens: number;
  readonly costMicros: number;
  readonly platformCode: string;
  readonly priced: boolean;
}

export interface BackupView {
  readonly id: string;
  readonly path: string;
  readonly sizeBytes: number;
  readonly checksum: string;
  readonly schemaVersion: number;
  readonly kind: string;
  readonly present: boolean;
  readonly createdAt: string;
}

export interface BackupOutcome {
  readonly path: string;
  readonly sizeBytes: number;
  readonly checksum: string;
  readonly schemaVersion: number;
  readonly kind: string;
  readonly createdAt: string;
}

export interface CredentialRefView {
  readonly id: string;
  readonly refName: string;
  readonly scope: string;
  readonly ownerId: string;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface ModelProbeOutcome {
  readonly ok: boolean;
  /** 0 成功、1 模型不可用、2 联网关闭、3 平台未配置。 */
  readonly exitCode: number;
  readonly platformCode: string;
  readonly modelName: string;
  readonly latencyMs: number;
  /** 本次探针的调用审计标识，未发起调用时为空串。 */
  readonly callId: string;
  readonly snippet: string;
  readonly errorCode: string | null;
  /** 是否解析到可用密钥，只报存在与否。 */
  readonly keyPresent: boolean;
}

export interface LlmCall {
  readonly id: string;
  readonly purpose: string;
  readonly platformCode: string;
  readonly modelName: string;
  readonly promptTokens: number;
  readonly completionTokens: number;
  readonly latencyMs: number;
  readonly attempt: number;
  readonly status: string;
  readonly errorCode: string | null;
  readonly createdAt: string;
}

/** 认知节点类型。 */
export type NodeKind =
  | "idea"
  | "judgment"
  | "framework"
  | "principle"
  | "question"
  | "evidence";

/** 认知关系类型。冲突关系双向可见，其余保持方向语义。 */
export type EdgeRelation = "supports" | "conflicts" | "derives" | "analogous" | "applies";

export interface ThoughtNode {
  readonly id: string;
  readonly kind: NodeKind;
  readonly content: string;
  readonly normalizedContent: string;
  readonly sourceKind: string;
  readonly sourceRef: string;
  readonly domains: readonly string[];
  readonly layers: readonly LayerKey[];
  readonly activation: number;
  readonly activationUpdatedAt: string;
  readonly version: number;
  readonly supersededBy?: string;
  readonly clusterId: string | null;
  readonly createdAt: string;
}

export interface GraphNode {
  readonly id: string;
  readonly kind: NodeKind;
  readonly content: string;
  readonly domains: readonly string[];
  readonly layers: readonly LayerKey[];
  readonly activation: number;
  readonly activationUpdatedAt: string;
  readonly clusterId: string | null;
}

export interface GraphEdge {
  readonly id: string;
  readonly from: string;
  readonly to: string;
  readonly relation: EdgeRelation;
  readonly weight: number;
}

export interface GraphView {
  readonly nodes: readonly GraphNode[];
  readonly edges: readonly GraphEdge[];
  readonly clusters: readonly ClusterView[];
  readonly totalNodes: number;
  readonly truncated: boolean;
}

/** 固化时算出的认知社区：同一批想法里互相支撑的一组。 */
export interface ClusterView {
  readonly id: string;
  readonly label: string;
  readonly domain: string;
  readonly layer: string;
  readonly memberCount: number;
  readonly memberIds: readonly string[];
}

export interface NodeLink {
  readonly edgeId: string;
  readonly relation: EdgeRelation;
  readonly weight: number;
  readonly status: string;
  readonly direction: "in" | "out" | "both";
  readonly peerId: string;
  readonly peerKind: NodeKind;
  readonly peerContent: string;
}

export interface NodeActivation {
  readonly id: string;
  readonly sessionId: string | null;
  readonly increment: number;
  readonly occurredAt: string;
}

export interface NodeDetail {
  readonly node: ThoughtNode;
  readonly links: readonly NodeLink[];
  readonly activations: readonly NodeActivation[];
}

export interface NodeUpsert {
  readonly nodeId: string;
  readonly outcome: "created" | "matched";
}

export interface EdgeUpsert {
  readonly edgeId: string;
  readonly created: boolean;
  readonly weight: number;
}

export interface ActivationOutcome {
  readonly activated: number;
  readonly propagated: number;
  readonly propagatedFar: number;
  readonly hops: number;
}

export interface ThoughtRecord {
  readonly id: string;
  readonly sessionId?: string;
  readonly question: string;
  readonly topicKey: string;
  readonly domains: readonly string[];
  readonly layers: readonly LayerKey[];
  readonly conclusion: string;
  readonly adopted: boolean;
  readonly reason: string;
  readonly createdAt: string;
}

export interface RecordOutcome {
  readonly recordId: string;
  readonly judgmentId: string;
  readonly frameworkIds: readonly string[];
  readonly divergenceIds: readonly string[];
  readonly linkedPrior: readonly string[];
  readonly activated: number;
}

export interface ConsolidationReport {
  readonly runId: string;
  readonly mode: "manual" | "idle";
  readonly strengthenedCount: number;
  readonly decayedCount: number;
  readonly mergedCount: number;
  readonly conflictCount: number;
  readonly clusterCount: number;
  readonly merged: readonly (readonly [string, string])[];
  readonly conflicts: readonly (readonly [string, string])[];
  readonly startedAt: string;
  readonly finishedAt: string;
}

export interface ConsolidationRun {
  readonly id: string;
  readonly mode: string;
  readonly startedAt: string;
  readonly finishedAt: string | null;
  readonly strengthenedCount: number;
  readonly decayedCount: number;
  readonly mergedCount: number;
  readonly conflictCount: number;
}

/** 主动助理洞察类型。 */
export type InsightKindKey = "relation" | "conflict" | "blindspot";

/** 主动助学的触发规则。 */
export interface CompanionRules {
  readonly triggerOnRecord: boolean;
  readonly triggerOnCapture: boolean;
  readonly minActivation: number;
  readonly contextNodes: number;
}

export interface CompanionSettings {
  readonly enabled: boolean;
  readonly dailyLimit: number;
  readonly rules: CompanionRules;
  readonly usedToday: number;
  readonly updatedAt: string;
}

/** 一条由助理推送或固化识别的洞察。 */
export interface Insight {
  readonly id: string;
  readonly kind: InsightKindKey;
  readonly title: string;
  readonly summary: string;
  readonly relatedNodeIds: readonly string[];
  readonly relatedMasterIds: readonly string[];
  readonly evidence: readonly string[];
  readonly status: string;
  readonly action: string;
  readonly reason: string;
  readonly source: string;
  readonly createdAt: string;
}

/** 触发一次后台碰撞的信号。 */
export interface CollisionSignal {
  readonly sourceKind: string;
  readonly sourceRef: string;
  readonly content: string;
  readonly domains?: readonly string[];
  readonly layers?: readonly LayerKey[];
}

export interface CollisionOutcome {
  readonly generated: readonly Insight[];
  readonly pushed: number;
  readonly skipped?: string;
  readonly remaining: number;
}

/** 同一主题下的判断演化聚合。 */
export interface TopicView {
  readonly topicKey: string;
  readonly recordCount: number;
  readonly adoptedCount: number;
  readonly latestConclusion: string;
  readonly latestAt: string;
  readonly createdAt: string;
}

export interface DomainTrend {
  readonly domain: string;
  readonly count: number;
}

/** 年轮概览。 */
export interface RingOverview {
  readonly topNodes: readonly GraphNode[];
  readonly fastestDomains: readonly DomainTrend[];
  readonly newEdgeCount: number;
  readonly principleCount: number;
}

/** 已沉淀的个人原则印章。 */
export interface PrincipleSeal {
  readonly nodeId: string;
  readonly content: string;
  readonly domains: readonly string[];
  readonly layers: readonly LayerKey[];
  readonly activation: number;
  readonly adoptedCount: number;
  readonly createdAt: string;
}

/** 蒸馏阶段。顺序即执行顺序。 */
export type DistillStageKey =
  | "skeleton"
  | "extract"
  | "verify"
  | "compose"
  | "map"
  | "stress"
  | "deliver"
  | "done";

export type DistillStateKey =
  | "pending"
  | "awaiting_confirmation"
  | "running"
  | "failed"
  | "done";

export type IntakeStateKey =
  | "queued"
  | "awaiting_confirmation"
  | "confirmed"
  | "rejected"
  | "distilling"
  | "done"
  | "failed";

export type SignalStatusKey = "pending" | "accepted" | "rejected";

/** 一份入库材料。文件与链接只登记来源，文本型材料随任务保存正文。 */
export interface IntakeMaterial {
  readonly title: string;
  readonly kind?: string;
  readonly sourceRef?: string;
  readonly text?: string;
}

export interface SkeletonDraft {
  readonly summary: string;
  readonly domain: string;
  readonly layers: readonly LayerKey[];
  readonly themes: readonly string[];
  readonly angles: readonly string[];
}

export interface CandidateDraft {
  readonly id: string;
  readonly track: string;
  readonly title: string;
  readonly summary: string;
  readonly layer: LayerKey;
  readonly evidence: readonly string[];
}

export interface ExcludedCandidate {
  readonly id: string;
  readonly track: string;
  readonly title: string;
  readonly stage: string;
  readonly reason: string;
}

export interface SkillUnitDraft {
  readonly candidateId: string;
  readonly title: string;
  readonly layer: LayerKey;
  readonly triggerCondition: string;
  readonly steps: readonly string[];
  readonly mechanism: string;
  readonly boundary: string;
  readonly evidence: readonly string[];
}

export interface SkillGroup {
  readonly layer: LayerKey;
  readonly name: string;
  readonly titles: readonly string[];
}

export interface SkillLink {
  readonly from: string;
  readonly to: string;
  readonly relation: string;
}

export interface StressCase {
  readonly question: string;
  readonly decoy: boolean;
  readonly expected: string;
  readonly answer: string;
  readonly passed: boolean;
}

/** 检查点草稿，体现流水线到当前阶段的全部产出。 */
export interface DistillDraft {
  readonly skeleton?: SkeletonDraft;
  readonly extracted: readonly CandidateDraft[];
  readonly verified: readonly CandidateDraft[];
  readonly excluded: readonly ExcludedCandidate[];
  readonly units: readonly SkillUnitDraft[];
  readonly skillMap: readonly SkillGroup[];
  readonly links: readonly SkillLink[];
  readonly stress: readonly StressCase[];
  readonly stressPassRate: number;
  readonly packDir?: string;
  readonly outcome?: InstallOutcome;
}

export interface DistillJobView {
  readonly id: string;
  readonly sourceKind: string;
  readonly sourceRef: string;
  readonly masterId: string;
  readonly masterName: string;
  readonly domain: string;
  readonly outputDir: string;
  readonly stage: DistillStageKey;
  readonly stageName: string;
  readonly state: DistillStateKey;
  readonly materialCount: number;
  readonly modelCalls: number;
  readonly errorCode?: string;
  readonly updatedAt: string;
  readonly createdAt: string;
}

export interface DistillDetail {
  readonly job: DistillJobView;
  readonly draft: DistillDraft;
}

export interface OverlapSummary {
  readonly materials: number;
  readonly duplicates: number;
  readonly maxRatio: number;
  readonly notes: readonly string[];
}

export interface IntakeJobView {
  readonly id: string;
  readonly masterRef: string;
  readonly masterName: string;
  readonly domain: string;
  readonly mode: string;
  readonly state: IntakeStateKey;
  readonly materialCount: number;
  readonly acceptedCount: number;
  readonly rejectedCount: number;
  readonly overlap: OverlapSummary;
  readonly updatedAt: string;
  readonly createdAt: string;
}

export interface SignalView {
  readonly id: string;
  readonly jobId: string;
  readonly masterId: string;
  readonly title: string;
  readonly sourceRef: string;
  readonly kind: string;
  readonly text: string;
  readonly status: SignalStatusKey;
  readonly decisionReason: string;
  readonly overlapRatio: number;
  readonly discoveredAt: string;
}

export interface DiscoverySettings {
  readonly enabled: boolean;
  readonly schedule: unknown;
  readonly updatedAt: string;
}

export interface DiscoveryOutcome {
  readonly triggered: boolean;
  readonly reason?: string;
  readonly discovered: number;
  readonly saved: number;
  readonly pending: number;
  readonly jobId?: string;
}

export type CaptureKindKey =
  | "clipboard_text"
  | "clipboard_image"
  | "window"
  | "file";

export interface CaptureCapabilityView {
  readonly kind: CaptureKindKey;
  readonly label: string;
  readonly enabled: boolean;
  readonly available: boolean;
  readonly consentedAt: string | null;
}

export interface CaptureSettingsView {
  readonly paused: boolean;
  readonly dedupSeconds: number;
  readonly redactionEnabled: boolean;
  readonly redactionTerms: number;
  readonly capabilities: readonly CaptureCapabilityView[];
  /** 当前生效的受关注目录。为空时文件活动不可用。 */
  readonly watchRoots: readonly string[];
}

export interface CaptureEventView {
  readonly id: string;
  readonly kind: CaptureKindKey;
  readonly occurredAt: string;
  readonly sourceApp: string;
  readonly payload: unknown;
  readonly contentHash: string;
  readonly redacted: boolean;
  readonly createdAt: string;
}

export interface CaptureSummaryView {
  readonly id: string;
  readonly eventId: string;
  readonly topic: string;
  readonly excerpt: string;
  readonly createdAt: string;
}

export interface CaptureAuditView {
  readonly id: string;
  readonly kind: string;
  readonly action: string;
  readonly reason: string;
  readonly createdAt: string;
}

export interface CaptureOutcome {
  readonly paused: boolean;
  readonly polled: number;
  readonly written: number;
  readonly skippedDisabled: number;
  readonly skippedDuplicate: number;
  readonly redacted: number;
  readonly dropped: number;
  readonly errors: number;
}

export interface KbSourceView {
  readonly id: string;
  readonly path: string;
  readonly available: boolean;
  readonly paused: boolean;
  readonly lastScanAt: string | null;
  readonly lastSuccessAt: string | null;
  readonly docCount: number;
}

export interface KbDocumentView {
  readonly id: string;
  readonly sourceId: string;
  readonly path: string;
  readonly normalizedName: string;
  readonly versionLabel: string;
  readonly domain: string;
  readonly topicId: string | null;
  readonly topicName: string;
  readonly fileSize: number;
  readonly createdAt: string | null;
  readonly modifiedAt: string | null;
  readonly available: boolean;
}

export interface KbTopicView {
  readonly id: string;
  readonly displayName: string;
  readonly docCount: number;
  readonly latestModifiedAt: string | null;
}

export interface KbScanOutcome {
  readonly sourceId: string;
  readonly available: boolean;
  readonly scanned: number;
  readonly added: number;
  readonly updated: number;
  readonly removed: number;
  readonly skipped: number;
  readonly reason: string | null;
}

export interface KbSearchHit {
  readonly document: KbDocumentView;
  readonly matchedBy: string;
}

export interface KbDomainStat {
  readonly domain: string;
  readonly docCount: number;
}

export interface KbRingStat {
  readonly period: string;
  readonly added: number;
  readonly total: number;
}

export interface KnowledgeOverview {
  readonly sourceCount: number;
  readonly availableSources: number;
  readonly docCount: number;
  readonly topicCount: number;
  readonly domains: readonly KbDomainStat[];
  readonly topics: readonly KbTopicView[];
  readonly rings: readonly KbRingStat[];
}

// ---------- P8 自我蒸馏 ----------

export interface SelfItemView {
  readonly id: string;
  readonly draftId: string;
  readonly ordinal: number;
  readonly title: string;
  readonly layer: LayerKey;
  readonly triggerCondition: string;
  readonly steps: readonly string[];
  readonly mechanism: string;
  readonly boundary: string;
  readonly evidence: readonly string[];
  readonly sourceRecordId: string | null;
  readonly status: string;
  readonly statusLabel: string;
  readonly createdAt: string;
}

export interface SelfDraftView {
  readonly id: string;
  readonly status: string;
  readonly recordCount: number;
  readonly masterId: string;
  readonly modelCalls: number;
  readonly errorCode: string | null;
  readonly note: string;
  readonly updatedAt: string;
  readonly createdAt: string;
}

export interface SelfDraftDetail {
  readonly draft: SelfDraftView;
  readonly items: readonly SelfItemView[];
  readonly pendingCount: number;
  readonly acceptedCount: number;
  readonly rejectedCount: number;
}

export interface SelfReadiness {
  readonly recordCount: number;
  readonly required: number;
  readonly unlocked: boolean;
  readonly installed: boolean;
  readonly seatEnabled: boolean;
  readonly currentVersion: number;
  readonly latestDraft: SelfDraftView | null;
}

// ---------- P8 数据主权 ----------

export interface DataTableCount {
  readonly table: string;
  readonly label: string;
  readonly rows: number;
}

export interface DataScope {
  readonly tables: readonly DataTableCount[];
  readonly tableCount: number;
  readonly rowCount: number;
}

export interface ExportOutcome {
  readonly path: string;
  readonly tableCount: number;
  readonly rowCount: number;
  readonly bytes: number;
  readonly createdAt: string;
}

export interface PurgeOutcome {
  readonly scope: string;
  readonly tableCount: number;
  readonly rowCount: number;
  readonly createdAt: string;
}

export interface DataEventView {
  readonly id: string;
  readonly kind: string;
  readonly kindLabel: string;
  readonly scope: string;
  readonly tableCount: number;
  readonly rowCount: number;
  readonly location: string;
  readonly createdAt: string;
}

// ---------- P9 资产统计 ----------

export interface AssetRootView {
  readonly id: string;
  readonly path: string;
  readonly available: boolean;
  readonly lastScanAt: string | null;
  readonly skillCount: number;
}

export interface SkillView {
  readonly id: string;
  readonly rootId: string;
  readonly name: string;
  readonly description: string;
  readonly category: string;
  readonly tags: readonly string[];
  readonly enabled: boolean;
  readonly source: string;
  readonly path: string;
  readonly version: string;
  readonly needsRepair: boolean;
  readonly repairReason: string;
  readonly missing: boolean;
  readonly modifiedAt: string | null;
}

export interface SkillDependencyView {
  readonly name: string;
  readonly version: string;
  readonly kind: string;
}

export interface SkillDetail {
  readonly skill: SkillView;
  readonly dependencies: readonly SkillDependencyView[];
  readonly manifestExcerpt: string;
}

export interface CategoryStat {
  readonly category: string;
  readonly skillCount: number;
  readonly enabledCount: number;
}

export interface PlatformStat {
  readonly code: string;
  readonly displayName: string;
  readonly modelName: string;
  readonly enabled: boolean;
  readonly status: string;
}

export interface AssetSummary {
  readonly rootCount: number;
  readonly availableRoots: number;
  readonly skillCount: number;
  readonly enabledCount: number;
  readonly disabledCount: number;
  readonly needsRepairCount: number;
  readonly recentAdded: number;
  readonly recentRemoved: number;
  readonly categories: readonly CategoryStat[];
  readonly platforms: readonly PlatformStat[];
}

export interface AssetScanOutcome {
  readonly rootId: string;
  readonly rootPath: string;
  readonly available: boolean;
  readonly scanned: number;
  readonly added: number;
  readonly updated: number;
  readonly removed: number;
  readonly needsRepair: number;
  readonly reason: string | null;
}

export interface CommandMap {
  app_info: { request: Record<string, never>; response: AppInfo };
  db_status: { request: Record<string, never>; response: DbStatus };
  furnace_snapshot: { request: Record<string, never>; response: FurnaceSnapshot };
  master_list: {
    request: { domain?: string; layer?: LayerKey };
    response: readonly MasterSummary[];
  };
  master_detail: { request: { masterId: string }; response: MasterDetail };
  master_versions: {
    request: { masterId: string };
    response: readonly VersionView[];
  };
  master_revert: {
    request: { masterId: string; version: number };
    response: number;
  };
  master_flag_unit: {
    request: { unitId: string; reason: string };
    response: null;
  };
  master_install: { request: { packPath: string }; response: InstallOutcome };
  master_validate: { request: { packPath: string }; response: unknown };
  coverage_matrix: { request: Record<string, never>; response: CoverageMatrix };
  master_domains: { request: Record<string, never>; response: readonly string[] };
  corpus_list: {
    request: { masterId?: string };
    response: readonly CorpusItemView[];
  };
  corpus_search: {
    request: { query: string; limit?: number };
    response: readonly CorpusSearchHit[];
  };
  seed_packs_install: {
    request: Record<string, never>;
    response: SeedInstallReport;
  };
  settings_get: { request: { key: SettingKey }; response: string | null };
  settings_set: { request: { key: SettingKey; value: string }; response: null };
  networking_get: { request: Record<string, never>; response: boolean };
  networking_set: { request: { enabled: boolean }; response: boolean };
  platform_list: { request: Record<string, never>; response: readonly PlatformView[] };
  platform_upsert: {
    request: {
      code: string;
      displayName: string;
      endpoint: string;
      modelName: string;
      inputPriceMicrosPer1k?: number;
      outputPriceMicrosPer1k?: number;
      currency?: string;
    };
    response: PlatformView;
  };
  platform_enable: {
    request: { code: string; enabled: boolean };
    response: PlatformView;
  };
  llm_calls: { request: { limit?: number }; response: readonly LlmCall[] };
  council_candidates: {
    request: { question?: string; domains?: readonly string[] };
    response: CandidatePool;
  };
  council_create: {
    request: {
      question: string;
      strategy?: CouncilStrategy;
      domains?: readonly string[];
      includeSelf?: boolean;
    };
    response: CouncilSessionView;
  };
  council_select: {
    request: {
      sessionId: string;
      strategy?: CouncilStrategy;
      size?: number;
      pinned?: readonly string[];
    };
    response: CouncilSelection;
  };
  council_rotate: {
    request: {
      sessionId: string;
      strategy?: CouncilStrategy;
      size?: number;
      pinned?: readonly string[];
    };
    response: CouncilSelection;
  };
  council_run: { request: { sessionId: string }; response: CouncilOutcome };
  council_turns: {
    request: { sessionId: string; rotation?: number };
    response: readonly CouncilSeatSpeech[];
  };
  council_retry_seat: {
    request: { sessionId: string; rotation: number; masterId: string; round: number };
    response: readonly CouncilSeatSpeech[];
  };
  council_followup: {
    request: {
      parentSessionId: string;
      anchor: FollowUpAnchor;
      question: string;
      inheritPanel?: boolean;
    };
    response: CouncilSessionView;
  };
  council_conclusion: {
    request: { sessionId: string };
    response: CouncilConclusionView;
  };
  council_cancel: { request: { sessionId: string }; response: CouncilSessionView };
  council_recoverable: {
    request: Record<string, never>;
    response: readonly CouncilSessionView[];
  };
  echo_check: { request: { sessionId: string }; response: readonly EchoHit[] };
  principle_revoke: {
    request: { nodeId: string; reason?: string };
    response: boolean;
  };
  council_sources: {
    request: { sessionId: string; rotation?: number };
    response: readonly CouncilSourceView[];
  };
  council_search: {
    request: { query: string; limit?: number; confirm?: string };
    response: SearchOutcome;
  };
  connector_list: {
    request: Record<string, never>;
    response: readonly ConnectorView[];
  };
  connector_upsert: {
    request: {
      id?: string;
      kind: string;
      displayName: string;
      endpoint: string;
      config?: unknown;
    };
    response: ConnectorView;
  };
  connector_enable: {
    request: { id: string; enabled: boolean };
    response: ConnectorView;
  };
  connector_test: {
    request: { id: string; confirm?: string };
    response: ConnectorTestOutcome;
  };
  connector_calls: {
    request: { limit?: number };
    response: readonly ConnectorCallView[];
  };
  council_sessions: {
    request: { limit?: number };
    response: readonly CouncilSessionView[];
  };
  council_session: { request: { sessionId: string }; response: CouncilSessionDetail };
  master_history: {
    request: { masterId: string; limit?: number };
    response: readonly MasterHistoryEntry[];
  };
  tuning_get: { request: Record<string, never>; response: readonly TuningItem[] };
  tuning_set: {
    request: { values: readonly (readonly [string, string])[] };
    response: readonly TuningItem[];
  };
  network_upsert_node: {
    request: {
      kind: NodeKind;
      content: string;
      sourceRef: string;
      domains?: readonly string[];
      layers?: readonly LayerKey[];
    };
    response: NodeUpsert;
  };
  network_link: {
    request: {
      fromId: string;
      toId: string;
      relation: EdgeRelation;
      weight?: number;
    };
    response: EdgeUpsert;
  };
  network_activate: {
    request: {
      nodeIds: readonly string[];
      increment?: number;
      sessionId?: string;
    };
    response: ActivationOutcome;
  };
  network_decay: { request: { limit?: number }; response: number };
  network_node: { request: { nodeId: string }; response: NodeDetail };
  network_graph: {
    request: {
      domain?: string;
      layer?: LayerKey;
      kind?: NodeKind;
      minActivation?: number;
      limit?: number;
      clusterId?: string;
    };
    response: GraphView;
  };
  network_resolve_conflict: {
    request: { edgeId: string; decision: "keep" | "drop"; reason?: string };
    response: string;
  };
  network_record_session: {
    request: { sessionId: string };
    response: RecordOutcome;
  };
  records_list: { request: { limit?: number }; response: readonly ThoughtRecord[] };
  records_compare: { request: { topicKey: string }; response: readonly ThoughtRecord[] };
  record_decision: {
    request: { recordId: string; adopted: boolean; reason?: string };
    response: boolean;
  };
  consolidate_trigger: {
    request: { mode?: "manual" | "idle" };
    response: ConsolidationReport;
  };
  consolidate_report: {
    request: { runId: string };
    response: ConsolidationReport;
  };
  consolidate_runs: {
    request: { limit?: number };
    response: readonly ConsolidationRun[];
  };
  companion_settings: { request: Record<string, never>; response: CompanionSettings };
  companion_enable: {
    request: { enabled: boolean };
    response: CompanionSettings;
  };
  companion_limit: { request: { limit: number }; response: CompanionSettings };
  companion_rules: {
    request: { rules: CompanionRules };
    response: CompanionSettings;
  };
  companion_collide: {
    request: { signal: CollisionSignal };
    response: CollisionOutcome;
  };
  insights_list: {
    request: {
      kind?: InsightKindKey;
      status?: string;
      source?: string;
      limit?: number;
    };
    response: readonly Insight[];
  };
  insight_mark: {
    request: { insightId: string; action: string; reason?: string };
    response: Insight;
  };
  insight_convert: {
    request: { insightId: string };
    response: CouncilSessionView;
  };
  topics_list: { request: { limit?: number }; response: readonly TopicView[] };
  principles_list: { request: { limit?: number }; response: readonly PrincipleSeal[] };
  promote_principles: { request: Record<string, never>; response: readonly string[] };
  ring_overview: { request: Record<string, never>; response: RingOverview };
  distill_start: {
    request: {
      masterId: string;
      masterName: string;
      domain: string;
      materials: readonly IntakeMaterial[];
      outputDir?: string;
    };
    response: DistillJobView;
  };
  distill_from_intake: {
    request: { intakeJobId: string; outputDir?: string };
    response: DistillJobView;
  };
  distill_list: {
    request: { status?: string; limit?: number };
    response: readonly DistillJobView[];
  };
  distill_detail: { request: { jobId: string }; response: DistillDetail };
  distill_confirm: { request: { jobId: string }; response: DistillJobView };
  distill_resume: { request: { jobId: string }; response: DistillJobView };
  intake_create: {
    request: {
      masterId: string;
      masterName: string;
      domain: string;
      materials: readonly IntakeMaterial[];
    };
    response: IntakeJobView;
  };
  intake_list: {
    request: { status?: string; limit?: number };
    response: readonly IntakeJobView[];
  };
  intake_preview: { request: { jobId: string }; response: readonly SignalView[] };
  intake_confirm: {
    request: {
      jobId: string;
      acceptedIds: readonly string[];
      rejectedIds: readonly string[];
      reason?: string;
    };
    response: IntakeJobView;
  };
  discovery_settings: { request: Record<string, never>; response: DiscoverySettings };
  discovery_enable: { request: { enabled: boolean }; response: DiscoverySettings };
  discovery_schedule: {
    request: { schedule: unknown };
    response: DiscoverySettings;
  };
  discovery_run: {
    request: { masterId: string; masterName: string; domain: string; query?: string };
    response: DiscoveryOutcome;
  };
  capture_settings: { request: Record<string, never>; response: CaptureSettingsView };
  capture_set_capability: {
    request: { kind: CaptureKindKey; enabled: boolean };
    response: CaptureSettingsView;
  };
  capture_set_paused: {
    request: { paused: boolean };
    response: CaptureSettingsView;
  };
  capture_set_redaction: {
    request: { enabled: boolean; terms: readonly string[]; mask?: string };
    response: CaptureSettingsView;
  };
  capture_set_dedup: {
    request: { seconds: number };
    response: CaptureSettingsView;
  };
  capture_set_watch_roots: {
    request: { paths: readonly string[] };
    response: CaptureSettingsView;
  };
  capture_collect: { request: Record<string, never>; response: CaptureOutcome };
  capture_events: {
    request: { kind?: CaptureKindKey; from?: string; to?: string; limit?: number };
    response: readonly CaptureEventView[];
  };
  capture_summaries: {
    request: { eventId: string };
    response: readonly CaptureSummaryView[];
  };
  capture_delete_event: { request: { eventId: string }; response: boolean };
  capture_audit: { request: { limit?: number }; response: readonly CaptureAuditView[] };
  kb_sources: { request: Record<string, never>; response: readonly KbSourceView[] };
  kb_add_source: { request: { path: string }; response: KbSourceView };
  kb_remove_source: { request: { sourceId: string }; response: boolean };
  kb_scan: { request: { sourceId?: string }; response: readonly KbScanOutcome[] };
  kb_documents: {
    request: { sourceId?: string; domain?: string; topicId?: string; limit?: number };
    response: readonly KbDocumentView[];
  };
  kb_search: {
    request: { query: string; limit?: number };
    response: readonly KbSearchHit[];
  };
  kb_overview: { request: { topicLimit?: number }; response: KnowledgeOverview };
  self_readiness: { request: Record<string, never>; response: SelfReadiness };
  self_draft: { request: { draftId?: string }; response: SelfDraftDetail | null };
  self_start: { request: Record<string, never>; response: SelfDraftDetail };
  self_decide: {
    request: { draftId: string; itemId: string; accepted: boolean };
    response: SelfDraftDetail;
  };
  self_install: { request: { draftId: string }; response: InstallOutcome };
  self_set_seat: { request: { enabled: boolean }; response: SelfReadiness };
  data_scope: { request: Record<string, never>; response: DataScope };
  data_export: { request: { path?: string }; response: ExportOutcome };
  data_purge: { request: { confirm?: boolean }; response: PurgeOutcome };
  data_events: { request: { limit?: number }; response: readonly DataEventView[] };
  asset_roots: { request: Record<string, never>; response: readonly AssetRootView[] };
  asset_add_root: { request: { path: string }; response: AssetRootView };
  asset_remove_root: { request: { rootId: string }; response: boolean };
  asset_scan: { request: { rootId?: string }; response: readonly AssetScanOutcome[] };
  asset_skills: {
    request: {
      rootId?: string;
      category?: string;
      tag?: string;
      enabled?: boolean;
      needsRepair?: boolean;
      query?: string;
      limit?: number;
    };
    response: readonly SkillView[];
  };
  asset_skill_detail: { request: { skillId: string }; response: SkillDetail };
  asset_summary: { request: Record<string, never>; response: AssetSummary };
  cost_summary: { request: { days?: number }; response: CostSummary };
  cost_estimate: {
    request: { seats?: number; rounds?: number; searches?: number };
    response: CostEstimate;
  };
  backup_create: { request: { kind?: string }; response: BackupOutcome };
  backup_list: { request: { limit?: number }; response: readonly BackupView[] };
  backup_restore: { request: { path: string }; response: BackupOutcome };
  credential_set: {
    request: { scope: string; ownerId: string; secret: string };
    response: CredentialRefView;
  };
  credential_status: {
    request: { scope: string; ownerId: string };
    response: boolean;
  };
  model_probe: { request: Record<string, never>; response: ModelProbeOutcome };
}

export type CommandName = keyof CommandMap;
export type CommandRequest<K extends CommandName> = CommandMap[K]["request"];
export type CommandResponse<K extends CommandName> = CommandMap[K]["response"];

export const COMMAND_NAMES: readonly CommandName[] = [
  "app_info",
  "db_status",
  "furnace_snapshot",
  "master_list",
  "master_detail",
  "master_versions",
  "master_revert",
  "master_flag_unit",
  "master_install",
  "master_validate",
  "coverage_matrix",
  "master_domains",
  "corpus_list",
  "corpus_search",
  "seed_packs_install",
  "settings_get",
  "settings_set",
  "networking_get",
  "networking_set",
  "platform_list",
  "platform_upsert",
  "platform_enable",
  "llm_calls",
  "council_candidates",
  "council_create",
  "council_select",
  "council_rotate",
  "council_run",
  "council_turns",
  "council_retry_seat",
  "council_followup",
  "council_conclusion",
  "council_cancel",
  "council_recoverable",
  "echo_check",
  "principle_revoke",
  "council_sources",
  "council_search",
  "connector_list",
  "connector_upsert",
  "connector_enable",
  "connector_test",
  "connector_calls",
  "council_sessions",
  "council_session",
  "master_history",
  "tuning_get",
  "tuning_set",
  "network_upsert_node",
  "network_link",
  "network_activate",
  "network_decay",
  "network_node",
  "network_graph",
  "network_resolve_conflict",
  "network_record_session",
  "records_list",
  "records_compare",
  "record_decision",
  "consolidate_trigger",
  "consolidate_report",
  "consolidate_runs",
  "companion_settings",
  "companion_enable",
  "companion_limit",
  "companion_rules",
  "companion_collide",
  "insights_list",
  "insight_mark",
  "insight_convert",
  "topics_list",
  "principles_list",
  "promote_principles",
  "ring_overview",
  "distill_start",
  "distill_from_intake",
  "distill_list",
  "distill_detail",
  "distill_confirm",
  "distill_resume",
  "intake_create",
  "intake_list",
  "intake_preview",
  "intake_confirm",
  "discovery_settings",
  "discovery_enable",
  "discovery_schedule",
  "discovery_run",
  "capture_settings",
  "capture_set_capability",
  "capture_set_paused",
  "capture_set_redaction",
  "capture_set_dedup",
  "capture_set_watch_roots",
  "capture_collect",
  "capture_events",
  "capture_summaries",
  "capture_delete_event",
  "capture_audit",
  "kb_sources",
  "kb_add_source",
  "kb_remove_source",
  "kb_scan",
  "kb_documents",
  "kb_search",
  "kb_overview",
  "self_readiness",
  "self_draft",
  "self_start",
  "self_decide",
  "self_install",
  "self_set_seat",
  "data_scope",
  "data_export",
  "data_purge",
  "data_events",
  "asset_roots",
  "asset_add_root",
  "asset_remove_root",
  "asset_scan",
  "asset_skills",
  "asset_skill_detail",
  "asset_summary",
  "cost_summary",
  "cost_estimate",
  "backup_create",
  "backup_list",
  "backup_restore",
  "credential_set",
  "credential_status",
  "model_probe",
];
