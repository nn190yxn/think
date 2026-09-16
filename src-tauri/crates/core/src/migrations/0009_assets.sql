-- P9 资产统计：Skill 根目录、Skill 索引与依赖。
--
-- 只读取 Skill 的清单文件（manifest.json 或 SKILL.md 的 YAML 头），不读正文。
-- 清单缺失或格式无效时保留记录并标记 needs_repair，绝不因为一个坏 Skill
-- 放弃整次扫描。

CREATE TABLE IF NOT EXISTS asset_roots (
    id           TEXT PRIMARY KEY,
    path         TEXT NOT NULL,
    last_scan_at TEXT,
    created_at   TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS asset_roots_by_path
    ON asset_roots (path);

CREATE TABLE IF NOT EXISTS skills (
    id              TEXT PRIMARY KEY,
    root_id         TEXT NOT NULL REFERENCES asset_roots (id) ON DELETE CASCADE,
    path            TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    category        TEXT NOT NULL DEFAULT '',
    tags_json       TEXT NOT NULL DEFAULT '[]',
    enabled         INTEGER NOT NULL DEFAULT 1,
    manifest_source TEXT NOT NULL DEFAULT '',
    version         TEXT NOT NULL DEFAULT '',
    needs_repair    INTEGER NOT NULL DEFAULT 0,
    repair_reason   TEXT NOT NULL DEFAULT '',
    content_hash    TEXT NOT NULL DEFAULT '',
    installed_at    TEXT,
    modified_at     TEXT,
    first_seen_at   TEXT NOT NULL,
    last_seen_at    TEXT NOT NULL,
    missing         INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX IF NOT EXISTS skills_by_path
    ON skills (root_id, path);

CREATE INDEX IF NOT EXISTS skills_by_category
    ON skills (category);

CREATE INDEX IF NOT EXISTS skills_by_missing
    ON skills (missing, enabled);

CREATE TABLE IF NOT EXISTS skill_dependencies (
    id       TEXT PRIMARY KEY,
    skill_id TEXT NOT NULL REFERENCES skills (id) ON DELETE CASCADE,
    name     TEXT NOT NULL,
    version  TEXT NOT NULL DEFAULT '',
    kind     TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS skill_dependencies_by_skill
    ON skill_dependencies (skill_id);

-- 每次扫描留痕。root_id 不设外键：根目录移除后扫描历史仍可回看。
CREATE TABLE IF NOT EXISTS asset_scans (
    id           TEXT PRIMARY KEY,
    root_id      TEXT NOT NULL,
    root_path    TEXT NOT NULL DEFAULT '',
    status       TEXT NOT NULL,
    scanned      INTEGER NOT NULL DEFAULT 0,
    added        INTEGER NOT NULL DEFAULT 0,
    updated      INTEGER NOT NULL DEFAULT 0,
    removed      INTEGER NOT NULL DEFAULT 0,
    needs_repair INTEGER NOT NULL DEFAULT 0,
    reason       TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS asset_scans_by_time
    ON asset_scans (created_at DESC);
