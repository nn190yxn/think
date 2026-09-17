-- 同题对立度：按题分别预计算每位大师与其余大师的用词差异。
-- 换批补位时用它挑出在缺口题上立场不同的人，避免只按「领域不同」判断火花。
CREATE TABLE IF NOT EXISTS master_layer_pairings (
    master_a_id      TEXT NOT NULL REFERENCES masters(id) ON DELETE CASCADE,
    master_b_id      TEXT NOT NULL REFERENCES masters(id) ON DELETE CASCADE,
    layer            TEXT NOT NULL,
    opposition_score REAL NOT NULL,
    computed_at      TEXT NOT NULL,
    PRIMARY KEY (master_a_id, master_b_id, layer)
);

CREATE INDEX IF NOT EXISTS master_layer_pairings_by_layer
    ON master_layer_pairings (layer, master_b_id);
