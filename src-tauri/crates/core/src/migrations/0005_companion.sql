-- P5 主动助理：主动助学设置与洞察来源标注。
--
-- insights 表由 P4 建立，用于固化识别出的冲突。主动助理在同一张表上追加
-- 推送洞察，因此需要区分来源：只有 source = 'companion' 的洞察计入每日
-- 推送上限，用户手动触发的固化识别不受上限约束。

CREATE TABLE IF NOT EXISTS companion_settings (
    id          TEXT PRIMARY KEY,
    enabled     INTEGER NOT NULL DEFAULT 0,
    daily_limit INTEGER NOT NULL DEFAULT 5,
    rules_json  TEXT NOT NULL DEFAULT '{}',
    updated_at  TEXT NOT NULL
);

-- 默认关闭主动助学，符合本地优先与「逐项开启」的约定。
INSERT OR IGNORE INTO companion_settings (id, enabled, daily_limit, rules_json, updated_at)
VALUES ('default', 0, 5, '{}', strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));

ALTER TABLE insights ADD COLUMN source TEXT NOT NULL DEFAULT 'consolidation';

CREATE INDEX IF NOT EXISTS insights_by_source_time
    ON insights (source, created_at DESC);
