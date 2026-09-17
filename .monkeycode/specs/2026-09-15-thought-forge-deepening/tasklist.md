# 思想熔炉深化 · 任务清单

范围：P10 会诊与网络深化、P11 追问与逐席发言、P12 连接器与联网检索、P13 检索安全与分歧判定、P14 成本与凭据与备份、P15 自我约束与运行可恢复、P16 模型链路与连接器真机验证。P10 至 P15 可在当前环境实施并验证；P16 只交付方案与接口定义。

## P10 会诊与网络深化

目标：会诊深度随分歧自适应，分歧曲线可回看，调节常数可调，激活传得更远，认知地图看到成团结构。

- [x] 10.1 新增 `0010_tuning.sql`：`council_round_metrics`、`thought_clusters`、`thought_nodes.cluster_id`
- [x] 10.2 注册迁移 version 10，`latest_version()` 更新为 10，`DATA_TABLES` 收入两张新表并保持子表先于父表
- [x] 10.3 实现 `council/tuning.rs`：十一项调参项定义、取值范围校验、整体原子写入与默认值回退
- [x] 10.4 实现轮次指标计算与 `council_round_metrics` 读写，`SessionDetail` 返回逐轮指标
- [x] 10.5 会诊编排支持按分歧度与上限自适应追加质询轮，交叉质询提示词携带累计历史并按上限截断
- [x] 10.6 `CouncilOutcome` 增加讨论轮次数与逐轮指标
- [x] 10.7 激活传播扩展为多跳：按权重与折半系数折算第一跳、按跳间衰减折算第二跳，结果报告各跳计数
- [x] 10.8 实现 `network/cluster.rs`：确定性标签传播、领域与层次标签生成、最小团规模过滤
- [x] 10.9 固化调度在聚类开关开启时执行聚类并写入 `thought_clusters` 与节点 `cluster_id`，历史社区保留
- [x] 10.10 `GraphView` 返回社区列表，`GraphNode` 带所属社区标识，`GraphFilter` 支持按团过滤
- [x] 10.11 新增 `tuning_get` 与 `tuning_set` 两条命令，`network_activate` 返回各跳计数
- [x] 10.12 前端补类型、demo 状态与命令 stub
- [x] 10.13 会诊境界实现分歧曲线（折线加阈值参考线、单轮提示）与表格等效视图
- [x] 10.14 观境界实现社区呈现（团区域与标签）与社区列表等效视图、按团过滤
- [x] 10.15 我境界实现调参面板（分组、当前值、默认值、范围与越界提示）
- [x] 10.16 补 `tests/tuning.rs`、`tests/cluster.rs` 并扩展 `tests/council.rs`、`tests/network.rs`、`CouncilRealm.test.tsx`、`ObserveRealm.test.tsx`
- [x] 10.17 运行全量门禁

门禁 P10：`cargo test -p thought-forge-core` 全部二进制通过；`pnpm typecheck`、`pnpm test`、`pnpm build` 通过；轮次上限、追加单调、参数边界与原子性、传播与聚类可复现五条属性测试通过。

## P11 追问与逐席发言

目标：会诊中随时能看到每位大师自己的发言，会诊结束后有一页可读的完整分析，并能围绕某段判断继续追问。

- [x] 11.1 新增 `0011_followup.sql`：`council_sessions` 增加追问锚点字段与 `parent_session_id` 索引
- [x] 11.2 注册迁移 version 11，`latest_version()` 更新为 11
- [x] 11.3 实现 `seat_speech`：按席位与轮次归组发言，合成席位状态
- [x] 11.4 实现失败席位单轮重试，不重跑整场
- [x] 11.5 实现 `council/followup.rs`：锚点校验、文字截断与追问会话创建，支持阵容继承
- [x] 11.6 实现 `followup_prompt` 与 `council_followup` 调用用途
- [x] 11.7 追问收敛后建立原结论节点到追问结论节点的衍生连线
- [x] 11.8 新增 `council_turns`、`council_retry_seat`、`council_followup`、`council_conclusion` 四条命令并注册到桌面壳
- [x] 11.9 前端补类型、demo 状态与命令 stub
- [x] 11.10 会诊境界实现逐席发言展开、轮次标注、按轮次分组的发言记录与表格等效视图
- [x] 11.11 会诊境界实现锚点选择与追问入口、追问会话标记与返回母会话
- [x] 11.12 补 `tests/followup.rs` 并扩展 `tests/council.rs`、`CouncilRealm.test.tsx`
- [x] 11.13 实现 `conclusion_view` 与 `council_conclusion` 命令：结论要点、分歧与未决、收敛过程、逐席依据、外部来源、演化链六段组装
- [x] 11.14 会诊结论详情页：六段结构、事实与判断的分段标记、单轮会诊的「轮次不足」提示
- [x] 11.15 运行全量门禁

