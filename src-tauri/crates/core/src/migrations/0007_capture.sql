-- P7 采集与知识地形：行为采集事件、能力开关、派生摘要与知识库索引。
--
-- 采集事件只登记元数据与内容哈希，正文按脱敏规则处理后存放在 payload_json。
-- 能力开关逐项控制，默认全部关闭；全局暂停状态存于 settings 表。

CREATE TABLE IF NOT EXISTS capture_events (
    id           TEXT PRIMARY KEY,
    kind         TEXT NOT NULL,
    occurred_at  TEXT NOT NULL,
    source_app   TEXT NOT NULL DEFAULT '',
    payload_json TEXT NOT NULL DEFAULT '{}',
    content_hash TEXT NOT NULL DEFAULT '',
    redacted     INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS capture_events_by_time
    ON capture_events (occurred_at DESC, id ASC);

CREATE INDEX IF NOT EXISTS capture_events_by_kind
    ON capture_events (kind, occurred_at DESC);

CREATE INDEX IF NOT EXISTS capture_events_by_hash
    ON capture_events (kind, content_hash, occurred_at DESC);

-- 派生摘要：删除采集事件时在同一事务内级联删除。
CREATE TABLE IF NOT EXISTS capture_summaries (
    id         TEXT PRIMARY KEY,
    event_id   TEXT NOT NULL REFERENCES capture_events (id) ON DELETE CASCADE,
    topic      TEXT NOT NULL DEFAULT '',
    excerpt    TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS capture_summaries_by_event
    ON capture_summaries (event_id);

-- 每类能力一行。enabled 默认 0，consented_at 记录最近一次开启时的显式确认时间。
CREATE TABLE IF NOT EXISTS capture_settings (
    kind         TEXT PRIMARY KEY,
    enabled      INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL,
    consented_at TEXT
);

INSERT OR IGNORE INTO capture_settings (kind, enabled, updated_at) VALUES
    ('clipboard_text', 0, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('clipboard_image', 0, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('window', 0, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('file', 0, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));

-- 开启与暂停审计：能力切换、全局暂停与恢复都留痕，供用户回溯。
CREATE TABLE IF NOT EXISTS capture_audit (
    id         TEXT PRIMARY KEY,
    kind       TEXT NOT NULL,
    action     TEXT NOT NULL,
    reason     TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS capture_audit_by_time
    ON capture_audit (created_at DESC);

-- 知识库来源：只读元数据、不读正文。来源离线时保留索引并标记不可用。
CREATE TABLE IF NOT EXISTS kb_sources (
    id              TEXT PRIMARY KEY,
    path            TEXT NOT NULL,
    available       INTEGER NOT NULL DEFAULT 1,
    paused          INTEGER NOT NULL DEFAULT 0,
    last_scan_at    TEXT,
    last_success_at TEXT,
    created_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS kb_topics (
    id                 TEXT PRIMARY KEY,
    display_name       TEXT NOT NULL,
    manual_name        TEXT,
    doc_count          INTEGER NOT NULL DEFAULT 0,
    latest_created_at  TEXT,
    latest_modified_at TEXT
);

CREATE TABLE IF NOT EXISTS kb_documents (
    id              TEXT PRIMARY KEY,
    source_id       TEXT NOT NULL REFERENCES kb_sources (id) ON DELETE CASCADE,
    path            TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    version_label   TEXT NOT NULL DEFAULT '',
    domain          TEXT NOT NULL DEFAULT '',
    topic_id        TEXT,
    file_size       INTEGER NOT NULL DEFAULT 0,
    metadata_hash   TEXT NOT NULL DEFAULT '',
    created_at      TEXT,
    modified_at     TEXT,
    available       INTEGER NOT NULL DEFAULT 1,
    indexed_at      TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS kb_documents_by_path
    ON kb_documents (source_id, path);

CREATE INDEX IF NOT EXISTS kb_documents_by_topic
    ON kb_documents (topic_id);

CREATE INDEX IF NOT EXISTS kb_documents_by_domain
    ON kb_documents (domain);

CREATE VIRTUAL TABLE IF NOT EXISTS kb_search USING fts5(
    doc_id UNINDEXED,
    title,
    normalized_name,
    path,
    topic
);
