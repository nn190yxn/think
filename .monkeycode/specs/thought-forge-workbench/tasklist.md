# 自我提升的思想熔炉 · 实施任务清单

Feature Name: thought-forge-workbench
Updated: 2026-09-14

## 执行方式

整体方案一次性设计完成，实施按阶段推进。每个阶段完成并通过该阶段门禁后，才进入下一阶段。门禁不通过时先修复本阶段，不向下游带问题。

本清单覆盖全部八个阶段。当前进度以任务条目的完成状态为准。

## P1 骨架

目标：工程可启动，两套主题可切换，迁移可重放。

- [x] 1.1 建立本仓库根目录的工程，配置 Tauri 2、React 19、TypeScript 严格模式、Vite 7、Vitest 与 pnpm 10
- [x] 1.2 配置 Cargo workspace 与 `src-tauri` 依赖，落地 lib 与 bin 双结构，纯逻辑拆入 `crates/core`
- [x] 1.3 实现类型化 command 边界：`CommandResult<T>` 判别联合、稳定错误码、`commandClient` 统一封装
- [x] 1.4 实现 SQLite 连接与应用数据目录初始化，开启外键、WAL 与 busy timeout
- [x] 1.5 实现版本化迁移执行器，落地 P1 所需基础迁移
- [x] 1.6 落地设计令牌：间距、圆角、描边、阴影、模糊、字号、行高、动效、六层层次色
- [x] 1.7 落地窑变与素瓷两套主题，实现根元素主题切换与切换即时生效
- [x] 1.8 实现炉脊导航、五个境界路由骨架与视点切换过渡
- [x] 1.9 实现炉温组件与常量映射，接入后端快照统计
- [x] 1.10 配置 NSIS 与 WebView2 bootstrapper 基础打包参数
- [x] 1.11 编写前端无障碍契约测试与 Rust 基础单元测试

门禁 P1：已通过。`pnpm typecheck` 无错误；`pnpm test` 37 项全绿；`cargo test -p thought-forge-core` 7 项全绿；迁移重复执行与账本连续性有测试覆盖；两套主题共用令牌名，切换只改根元素属性，测试断言切换后 DOM 结构不变。

遗留说明：`thought-forge-desktop` 需要 WebView 系统库，本环境为 Linux 无桌面依赖，因此桌面壳未在本机编译；命令包装层按 `CommandResult` 形状实现并已由内核测试间接覆盖。

## P2 大师库

目标：可安装大师包并浏览版本历史，语料可检索可溯源。

- [x] 2.1 定义大师包目录格式与 frontmatter schema，含领域、层次、版本与来源标注
- [x] 2.2 实现大师包校验与安装，校验失败拒绝安装并给出原因
- [x] 2.3 实现 `masters`、`master_units` 与 `master_versions` 表与仓储
- [x] 2.4 实现大师包版本化：更新生成新版本、保留旧版本快照、支持回退
- [x] 2.5 实现层次取值校验，限定道、法、术、气、器、势六层
- [x] 2.6 实现 `corpus_items`、`corpus_search`（FTS5）与 `corpus_citations` 表与仓储
- [x] 2.7 实现原始语料登记、索引与检索
- [x] 2.8 实现技能单元到语料的位置映射与引用溯源
- [x] 2.9 实现领域与层次覆盖矩阵及空缺推荐
- [x] 2.10 实现大师架子与资产图谱界面
- [x] 2.11 编写版本化、快照保留、语料索引与覆盖矩阵测试

门禁 P2：已通过（大师库范围）。`cargo test -p thought-forge-core` 21 项全绿（含 12 项大师库与 2 项种子包）；六个种子大师包六层各一位、覆盖矩阵无空缺、语料可检索；`pnpm typecheck` 无错误；`pnpm test` 40 项全绿；藏境界可浏览覆盖矩阵、大师架子、四要素与来源溯源、版本历史并回退。

已知缺口：任务 2.10 中的「资产图谱」未实现。藏境界当前只有大师架子、语料检索与知识地形；需求 2「Skill 与 AI 资产统计」的 Skill 目录扫描、`skills` 表、`get_asset_summary` 与 Skill 详情均缺失，`ai_platforms` 表仅覆盖需求 2 的平台识别部分。详见「阶段复盘」。

遗留说明：`thought-forge-desktop` 命令包装层（`master_install`、`coverage_matrix` 等）已按 `CommandResult` 形状实现，但桌面壳需 WebView 系统库，本环境未编译；命令覆盖由 `crates/core` 测试间接保证。

## P3 骑士团会诊

目标：会诊链路完整可用，骑士团可选角可换批。