门禁 P11：`cargo test -p thought-forge-core` 全部二进制通过；`pnpm typecheck`、`pnpm test`、`pnpm build` 通过；锚点必填、锚点类型受限、母会话不变、阵容继承、追问衍生连线五条属性测试通过，且结论详情页六段内容与库中记录逐项对应。

## P12 连接器与联网检索

目标：大师作答前能取到外部背景，且六席不因共享同一份材料而趋同。

- [x] 12.1 新增 `0012_connectors.sql`：`connectors`、`connector_calls`、`council_sources`
- [x] 12.2 注册迁移 version 12，`latest_version()` 更新为 12，`DATA_TABLES` 收入三张新表并保持子表先于父表
- [x] 12.3 实现 `connector/mod.rs`：`SearchProvider`、`PageReader`、`ToolProvider` 三个抽象与 DTO，配置指向工具型服务器之外的 MCP 服务时拒绝
- [x] 12.4 实现 `connector/repo.rs`：连接器配置读写、调用审计、检索快照读写
- [x] 12.5 实现 `connector/service.rs`：共享背景检索、席位补充检索、结果数截断、正文快照开关、失败降级
- [x] 12.6 调参项扩展为十七项，纳入连接器结果数上限、每次会诊检索次数上限、正文快照开关与超时
- [x] 12.7 会诊编排接入共享背景与席位补充检索，提示词标注获取时刻与来源编号并要求区分事实与判断
- [x] 12.8 快照在会诊启动时冻结，后续轮次复用，回看历史会诊不发起新检索
- [x] 12.9 新增 7 条命令（`connector_list`/`connector_upsert`/`connector_enable`/`connector_test`/`connector_calls`/`council_sources`/`council_search`）并注册到桌面壳
- [x] 12.10 外壳接入位就绪：`ShellConnector` 提供 `Retrieval` 注入点；当时仅装配占位实现，后由 P16.6 替换为真实连接器（见遗留说明）
- [x] 12.11 前端补类型、demo 状态与命令 stub
- [x] 12.12 会诊境界呈现共享背景、逐席来源标注与「本次未获得外部背景」提示
- [x] 12.13 我境界连接器面板：分类开关、地址配置、连通测试与调用审计
- [x] 12.14 补 `tests/connector.rs` 并扩展 `tests/council.rs`、`CouncilRealm.test.tsx`
- [x] 12.15 运行全量门禁

门禁 P12：`cargo test -p thought-forge-core` 全部二进制通过；`pnpm typecheck`、`pnpm test`、`pnpm build` 通过；连接器默认关闭、检索隔离、共享背景一致、快照冻结、上限约束与检索失败降级六条属性测试通过。

## P13 检索安全与分歧判定

目标：对外发送的问句可控可核对，外部网页里的指令不能反过来指挥大师，分歧判定不再被同词反义骗过。

