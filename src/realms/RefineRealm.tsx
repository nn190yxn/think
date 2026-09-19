import { useCallback, useEffect, useState } from "react";
import { RealmShell } from "./RealmShell";
import { useCommands } from "../app/ipc";
import { LayerGlyph } from "../components/LayerGlyph";
import { layerOf } from "../domain/layers";
import { intakeModeLabel, intakeStateLabel } from "../domain/labels";
import type {
  DiscoverySettings,
  DistillDetail,
  DistillJobView,
  DistillStageKey,
  IntakeJobView,
  IntakeMaterial,
  SignalView,
} from "../ipc/commands";

/** 蒸馏的七个阶段：stage 键与流水线状态一致，视图只按它高亮。 */
const STAGES: readonly { key: DistillStageKey; label: string; detail: string }[] = [
  { key: "skeleton", label: "通读材料", detail: "先理出大纲，等你确认后再开始" },
  { key: "extract", label: "分路提取", detail: "框架、原则、案例、反例、术语同时进行" },
  { key: "verify", label: "三重验证", detail: "能否跨领域佐证、能否回答新问题、是否与已有内容重复" },
  { key: "compose", label: "凝成技能", detail: "四个要素齐全才算成立" },
  { key: "map", label: "连成技能网", detail: "让技能之间彼此关联" },
  { key: "stress", label: "压力测试", detail: "拿干扰题试试，触发不准就重来" },
  { key: "deliver", label: "装进大师库", detail: "完成后进入大师库，成为一位可邀请的参与者" },
];

/** 五路提取，顺序与提取器一致。 */
const TRACKS: readonly { key: string; label: string }[] = [
  { key: "framework", label: "框架" },
  { key: "principle", label: "原则" },
  { key: "case", label: "案例" },
  { key: "counterexample", label: "反例" },
  { key: "term", label: "术语" },
];

/** 三重验证的判据。 */
const SIEVES: readonly string[] = ["跨域独立佐证", "能回答未明说的新问题", "与既有方法论不重复"];

const STAGE_ORDER = STAGES.map((stage) => stage.key);

const DEMO_MATERIAL: IntakeMaterial = {
  title: "手动投喂示例语料",
  kind: "text",
  text: "一段待蒸馏的原始材料。",
};

function stageState(job: DistillJobView, key: DistillStageKey): "done" | "active" | "idle" {
  if (job.state === "done") {
    return "done";
  }
  const current = STAGE_ORDER.indexOf(job.stage);
  const index = STAGE_ORDER.indexOf(key);
  if (index < current) {
    return "done";
  }
  return index === current ? "active" : "idle";
}

/**
 * 炼：蒸馏熔炉。按阶段展示蒸馏进度，并让用户看清
 * 「提炼出了什么」与「淘汰了什么、为什么」。
 */

interface DiscoveryScheduleView {
  enabled: boolean;
  intervalHours: number;
}

function readDiscoverySchedule(value: unknown): DiscoveryScheduleView {
  if (typeof value === "object" && value !== null) {
    const record = value as Record<string, unknown>;
    const rawHours = Number(record.intervalHours ?? record.interval_hours);
    return {
      enabled: record.enabled === true,
      intervalHours:
        Number.isFinite(rawHours) && rawHours > 0 ? Math.floor(rawHours) : 24,
    };
  }
  return { enabled: false, intervalHours: 24 };
}