- [x] 3.1 实现候选池模型与查询，返回相关度、对立度、领域距离三项评分
- [x] 3.2 实现 `master_pairings` 预计算与大师对立度离线任务
- [x] 3.3 实现三种轮换策略选角：稳妥、碰撞、意外
- [x] 3.4 实现层次覆盖约束与席位下限降级逻辑
- [x] 3.5 实现保留席位与锁定机制
- [x] 3.6 实现 `council_sessions`、`council_turns` 表与仓储，记录大师包版本与阵容轮次
- [x] 3.7 实现会诊编排：问题解析、选角、第一轮隔离独立作答
- [x] 3.8 实现第二轮交叉质询与收敛裁决
- [x] 3.9 实现换批与阵容回流，支持在多个阵容之间对比
- [x] 3.10 实现圆桌界面：席位、光束三阶段动画、分歧珠子、策略开关、换批把手
- [x] 3.11 实现大师徽记与大师包视觉配置
- [x] 3.12 实现模型调用层：OpenAI-compatible 客户端、流式响应、重试、调用审计
- [x] 3.13 实现设置页模型平台配置与联网能力逐项开启
- [x] 3.14 编写独立性、层次覆盖、选角确定性、保留席位四条属性测试

门禁 P3：已通过（内核与界面），桌面壳命令待完整桌面工具链下编译。

- 内核：`cargo test -p thought-forge-core` 43 项全绿（core 7 + council 9 + llm 13 + masters 12 + seed_packs 2）。四条属性测试覆盖独立性（第一轮提示词不含他人单元与原始语料）、层次覆盖（六层各取一位，换批后仍满足）、选角确定性（同池同策略两次结果一致）、保留席位（锁定席位置换批仍在名单）。
- 选角实现要点：层次覆盖用二部图最大匹配（Kuhn 增广），避免独苗大师被其他层先抢走；换批排除项按重罚而非硬过滤处理，候选池被抽干时回头复用老人也不击穿覆盖约束。
- 界面：圆桌六席、三阶段光束、分歧珠子、策略开关、换批把手、候选池三项评分；设置页联网开关（默认关）、平台清单与最近调用审计。`pnpm typecheck` 无错误，`pnpm test` 47 项全绿（8 个文件）。
- 命令层：新增 `council_candidates/create/select/rotate/run/sessions/session`、`master_history`、`platform_list/upsert/enable`、`networking_get/set`、`llm_calls`；模型传输在桌面壳用 `reqwest`（blocking + rustls）实现，密钥只从用户环境变量 `THOUGHT_FORGE_API_KEY` 读取，不入库。`cargo metadata` 已解析通过（482 个包）。

遗留说明：桌面壳仍依赖 WebView 系统库，本环境未编译（与 P2 相同）；`council_run` 的真实网络路径只在用户机器上生效。大师徽记当前用层次几何徽记与层次色，未引入每位大师的独立图标。

## P4 思维网络

目标：网络写入与强化生效，固化可用。

- [x] 4.1 实现 `thought_nodes`、`thought_edges`、`node_activations` 表与仓储
- [x] 4.2 实现节点类型与五类关系类型，含冲突关系双向可见
- [x] 4.3 实现会诊结果写入网络，自动建立节点与连线
- [x] 4.4 实现激活传播：节点及一跳邻居激活度提升
- [x] 4.5 实现时间衰减与半衰期配置
- [x] 4.6 实现共激活强化与连线权重更新
- [x] 4.7 实现固化调度：空闲触发、手动触发、批量事务与处理上限
- [x] 4.8 实现固化四件事：强化、衰减、节点合并、冲突识别
- [x] 4.9 实现 `consolidation_runs` 报告与查询
- [x] 4.10 实现思维星图：力导向布局、三级视点、节点与连线视觉编码
- [x] 4.11 实现待裁决与新入炉处理入口
- [x] 4.12 实现成长轨迹与演化链
- [x] 4.13 编写激活单调、固化幂等、网络汇聚三条属性测试

门禁 P4：已通过（内核与界面），桌面壳命令待完整桌面工具链下编译。

- 内核：`cargo test -p thought-forge-core` 全绿（core 7 + council 9 + llm 13 + masters 12 + network 17 + network_perf 1 + seed_packs 2）。三条属性测试改用 `proptest`（各 32 例随机输入）：激活单调（`property_activation_never_reduces`）、固化幂等（`property_consolidation_is_idempotent`）、网络汇聚（`property_council_converges_into_network`）。
- 性能：`network_perf.rs` 写入十万节点后调用 `get_graph`，实测含建库 1.37 秒（`cargo test` 未优化构建），断言检索阶段 3 秒内完成且结果按上限截断。
- 实现要点：节点按 `normalized_content` 去重合并，被合并节点只标 `superseded_by` 不物理删除；激活 `activation = decayed + increment`，一跳邻居按权重折半；固化四件事在批量事务内完成，幂等靠共激活计数清零与连线状态跃迁一次；冲突关系按节点 id 排序归一化，仅 `Conflicts` 双向可见。
- 界面：思维星图（确定性力导向、星云/星图/心核三级视点、`J`/`K` 移动、`Enter` 入心核、`Space` 发起会诊）、待裁决冲突列表、成长轨迹与演化链、记忆固化面板。`pnpm typecheck` 无错误，`pnpm test` 62 项全绿（10 个文件）。
- 命令层：新增 16 条网络命令（节点/连线/激活/衰减/图谱/裁决/落网/记录列表/记录对比/记录决策/固化触发/固化报告/固化历史）。