- [x] 13.1 新增 `0013_search_safety.sql`：`connector_calls` 补 `query_original`/`query_sent`/`redacted`，`council_sources` 补 `flagged`，`council_round_metrics` 补 `method`/`fell_back`，`council_turns` 与 `llm_calls` 补 `prompt_version`
- [x] 13.2 注册迁移 version 13，`latest_version()` 更新为 13
- [x] 13.3 实现 `connector/guard.rs`：复用 `RedactionRules` 的发送前脱敏、关键词与问句两种模式、`PreparedQuery` 生成
- [x] 13.4 连接器服务接入脱敏与预演：`council_search` 与 `connector_test` 支持两阶段确认，首次返回待确认内容与指纹且不发起请求，带同一指纹再次调用才真正检索，指纹不符返回 `E_INVALID_INPUT`
- [x] 13.5 调用审计写入原始问句、实际发送串与是否脱敏
- [x] 13.6 实现 `sanitize_external`：注入特征识别与边界包裹，命中时置 `flagged` 且不删除原文
- [x] 13.7 `sources_block` 加强为不可信资料声明加起止标记，并保留来源编号、发布时间与获取时刻
- [x] 13.8 快照落库记录 `flagged`，来源清单界面对命中项显示警示标记
- [x] 13.9 实现 `council/divergence.rs`：`DivergenceMode` 三种取值、候选对筛选、`PolarityJudge` 抽象与混合判定
- [x] 13.10 轮次指标记录 `method` 与 `fell_back`，极性不可用时回退词面判定并在界面标注
- [x] 13.11 定义 `PROMPT_VERSION` 并把版本号写入轮次记录与调用审计，结论页展示该版本
- [x] 13.12 调参项扩展为二十二项，纳入发送模式、预演开关、判定方式、重合度下限与对数上限
- [x] 13.13 前端补类型、demo 状态与命令 stub
- [x] 13.14 会诊境界补预演确认面板与本轮判定方式标注，结论页补提示词版本
- [x] 13.15 补 `tests/retrieval_guard.rs`、`tests/divergence.rs` 并扩展 `tests/connector.rs`、`tests/council.rs`、`CouncilRealm.test.tsx`
- [x] 13.16 运行全量门禁

门禁 P13：`cargo test -p thought-forge-core` 全部二进制通过；`pnpm typecheck`、`pnpm test`、`pnpm build` 通过；脱敏可核对、关键词不外发、预演不落调用、注入标注、极性优先、提示词可追溯六条属性测试通过。

## P14 成本与凭据与备份

目标：花钱之前先看到估算，密钥不用再手配环境变量，数据可备份可恢复，半年后回看能复现全部依据。

- [x] 14.1 新增 `0014_governance.sql`：平台单价列、调用费用列、`cost_days`、`credential_refs`、`backups`、节点撤销列与会话运行控制列
- [x] 14.2 注册迁移 version 14，`latest_version()` 更新为 14，`DATA_TABLES` 收入三张新表
- [x] 14.3 实现 `cost.rs`：估算公式、整数微元记账、日与月汇总、三种超限策略与降级范围计算；调参项由二十二项扩展为二十八项，纳入日与月上限、超限策略、备份保留份数、迁移前备份开关与快照保留天数
- [x] 14.4 会诊发起前接入配额闸门，被压缩的轮次或席位数写入会话记录
- [x] 14.5 平台配置扩展输入与输出单价与币种，未配置单价时费用按零计并标注
- [x] 14.6 实现 `credential.rs`：`CredentialStore` 抽象、引用名生成与唯一约束、状态查询
- [x] 14.7 外壳实现操作系统凭据库读写，环境变量保留为回退路径
- [x] 14.8 实现 `backup.rs`：`VACUUM INTO` 创建、完整性校验、保留份数裁剪、恢复准备
- [x] 14.9 迁移执行器在版本提升前自动创建 `pre_migration` 备份，失败则中止迁移
- [x] 14.10 正文快照按保留天数清理，元数据行保留
- [x] 14.11 新增 7 条命令（`cost_summary`/`cost_estimate`/`backup_create`/`backup_list`/`backup_restore`/`credential_set`/`credential_status`）并注册到桌面壳
- [x] 14.12 前端补类型、demo 状态与命令 stub
- [x] 14.13 我境界实现成本面板（估算、日与月累计、上限与策略）、凭据设置界面与备份管理界面
- [x] 14.14 结论页呈现本次调用次数与费用估算
- [x] 14.15 补 `tests/cost.rs`、`tests/credential.rs`、`tests/backup.rs` 并扩展 `tests/council.rs`、`SelfRealm.test.tsx`
- [x] 14.16 运行全量门禁

