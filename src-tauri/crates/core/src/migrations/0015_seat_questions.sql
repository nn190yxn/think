-- P17 六题会诊：阵容把每个席位指派到一道题，逐席发言与提示词都以这套指派为准。

-- 与 master_ids_json 同序，元素形如 {"masterId":"...","layer":"dao"}。
-- 历史行保持空数组，读取时按大师声明的层次回退。
ALTER TABLE council_panels ADD COLUMN seats_json TEXT NOT NULL DEFAULT '[]';