遗留说明：桌面壳仍依赖 WebView 系统库，本环境未编译（与 P2/P3 相同）；属性测试使用固定随机种子范围的 32 例输入，未纳入长期回归基线。

## P5 主动助理

目标：助理可在后台碰撞并推送洞察，迭代闭环成型。

- [x] 5.1 实现 `insights`、`companion_settings` 表与仓储
- [x] 5.2 实现主动助学开关、触发规则与每日推送上限
- [x] 5.3 实现后台轻量碰撞：受控上下文、单次模型调用
- [x] 5.4 实现关联洞察、冲突洞察与盲区洞察三类生成
- [x] 5.5 实现洞察处置：采纳、忽略、转为会诊
- [x] 5.6 实现余烬界面，含分色、衰减与克制表现
- [x] 5.7 实现迭代闭环：主题聚合、演化链串联、修正记录
- [x] 5.8 实现连续采纳三次提升为个人原则节点
- [x] 5.9 实现我境界界面：演化长河、原则印章、年轮概览
- [x] 5.10 编写推送上限与主动助学可关闭属性测试

门禁 P5：后台碰撞可产出洞察；推送不超过上限；关闭后无新推送；两条属性测试通过。

验证记录（2026-09-14）：

- 迁移：`0005_companion.sql` 建 `companion_settings`（默认 `id='default'`、关闭、上限 5），并给 `insights` 加 `source` 列区分 `companion`（计入每日上限）与 `consolidation`（手动固化不限）；`latest_version()`=5。
- 内核：`companion/mod.rs`（关系/冲突/盲区三类、设置与规则、碰撞信号与结果、主题/领域/年轮/原则视图）、`companion/repo.rs`（设置读写、`used_today`/`remaining_today`、`push_insight` 达上限返 `None`、洞察处置）、`companion/collide.rs`（bigram 相关度检索节点 + 领域检索大师、单次模型调用、上下文限 1200 字符、无相关节点补盲区洞察、模型失败只记审计）、`companion/growth.rs`（主题聚合、连续采纳 ≥3 提升原则节点并以 `Derives` 连回原判断、年轮概览）。
- 命令层：新增 12 条命令（设置读取/开关/上限/规则、碰撞、洞察列表/处置/转会诊、主题、原则列表/提升、年轮概览）。
- 测试：`cargo test -p thought-forge-core` 全绿（companion 12 项，含两条 `proptest` 属性：推送上限、主动助学关闭后不推送不调用模型）。
- 界面：余烬（画布边缘簇、漂浮卡片、关联赭金/冲突朱砂/盲区玄青、按时间衰减、采纳/忽略/转为会诊、转会诊直达圆桌会话）；我境界新增主动助学设置、演化长河、原则印章、年轮概览。
- 前端：`pnpm typecheck` 无错误，`pnpm test` 68 项全绿（11 个文件）。

遗留说明：安装 WebView/GLib 系统库后桌面壳已可编译，命令层由 `cargo check -p thought-forge-desktop` 覆盖；主动助学默认关闭，端到端的真实模型碰撞需在桌面壳内开启联网与模型平台后验证。

## P6 蒸馏流水线

目标：可完整蒸馏出一位大师并安装，入库双通道可用。

- [x] 6.1 实现 `distill_jobs` 表与六阶段状态机，含检查点写入与续跑
- [x] 6.2 实现阶段0 整体理解与骨架确认门
- [x] 6.3 实现阶段1 五路并行提取：框架、原则、案例、反例、术语
- [x] 6.4 实现阶段1.5 三重验证与排除原因记录
- [x] 6.5 实现阶段2 技能单元生成，强制四要素：触发条件、步骤、机制、边界
- [x] 6.6 实现阶段3 技能地图与交叉链接
- [x] 6.7 实现阶段4 压力测试与诱饵题用例
- [x] 6.8 实现阶段5 交付：技能地图、术语表、精华摘要与安装
- [x] 6.9 实现 `intake_jobs` 与 `signals` 表及仓储
- [x] 6.10 实现手动投喂通道：姓名、作品、文件、链接
- [x] 6.11 实现主动搜集通道：名单配置、公开资料检索、资料重叠度评估
- [x] 6.12 实现待确认清单与逐批确认，未确认不进入蒸馏
- [x] 6.13 实现大师包不适用反馈，接入下次蒸馏的负面依据
- [x] 6.14 实现炼境界界面：剖面炉体、五路提取臂、三层筛网、弃料托盘
- [x] 6.15 编写检查点续跑、蒸馏准入、主动搜集可控三条属性测试