门禁 P14：`cargo test -p thought-forge-core` 全部二进制通过；`pnpm typecheck`、`pnpm test`、`pnpm build` 通过；费用可加、配额生效、密钥不入库、恢复先校验、迁移前备份五条属性测试通过。

## P15 自我约束与运行可恢复

目标：会诊不会退化成自我确认，原则可以撤销，会诊中途关掉应用也不丢。

- [x] 15.1 实现 `council/echo.rs`：结论与既有原则的重合度比较、阈值判定与 `echo` 提示记录写入
- [x] 15.2 实现原则撤销：置 `status`/`revoked_reason`/`revoked_at`，建上下文时跳过已撤销节点，历史发言不变
- [x] 15.3 会话支持「这一场不带我」，`self_seat_included` 写入并参与选区逻辑
- [x] 15.4 实现 `council/control.rs`：取消请求、轮次边界检查、心跳更新与中断会话识别
- [x] 15.5 会诊编排在每轮开始与每次席位调用前检查取消，取消后保留已完成轮次并进入收敛裁决
- [x] 15.6 启动时返回可恢复会话，支持继续或放弃，继续时从最后一条成功轮次之后接着跑
- [x] 15.7 扩展调参项纳入回音阈值与中断心跳间隔
- [x] 15.8 新增 4 条命令（`council_cancel`/`council_recoverable`/`echo_check`/`principle_revoke`）并注册到桌面壳
- [x] 15.9 前端补类型、demo 状态与命令 stub
- [x] 15.10 会诊境界补取消按钮与取消后状态、启动恢复提示、回音提示与「这一场不带我」入口
- [x] 15.11 我境界补原则撤销确认与撤销留痕展示
- [x] 15.12 补 `tests/echo.rs`、`tests/control.rs` 并扩展 `tests/council.rs`、`tests/self.rs`、`CouncilRealm.test.tsx`
- [x] 15.13 运行全量门禁

门禁 P15：`cargo test -p thought-forge-core` 全部二进制通过；`pnpm typecheck`、`pnpm test`、`pnpm build` 通过；回音可见、撤销生效、取消保值、中断可识别四条属性测试通过。

## P16 模型链路与连接器真机验证

目标：给出一份可在 Windows 真机逐项判定模型链路与连接器是否可用的方案与清单。

- [x] 16.1 交付 Windows 真机端到端验证方案与验证清单（随本文档，见 `design.md` 的「外壳自检命令」、下表与可照做的执行手册 `windows-verification.md`）
- [x] 16.2 实现外壳自检命令 `model_probe` 与退出码约定（0 成功、1 模型不可用、2 联网关闭、3 平台未配置）
- [ ] 16.3 真机验证模型平台连通与密钥不落库
- [ ] 16.4 真机验证会诊全链路与调用审计
- [ ] 16.5 真机验证蒸馏全链路
- [ ] 16.6 真机验证搜索、网页阅读与 MCP 三类连接器及其审计
- [ ] 16.7 真机验证操作系统凭据库读写与凭据引用恢复
- [ ] 16.8 真机验证备份创建、恢复与迁移前自动备份
- [ ] 16.9 真机验证 NSIS 与 WiX 安装包安装、自动升级与签名
- [ ] 16.10 记录真机验证结论、偏差与后续动作

门禁 P16：验证清单逐项通过并留下结论；未通过项记录偏差、影响范围与后续动作。

### 真机验证清单

