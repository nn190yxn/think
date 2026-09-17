import { useEffect, useMemo, useState } from "react";
import type { CSSProperties } from "react";
import { LAYER_KEYS, layerOf, type LayerKey } from "../domain/layers";
import { LayerGlyph } from "../components/LayerGlyph";
import { DivergenceCurve } from "../components/DivergenceCurve";
import { SeatSpeech } from "../components/SeatSpeech";
import { FollowUpForm } from "../components/FollowUpForm";
import { CouncilConclusion } from "../components/CouncilConclusion";
import { RealmShell } from "./RealmShell";
import { useCommand, useCommands } from "../app/ipc";
import { anchorKindLabel, formatTime, queryModeLabel } from "../domain/labels";
import type {
  CouncilConclusionView,
  CouncilOutcome,
  CouncilSeat,
  CouncilSeatSpeech,
  CouncilSessionView,
  CouncilSourceView,
  CouncilStrategy,
  EchoHit,
  FollowUpAnchor,
  SearchOutcome,
} from "../ipc/commands";

const STRATEGIES: readonly {
  readonly key: CouncilStrategy;
  readonly label: string;
  readonly hint: string;
}[] = [
  { key: "steady", label: "稳妥", hint: "取相关度最高的判断框架" },
  { key: "clash", label: "碰撞", hint: "取彼此观点最对立的大师" },
  { key: "serendipity", label: "意外", hint: "取领域距离最远的大师" },
];

const DEFAULT_SIZE = 6;

type Phase = "idle" | "selecting" | "running" | "done";

/** 席位角度：与 CSS 中的旋转保持一致，光束据此对齐。 */
function seatAngle(index: number): number {
  return (360 / LAYER_KEYS.length) * index - 90;
}

interface SeatSlot {
  readonly key: string;
  readonly layer: LayerKey;
  readonly angle: number;
  readonly seat: CouncilSeat | null;
}

/**
 * 会：圆桌会诊。
 *
 * 发起后依次是选角、第一轮隔离作答、第二轮交叉质询与收敛裁决；
 * 三条光束状态与阶段对应，换批把手整批更换席位并保留锁定的大师。
 */
