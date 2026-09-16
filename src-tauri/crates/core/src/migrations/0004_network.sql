-- P4 思维网络：认知节点、认知连线、激活记录、固化报告、思考记录与冲突洞察。

-- 认知节点。内容按 normalized_content 去重合并，合并后旧节点只标记 superseded_by，
-- 不物理删除，历史连线仍可回溯。
CREATE TABLE IF NOT EXISTS thought_nodes (
    id                    TEXT PRIMARY KEY,
    kind                  TEXT NOT NULL,
    content               TEXT NOT NULL,
    normalized_content    TEXT NOT NULL,
    source_kind           TEXT NOT NULL DEFAULT '',
    source_ref            TEXT NOT NULL DEFAULT '',
    domains_json          TEXT NOT NULL DEFAULT '[]',
    layers_json           TEXT NOT NULL DEFAULT '[]',
    activation            REAL NOT NULL DEFAULT 0,
    activation_updated_at TEXT NOT NULL,
    version               INTEGER NOT NULL DEFAULT 1,
    superseded_by         TEXT REFERENCES thought_nodes(id),
    created_at            TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS thought_nodes_by_kind
    ON thought_nodes (kind, created_at);

CREATE INDEX IF NOT EXISTS thought_nodes_by_normalized
    ON thought_nodes (normalized_content);

CREATE INDEX IF NOT EXISTS thought_nodes_by_activation
    ON thought_nodes (activation DESC);

CREATE INDEX IF NOT EXISTS thought_nodes_by_source
    ON thought_nodes (source_kind, source_ref);

-- 认知连线。冲突关系天然双向，查询时按两端同时匹配。
CREATE TABLE IF NOT EXISTS thought_edges (
    id                TEXT PRIMARY KEY,
    from_node_id      TEXT NOT NULL REFERENCES thought_nodes(id) ON DELETE CASCADE,
    to_node_id        TEXT NOT NULL REFERENCES thought_nodes(id) ON DELETE CASCADE,
    relation          TEXT NOT NULL,
    weight            REAL NOT NULL DEFAULT 0.5,
    co_activation_count INTEGER NOT NULL DEFAULT 0,
    last_activated_at TEXT,
    status            TEXT NOT NULL DEFAULT 'active',
    created_at        TEXT NOT NULL,
    UNIQUE (from_node_id, to_node_id, relation)
);

CREATE INDEX IF NOT EXISTS thought_edges_by_from
    ON thought_edges (from_node_id, status);

CREATE INDEX IF NOT EXISTS thought_edges_by_to
    ON thought_edges (to_node_id, status);

CREATE INDEX IF NOT EXISTS thought_edges_by_coactivation
    ON thought_edges (co_activation_count, last_activated_at);

-- 每一次唤醒都留痕，便于解释激活度的来源。
CREATE TABLE IF NOT EXISTS node_activations (
    id          TEXT PRIMARY KEY,
    node_id     TEXT NOT NULL REFERENCES thought_nodes(id) ON DELETE CASCADE,
    session_id  TEXT,
    increment   REAL NOT NULL,
    occurred_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS node_activations_by_node
    ON node_activations (node_id, occurred_at DESC);

-- 会诊落成思考记录，作为网络节点的来源凭据。
CREATE TABLE IF NOT EXISTS thought_records (
    id           TEXT PRIMARY KEY,
    session_id   TEXT,
    question     TEXT NOT NULL,
    topic_key    TEXT NOT NULL,
    domains_json TEXT NOT NULL DEFAULT '[]',
    layers_json  TEXT NOT NULL DEFAULT '[]',
    conclusion   TEXT NOT NULL DEFAULT '',
    adopted      INTEGER NOT NULL DEFAULT 0,
    reason       TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS thought_records_by_topic
    ON thought_records (topic_key, created_at DESC);

-- 固化报告。mode 取 manual 或 idle，report_json 保存本次处理的明细。
CREATE TABLE IF NOT EXISTS consolidation_runs (
    id                TEXT PRIMARY KEY,
    mode              TEXT NOT NULL,
    started_at        TEXT NOT NULL,
    finished_at       TEXT,
    strengthened_count INTEGER NOT NULL DEFAULT 0,
    decayed_count     INTEGER NOT NULL DEFAULT 0,
    merged_count      INTEGER NOT NULL DEFAULT 0,
    conflict_count    INTEGER NOT NULL DEFAULT 0,
    report_json       TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS consolidation_runs_by_time
    ON consolidation_runs (started_at DESC);

-- 洞察。固化识别出的冲突先落这里，主动助理在 P5 负责推送与处置。
CREATE TABLE IF NOT EXISTS insights (
    id                     TEXT PRIMARY KEY,
    kind                   TEXT NOT NULL,
    title                  TEXT NOT NULL,
    summary                TEXT NOT NULL DEFAULT '',
    related_node_ids_json  TEXT NOT NULL DEFAULT '[]',
    related_master_ids_json TEXT NOT NULL DEFAULT '[]',
    evidence_json          TEXT NOT NULL DEFAULT '[]',
    status                 TEXT NOT NULL DEFAULT 'new',
    action                 TEXT NOT NULL DEFAULT '',
    reason                 TEXT NOT NULL DEFAULT '',
    created_at             TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS insights_by_kind
    ON insights (kind, created_at DESC);
