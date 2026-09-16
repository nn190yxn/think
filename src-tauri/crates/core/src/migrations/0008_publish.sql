-- P8 自我蒸馏与发布：自我蒸馏草稿、候选条目与数据主权操作审计。
--
-- 自我蒸馏把用户的历史思考记录蒸成「你」这位大师的初稿。候选条目逐条确认，
-- 未确认不进入安装。数据导出与清除都写 data_events，便于用户回看自己动过什么。

CREATE TABLE IF NOT EXISTS self_drafts (
    id           TEXT PRIMARY KEY,
    status       TEXT NOT NULL,
    record_count INTEGER NOT NULL DEFAULT 0,
    master_id    TEXT NOT NULL,
    model_calls  INTEGER NOT NULL DEFAULT 0,
    error_code   TEXT,
    note         TEXT NOT NULL DEFAULT '',
    updated_at   TEXT NOT NULL,
    created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS self_drafts_by_time
    ON self_drafts (created_at DESC);

CREATE TABLE IF NOT EXISTS self_items (
    id                TEXT PRIMARY KEY,
    draft_id          TEXT NOT NULL,
    ordinal           INTEGER NOT NULL,
    title             TEXT NOT NULL,
    layer             TEXT NOT NULL,
    trigger_condition TEXT NOT NULL DEFAULT '',
    steps_json        TEXT NOT NULL DEFAULT '[]',
    mechanism         TEXT NOT NULL DEFAULT '',
    boundary          TEXT NOT NULL DEFAULT '',
    evidence_json     TEXT NOT NULL DEFAULT '[]',
    source_record_id  TEXT,
    status            TEXT NOT NULL DEFAULT 'pending',
    created_at        TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS self_items_by_draft
    ON self_items (draft_id, ordinal ASC);

CREATE TABLE IF NOT EXISTS data_events (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    scope       TEXT NOT NULL DEFAULT 'all',
    table_count INTEGER NOT NULL DEFAULT 0,
    row_count   INTEGER NOT NULL DEFAULT 0,
    location    TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS data_events_by_time
    ON data_events (created_at DESC);