export function CouncilRealm({
  seed,
  sessionId,
}: {
  readonly seed?: string | undefined;
  readonly sessionId?: string | undefined;
} = {}) {
  const client = useCommands();
  const candidates = useCommand("council_candidates", {});
  const tuning = useCommand("tuning_get", {});

  const [question, setQuestion] = useState(seed ?? "");
  const [strategy, setStrategy] = useState<CouncilStrategy>("steady");
  const [pinned, setPinned] = useState<readonly string[]>([]);
  const [session, setSession] = useState<CouncilSessionView | null>(null);
  const [seats, setSeats] = useState<readonly CouncilSeat[]>([]);
  const [phase, setPhase] = useState<Phase>("idle");
  const [outcome, setOutcome] = useState<CouncilOutcome | null>(null);
  const [recorded, setRecorded] = useState<string | null>(null);
  const [speech, setSpeech] = useState<readonly CouncilSeatSpeech[]>([]);
  const [sources, setSources] = useState<readonly CouncilSourceView[]>([]);
  const [conclusion, setConclusion] = useState<CouncilConclusionView | null>(null);
  const [anchor, setAnchor] = useState<FollowUpAnchor | null>(null);
  const [parentSession, setParentSession] = useState<CouncilSessionView | null>(null);
  const [followBusy, setFollowBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchOutcome, setSearchOutcome] = useState<SearchOutcome | null>(null);
  const [searchNote, setSearchNote] = useState<string | null>(null);
  const [includeSelf, setIncludeSelf] = useState(true);
  const [cancelling, setCancelling] = useState(false);
  const [recoverable, setRecoverable] = useState<readonly CouncilSessionView[]>([]);
  const [echoes, setEchoes] = useState<readonly EchoHit[]>([]);

  // 星图传来的种子问题会替换输入框内容，让「以此发起会诊」有明确落点。
  useEffect(() => {
    if (seed) {
      setQuestion(seed);
    }
  }, [seed]);

  // 上次中途退出时留下的未完成会话，启动时提示继续或放弃。
  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        const list = await client.call("council_recoverable", {});
        if (alive) {
          setRecoverable(list);
        }
      } catch {
        if (alive) {
          setRecoverable([]);
        }
      }
    })();
    return () => {
      alive = false;
    };
  }, [client]);

  // 余烬转来的洞察已经建好会话，这里直接载入，避免再建一个空会话。
  useEffect(() => {
    if (!sessionId) {
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const detail = await client.call("council_session", { sessionId });
        if (cancelled) {
          return;
        }
        setSession(detail.session);
        setQuestion(detail.session.question);
        setSeats([]);
        setPhase("idle");
        setOutcome(null);
        setConclusion(null);
        setSources([]);
        void loadSpeech(sessionId);
        const rotation =
          detail.panels.length > 0
            ? detail.panels[detail.panels.length - 1]!.rotation
            : 0;
        void loadSources(sessionId, rotation);
      } catch {
        if (!cancelled) {
          setError("会诊会话读取失败");
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [sessionId, client]);

  const slots = useMemo<SeatSlot[]>(() => {
    return LAYER_KEYS.map((layer, index) => {
      const seat = seats.find((item) => item.layer === layer) ?? null;
      return { key: layer, layer, angle: seatAngle(index), seat };
    });
  }, [seats]);

  const busy = phase === "selecting" || phase === "running";

  // 曲线上的参考线取自与本次会诊同一套调参，避免界面上出现两套阈值。
  const threshold = useMemo(() => {
    const item = tuning.data?.find((entry) => entry.key === "council.divergence_threshold");
    const parsed = item ? Number(item.value) : Number.NaN;
    return Number.isFinite(parsed) ? parsed : null;
  }, [tuning.data]);

  // 共享背景的 masterId 为空，其余按席位分组，用于标注「谁补充检索了什么」。
  const background = useMemo(
    () => sources.filter((source) => source.masterId === null),
    [sources],
  );
  const seatSources = useMemo(() => {
    const grouped = new Map<string, CouncilSourceView[]>();
    for (const source of sources) {
      if (!source.masterId) {
        continue;
      }
      const list = grouped.get(source.masterId) ?? [];
      list.push(source);
      grouped.set(source.masterId, list);
    }
    return [...grouped.entries()];
  }, [sources]);

  async function loadSpeech(sessionId: string) {
    try {
      const seats = await client.call("council_turns", { sessionId });
      setSpeech(seats);
    } catch {
      setSpeech([]);
    }
  }

  async function loadSources(sessionId: string, rotation: number) {
    try {
      const list = await client.call("council_sources", { sessionId, rotation });
      setSources(list);
    } catch {
      setSources([]);
    }
  }

  function latestRotation(): number {
    return Math.max(0, (session?.rotationCount ?? 1) - 1);
  }

  /** 重试失败席位：只重跑该席位该轮，不重跑整场。 */
  async function retrySeat(masterId: string, round: number) {
    if (!session) {
      return;
    }
    setError(null);
    setFollowBusy(true);
    try {
      const seats = await client.call("council_retry_seat", {
        sessionId: session.id,
        rotation: latestRotation(),
        masterId,
        round,
      });
      setSpeech(seats);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "席位重试失败");
    } finally {
      setFollowBusy(false);
    }
  }

  async function openConclusion() {
    if (!session) {
      return;
    }
    setError(null);
    try {
      const view = await client.call("council_conclusion", { sessionId: session.id });
      setConclusion(view);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "结论读取失败");
    }
  }

  /**
   * 主动查资料。预演开启时首次调用只拿到待发送内容，确认后才真正发出请求；
   * 结果只作参考，不会算进本次会诊共享的外来材料。
   */
  async function runSearch(confirm?: string) {
    const query = searchQuery.trim();
    if (!query) {
      setSearchNote("先写下要查的内容");
      return;
    }
    setSearchNote(null);
    try {
      const outcome = await client.call("council_search", {
        query,
        ...(confirm ? { confirm } : {}),
      });
      setSearchOutcome(outcome);
      if (!outcome.pending) {
        setSearchNote(`这次查到 ${outcome.hits.length} 条资料`);
      }
    } catch (cause) {
      setSearchOutcome(null);
      setSearchNote(cause instanceof Error ? cause.message : "查询没能完成");
    }
  }

  /** 围绕选中的一段判断新建追问会话，原会话保持不动。 */
  async function submitFollowUp(question: string, inheritPanel: boolean) {
    if (!anchor || !session) {
      return;
    }
    setError(null);
    setFollowBusy(true);
    try {
      const created = await client.call("council_followup", {
        parentSessionId: session.id,
        anchor,
        question,
        inheritPanel,
      });
      setParentSession(session);
      setSession(created);
      setQuestion(created.question);
      setSeats([]);
      setOutcome(null);
      setSpeech([]);
      setSources([]);
      setConclusion(null);
      setRecorded(null);
      setAnchor(null);
      setPhase("idle");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "追问未能发起");
    } finally {
      setFollowBusy(false);
    }
  }

  async function backToParent() {
    const target = parentSession?.id ?? session?.parentSessionId ?? null;
    if (!target) {
      return;
    }
    setError(null);
    try {
      const detail = await client.call("council_session", { sessionId: target });
      setSession(detail.session);
      setQuestion(detail.session.question);
      setParentSession(null);
      setOutcome(null);
      setConclusion(null);
      setSources([]);
      setSeats([]);
      await loadSpeech(detail.session.id);
      const rotation =
        detail.panels.length > 0
          ? detail.panels[detail.panels.length - 1]!.rotation
          : 0;
      await loadSources(detail.session.id, rotation);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "母会话读取失败");
    }
  }

  async function start(includeSelfOverride?: boolean) {
    const asked = question.trim();
    if (!asked) {
      setError("先写下一个要判断的问题");
      return;
    }
    const useSelf = includeSelfOverride ?? includeSelf;
    setError(null);
    setOutcome(null);
    setRecorded(null);
    setConclusion(null);
    setAnchor(null);
    setParentSession(null);
    setSources([]);
    setEchoes([]);
    setSeats([]);
    setPhase("selecting");
    try {
      // 已载入的会话若仍是同一议题，直接复用，避免重复建会话。
      const active =
        session && session.question === asked && session.selfSeatIncluded === useSelf
          ? session
          : await client.call("council_create", {
              question: asked,
              strategy,
              includeSelf: useSelf,
            });
      setSession(active);
      const selected = await client.call("council_select", {
        sessionId: active.id,
        strategy,
        size: DEFAULT_SIZE,
        pinned,
      });
      setSeats(selected.seats);
      setPhase("running");
      const result = await client.call("council_run", { sessionId: active.id });
      setOutcome(result);
      await loadSpeech(active.id);
      await loadSources(active.id, result.rotation);
      // 结论落入思维网络：判断、框架、分歧都成为节点并强化连线。
      try {
        const written = await client.call("network_record_session", {
          sessionId: active.id,
        });
        setRecorded(
          `已写入思维网络：判断 1、框架 ${written.frameworkIds.length}、分歧 ${written.divergenceIds.length}`,
        );
      } catch {
        setRecorded(null);
      }
      // 与既有原则高度重合的部分要提示，避免会诊退化成自我确认。
      try {
        setEchoes(await client.call("echo_check", { sessionId: active.id }));
      } catch {
        setEchoes([]);
      }
      setPhase("done");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "会诊未能完成");
      setPhase("idle");
    }
  }

  async function rotate() {
    if (!session) {
      setError("先发起一次会诊，再换批");
      return;
    }
    setError(null);
    setOutcome(null);
    setRecorded(null);
    setConclusion(null);
    setAnchor(null);
    setSpeech([]);
    setSources([]);
    setPhase("selecting");
    try {
      const selected = await client.call("council_rotate", {
        sessionId: session.id,
        strategy,
        size: DEFAULT_SIZE,
        pinned,
      });
      setSeats(selected.seats);
      setPhase("idle");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "换批未能完成");
      setPhase("idle");
    }
  }

  function togglePin(masterId: string) {
    setPinned((current) =>
      current.includes(masterId)
        ? current.filter((id) => id !== masterId)
        : [...current, masterId],
    );
  }

  /** 接上一次中断的会话继续跑：只补未完成的轮次，已完成的发言与裁决不重来。 */
  async function resume(target: CouncilSessionView) {
    setError(null);
    setOutcome(null);
    setRecorded(null);
    setConclusion(null);
    setAnchor(null);
    setParentSession(null);
    setSources([]);
    setSpeech([]);
    setEchoes([]);
    setRecoverable((current) => current.filter((item) => item.id !== target.id));
    setSession(target);
    setQuestion(target.question);
    setPhase("running");
    try {
      const result = await client.call("council_run", { sessionId: target.id });
      setOutcome(result);
      await loadSpeech(target.id);
      await loadSources(target.id, result.rotation);
      try {
        setEchoes(await client.call("echo_check", { sessionId: target.id }));
      } catch {
        setEchoes([]);
      }
      setPhase("done");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "续跑未能完成");
      setPhase("idle");
    }
  }

  function discard(target: CouncilSessionView) {
    setRecoverable((current) => current.filter((item) => item.id !== target.id));
  }

  /** 请求取消：在当前轮次结束时生效，已完成的轮次保留。 */
  async function cancel() {
    if (!session) {
      return;
    }
    setCancelling(true);
    try {
      const view = await client.call("council_cancel", { sessionId: session.id });
      setSession(view);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "取消失败");
    } finally {
      setCancelling(false);
    }
  }

  const phaseLabel: Record<Phase, string> = {
    idle: "待选角",
    selecting: "正在选角",
    running: "隔离作答与交叉质询中",
    done: "已收敛",
  };

  return (
    <RealmShell realm="council">
      <div className="council">
        {recoverable.length > 0 ? (
          <section className="council__recover" role="status">
            <h2 className="section-head">未完成的会诊</h2>
            <p className="council__recover-note">
              上次退出时这场会诊还没跑完，可以接着跑，已完成的发言与裁决不会重来。
            </p>
            <ul className="council__recover-list">
              {recoverable.map((item) => (
                <li key={item.id} className="council__recover-item">
                  <span className="council__recover-question">{item.question}</span>
                  <button
                    className="council__recover-go"
                    type="button"
                    onClick={() => void resume(item)}
                  >
                    继续
                  </button>
                  <button
                    className="council__recover-drop"
                    type="button"
                    onClick={() => discard(item)}
                  >
                    放弃
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ) : null}

        <form
          className="council__ask"
          onSubmit={(event) => {
            event.preventDefault();
            void start();
          }}
        >
          <label className="council__ask-label" htmlFor="council-question">
            议题
          </label>
          <input
            id="council-question"
            className="council__ask-input"
            value={question}
            placeholder="写下你此刻真正要判断的一件事"
            onChange={(event) => setQuestion(event.target.value)}
          />
          <button className="council__ask-go" type="submit" disabled={busy}>
            {busy ? "会诊中" : "发起会诊"}
          </button>
        </form>

        <div className="council__strategy" role="group" aria-label="选角策略">
          <span className="council__strategy-label">选角</span>
          {STRATEGIES.map((item) => (
            <button
              key={item.key}
              type="button"
              className="strategy"
              aria-pressed={strategy === item.key}
              data-active={strategy === item.key}
              title={item.hint}
              onClick={() => setStrategy(item.key)}
            >
              {item.label}
            </button>
          ))}
          <button
            className="council__rotate"
            type="button"
            onClick={() => void rotate()}
            disabled={busy}
          >
            换一批
          </button>
          <label className="council__self">
            <input
              type="checkbox"
              checked={!includeSelf}
              onChange={(event) => setIncludeSelf(!event.target.checked)}
            />
            这一场不带我
          </label>
          {phase === "running" ? (
            <button
              className="council__cancel"
              type="button"
              onClick={() => void cancel()}
              disabled={cancelling || session?.cancelRequested === true}
            >
              {session?.cancelRequested ? "已请求取消" : cancelling ? "正在取消" : "取消"}
            </button>
          ) : null}
        </div>

        {session?.parentSessionId ? (
          <div className="council__thread" role="status">
            <span className="council__thread-note">
              这是一场追问会话
              {session.anchorKind ? ` · 追问对象：${anchorKindLabel(session.anchorKind)}` : ""}
              {session.panelInherited
                ? " · 沿用上一场的大师阵容"
                : " · 这一场另外选人"}
            </span>
            <button
              className="council__thread-back"
              type="button"
              onClick={() => void backToParent()}
            >
              返回母会话
            </button>
          </div>
        ) : null}

        {anchor ? (
          <FollowUpForm
            anchor={anchor}
            busy={followBusy}
            onSubmit={(asked, inherit) => void submitFollowUp(asked, inherit)}
            onCancel={() => setAnchor(null)}
          />
        ) : null}

        <div className="roundtable" data-phase={phase}>
          <div className="roundtable__beams" aria-hidden="true">
            {slots.map((slot) => (
              <span
                key={slot.key}
                className="beam"
                data-layer={slot.layer}
                style={{ "--beam-angle": `${slot.angle - 90}deg` } as CSSProperties}
              />
            ))}
          </div>
          <div className="roundtable__ring" aria-hidden="true" />
          <ul className="roundtable__seats">
            {slots.map((slot) => {
              const layer = layerOf(slot.layer);
              const seat = slot.seat;
              return (
                <li
                  key={slot.key}
                  className="seat"
                  data-layer={slot.layer}
                  data-filled={seat ? "true" : "false"}
                  style={{ "--angle": `${slot.angle}deg` } as CSSProperties}
                >
                  <span className="seat__sigil">
                    <LayerGlyph glyph={layer.glyph} size={14} />
                  </span>
                  <span className="seat__layer">{layer.name}</span>
                  <span className="seat__name">{seat?.name ?? "待选角"}</span>
                  <span className="seat__question">{layer.question}</span>
                  {seat ? <span className="seat__domain">{seat.domain}</span> : null}
                  {seat ? (
                    <>
                      <span className="seat__score">契合度 {seat.score.toFixed(2)}</span>
                      <button
                        className="seat__pin"
                        type="button"
                        aria-pressed={seat.pinned || pinned.includes(seat.masterId)}
                        data-pinned={seat.pinned || pinned.includes(seat.masterId)}
                        onClick={() => togglePin(seat.masterId)}
                      >
                        {seat.pinned || pinned.includes(seat.masterId) ? "已锁定" : "锁定"}
                      </button>
                    </>
                  ) : null}
                </li>
              );
            })}
          </ul>
          <div className="roundtable__core">
            <span className="roundtable__core-label">{phaseLabel[phase]}</span>
            <p className="prose roundtable__core-text">
              {session?.question ?? "尚未发起会诊"}
            </p>
          </div>
        </div>

        {error ? (
          <p className="council__note" data-tone="warn">
            {error}
          </p>
        ) : null}

        {outcome ? (
          <section className="verdict">
            <h2 className="section-head">本次结论</h2>
            <p className="prose verdict__text">
              {outcome.conclusion || "本次会诊未产生结论"}
            </p>
            {outcome.divergences.length > 0 ? (
              <ul className="beads" aria-label="主要分歧">
                {outcome.divergences.map((item) => (
                  <li key={item.text} className="bead" data-layer={item.layer}>
                    <span className="bead__layer">
                      {layerOf(item.layer).name} · {layerOf(item.layer).question}
                    </span>
                    {item.text}
                  </li>
                ))}
              </ul>
            ) : null}
            {session?.cancelRequested ? (
              <p className="council__note" data-tone="warn">
                这场会诊已被取消
                {session.cancelledAt ? `（取消于 ${formatTime(session.cancelledAt)}）` : ""}
                ，已完成的 {outcome.rounds} 轮保留，未跑完的轮次不再执行。
              </p>
            ) : null}
            {!session?.selfSeatIncluded && session ? (
              <p className="council__note">这一场按你的要求没有带你自己的席位。</p>
            ) : null}
            {echoes.length > 0 ? (
              <section className="echo" aria-label="和既有原则重合的地方">
                <h3 className="section-head">和既有原则重合的地方</h3>
                <p className="council__note">
                  结论里有 {echoes.length} 处和你已经沉淀的原则高度重合，
                  值得留意这是不是只是在自我确认。
                </p>
                <ul className="beads">
                  {echoes.map((hit) => (
                  <li key={hit.nodeId} className="bead">
                      {hit.content}（重合度 {hit.overlap.toFixed(2)}）
                  </li>
                  ))}
                </ul>
                <button
                  className="echo__exclude"
                  type="button"
                  onClick={() => {
                    setIncludeSelf(false);
                    void start(false);
                  }}
                >
                  这一场不带我，重开一次
                </button>
              </section>
            ) : null}
            <h3 className="section-head">分歧的变化</h3>
            <DivergenceCurve metrics={outcome.metrics} threshold={threshold} />
            {recorded ? <p className="council__note">{recorded}</p> : null}
            <p className="council__note">
              本次讨论共 {outcome.rounds} 轮，首轮 {outcome.answered} 位作答，{outcome.failed}{" "}
              位没能作答，全部记入调用记录。
            </p>
            <div className="council__verdict-actions">
              <button
                className="council__conclusion-open"
                type="button"
                onClick={() => void openConclusion()}
              >
                查看结论详情
              </button>
            </div>
          </section>
        ) : null}

        {conclusion ? (
          <CouncilConclusion
            view={conclusion}
            threshold={threshold}
            onFollowUp={setAnchor}
          />
        ) : speech.length > 0 ? (
          <SeatSpeech
            seats={speech}
            sources={sources}
            busy={followBusy}
            onRetry={(masterId, round) => void retrySeat(masterId, round)}
            onFollowUp={(masterId, round, text) =>
              setAnchor({ kind: "answer", text, masterId, round })
            }
          />
        ) : null}

        {phase === "done" ? (
          <section className="external" aria-label="外部资料">
            <h2 className="section-head">外部资料</h2>
            <div className="manual-search">
              <label className="manual-search__label" htmlFor="manual-search-input">
                主动查资料
              </label>
              <input
                id="manual-search-input"
                className="manual-search__input"
                value={searchQuery}
                placeholder="要对外查什么？"
                onChange={(event) => setSearchQuery(event.target.value)}
              />
              <button
                className="manual-search__go"
                type="button"
                onClick={() => void runSearch()}
              >
                查一查
              </button>
            </div>
            {searchOutcome?.pending && searchOutcome.prepared ? (
              <div className="preflight" role="group" aria-label="发送前确认">
                <p className="preflight__title">发送前确认</p>
                <dl className="preflight__pairs">
                  <dt>原始内容</dt>
                  <dd className="mono">{searchOutcome.prepared.original}</dd>
                  <dt>实际发送</dt>
                  <dd className="mono">{searchOutcome.prepared.sent}</dd>
                  <dt>发送模式</dt>
                  <dd>{queryModeLabel(searchOutcome.prepared.mode)}</dd>
                </dl>
                {searchOutcome.prepared.redacted ? (
                  <p className="preflight__note">
                    检测到敏感内容，已用掩码替换后才发送。
                  </p>
                ) : null}
                <div className="preflight__actions">
                  <button
                    className="manual-search__go"
                    type="button"
                    onClick={() => void runSearch(searchOutcome.fingerprint)}
                  >
                    确认发送
                  </button>
                  <button
                    className="manual-search__go"
                    type="button"
                    onClick={() => {
                      setSearchOutcome(null);
                      setSearchNote("已取消本次查询");
                    }}
                  >
                    取消
                  </button>
                </div>
              </div>
            ) : null}
            {searchOutcome && !searchOutcome.pending ? (
              <ul className="manual-search__hits">
                {searchOutcome.hits.map((hit) => (
                  <li key={hit.url} className="manual-search__hit">
                    <span className="manual-search__hit-title">{hit.title}</span>
                    <span className="manual-search__hit-snippet">{hit.snippet}</span>
                    <a
                      className="external__url"
                      href={hit.url}
                      target="_blank"
                      rel="noreferrer"
                    >
                      {hit.url}
                    </a>
                  </li>
                ))}
              </ul>
            ) : null}
            {searchNote ? <p className="council__note">{searchNote}</p> : null}
            <p className="council__note" data-tone="muted">
              主动查到的资料只作参考，不会算进本次会诊共享的外来材料。
            </p>
            {background.length === 0 ? (
              <p className="council__note">
                本次未获得外部背景，各席位仅凭自己的技能单元作答。
              </p>
            ) : (
              <>
                <p className="council__note">
                  共享背景：在会诊启动时冻结，全席看到同一份材料。
                </p>
                <ul className="external__list">
                  {background.map((source) => (
                    <li key={source.id} className="external__item">
                      <span className="external__title">{source.title}</span>
                      <span className="external__meta">
                        获取于 {formatTime(source.fetchedAt)}
                        {source.publishedAt
                          ? ` · 发布于 ${formatTime(source.publishedAt, { dateOnly: true })}`
                          : ""}
                      </span>
                      <a
                        className="external__url"
                        href={source.url}
                        target="_blank"
                        rel="noreferrer"
                      >
                        {source.url}
                      </a>
                    </li>
                  ))}
                </ul>
              </>
            )}
            {seatSources.length > 0 ? (
              <ul className="external__seats">
                {seatSources.map(([masterId, list]) => (
                  <li key={masterId} className="external__seat">
                    <span className="external__seat-name">
                      {speech.find((seat) => seat.masterId === masterId)?.masterName ??
                        masterId}
                    </span>
                    <span className="external__seat-count mono">
                      另外查了 {list.length} 条资料
                    </span>
                  </li>
                ))}
              </ul>
            ) : null}
          </section>
        ) : null}

        {candidates.data && candidates.data.candidates.length > 0 ? (
          <details className="candidates">
            <summary className="candidates__summary">
              候选池 · {candidates.data.candidates.length} 位大师
            </summary>
            <ul className="candidates__list">
              {candidates.data.candidates.map((candidate) => (
                <li key={candidate.masterId} className="candidate">
                  <span className="candidate__name">{candidate.name}</span>
                  <span className="candidate__domain">{candidate.domain}</span>
                  <span className="candidate__score mono">
                    相关度 {candidate.relevance.toFixed(2)} · 对立度{" "}
                    {candidate.opposition.toFixed(2)} · 领域跨度{" "}
                    {candidate.domainDistance.toFixed(2)}
                  </span>
                </li>
              ))}
            </ul>
          </details>
        ) : null}

        <p className="council__note">
          六席按道法术气器势各取一位。第一轮每位大师在隔离上下文中独立作答，第二轮才交叉质询。
        </p>
      </div>
    </RealmShell>
  );
}