| 编号 | 验证项 | 前置条件 | 操作步骤 | 期望结果 | 失败判定 |
|---|---|---|---|---|---|
| V1 | 桌面壳可构建 | Windows、WebView2、Rust 工具链、Node 与 pnpm | 在 `src-tauri` 执行 `cargo build -p thought-forge-desktop` | 构建成功，无缺失系统库 | 任一编译或链接错误 |
| V2 | 平台配置可保存 | 一个 OpenAI-compatible 端点的地址与模型名 | 调用 `platform_upsert` 后调用 `platform_list` | 平台状态为 `ready`，端点与模型名与输入一致 | 状态仍为 `unconfigured` |
| V3 | 探针连通 | 已设置 `THOUGHT_FORGE_API_KEY`、联网能力开启、平台已启用 | 调用 `model_probe` | `ok` 为真，返回耗时与 `call_id`，退出码为 0 | 返回错误码或耗时异常 |
| V4 | 密钥不落库 | 同 V3 | 对数据库执行全表文本检索，查找密钥字符串 | 除 `settings` 中联网开关外无命中，密钥不出现在任何表 | 任一表中命中密钥 |
| V5 | 调用审计 | 同 V3 | 调用 `llm_calls` | 最新一条为本次探针，用途、平台、模型、耗时与状态齐全 | 缺失审计行或字段为空 |
| V6 | 会诊全链路 | 已装大师包、联网开启、平台启用 | 创建会话、选角后调用 `council_run` | 返回结论与逐轮指标，`council_turns` 记录各轮，`llm_calls` 逐次留痕 | 任一轮次缺失或整场失败 |
| V7 | 蒸馏全链路 | 一份可解析的语料与目标大师名 | 建入库任务并跑完六阶段 | 产出技能单元并安装为新版本，检查点可续跑 | 阶段中断且无法续跑 |
| V8 | 采集注入 | Windows 桌面环境 | 开启单项采集能力后复制文本或切换前台窗口 | 采集事件落库并写入审计，关闭后不再产生新事件 | 关闭后仍产生事件 |
| V9 | 安装包安装 | 已签名的 NSIS 与 WiX 产物 | 分别安装两个产物 | 安装成功，应用可启动 | 安装失败或启动失败 |
| V10 | 自动升级 | 已配置真实 `pubkey` 与 `updater.endpoints`，发布一个更高版本 | 在旧版本内触发更新 | 检测到新版本并完成升级，数据保留 | 未检测到更新或升级后数据丢失 |
| V11 | 搜索连接器连通 | 一个可用检索服务的地址与其密钥 | 配置搜索连接器后调用 `connector_test`，再调用 `council_search` | 返回带标题、网址、摘要与时间的结果，`connector_calls` 留有审计 | 无结果或未写审计 |
| V12 | 共享背景与检索隔离 | 搜索连接器启用、共享背景与席位检索开关开启 | 跑一次会诊，检查各席位提示词 | 共享背景对全部席位一致，席位补充检索只出现在自身提示词 | 某席位看到他人的补充检索结果 |
| V13 | MCP 工具接入 | 一个提供工具能力声明的 MCP 服务器 | 配置后调用 `connector_test` 与一次工具调用 | 工具清单可读，调用结果入快照与审计 | 拒绝提供工具能力的服务器仍被接受 |
| V14 | 检索发送可控 | 搜索连接器启用，`connector.query_mode` 为 `keyword` | 发起一次含手机号与新邮件的检索，核对 `connector_calls` | 实际发送串不含原文敏感片段，`redacted` 为真，原始问句留痕 | 实际发送串与原始问句相同 |
| V15 | 凭据库读写 | Windows 凭据管理器可用 | 在设置界面填入密钥后重启应用，再发起一次模型调用 | 密钥从凭据库读取，调用成功，数据库中无密钥本体 | 重启后需重新配置或库中出现密钥 |
| V16 | 备份与恢复 | 数据目录可写，已有一份备份 | 调用 `backup_create`，篡改备份文件后调用 `backup_restore`，再用完好备份恢复 | 篡改件被拒且现有数据不变，完好件恢复成功且数据一致 | 篡改件被接受或恢复后数据丢失 |
| V17 | 迁移前自动备份 | 存在可提升版本的数据库 | 触发一次迁移 | 迁移完成后 `backups` 中存在 `pre_migration` 记录 | 迁移执行但无备份记录 |
| V18 | 外部内容隔离 | 一个正文含指令性语句的可控网页 | 让该网页成为检索结果并跑一次会诊 | 提示词中该段被边界标记包裹，快照 `flagged` 为真，结论标注依据来源 | 外部指令被当成系统指示执行 |

