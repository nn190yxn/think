-- P14 成本、凭据与备份，并顺带补齐 P15 所需的撤销与运行控制列。

-- 平台单价按每千 token 记，连接器单价按每次调用记在连接器 config 里。
-- 全部以整数微元保存，避免浮点累加误差。
ALTER TABLE ai_platforms ADD COLUMN input_price_micros_per_1k INTEGER NOT NULL DEFAULT 0;
ALTER TABLE ai_platforms ADD COLUMN output_price_micros_per_1k INTEGER NOT NULL DEFAULT 0;
ALTER TABLE ai_platforms ADD COLUMN currency TEXT NOT NULL DEFAULT 'CNY';

ALTER TABLE llm_calls ADD COLUMN cost_micros INTEGER NOT NULL DEFAULT 0;
ALTER TABLE connector_calls ADD COLUMN cost_micros INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS llm_calls_by_time ON llm_calls (created_at DESC);
CREATE INDEX IF NOT EXISTS connector_calls_by_time ON connector_calls (created_at DESC);

-- 按日汇总，让配额检查保持常数时间。
CREATE TABLE IF NOT EXISTS cost_days (
    day          TEXT PRIMARY KEY,
    calls        INTEGER NOT NULL DEFAULT 0,
    tokens       INTEGER NOT NULL DEFAULT 0,
    cost_micros  INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL
);

-- 凭据引用。只保存引用名，密钥本体在操作系统凭据库。
CREATE TABLE IF NOT EXISTS credential_refs (
    id         TEXT PRIMARY KEY,
    ref_name   TEXT NOT NULL UNIQUE,
    scope      TEXT NOT NULL,
    owner_id   TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 备份留痕。
CREATE TABLE IF NOT EXISTS backups (
    id             TEXT PRIMARY KEY,
    path           TEXT NOT NULL,
    size_bytes     INTEGER NOT NULL DEFAULT 0,
    checksum       TEXT NOT NULL DEFAULT '',
    schema_version INTEGER NOT NULL DEFAULT 0,
    kind           TEXT NOT NULL DEFAULT 'manual',
    present        INTEGER NOT NULL DEFAULT 1,
    created_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS backups_by_time ON backups (created_at DESC);

-- 原则撤销：撤销后不再进入会诊上下文，历史发言保持不变。
ALTER TABLE thought_nodes ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
ALTER TABLE thought_nodes ADD COLUMN revoked_reason TEXT NOT NULL DEFAULT '';
ALTER TABLE thought_nodes ADD COLUMN revoked_at TEXT;

-- 运行控制与配额压缩范围：取消在轮末生效，压缩后的上下限随会话留痕。
ALTER TABLE council_sessions ADD COLUMN self_seat_included INTEGER NOT NULL DEFAULT 1;
ALTER TABLE council_sessions ADD COLUMN cancel_requested INTEGER NOT NULL DEFAULT 0;
ALTER TABLE council_sessions ADD COLUMN cancelled_at TEXT;
ALTER TABLE council_sessions ADD COLUMN heartbeat_at TEXT;
ALTER TABLE council_sessions ADD COLUMN quota_policy TEXT NOT NULL DEFAULT '';
ALTER TABLE council_sessions ADD COLUMN quota_max_rounds INTEGER;
ALTER TABLE council_sessions ADD COLUMN quota_max_seats INTEGER;
ALTER TABLE council_sessions ADD COLUMN quota_reason TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS council_sessions_by_status
    ON council_sessions (status, heartbeat_at);
