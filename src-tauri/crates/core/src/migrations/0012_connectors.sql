-- P12 连接器与联网检索：连接器配置、调用审计与检索快照。

-- 连接器配置。密钥不入库，配置里只保存服务地址与能力声明。
CREATE TABLE IF NOT EXISTS connectors (
    id           TEXT PRIMARY KEY,
    kind         TEXT NOT NULL,
    display_name TEXT NOT NULL,
    endpoint     TEXT NOT NULL DEFAULT '',
    config_json  TEXT NOT NULL DEFAULT '{}',
    enabled      INTEGER NOT NULL DEFAULT 0,
    status       TEXT NOT NULL DEFAULT 'unconfigured',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS connectors_by_kind
    ON connectors (kind, enabled);

-- 连接器调用审计。与 llm_calls 分离，便于分别观察模型成本与检索成本。
CREATE TABLE IF NOT EXISTS connector_calls (
    id           TEXT PRIMARY KEY,
    connector_id TEXT,
    kind         TEXT NOT NULL,
    purpose      TEXT NOT NULL,
    session_id   TEXT,
    query        TEXT NOT NULL DEFAULT '',
    result_count INTEGER NOT NULL DEFAULT 0,
    latency_ms   INTEGER NOT NULL DEFAULT 0,
    status       TEXT NOT NULL DEFAULT 'ok',
    error_code   TEXT,
    created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS connector_calls_by_purpose
    ON connector_calls (purpose, created_at DESC);

-- 检索快照。master_id 为空表示共享背景，非空表示该席位的补充检索。
CREATE TABLE IF NOT EXISTS council_sources (
    id             TEXT PRIMARY KEY,
    session_id     TEXT NOT NULL REFERENCES council_sessions(id) ON DELETE CASCADE,
    panel_rotation INTEGER NOT NULL,
    round          INTEGER NOT NULL DEFAULT 0,
    master_id      TEXT,
    kind           TEXT NOT NULL,
    title          TEXT NOT NULL DEFAULT '',
    url            TEXT NOT NULL DEFAULT '',
    snippet        TEXT NOT NULL DEFAULT '',
    published_at   TEXT,
    fetched_at     TEXT NOT NULL,
    body           TEXT,
    created_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS council_sources_by_session
    ON council_sources (session_id, panel_rotation, master_id);