## 遗留说明

- 真机验证附有一个只读检查器 `crates/core/examples/forge_verify.rs`：它读 `data::DATA_TABLES` 覆盖的文本列做密钥残留扫描，并按需核对模型审计、采集与关注目录、检索审计与脱敏、备份留痕与文件在位、外部来源标记。它以 `SQLITE_OPEN_READ_ONLY` 打开数据库，可与应用并行运行，用法与逐项步骤见 `windows-verification.md`。检查器自带 `cargo test -p thought-forge-core --examples` 覆盖：空库上所有项必须可解析且不误报未过、健康数据全部通过、逐项植入缺陷必被对应项抓住、密钥扫描命中且不回显；取证查询失败一律计入当前检查项，避免库结构漂移被读成 0 而假通过。
- 安装 `libwebkit2gtk-4.1-dev`、`libgtk-3-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`libxdo-dev` 与 `pkg-config` 后，桌面壳已可在本机 `cargo check`/`cargo build`；P16 中依赖 Windows 专有产物的项目（NSIS 与 MSI 安装包、自动升级、系统凭据库）仍需完整 Windows 工具链执行。
- `tauri.conf.json` 的 `pubkey` 与 `updater.endpoints` 仍为占位符，V10 在替换为真实签名密钥与托管域名前无法执行。发布工作流已加一道发布前检查（`scripts/check-release-config.mjs`，可用 `pnpm check:release-config` 本地预跑）：它在安装依赖与编译之前判定 `plugins.updater` 是否存在、`pubkey` 是否为空或占位、是否像 base64 公钥、`endpoints` 是否为 https 且不指向保留主机名，任一项不满足即以非零退出码中止；这样占位配置不会再静默产出无法验证更新的安装包。该检查只判静态配置值，域名是否真的在提供更新清单仍需 V10 真机确认。
- `bundle.targets` 使用 `["nsis", "msi"]`：Tauri v2 的 Windows WiX 产物对应 `msi` 目标，`wix` 不是合法取值。
- 聚类在固化时计算，图规模上限沿用 `MAX_GRAPH_LIMIT`；超过该上限的图只对可见子图划分社区。
- 连接器在内核只保留抽象，真实调用由桌面外壳注入（`src/connector.rs`）：搜索按 SearXNG 兼容的 JSON 接口（`GET {endpoint}/search?q=&format=json`，端点未带 `/search` 时自动补齐），网页阅读按 HTTP GET 加通用 HTML 正文提取，MCP 按 JSON-RPC 2.0 over Streamable HTTP（`initialize` → `notifications/initialized` → `tools/list`、`tools/call`，自动记录 `Mcp-Session-Id`）。解析、正文提取与 JSON-RPC 负载抽取都是纯函数，离线单测覆盖；网络层失败统一转成 `E_NETWORK_OFF`（对端返回失败）或 `E_MALFORMED_RESPONSE`（结构不符），由编排降级为「本次未获得外部背景」。`HttpToolProvider` 仍受内核 `validate_tools` 约束：未声明任何工具的服务器在写入配置阶段即被拒绝。连接器密钥按 `thought-forge/connector/{id}` 取系统凭据库，环境变量 `THOUGHT_FORGE_CONNECTOR_KEY` 为回退，密钥不入库。
- `cargo clippy -p thought-forge-core --all-targets -- -D warnings` 与 `cargo clippy -p thought-forge-desktop --all-targets -- -D warnings` 已归零，并作为发布工作流的静态检查门禁；两个 crate 的 `rust-version` 统一声明为 `1.85`（依赖树含 edition 2024 crate，且 core 用到 1.82 才稳定的 `Option::is_none_or`），原先的 `1.77.2` 为虚假下限。

## 参考资料

- 需求文档：`当前工作区/.monkeycode/specs/2026-09-15-thought-forge-deepening/requirements.md`
- 技术设计：`当前工作区/.monkeycode/specs/2026-09-15-thought-forge-deepening/design.md`
- 既有规格：`当前工作区/.monkeycode/specs/thought-forge-workbench/`