门禁 P6：已通过（内核与界面），桌面壳与真实公开检索待完整工具链下验证。

验证记录（2026-09-14）：

- 迁移：`0006_distill.sql` 建 `distill_jobs`（`stage`/`state`/`checkpoint_json`/`error_code`/`model_calls`）、`intake_jobs`、`signals`（`status` 待确认/已接受/已拒绝）、`discovery_settings`（默认 `id='default'` 关闭）；`latest_version()`=6。
- 内核：`distill/mod.rs`（七阶段视图、任务/草稿/信号/设置 DTO、四要素与阈值常量）、`distill/repo.rs`（任务与检查点读写、信号决策、已知来源、入库任务、搜集设置）、`distill/pipeline.rs`（阶段0 整体理解后停在骨架确认门，阶段1 五路各一次模型调用，阶段1.5 三重验证用 bigram Jaccard 去重并把未通过候选记 `excluded`，阶段2 四要素缺一即排除，阶段3 本地算技能地图与 `MAX_SKILL_LINKS=200` 链接，阶段4 压力测试含诱饵题，阶段5 交付写 `master.json` 并 `master_repo::install`；`stage` 始终指向「下一个待执行阶段」实现检查点续跑；happy path `model_calls=9`）、`distill/intake.rs`（手动投喂直接确认、主动搜集关闭返回 `reason:"disabled"` 且不发请求、`DiscoveryClient` trait 与 `BlockedDiscovery` 占位、重叠度评估、负面依据）。
- 命令层：新增 14 条命令（蒸馏启动/从入库启动/列表/详情/确认/续跑，入库创建/列表/预览/确认，搜集设置/开关/排程/运行）；`discovery_run` 命令层注入 `BlockedDiscovery`，真实检索由外壳接入。
- 测试：`cargo test -p thought-forge-core` 全绿；`tests/distill.rs` 9 项，含三条 `proptest` 属性：只有通过验证的候选才被录取（`property_only_verified_candidates_are_admitted`）、续跑不重复已完成阶段（`property_resume_does_not_repeat_completed_stages`）、主动搜集开关可控（`property_discovery_is_controllable`）。
- 界面：炼境界剖面炉体（六阶段，当前阶段火光、已完成余温）、五路提取臂（按轨道亮起并显示候选数）、三层筛网、弃料托盘（逐条列出淘汰候选与原因）、技能锭（四要素强制展开）、压力测试（诱饵题标记与通过率）、入库双通道（手动投喂、待确认清单逐条/批量确认、主动搜集开关与立即检索）。
- 前端：`pnpm typecheck` 无错误，`pnpm test` 75 项全绿（12 个文件），`pnpm build` 通过；预览 stub 补齐 14 条 P6 命令与可变 demo 状态。

遗留说明：安装 WebView/GLib 系统库后桌面壳已可编译；主动搜集的真实公开检索尚未接入外壳，`discovery_run` 当前返回 `BlockedDiscovery` 结果，全链路的真实检索与模型蒸馏需在桌面壳内开启联网与模型平台后验证。

## P7 采集与知识地形

目标：系统级感知可用，知识统计成型。

- [x] 7.1 实现 `capture_events`、`capture_settings` 与 `capture_summaries` 表
- [x] 7.2 实现剪贴板采集：序列号变更检测、文本与图片、哈希去重
- [x] 7.3 实现前台窗口采集：事件钩子优先、轮询兜底、连续采样合并
- [x] 7.4 实现文件活动采集：notify 递归监听、有界队列、trailing window 合并
- [x] 7.5 实现采集写入链路：单 worker、批量事务、脱敏规则引擎
- [x] 7.6 实现采集能力逐项开关、全局暂停与开启审计
- [x] 7.7 实现采集台界面：原始记录浏览、筛选与删除
- [x] 7.8 实现知识库扫描、名称规范化与智能归组
- [x] 7.9 实现知识地形与年轮趋势可视化
- [x] 7.10 实现采集幂等、暂停、脱敏、离线保留属性测试

门禁 P7：已通过（内核、界面与外壳）。四类采集逐项可开关、可全局暂停；脱敏生效；四条属性测试通过；来源离线时既有索引保持可读。OS 级抓取已由外壳接入（见下方补充记录），真机剪贴板与前台窗口效果仍需 Windows 环境验收。

验证记录（2026-09-14）：

