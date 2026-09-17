-- 立场摘要：会诊结束时把每个席位在自己那一题上的最新判断记成一句话。
-- 同一主题再次会诊时用它对比每题立场的变化，纯本地推导，不额外调用模型。
CREATE TABLE IF NOT EXISTS council_stances (
    session_id     TEXT NOT NULL REFERENCES council_sessions(id) ON DELETE CASCADE,
    panel_rotation INTEGER NOT NULL,
    master_id      TEXT NOT NULL,
    master_name    TEXT NOT NULL,
    layer          TEXT NOT NULL,
    summary        TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    PRIMARY KEY (session_id, panel_rotation, master_id)
);

CREATE INDEX IF NOT EXISTS council_stances_by_session
    ON council_stances (session_id, panel_rotation);
