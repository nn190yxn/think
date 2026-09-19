# 思想熔炉 · 接通审计（2026-09-19）

目的：开工前核对「界面有、数据有，但没有任何地方真的用它做决定」的地方。三类对象分开看：**决定侧**（数据是否进入选角/发言/收敛）、**入口侧**（内核有命令但界面没有入口）、**展示侧**（只给人看，属设计意图，不算缺陷）。

审计方法是双向核对：从「谁写了」出发找「谁读了」，再从「谁读了」回到「它有没有影响结果」。

## 一、结论先说

主体是通的。六题轴、分歧判定、回音检查、联网检索、费用闸门、思维网络固化，都能追到消费点。真正的缺口集中在三处：**大师包没有安装入口**、**六题档案没进选角**、**采集只进不出**。

## 二、已接通（核对过，不必再动）

| 环节 | 消费点 |
|---|---|
| 席位题注入发言提示词 | `council/orchestrator.rs:532-533`、`council/orchestrator.rs:558` |
| 同题对立度 | 写入 `council/pairings.rs:115-119`，选角读取 `council/select.rs:104-105` |
| 每轮分歧与收敛 | `council/orchestrator.rs:290`（分歧判定方式）、`:315`（轮次上限）、`:475`（轮次循环） |
| 调参面板 | 28 项里 27 项有消费方：`network/repo.rs:263-326`、`network/consolidate.rs:49-54`、`connector/service.rs:45-221`、`cost.rs:268-271`、`council/divergence.rs:163-164`、`council/echo.rs:43`、`backup.rs:225`、`src-tauri/src/commands.rs:860`（中断判定）、`:995`/`:1138`（发送前预演）、`:2400-2403`（备份保留与快照保留） |
| 联网前脱敏 | `connector/guard.rs:125` 复用采集的脱敏规则，不新建规则集 |
| 洞察 → 会诊 | `src-tauri/src/commands.rs:1552-1580`（洞察转成一场会诊），界面入口 `src/components/EmberLayer.tsx:80` |

## 三、有内核、有类型、有演示数据，但界面没有入口

十五个命令在 `src/ipc/commands.ts` 有类型、在 `src/ipc/client.ts` 有演示实现、在内核里有实现，但 `src/` 里没有任何调用点。

| 命令 | 影响 | 处置建议 |
|---|---|---|
| `master_install`、`master_validate` | 「藏」只有一个「安装种子大师包」按钮（`src/realms/VaultRealm.tsx:194`），**用户装不了自己手上的大师包**。名册想长到 20 位，目前只剩蒸馏一条路 | 本规格补入口 |
| `companion_collide` | 陪练面板能开关、能设上限、能写规则（`companion_settings` / `companion_enable` / `companion_limit` / `companion_rules` 都有界面），但**对撞本体没有入口**；`companion::` 在内核里也没有跨模块消费者（只有 `companion_repo` 支撑「洞察」列表） | 待拍板 |
| `discovery_schedule` | 定时发现配不了（手动发现 `discovery_run` 有界面） | 待拍板 |
| `network_upsert_node`、`network_link`、`network_activate`、`network_decay` | 思维网络只能看，不能手动加节点、连线、手动激活或衰减 | 可不动（自动生成符合设计） |
| `council_sessions`、`consolidate_report`、`capture_summaries`、`distill_start`、`settings_get`、`master_versions`、`master_domains` | 备用接口或另有入口（蒸馏走 `distill_from_intake`，历史走 `council_recoverable` 与 `council_search`） | 可不动 |

## 四、存了或算了，但没有任何地方拿它做决定

1. **六题档案**（`LayerProfile`，`src-tauri/crates/core/src/master/mod.rs:152`）
   内核按题汇总、界面在「藏」的大师档案里显示（`src/realms/VaultRealm.tsx:325-329`，前端用 `src/domain/layers.ts` 的 `layerDepth` 重算一遍），但 `council/select.rs` 不读它——**选角完全不知道谁在哪一题上料多**。这正是本规格 P20.2 与 P20.3 要接的线。

2. **`connector.timeout_secs`**（设置里的「连接器超时」）
   在 `council/tuning.rs:213`（设置项定义）与 `:607`（读进快照字段）出现，此外全库无消费点——`crates/core/src/connector/` 目录里连 `timeout` 这个词都不存在。**目前是一根死滑杆**：推了不会有任何变化。其余 27 项都追到了消费方。

3. **采集（capture）**
   采集到的事件与摘要只有两个去处：自己的列表（`src/realms/SelfRealm.tsx:883-960` 展示、立即采集、删除）与脱敏规则被检索复用。**没有任何动作把采集到的内容送进录入、蒸馏或思考网络**——采集目前是个只进不出的收件箱。界面动作只有暂停、立即采集、删除。

4. **知识库（kb）**
   `crate::kb::` 在内核里没有跨模块消费者，`kb_*` 命令只服务「藏」里的资料库分区。资料库与会诊、蒸馏之间没有连线，是两条平行线。

5. **技能资产（asset）**
   `skill_dependencies`、`asset_scans` 只被自身 realm 消费，同样是自成一块的资产清单，不参与会诊与蒸馏。

## 五、展示侧（只给人看，属设计意图）

`coverage_matrix`（六题覆盖矩阵）、`ring_overview`（年轮概览）、`furnace_snapshot`（熔炉快照）、`council_round_metrics`（每轮分歧度）都只用于呈现。这些是「给人看的仪表」，不算未接通；要避免的是把上面第三、四节的东西误当成展示侧。

## 六、对 P20 至 P22 的影响

1. **P20 要加一项：大师包安装入口**（`master_install` + `master_validate`）。否则 20 位名册的「再招 14 位」只有蒸馏一条路。
2. **P20.8 的空位提示文案要改**：现在写「去「藏」装种子包，或在「炼」蒸馏一位」——前半句只对内置的 6 个种子包成立。
3. **P20.2 与 P20.3 方向被证实**：六题档案确实是当前最要害的一处「算了没用于做决定」。
4. **待拍板项**：采集出口、陪练对撞入口、定时发现入口、`connector.timeout_secs` 的死滑杆处置。

## 七、审计的边界（诚实交代）

- 结论来自静态核对（写入点与读取点的双向比对），**没有在真机上逐项点击验证**。
- 「十五个命令没有界面入口」的判断依据是 `src/` 全目录搜索无调用点；若某处通过变量拼接调用命令名，本审计会漏判。
- 调参面板「27 项有消费方」的判断依据是每根键都能找到读取点；**读取点是否在真机上生效**（例如某个分支永远不进入）未逐项验证。
