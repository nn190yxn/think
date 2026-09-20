# 软件审计：37 条需求的达成率（2026-09-19）

## 方法与结论

**方法**：每条需求对四头核对——内核模块、界面入口、自动化用例、任务清单状态。**静态核对 + 门禁实测，不含真机走查**（真机另列，见第五节）。

**结论先说**：

- **37 条全部有实现**，没有"文档里有、代码里没有"的条目。
- 接通审计查出的六处缺口（大师包安装入口、六题档案进选角、采集只进不出、连接器超时滑杆、陪练对撞、定时发现）**已全部处置**并标注。
- 三套门禁全绿：前端 155 条用例、内核全部用例（含 `council` 44 条、`distill` 11 条）、桌面壳 32 条，两个 crate clippy 零警告。
- **真正的缺口在最后一公里**：8 项真机验收未做，安装包签名证书未知，**没有任何一条需求是在真机界面上逐条走查过的**。

图例：✅ 已实现且有用例或审计记录 · 🟡 已实现，只缺真机走查 · ⚪ 属于内容/人工判断，非代码

## 一、基线 16 条（`thought-forge-workbench`）

| # | 需求 | 落地位置 | 状态 |
|---|---|---|---|
| 1 | 系统级感知（剪贴板/前台窗口/文件活动，关闭即不记录） | `core/capture`、`realms/SelfRealm` | 🟡 用例覆盖（能力默认关闭、开关留痕、暂停不轮询、删除清摘要） |
| 2 | Skill 与 AI 资产统计 | `core/asset`、`realms/SelfRealm` | ✅ 17 条用例 |
| 3 | 知识库统计与分类（只读元数据） | `core/kb` | ✅ 与 `realms/SelfRealm` 联动 |
| 4 | 大师维度模型（领域 × 层次、缺位提示） | `core/master`、`core/network`（覆盖矩阵） | ✅ 缺位提示与建议清单有断言 |
| 5 | 大师蒸馏（可执行方法论带触发与边界） | `core/distill`（六阶段 + 六题收口） | ✅ 11 条用例 + 桩客户端全流程 |
| 6 | 多大师会诊（隔离作答、交叉质询、收敛） | `core/council`、`realms/CouncilRealm` | 🟡 44 条内核用例；真机全链路未走 |
| 7 | 成长轨迹沉淀 | `core/council` 留档、`realms/SelfRealm` 历史回顾 | ✅ 跨来源合并时间轴 |
| 8 | 隐私与数据主权 | `core/capture`、`core/data`、`realms/SelfRealm` | ✅ 导出/清除/留痕 |
| 9 | 离线可用性 | 全模块本地优先 | 🟡 断网场景未真机走 |
| 10 | 思维网络（节点连线、激活、衰减、固化） | `core/network` | ✅ 20 条用例（含多跳与幂等） |
| 11 | 主动助学助理 | `core/companion`、`components/EmberLayer` | 🟡 面板与规则有，对撞入口 23.2 已补；真机未走 |
| 12 | 迭代闭环（复访、修正、提升为原则） | `core/self_distill`、`core/network` | 🟡 原则可撤销、回音可见已接 |
| 13 | 认知骑士团与轮换（三档策略、整批换人） | `core/council/select`、`components/SeatPicker` | ✅ 选角与换批用例 |
| 14 | 分层知识库（四部分） | `core/corpus`、`core/master`、`core/kb` | ✅ 语料与单元分层 |
| 15 | 大师入库双通道 | `core/distill/intake`、`realms/VaultRealm` | ✅ 主动搜集默认关闭、逐批确认 |
| 16 | 大师包版本化与更新 | `core/master/repo` | ✅ 版本、快照、迁移前备份 |

## 二、深化 15 条（`2026-09-15-thought-forge-deepening`）

| # | 需求 | 落地位置 | 状态 |
|---|---|---|---|
| 1 | 自适应讨论轮次 | `core/council/orchestrator` | ✅ 轮次上限自适应 |
| 2 | 分歧度收敛曲线 | `core/council/divergence`、`components/DivergenceCurve` | ✅ 逐轮分歧度 |
| 3 | 调参项有边界 | `core/council/tuning` | ✅ 28 项，27 项有消费方，越界被拒 |
| 4 | 多跳激活传播 | `core/network` | ✅ 可复现用例 |
| 5 | 认知聚类 | `core/network/consolidate` | ✅ 社区划分 |
| 6 | 模型链路真机验证方案 | `examples/forge_verify` | ✅ 7 条；探针与退出码约定 |
| 7 | 逐席发言可见可追溯 | `components/SeatSpeech`、`realms/CouncilRealm` | ✅ 失败席位带错误码 |
| 8 | 落点追问 | `components/FollowUpForm` | ✅ 母会话不变、阵容继承 |
| 9 | 连接器三类（搜索/网页阅读/MCP） | `core/connector` | 🟡 握手与审计有用例；真机未走 |
| 10 | 会诊检索与检索快照 | `core/connector`、`core/council` | ✅ 结果冻结进快照 |
| 11 | 检索安全与外部内容可信 | `core/connector/guard` | ✅ 复用采集脱敏规则 |
| 12 | 分歧判定可信 | `core/council/divergence` | ✅ 极性优先可核对 |
| 13 | 成本与配额治理 | `core/cost` | ✅ 整数微元记账，预算闸门 |
| 14 | 凭据、备份与复现 | `core/credential`、`core/backup` | 🟡 用例齐（含迁移前备份、校验拒绝损坏）；凭据库真机未走 |
| 15 | 自我约束与运行可恢复 | `core/council`、`core/furnace` | ✅ 中断可识别、取消保值 |

## 三、六题会诊 6 条（`2026-09-17-thought-forge-six-questions`）

| # | 需求 | 落地位置 | 状态 |
|---|---|---|---|
| R1 | 六题档案（空缺保留） | `core/master::layer_profile` | ✅ 固定六维、缺为 0 |
| R2 | 大师层次声明 | `master.json` 的 `layers` | ✅ 与单元一致性有断言 |
| R3 | 席位锚定到题 | `core/council/orchestrator` | ✅ 席位题注入发言提示词 |
| R4 | 同题对立 | `core/council/pairings`、`select` | ✅ 只取该题单元文本 |
| R5 | 按题换批补人 | `components/SeatPicker`、`core/council` | ✅ 点将留痕、历史阵容不丢 |
| R6 | 记录累积（立场演化） | `core/council` 立场变化 | ✅ 延续/调整/转向/新谈/停谈 |

## 四、补充：点将与名册规格（P20–P24）

P20 全部 15 项、P21 全部 4 项、P23 全部 5 项已完成，P20.15 全量门禁经复核者本机实测通过；P24 为名册扩到 20 位（规划已出，待执行）。接通审计六处处置状态已更新到实测结果。

## 五、审计边界（诚实交代）

1. **没有真机走查**：以上均为静态核对与自动化门禁。界面手感、视觉、断网行为、凭据库真实读写，**一项都没在真机上逐条确认过**。
2. **"有用例"不等于"真机上一定对"**：用例跑在内存库与桩客户端上，真实模型、真实凭据库、真实文件系统仍是另一回事。
3. **达成率的准确说法**：按需求条目与代码/用例核对，**实现率 37/37**；按"真机确认可用"标准，**已确认 0/37**（8 项真机验收待做）。
4. 本审计未覆盖：性能上限、并发、长期运行稳定性、异常磁盘/权限场景。
