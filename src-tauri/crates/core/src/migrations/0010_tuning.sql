-- P10 会诊与网络深化：逐轮分歧指标、认知社区与节点归属。

-- 会诊逐轮分歧指标，按阵容轮次独立记录。换批后曲线从新一轮次重新开始。
CREATE TABLE IF NOT EXISTS council_round_metrics (
    session_id        TEXT NOT NULL REFERENCES council_sessions(id) ON DELETE CASCADE,
    panel_rotation    INTEGER NOT NULL,
    round             INTEGER NOT NULL,
    participant_count INTEGER NOT NULL DEFAULT 0,
    avg_similarity    REAL NOT NULL DEFAULT 0,
    min_similarity    REAL NOT NULL DEFAULT 0,
    divergence        REAL NOT NULL DEFAULT 0,
    converged         INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL,
    PRIMARY KEY (session_id, panel_rotation, round)
);

CREATE INDEX IF NOT EXISTS council_round_metrics_by_session
    ON council_round_metrics (session_id, panel_rotation, round);

-- 认知社区。每次固化写一批新行，历史保留。
CREATE TABLE IF NOT EXISTS thought_clusters (
    id           TEXT PRIMARY KEY,
    run_id       TEXT,
    label        TEXT NOT NULL,
    domain       TEXT NOT NULL DEFAULT '',
    layer        TEXT NOT NULL DEFAULT '',
    member_count INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS thought_clusters_by_run
    ON thought_clusters (run_id);

ALTER TABLE thought_nodes ADD COLUMN cluster_id TEXT;

CREATE INDEX IF NOT EXISTS thought_nodes_by_cluster
    ON thought_nodes (cluster_id);