- 迁移：`0007_capture.sql` 建 `capture_events`（kind/occurred_at/source_app/payload_json/content_hash/redacted）、`capture_summaries`（随事件级联删除）、`capture_settings`（四类默认关闭、记录显式同意时间）、`capture_audit`、`kb_sources`/`kb_topics`/`kb_documents`/`kb_search`（FTS5）；`db/migrations.rs` 注册 version 7，`latest_version()`=7。
- 内核·采集：`capture/mod.rs`（四类能力常量、原始样本与 `CaptureSource` trait、`NoopCaptureSource`、`parse_epoch`/`normalize_text` 与轮询/去重/队列常量）、`capture/redact.rs`（内置数字串/长令牌/邮箱 + 自定义词条，JSON 递归脱敏）、`capture/repo.rs`（能力开关、暂停、去重窗口、脱敏规则、审计、事件/摘要读写）、`capture/pipeline.rs`（`collect_once` 先取全部样本再单事务写入；窗口连续采样按时长合并、文件活动按 trailing window 合并、有界队列计数丢弃、哈希去重、脱敏后落库）。
- 内核·知识库：`kb/mod.rs`（文档/主题/来源 DTO、`display_stem`/`version_label`/`topic_key` 归组）、`kb/repo.rs`（来源增删与可用性、主题重算、文档 upsert 与全文索引同步、缺失清理、FTS5 优先 + LIKE 回退检索、领域分布与年轮聚合）、`kb/service.rs`（递归只读元数据扫描、离线保留并标记不可用、扫描全部、地形总览）。
- 命令层：新增 17 条命令（采集设置/能力开关/暂停/脱敏/去重/采集一轮/事件列表/摘要/删除/审计，来源列表/登记/移除/扫描/文档/检索/概览）；`capture_collect` 由外壳注入的采集源驱动，内核侧保留 `NoopCaptureSource` 供纯逻辑测试。
- 测试：`cargo test -p thought-forge-core` 全绿（含库内 3 项脱敏单测）；`tests/capture.rs` 16 项、`tests/kb.rs` 13 项，含四条 `proptest` 属性：采集幂等（`property_repeated_ingest_is_idempotent`）、暂停不写入（`property_paused_never_writes`）、脱敏命中不落库（`property_redaction_removes_digits`）、来源离线保留可读（`property_offline_source_retains_documents`）。
- 界面：藏境界新增「大师架子 / 知识地形」视图切换，知识地形展示来源登记与扫描、主题分布（面积映射文档量、颜色映射领域）、年轮（圈层映射累计量、内含月新增）、主题检索；我境界新增采集台（全局暂停、四类能力逐项开关与系统不可用态、脱敏开关、去重窗口、采集一轮、原始记录按类型筛选与删除、开启审计）。
- 前端：`pnpm typecheck` 无错误，`pnpm test` 79 项全绿（13 个文件），`pnpm build` 通过；预览 stub 补齐 17 条 P7 命令与可变 demo 状态（采集设置/事件/来源/文档）。

补充记录（2026-09-16）：OS 级抓取已在外壳落地。`capture.rs` 实现 `CaptureSource`，剪贴板 800 毫秒、前台窗口 2 秒判定最小采样间隔，受关注目录由 `notify` 常驻递归监听并在每轮采集排空有界队列；`capture_win.rs` 用 `windows-sys` 读 `CF_UNICODETEXT`、`CF_DIB` 引用与 `GetForegroundWindow` 的窗口标题和进程名。能力可用性经 `ShellCapture::unavailable` 上报，非 Windows 平台剪贴板与前台窗口标记为不可用，界面禁用对应开关。Windows 目标 `cargo check` 已通过，真机效果待 Windows 环境验收。

补充记录（2026-09-16）：关注目录此前只有读取方没有写入方（外壳启动时读设置键 `capture.watch_roots`，但没有任何界面能写入），文件活动是个走不通的开关。现已补上：命令 `capture_set_watch_roots` 校验后重建监听并落盘，采集台新增「关注目录」区块负责增删，立即生效无需重启；内核的校验规则只接受已存在的绝对路径、上限 16 个、嵌套目录只留外层，启动时改用宽松版本丢弃坏项而不阻断启动。

补充记录（2026-09-16）：主动搜集已在外壳落地（`discovery.rs`）。`ShellDiscovery` 复用检索连接器，把搜索结果经脱敏与注入特征安检后转成待确认材料，并写入用途为 `distill_discovery` 的调用审计与当日成本；联网关闭或检索连接器未启用时返回 `E_NETWORK_OFF` 并说明原因。内核侧的 `BlockedDiscovery` 保留为接口占位，供纯逻辑测试与后续接入方参考。6 条外壳单测覆盖映射、无地址过滤、条数上限、成功与失败两条审计路径、离线报错。

## P8 自我蒸馏与发布

目标：自我蒸馏可用，安装包可安装可升级。

