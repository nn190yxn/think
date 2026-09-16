-- 大师库与分层知识库的第一层：大师包、技能单元、版本快照、原始语料与引用溯源。

CREATE TABLE IF NOT EXISTS masters (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    domain          TEXT NOT NULL,
    layers_json     TEXT NOT NULL,
    summary         TEXT NOT NULL DEFAULT '',
    style           TEXT NOT NULL DEFAULT '',
    blind_spots     TEXT NOT NULL DEFAULT '',
    status          TEXT NOT NULL DEFAULT 'ready',
    current_version INTEGER NOT NULL,
    installed_at    TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- 每次安装或更新都写入一整份版本记录，会诊只读当前版本。
CREATE TABLE IF NOT EXISTS master_versions (
    master_id        TEXT NOT NULL REFERENCES masters(id) ON DELETE CASCADE,
    version          INTEGER NOT NULL,
    unit_count       INTEGER NOT NULL,
    source_refs_json TEXT NOT NULL DEFAULT '[]',
    diff_json        TEXT NOT NULL DEFAULT '{}',
    note             TEXT NOT NULL DEFAULT '',
    created_at       TEXT NOT NULL,
    PRIMARY KEY (master_id, version)
);

CREATE TABLE IF NOT EXISTS master_units (
    id                TEXT PRIMARY KEY,
    master_id         TEXT NOT NULL REFERENCES masters(id) ON DELETE CASCADE,
    version           INTEGER NOT NULL,
    ordinal           INTEGER NOT NULL,
    title             TEXT NOT NULL,
    layer             TEXT NOT NULL,
    trigger_condition TEXT NOT NULL,
    steps_json        TEXT NOT NULL,
    mechanism         TEXT NOT NULL,
    boundary          TEXT NOT NULL,
    flagged_reason    TEXT,
    created_at        TEXT NOT NULL,
    UNIQUE (master_id, version, title)
);

CREATE INDEX IF NOT EXISTS master_units_by_version
    ON master_units (master_id, version, ordinal);

-- 原始语料只登记元数据并按路径引用，不复制正文，避免版权与体积问题。
CREATE TABLE IF NOT EXISTS corpus_items (
    id              TEXT PRIMARY KEY,
    source_kind     TEXT NOT NULL,
    source_ref      TEXT NOT NULL,
    title           TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    master_ids_json TEXT NOT NULL DEFAULT '[]',
    location_hint   TEXT NOT NULL DEFAULT '',
    content_hash    TEXT NOT NULL,
    byte_size       INTEGER NOT NULL DEFAULT 0,
    available       INTEGER NOT NULL DEFAULT 1,
    registered_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS corpus_items_by_hash
    ON corpus_items (content_hash);

-- 语料被删除时引用置空，摘录仍保留，于是大师包不受影响但会标注来源缺失。
CREATE TABLE IF NOT EXISTS corpus_citations (
    id             TEXT PRIMARY KEY,
    master_unit_id TEXT NOT NULL REFERENCES master_units(id) ON DELETE CASCADE,
    corpus_item_id TEXT REFERENCES corpus_items(id) ON DELETE SET NULL,
    excerpt        TEXT NOT NULL DEFAULT '',
    location       TEXT NOT NULL DEFAULT '',
    created_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS corpus_citations_by_unit
    ON corpus_citations (master_unit_id);

-- 只索引元数据字段，与 document-index 的做法一致，正文不进索引。
CREATE VIRTUAL TABLE IF NOT EXISTS corpus_search USING fts5(
    corpus_id UNINDEXED,
    title,
    normalized_name,
    source_ref,
    tokenize = 'trigram'
);
