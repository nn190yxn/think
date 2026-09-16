-- 骑士团会诊：大师对立度预计算、会诊会话与轮次、模型平台与调用审计。

-- 对立度随大师包更新离线预计算，避免每次会诊做全量语义比对。
CREATE TABLE IF NOT EXISTS master_pairings (
    master_a_id      TEXT NOT NULL REFERENCES masters(id) ON DELETE CASCADE,
    master_b_id      TEXT NOT NULL REFERENCES masters(id) ON DELETE CASCADE,
    opposition_score REAL NOT NULL,
    computed_at      TEXT NOT NULL,
    PRIMARY KEY (master_a_id, master_b_id)
);

CREATE INDEX IF NOT EXISTS master_pairings_by_b
    ON master_pairings (master_b_id);

CREATE TABLE IF NOT EXISTS council_sessions (
    id               TEXT PRIMARY KEY,
    question         TEXT NOT NULL,
    domains_json     TEXT NOT NULL DEFAULT '[]',
    layers_json      TEXT NOT NULL DEFAULT '[]',
    strategy         TEXT NOT NULL DEFAULT 'steady',
    status           TEXT NOT NULL DEFAULT 'draft',
    conclusion       TEXT NOT NULL DEFAULT '',
    divergences_json TEXT NOT NULL DEFAULT '[]',
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

-- 每一次换批都是一次阵容轮次，全部保留，便于对比不同阵容的判断。
CREATE TABLE IF NOT EXISTS council_panels (
    session_id      TEXT NOT NULL REFERENCES council_sessions(id) ON DELETE CASCADE,
    rotation        INTEGER NOT NULL,
    strategy        TEXT NOT NULL,
    master_ids_json TEXT NOT NULL DEFAULT '[]',
    pinned_ids_json TEXT NOT NULL DEFAULT '[]',
    layers_json     TEXT NOT NULL DEFAULT '[]',
    gaps_json       TEXT NOT NULL DEFAULT '[]',
    created_at      TEXT NOT NULL,
    PRIMARY KEY (session_id, rotation)
);

-- 轮次记录锁定大师包版本号，写入后不再改写，历史会诊可完整复现。
CREATE TABLE IF NOT EXISTS council_turns (
    id             TEXT PRIMARY KEY,
    session_id     TEXT NOT NULL REFERENCES council_sessions(id) ON DELETE CASCADE,
    round          INTEGER NOT NULL,
    panel_rotation INTEGER NOT NULL DEFAULT 0,
    role           TEXT NOT NULL,
    master_id      TEXT,
    master_version INTEGER,
    content        TEXT NOT NULL DEFAULT '',
    citations_json TEXT NOT NULL DEFAULT '[]',
    status         TEXT NOT NULL DEFAULT 'ok',
    error_code     TEXT,
    created_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS council_turns_by_session
    ON council_turns (session_id, panel_rotation, round, created_at);

CREATE INDEX IF NOT EXISTS council_turns_by_master
    ON council_turns (master_id, created_at);

-- 模型平台配置。endpoint 与模型名可改，密钥不入库，由用户在系统凭据中提供。
CREATE TABLE IF NOT EXISTS ai_platforms (
    id           TEXT PRIMARY KEY,
    code         TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    endpoint     TEXT NOT NULL,
    model_name   TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 0,
    status       TEXT NOT NULL DEFAULT 'unconfigured',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

-- 每次模型调用都留痕，含离线失败与重试，供审计与成本观察。
CREATE TABLE IF NOT EXISTS llm_calls (
    id                TEXT PRIMARY KEY,
    purpose           TEXT NOT NULL,
    platform_code     TEXT NOT NULL DEFAULT '',
    model_name        TEXT NOT NULL DEFAULT '',
    prompt_tokens     INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    latency_ms        INTEGER NOT NULL DEFAULT 0,
    attempt           INTEGER NOT NULL DEFAULT 1,
    status            TEXT NOT NULL DEFAULT 'ok',
    error_code        TEXT,
    created_at        TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS llm_calls_by_purpose
    ON llm_calls (purpose, created_at);