- [x] 8.1 实现自我蒸馏：基于历史记录生成用户本人大师包初稿
- [x] 8.2 实现铜镜入口与逐条确认流程
- [x] 8.3 实现用户本人大师作为可选席位参与会诊
- [x] 8.4 实现数据导出与清除
- [x] 8.5 实现离线态与权限态界面表现
- [x] 8.6 实现降低动态效果与高对比模式
- [x] 8.7 实现画布的列表与大纲等效视图
- [x] 8.8 实现命令面板与自然语言指令
- [x] 8.9 完成 NSIS 与 WiX 安装包配置与冒烟测试
- [x] 8.10 完成自动更新链路与 GitHub Actions 发布流程
- [x] 8.11 运行全量门禁并完成交付检查

门禁 P8：内核与界面全量门禁通过；无障碍与等效视图验收通过；NSIS 与 WiX 安装包、自动更新与 GitHub Actions 发布链路已完成配置。安装包实机安装与自动升级需完整 Windows 桌面工具链验收。

验证记录（2026-09-14）：

- 迁移：`0008_publish.sql` 建 `self_drafts`（初始与已安装草稿、版本、模型调用数）、`self_items`（逐条初稿与采纳/剔除决策）、`data_events`（导出与清除留痕）；`db/migrations.rs` 注册 version 8，`latest_version()`=8。
- 内核·自我蒸馏：`self/`（模块注册为 `self_distill`，`self` 为 Rust 关键字）实现基于本机 `thought_records` 的自我大师包蒸馏：记录数 `<20` 不调用模型，就绪后逐条产出待确认初稿，逐条采纳/剔除，安装写 `master_id="self"` 的完整快照并递增版本；`reserve_self_seat` 在存在自我大师时把选角规模加一，使「你」作为第七保留席位参与每次会诊；席位可独立开关（设置 key `self.seat_enabled`）。
- 内核·数据主权：`data/` 提供 `data_scope` 表级范围与行数、`data_export` 只读 JSON 导出（blob 以 `<blob:N bytes>` 占位）、`data_purge` 显式确认后按表清除并写 `data_events`。
- 命令层：新增 10 条命令（`self_readiness`/`self_draft`/`self_start`/`self_decide`/`self_install`/`self_set_seat`，`data_scope`/`data_export`/`data_purge`/`data_events`），命令层用 `self_service` 与 `data_service` 别名。
- 测试：`cargo test -p thought-forge-core` 全绿；`tests/self.rs` 10 项、`tests/data.rs` 5 项，含属性测试 `property_install_requires_adopted_item`（存在未决策初稿时安装被拒）。
- 界面·我境界：新增「铜镜 · 自我蒸馏」面板（解锁进度 `role="meter"`、启动铜镜、初稿逐条采纳/剔除、安装后席位开关）与「数据主权」面板（范围表、导出 JSON、清除二次确认与留痕）；采集台补权限态（炉口闭合提示）。
- 界面·离线态与权限态：`FurnaceTemp` 新增 `offline` prop，离线时显示虚线环与 `role="status"` 提示「离线运行 · 已装大师、记录与图谱仍可读可搜」；`App` 用 `networking_get` 驱动 `offline`。
- 界面·无障碍：新建 `src/app/preferences.ts`（`Preferences`/`usePreferences`/`applyPreferences`，`localStorage` 持久化 key `thought-forge.preferences`），以 `data-motion="reduced"`、`data-contrast="high"` 驱动主题；`tokens.css` 双主题适配降动效与高对比；我境界外观面板加两个开关。
- 界面·等效视图：`ObserveRealm` 加图形/列表/大纲三视图切换（列表 `role="list"`、大纲 `role="tree"`，用 `LayerGlyph` 保持层次可辨）。
- 界面·命令面板：新建 `src/components/CommandPalette.tsx`，`Ctrl/Cmd+K` 唤出，自然语言问句命中关键词即转会诊议题，`/` 开头按 id 前缀匹配结构化指令；`role="dialog"`/`combobox`/`option`，纯前端实现，无模型亦可用。
- 前端：`pnpm typecheck` 无错误，`pnpm test` 92 项全绿（14 个文件），`pnpm build` 通过（358.32 kB / gzip 111.15 kB）；预览 stub 补齐 P8 类型、demo 状态与离线态默认。`vitest.config.ts` 把 `testTimeout` 从默认 5000ms 提到 15000ms：本环境 CPU 与内存紧张，jsdom 冷启动会让首个用例偶发逼近 10s，属误报超时。
- 发布配置：`src-tauri/Cargo.toml` 加 `tauri-plugin-updater`/`tauri-plugin-process`，`lib.rs` 注册两插件；`capabilities/default.json` 加 `updater:default`/`process:allow-restart`；`tauri.conf.json` targets 加 `wix`、`createUpdaterArtifacts: true`、`plugins.updater`（endpoints + pubkey + `installMode: passive`）、wix `language: zh-CN`；新建 `.github/workflows/release-thought-forge.yml`（Windows tauri-action 发布，`projectPath: thought-forge`）。

