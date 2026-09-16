-- P13 检索安全与分歧判定：对外发送可核对、外部内容标注、判定方式留痕、提示词可追溯。

-- 检索审计补记实际发出的内容与是否脱敏，便于用户核对对外发送了什么。
ALTER TABLE connector_calls ADD COLUMN query_original TEXT NOT NULL DEFAULT '';
ALTER TABLE connector_calls ADD COLUMN query_sent TEXT NOT NULL DEFAULT '';
ALTER TABLE connector_calls ADD COLUMN redacted INTEGER NOT NULL DEFAULT 0;

-- 外部资料是否命中注入特征。
ALTER TABLE council_sources ADD COLUMN flagged INTEGER NOT NULL DEFAULT 0;

-- 轮次指标记录判定方式与是否回退，供界面标注。
ALTER TABLE council_round_metrics ADD COLUMN method TEXT NOT NULL DEFAULT 'lexical';
ALTER TABLE council_round_metrics ADD COLUMN fell_back INTEGER NOT NULL DEFAULT 0;

-- 提示词模板版本进入轮次记录与调用审计，与大师包版本共同构成复现条件。
ALTER TABLE council_turns ADD COLUMN prompt_version TEXT NOT NULL DEFAULT '';
ALTER TABLE llm_calls ADD COLUMN prompt_version TEXT NOT NULL DEFAULT '';