export function RefineRealm() {
  const client = useCommands();
  const [job, setJob] = useState<DistillJobView | null>(null);
  const [detail, setDetail] = useState<DistillDetail | null>(null);
  const [intake, setIntake] = useState<IntakeJobView | null>(null);
  const [signals, setSignals] = useState<readonly SignalView[]>([]);
  const [discovery, setDiscovery] = useState<DiscoverySettings | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const loadIntake = useCallback(async () => {
    const jobs = await client.call("intake_list", { limit: 1 });
    const next = jobs[0] ?? null;
    setIntake(next);
    setSignals(
      next && next.state === "awaiting_confirmation"
        ? await client.call("intake_preview", { jobId: next.id })
        : [],
    );
  }, [client]);

  const loadDistill = useCallback(async () => {
    const jobs = await client.call("distill_list", { limit: 1 });
    const next = jobs[0] ?? null;
    setJob(next);
    setDetail(next ? await client.call("distill_detail", { jobId: next.id }) : null);
  }, [client]);

  const refresh = useCallback(async () => {
    await Promise.all([loadIntake(), loadDistill()]);
  }, [loadIntake, loadDistill]);

  useEffect(() => {
    void (async () => {
      try {
        setDiscovery(await client.call("discovery_settings", {}));
        await refresh();
      } catch {
        setNote("蒸馏任务读取失败");
      }
    })();
  }, [client, refresh]);

  async function run(action: () => Promise<void>) {
    setBusy(true);
    setNote(null);
    try {
      await action();
    } catch (cause) {
      setNote(cause instanceof Error ? cause.message : "操作未完成");
    } finally {
      setBusy(false);
    }
  }

  const feedManually = () =>
    run(async () => {
      await client.call("intake_create", {
        masterId: "master-munger",
        masterName: "查理·芒格",
        domain: "投资",
        materials: [DEMO_MATERIAL],
      });
      await refresh();
    });

  const startDistill = () =>
    run(async () => {
      if (!intake) {
        return;
      }
      setJob(await client.call("distill_from_intake", { intakeJobId: intake.id }));
      await refresh();
    });

  const confirmSkeleton = () =>
    run(async () => {
      if (!job) {
        return;
      }
      await client.call("distill_confirm", { jobId: job.id });
      await refresh();
    });

  const resumeDistill = () =>
    run(async () => {
      if (!job) {
        return;
      }
      await client.call("distill_resume", { jobId: job.id });
      await refresh();
    });

  const decideSignals = (acceptedIds: readonly string[], rejectedIds: readonly string[]) =>
    run(async () => {
      if (!intake) {
        return;
      }
      await client.call("intake_confirm", {
        jobId: intake.id,
        acceptedIds,
        rejectedIds,
      });
      await refresh();
    });

  const toggleDiscovery = (enabled: boolean) =>
    run(async () => {
      setDiscovery(await client.call("discovery_enable", { enabled }));
    });


  const saveSchedule = (next: DiscoveryScheduleView) =>
    run(async () => {
      setDiscovery(
        await client.call("discovery_schedule", {
          schedule: {
            enabled: next.enabled,
            intervalHours: next.intervalHours,
          },
        }),
      );
    });

  const runDiscovery = () =>
    run(async () => {
      const outcome = await client.call("discovery_run", {
        masterId: "master-munger",
        masterName: "查理·芒格",
        domain: "投资",
      });
      setNote(
        outcome.triggered
          ? `搜集到 ${outcome.discovered} 条，新增 ${outcome.pending} 条待确认`
          : "主动搜集已关闭，本次未发出任何外部请求",
      );
    });

  const draft = detail?.draft;
  const pending = signals.filter((signal) => signal.status === "pending");
  const schedule = readDiscoverySchedule(discovery?.schedule);

  return (
    <RealmShell
      realm="refine"
      aside={
        job ? (
          <div className="refine__job">
            <span className="refine__master">{job.masterName}</span>
            <span className="refine__jobmeta">
              {job.state === "done" ? "已出窑" : job.stageName} · 提问模型{" "}
              {job.modelCalls} 次
            </span>
          </div>
        ) : null
      }
    >
      <section className="furnace">
        <h2 className="section-head">蒸馏历程</h2>
        <ol className="pipeline">
          {STAGES.map((stage, index) => (
            <li
              key={stage.key}
              className="pipeline__stage"
              data-state={job ? stageState(job, stage.key) : "idle"}
            >
              <span className="pipeline__index mono">{String(index).padStart(2, "0")}</span>
              <span className="pipeline__label">{stage.label}</span>
              <span className="pipeline__detail">{stage.detail}</span>
            </li>
          ))}
        </ol>
        <div className="furnace__actions">
          {!job ? (
            <>
              <button className="action" type="button" onClick={feedManually} disabled={busy}>
                添加材料
              </button>
              <button
                className="action action--ghost"
                type="button"
                onClick={startDistill}
                disabled={busy || !intake}
              >
                开始蒸馏
              </button>
            </>
          ) : null}
          {job && job.state === "awaiting_confirmation" ? (
            <button className="action" type="button" onClick={confirmSkeleton} disabled={busy}>
              确认大纲，开始蒸馏
            </button>
          ) : null}
          {job && job.state !== "done" && job.state !== "awaiting_confirmation" ? (
            <button className="action" type="button" onClick={resumeDistill} disabled={busy}>
              从上次中断处继续
            </button>
          ) : null}
        </div>
      </section>

      <section className="furnace">
        <h2 className="section-head">五路提取</h2>
        <ul className="arms">
          {TRACKS.map((track) => {
            const extracted = draft?.extracted.filter((item) => item.track === track.key) ?? [];
            return (
              <li key={track.key} className="arm" data-lit={extracted.length > 0}>
                <span className="arm__label">{track.label}</span>
                <span className="arm__count mono">{extracted.length}</span>
              </li>
            );
          })}
        </ul>
      </section>

      <section className="furnace">
        <h2 className="section-head">三重验证</h2>
        <ul className="sieves">
          {SIEVES.map((label) => (
            <li key={label} className="sieve">
              <span className="sieve__mesh" aria-hidden="true" />
              <span className="sieve__label">{label}</span>
              <span className="sieve__count mono">
                {draft ? `${draft.verified.length} 通过` : "待验证"}
              </span>
            </li>
          ))}
        </ul>
        <div className="tray">
          <h3 className="tray__head">未通过的候选</h3>
          {draft && draft.excluded.length > 0 ? (
            <ul className="tray__list">
              {draft.excluded.map((item) => (
                <li key={item.id} className="tray__item">
                  <span className="tray__title">{item.title}</span>
                  <span className="tray__reason">{item.reason}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="setting-row__hint">还没有被淘汰的候选。</p>
          )}
        </div>
      </section>

      <section className="furnace">
        <h2 className="section-head">提炼出的技能</h2>
        {draft && draft.units.length > 0 ? (
          <ul className="units">
            {draft.units.map((unit) => {
              const meta = layerOf(unit.layer);
              return (
                <li key={unit.candidateId} className="unit">
                  <div className="unit__head">
                    <span className="unit__layer" data-layer={unit.layer}>
                      <LayerGlyph glyph={meta.glyph} />
                      {meta.name}
                    </span>
                    <span className="unit__title">{unit.title}</span>
                  </div>
                  <dl className="unit__spec">
                    <div className="kv__row">
                      <dt>触发条件</dt>
                      <dd>{unit.triggerCondition}</dd>
                    </div>
                    <div className="kv__row">
                      <dt>执行步骤</dt>
                      <dd>{unit.steps.join(" → ")}</dd>
                    </div>
                    <div className="kv__row">
                      <dt>作用机制</dt>
                      <dd>{unit.mechanism}</dd>
                    </div>
                    <div className="kv__row">
                      <dt>适用边界</dt>
                      <dd>{unit.boundary}</dd>
                    </div>
                  </dl>
                </li>
              );
            })}
          </ul>
        ) : (
          <p className="setting-row__hint">还没有提炼出技能。</p>
        )}
      </section>

      <section className="furnace">
        <h2 className="section-head">压力测试</h2>
        {draft && draft.stress.length > 0 ? (
          <>
            <p className="setting-row__hint">
              通过率 <span className="mono">{Math.round(draft.stressPassRate * 100)}%</span>
              。带「干扰」标记的题目用来验证会不会被误触发。
            </p>
            <ul className="stress">
              {draft.stress.map((item) => (
                <li key={item.question} className="stress__case" data-passed={item.passed}>
                  <span className="stress__q">
                    {item.decoy ? <span className="stress__decoy">干扰题</span> : null}
                    {item.question}
                    <span className="stress__verdict" data-passed={item.passed}>
                      {item.passed ? "通过" : "没通过"}
                    </span>
                  </span>
                  <span className="stress__a">{item.answer}</span>
                </li>
              ))}
            </ul>
          </>
        ) : (
          <p className="setting-row__hint">压力测试在技能单元成型后执行。</p>
        )}
      </section>

      <section className="furnace">
        <h2 className="section-head">入库通道</h2>
        <div className="intake__modes">
          <button className="action action--ghost" type="button" onClick={feedManually} disabled={busy}>
            手动添加
          </button>
          <span className="intake__status">
            {intake
              ? `${intakeModeLabel(intake.mode)} · ${intakeStateLabel(intake.state)}`
              : "暂无入库任务"}
          </span>
        </div>

        {pending.length > 0 ? (
          <>
            <p className="setting-row__hint">
              主动搜集逐批确认：勾选要留下的资料，未确认不会进入蒸馏。
            </p>
            <ul className="signals">
              {pending.map((signal) => (
                <li key={signal.id} className="signal">
                  <div className="signal__head">
                    <span className="signal__title">{signal.title}</span>
                    <span className="signal__overlap mono">
                      与已有内容重合 {Math.round(signal.overlapRatio * 100)}%
                    </span>
                  </div>
                  <span className="signal__ref mono">{signal.sourceRef}</span>
                  <div className="signal__actions">
                    <button
                      className="action action--mini"
                      type="button"
                      disabled={busy}
                      onClick={() => decideSignals([signal.id], [])}
                    >
                      采纳
                    </button>
                    <button
                      className="action action--mini action--ghost"
                      type="button"
                      disabled={busy}
                      onClick={() => decideSignals([], [signal.id])}
                    >
                      剔除
                    </button>
                  </div>
                </li>
              ))}
            </ul>
            <button
              className="action"
              type="button"
              disabled={busy}
              onClick={() => decideSignals(pending.map((signal) => signal.id), [])}
            >
              全部采纳
            </button>
          </>
        ) : null}

        <div className="setting-row">
          <span className="setting-row__label">主动搜集</span>
          <button
            className="switch"
            type="button"
            role="switch"
            aria-label="主动搜集"
            aria-checked={discovery?.enabled === true}
            data-on={discovery?.enabled === true}
            onClick={() => void toggleDiscovery(discovery?.enabled !== true)}
          >
            {discovery?.enabled ? "已开启" : "已关闭"}
          </button>
        </div>
        <p className="setting-row__hint">
          默认关闭。关闭时不发起任何外部请求；开启后仍逐批确认。
        </p>
        <button
          className="action action--ghost"
          type="button"
          disabled={busy}
          onClick={runDiscovery}
        >
          立即搜集一批
        </button>
        <div className="setting-row">
          <span className="setting-row__label">定时发现</span>
          <button
            className="switch"
            type="button"
            role="switch"
            aria-label="定时发现"
            aria-checked={schedule.enabled}
            data-on={schedule.enabled}
            disabled={busy}
            onClick={() =>
              void saveSchedule({
                ...schedule,
                enabled: !schedule.enabled,
              })
            }
          >
            {schedule.enabled ? "已开启" : "已关闭"}
          </button>
        </div>
        <p className="setting-row__hint">
          定时关闭时只接受手动搜集；开启后按间隔排队，不会绕过逐批确认。
        </p>
        <div className="setting-row">
          <span className="setting-row__label">发现间隔</span>
          <div className="adopt-row">
            {[6, 12, 24].map((hours) => (
              <button
                key={hours}
                type="button"
                aria-pressed={schedule.intervalHours === hours}
                disabled={busy}
                onClick={() =>
                  void saveSchedule({
                    ...schedule,
                    intervalHours: hours,
                  })
                }
              >
                {hours} 小时
              </button>
            ))}
          </div>
        </div>

      </section>

      {note ? (
        <p className="setting-row__hint" data-tone="warn">
          {note}
        </p>
      ) : null}
    </RealmShell>
  );
}