遗留说明：安装 WebView/GLib 系统库后桌面壳已可编译；`tauri.conf.json` 的 `pubkey` 仍为占位符、`updater.endpoints` 指向尚未部署的域名，安装包实机安装、自动升级与签名链路需在完整 Windows 桌面工具链下验证。发布工作流未启用 `cargo fmt --check`：core crate 未采用 rustfmt 约定，避免引入全量空格级改动。

## P9 资产统计

目标：需求 2 的 Skill 与 AI 资产统计落地，藏境界补上资产图谱视图。

- [x] 9.1 新增 `0009_assets.sql`（`asset_roots`/`skills`/`skill_dependencies`/`asset_scans`）
- [x] 9.2 实现 `asset/` 内核模块：根目录登记、Skill 目录扫描、清单解析与仓储
- [x] 9.3 实现分类统计、启用比例与近 30 天变动汇总
- [x] 9.4 清单缺失或格式无效时保留记录并标记 `needs_repair`
- [x] 9.5 根目录离线时保留记录并标记不可读
- [x] 9.6 新增 7 条命令并注册到桌面壳
- [x] 9.7 前端补类型、预览状态与命令 stub
- [x] 9.8 藏境界新增资产图谱视图（汇总、分类分布、AI 能力、马赛克网格、待修复陶片、Skill 详情）
- [x] 9.9 补 `tests/assets.rs` 与 `VaultRealm.test.tsx` 用例
- [x] 9.10 运行全量门禁

门禁 P9：内核与界面全量门禁通过；需求 2 的六条验收（扫描识别 Skill 元数据、识别 AI 平台与接入状态、分类维度统计、扫描更新计数、单个 Skill 详情、清单缺失标记待修复）全部落地。

验证记录（2026-09-15）：

- 迁移：`0009_assets.sql` 建 `asset_roots`（登记路径与最近扫描时间）、`skills`（索引元数据、`needs_repair`/`repair_reason`、`missing`、`first_seen_at`/`last_seen_at`，唯一索引 `(root_id, path)`）、`skill_dependencies`（每次 upsert 整体重建）、`asset_scans`（扫描留痕，供近 30 天变动汇总）；`db/migrations.rs` 注册 version 9，`latest_version()`=9；`data/mod.rs` 的 `DATA_TABLES` 收入四张新表（子表先于父表）。
- 内核·清单解析：`asset/mod.rs` 优先读 `manifest.json`，其次读 `SKILL.md` 的 YAML 头；YAML 头用内联扁平子集解析（`key: value`、内联 `[a, b]` 与缩进短横线列表），不引入 `serde_yaml`。清单缺失或无效时保留记录、以目录名兜底并写 `manifest_missing`/`manifest_invalid`/`frontmatter_missing`/`name_missing`。
- 内核·扫描：`asset/service.rs` 与 `asset/repo.rs` 实现一层子目录各算一个 Skill、根目录自带清单且无子目录时根目录当单 Skill；`scan_root` 单事务写入并重建依赖，返回 `AssetScanOutcome`；根目录离线返回 `root_unavailable`，只标记 `missing` 不删记录；重扫幂等，目录消失才移除。
- 内核·统计：`summary` 汇总根目录数、可用根目录数、Skill 总数、启用/停用数、待修复数、近 30 天新增与移除、分类分布（空分类归「未归类」）与已接入平台状态。
- 命令层：新增 7 条命令（`asset_roots`/`asset_add_root`/`asset_remove_root`/`asset_scan`/`asset_skills`/`asset_skill_detail`/`asset_summary`），命令层用 `asset_service` 别名。
- 测试：`cargo test -p thought-forge-core --test assets` 17 项全绿（含三条属性测试）；`VaultRealm.test.tsx` 6 项全绿（新增资产图谱 3 项）。
- 界面·藏境界：视图开关从两档扩为三档（大师架子 / 资产图谱 / 知识地形）；资产图谱含顶部汇总（总数、覆盖领域、启用比例、近 30 天变动）、根目录登记与扫描/移除、分类分布、AI 能力、Skill 马赛克网格与详情面板；待修复 Skill 以虚线边框与斜纹底纹呈现为裂纹陶片。

遗留说明：UI 规范中资产马赛克的「面积映射使用频次」暂缓——当前数据模型不记录 Skill 使用次数，改为等面积网格、颜色映射分类；待使用频次进入采集口径后再补面积映射。桌面壳已可编译（补装 WebKit/GLib 系统库后 `cargo check`/`cargo build -p thought-forge-desktop` 通过），`cargo test -p thought-forge-desktop --lib` 覆盖命令层协议与错误码；命令层与前端契约由 `protocol.rs` 的 `include_str!` 用例双向锁定。

## 贯穿性任务

