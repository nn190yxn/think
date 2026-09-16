-- P11 追问与逐席发言：追问锚点与母会话关联。

-- 追问以新会话承载，母会话不被改写。锚点记录追问指向的那段判断。
ALTER TABLE council_sessions ADD COLUMN parent_session_id TEXT;
ALTER TABLE council_sessions ADD COLUMN anchor_kind TEXT NOT NULL DEFAULT '';
ALTER TABLE council_sessions ADD COLUMN anchor_text TEXT NOT NULL DEFAULT '';
ALTER TABLE council_sessions ADD COLUMN anchor_master_id TEXT;
ALTER TABLE council_sessions ADD COLUMN anchor_round INTEGER;
-- 锚点文字因超长被截断时为 1，供界面提示。
ALTER TABLE council_sessions ADD COLUMN anchor_truncated INTEGER NOT NULL DEFAULT 0;
-- 追问未能继承兵容时置 0，界面据此说明本次为常规选角。
ALTER TABLE council_sessions ADD COLUMN panel_inherited INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS council_sessions_by_parent
    ON council_sessions (parent_session_id);
