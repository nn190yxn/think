# 思想熔炉 · 六题会诊 · 任务清单

范围：P17 六题档案与席位锚定到题（本规格第一步）、P18 同题对立与按题换批（第二步）、P19 记录按题累积（第三步）。P17 可在当前环境实施并验证；P18 与 P19 依赖 P17 的接口。

## P17 六题档案与席位锚定到题

目标：每位大师都能看到自己在六题上的积累深浅；每场会诊的每个席位都明确回答其中一题，且圆桌与逐席发言口径一致。

- [x] 17.1 新增 `0015_seat_questions.sql`：`council_panels` 增加 `seats_json`（非空，默认 `'[]'`）
- [x] 17.2 注册迁移 version 15，`latest_version()` 更新为 15
- [x] 17.3 `master/mod.rs` 新增 `Layer::question()`、`LayerProfile`，`MasterDetail` 增加 `layerProfile`
- [x] 17.4 `master/repo.rs` 的 `detail()` 按六题顺序汇总 `layerProfile`，空缺题保留
- [x] 17.5 `council/mod.rs` 新增 `SeatRef`，`PanelView` 增加 `seats`
- [x] 17.6 `council/repo.rs` 的 `record_panel` / `panels` / `copy_panel` 读写 `seats_json`，历史行回退
- [x] 17.7 `council/orchestrator.rs` 的第一轮提示词按席位题注入核心问题，`PROMPT_VERSION` 递增
- [x] 17.8 `council/speech.rs` 改用阵容记录的题，消除与圆桌的口径不一致
- [x] 17.9 前端补类型与 demo 状态：`MasterDetail.layerProfile`、`CouncilPanel.seats`
- [x] 17.10 前端 `layers.ts` 新增 `layerDepth`，`VaultRealm` 大师档案新增「六题档案」分区与样式
- [x] 17.11 `VaultRealm` 与 `CouncilRealm` 标注席位所属题
- [x] 17.12 补 `tests/masters.rs`、`tests/council.rs` 断言，扩展 `VaultRealm.test.tsx`、`CouncilRealm.test.tsx`
- [x] 17.13 运行全量门禁

门禁 P17：`cargo test -p thought-forge-core` 全部二进制通过；两个 crate clippy 归零；`pnpm typecheck`、`pnpm test`、`pnpm build` 通过；六题档案的题序与计数、历史面板回退、第一轮提示词包含被指派题目三条断言通过。

## P18 同题对立与按题换批

目标：分歧落在题内，换批优先补缺口题。

- [x] 18.1 `scoring` 增加按题的对立度计算，只用该题下的单元文本
- [x] 18.2 新增 `0016_layer_pairings.sql` 的 `master_layer_pairings` 按题存对立度，安装与更新大师包时重算
- [x] 18.3 `Selection.gaps` 语义扩展为缺口题：该题无人站上，或池中该题可用候选不足两位（凑不出同题对立），或上一轮在该题上没谈拢
- [x] 18.4 `select` 的碰撞策略改为优先换入与上一任在该题上立场不同的人，仍保持确定性与历史阵容不丢失
- [x] 18.5 会诊结论的分歧清单标明所属题（`DivergenceView`，旧纯文本历史分歧回退到「法」）
- [x] 18.6 前端会诊结论按题分组呈现分歧，并保留表格等效视图
- [x] 18.7 补测试：同题对立的对称性、换批不新增缺口题、历史面板与历史分歧仍可读
- [x] 18.8 运行全量门禁
- [x] 18.9 前端呈现缺口题：圆桌席位加虚线边框与「这一题还要再谈」标记，并单独列出「还要再谈的题」分区
- [x] 18.10 席位卡支持一题站上多位：库里缺某一题的大师时，补位会把第二位放到已有人站上的题，卡内逐人列出并各自可锁定

R5.1 的三条缺口口径都已落地：无人站上、池中候选不足两位（凑不出同题对立）、上一轮在该题上没谈拢（分歧清单里出现过的题，由命令层从会诊记录读出后传入选角）。

## P19 记录按题累积

目标：同一主题多轮会诊时，能看出每题立场的演化。

- [x] 19.1 新增 `0017_seat_stances.sql` 的 `council_stances`，会诊收敛或取消时记录各席位在各自题上的立场摘要（取最后一轮成功发言的第一句，纯本地推导）
- [x] 19.2 结论页按题对比上一次同主题会诊的立场，给出延续 / 调整 / 转向 / 新谈 / 停谈与用词重合度
- [x] 19.3 前端在结论第「六 · 前后几次结论」段内按题展示立场变化与上一轮原文
- [x] 19.4 补测试并运行全量门禁