- [x] X.1 每个阶段结束时更新 `.monkeycode/docs/` 下对应文档
- [x] X.2 每个阶段结束后记录阶段复盘的偏差点与后续动作
- [x] X.3 每引入一位大师，同步更新覆盖矩阵与候选池统计（2026-09-21：名册满 20 位，六题各有 20 位有料，实际统计记入 `roster-plan.md` 第六节）
- [x] X.4 每次模型能力或平台变更，同步更新联网能力清单与调用审计口径（本轮只有内容补料与界面入口，无平台或模型能力变更，无需改口径）

## 阶段复盘

P1 至 P8 已完成，X.1 文档同步与 X.2 复盘记录已补齐。以下为跨阶段偏差点与后续动作汇总；X.3、X.4 属持续任务，随大师引入与平台变更触发。

偏差点：

- 桌面外壳依赖 WebView/GLib 系统库，Linux 开发环境默认缺失。应对：把纯逻辑拆为 `thought-forge-core` 独立 crate，各阶段门禁跑 `cargo test -p thought-forge-core`；补装 `libwebkit2gtk-4.1-dev` 等系统库后，外壳用 `cargo check`/`cargo build -p thought-forge-desktop` 验证。
- 联网与系统感知能力无法在内核直接落地：会诊联网、主动搜集、OS 级采集都需外壳注入。应对：内核以 trait 与占位实现保持纯逻辑可测（`GatedClient` 返 `E_NETWORK_OFF`、`BlockedDiscovery` 返 `disabled`、`NoopCaptureSource`），真实实现留给外壳。该约定已按计划兑现：联网经 `model.rs`/`connector.rs`，主动搜集经 `discovery.rs`，系统感知经 `capture.rs`。
- 自我蒸馏的材料来源在设计时偏外部语料，实现时收敛为本机 `thought_records`：自我大师应当反映用户自身判断。应对：`self/` 只读本机记录，`<20` 条不调用模型，产出逐条确认后才安装。
- `self` 是 Rust 关键字，模块无法直接命名。应对：模块注册为 `self_distill`，命令层用 `self_service`/`data_service` 别名。
- 发布链路的部分参数只能占位：`tauri.conf.json` 的 `pubkey` 与 `updater.endpoints` 需真实签名密钥与托管域名。应对：先完成配置与工作流骨架，留下显式占位符；后续补了发布前检查（`scripts/check-release-config.mjs`，可用 `pnpm check:release-config` 本地预跑），发布工作流在编译之前执行，占位配置不再静默产出无法验证更新的安装包。该检查只判静态配置值，域名是否真的提供更新清单仍需真机确认。
- core crate 未采用 rustfmt 约定，`cargo fmt -- --check` 会报大量既有漂移。应对：发布工作流有意不含 fmt 门禁，避免引入全量空格级改动。
- 本环境 CPU（2 核）与内存紧张，jsdom 冷启动会让单个用例偶发逼近 10s，命中 vitest 默认 5000ms 上限。应对：`vitest.config.ts` 将 `testTimeout` 提到 15000ms，消除负载抖动导致的误报。
- 需求 2「Skill 与 AI 资产统计」未进入任何阶段实施清单，P2 的 2.10 只完成了大师架子与覆盖矩阵，资产图谱部分缺失：设计中的 `AssetService`（`scan_assets`/`list_skills`/`get_skill_detail`/`get_asset_summary`）、`skills` 表、Skill 目录扫描与 `manifest.json`/`SKILL.md` 解析均未落地，`ai_platforms` 只覆盖平台识别。应对：登记为独立阶段 P9 并已补齐；`SKILL.md` 的 YAML 头改用内联扁平子集解析，未引入 `serde_yaml`。

后续动作：

- 在完整 Windows 桌面工具链下安装真实安装包，验证 NSIS 与 WiX 产物可安装，并跑通自动更新升级链路。
- 替换 `tauri.conf.json` 的 `pubkey` 占位符与 `updater.endpoints`，配置真实签名密钥与发布来源。
- 在外壳内补齐端到端联调。模型平台已接入（`model.rs` 的 `ReqwestTransport` + `model_probe` 自检命令）；连接器已接入真实实现（`connector.rs`：SearXNG 兼容检索、HTML 正文提取、MCP 工具客户端）；OS 采集已接入（`capture.rs` 的剪贴板与前台窗口轮询、`notify` 递归文件监听，`capture_win.rs` 的 Windows 原生探针）。剩余工作是在 Windows 真机上验证采集与检索的实际效果。
- 引入第一位真实大师时更新覆盖矩阵与候选池统计（X.3）；模型能力或平台变更时同步联网能力清单与调用审计口径（X.4）。

## 参考资料

- 需求文档：`当前工作区/.monkeycode/specs/thought-forge-workbench/requirements.md`
- 技术设计：`当前工作区/.monkeycode/specs/thought-forge-workbench/design.md`
- UI 规范：`当前工作区/.monkeycode/specs/thought-forge-workbench/ui-design.md`
