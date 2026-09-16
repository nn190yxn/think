-- P6 蒸馏流水线：蒸馏任务、入库任务、动态信号与主动搜集设置。
--
-- 蒸馏任务以 stage 指针记录下一个待执行阶段，checkpoint_json 保存已产出的草稿。
-- 中断后从 stage 继续，已完成阶段不重复执行。

CREATE TABLE IF NOT EXISTS distill_jobs (
    id              TEXT PRIMARY KEY,
    source_kind     TEXT NOT NULL,
    source_ref      TEXT NOT NULL,
    master_id       TEXT NOT NULL,
    master_name     TEXT NOT NULL,
    domain          TEXT NOT NULL,
    output_dir      TEXT NOT NULL,
    materials_json  TEXT NOT NULL DEFAULT '[]',
    negative_json   TEXT NOT NULL DEFAULT '[]',
    stage           TEXT NOT NULL,
    state           TEXT NOT NULL,
    checkpoint_json TEXT NOT NULL DEFAULT '{}',
    error_code      TEXT,
    model_calls     INTEGER NOT NULL DEFAULT 0,
    updated_at      TEXT NOT NULL,
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS distill_jobs_by_state
    ON distill_jobs (state, updated_at DESC);

CREATE TABLE IF NOT EXISTS intake_jobs (
    id                   TEXT PRIMARY KEY,
    master_ref           TEXT NOT NULL,
    master_name          TEXT NOT NULL,
    domain               TEXT NOT NULL,
    mode                 TEXT NOT NULL,
    state                TEXT NOT NULL,
    material_count       INTEGER NOT NULL DEFAULT 0,
    accepted_count       INTEGER NOT NULL DEFAULT 0,
    rejected_count       INTEGER NOT NULL DEFAULT 0,
    overlap_summary_json TEXT NOT NULL DEFAULT '{}',
    schedule_json        TEXT NOT NULL DEFAULT '{}',
    updated_at           TEXT NOT NULL,
    created_at           TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS intake_jobs_by_state
    ON intake_jobs (state, updated_at DESC);

-- 动态信号：手动投喂与主动搜集共用的候选材料。主动搜集产生的信号默认
-- 处于 pending，只有用户逐批确认后才能进入蒸馏。
CREATE TABLE IF NOT EXISTS signals (
    id              TEXT PRIMARY KEY,
    job_id          TEXT NOT NULL,
    master_id       TEXT NOT NULL,
    title           TEXT NOT NULL,
    source_ref      TEXT NOT NULL,
    kind            TEXT NOT NULL DEFAULT 'material',
    text            TEXT NOT NULL DEFAULT '',
    status          TEXT NOT NULL DEFAULT 'pending',
    decision_reason TEXT NOT NULL DEFAULT '',
    overlap_ratio   REAL NOT NULL DEFAULT 0,
    discovered_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS signals_by_job
    ON signals (job_id, status, discovered_at ASC);

-- 主动搜集默认关闭，符合本地优先与「逐项开启」的约定。
CREATE TABLE IF NOT EXISTS discovery_settings (
    id           TEXT PRIMARY KEY,
    enabled      INTEGER NOT NULL DEFAULT 0,
    schedule_json TEXT NOT NULL DEFAULT '{}',
    updated_at   TEXT NOT NULL
);

INSERT OR IGNORE INTO discovery_settings (id, enabled, schedule_json, updated_at)
VALUES ('default', 0, '{}', strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
