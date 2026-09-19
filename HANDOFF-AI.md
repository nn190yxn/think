# 交接单：给接手的执行者

写给接下来接手这份工作的 AI。作者：Monkeycode（2026-09-19）。人看的移交清单在 `HANDOVER.md`，
本文件只讲"你要做什么、按什么顺序、有哪些坑、什么算做完"。

---

## 零、一句话任务

在 `E:\程序开发\think`（Thought Forge，本地优先的认知骑士团工作台）里，先装好 Rust 工具链，
把内核基线跑绿，然后按任务清单推进 P20 至 P23 与设置页剩余两步；内核以外的人做项不要碰。

成功判据只有一条：**每条任务对应的门禁命令通过**。代码写完不算完成。

---

## 一、硬规矩（违反即返工）

1. **不许在没跑门禁的情况下把任务标成完成**，也不许在提交信息里写"已验证"而实际没跑。
2. **不许 force push、不许改写历史提交、不许动 `main` 的既有提交。**
3. **不许把 `.ohmyagent/` 提交**（那是本地代理配置，作者特意留作未跟踪）。
4. **只改任务清单点名的那部分**，不顺带"优化"相邻代码、不重排格式、不改无关文案。
5. **内核改动必须编译 + 跑测试**才算完成；没有工具链时就先装（第一节第二步），不许盲写后直接标完成。
6. **不确定就说不确定**，不许用听起来合理的结论填空；跑不了就说跑不了。
7. **拿不准的大改动先问 Alex**（重写、删功能、改结构、改口径）。
8. 每个任务**单独提交**，提交信息写清"改了什么、验证到什么程度、哪些没验"。

---

## 二、环境现状（事实，2026-09-19 实测）

| 项 | 状态 |
|---|---|
| 工作目录 | `E:\程序开发\think`，git 仓库，分支 `main` |
| Node / pnpm | **已装**。`pnpm gate:front` 实测全绿：类型检查 + 139 个前端用例 + 打包 |
| Rust / cargo | **未装**（`cargo`、`rustup` 都不在 PATH；`src-tauri\target` 目录不存在） |
| GitHub | **不通**（`github.com` 连接被重置；Gitee 正常）。本地领先远端 3 个提交未推 |
| 内核源码换行 | 工作区是 **CRLF**（索引是 LF）。编辑时用**单行锚点**，多行锚点会因换行符不一致匹配失败 |
| 已提交未推送 | `9104091`（设置页前三步 + 门禁脚本）、`41bd41c`（规格与审计 + 人工实测路线图）、`8e68f19`（内核选角改动，未编译） |

---

## 三、第一步：装工具链，把基线跑绿

按顺序做，做完**先停下来汇报**，不要顺手往下写业务代码。

### 3.1 MSVC 生成工具（必须先装，Rust 的 Windows 目标依赖它）

```
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive"
```

没有 winget 就去微软官网下 `vs_BuildTools.exe`，安装时勾**「使用 C++ 的桌面开发」**。
如果 Alex 已经下好安装包，直接双击装，同样勾这个工作负载。

### 3.2 Rust 工具链（要求 ≥ 1.85，目标 `x86_64-pc-windows-msvc`）

依赖树里有 edition 2024 的 crate，`src-tauri/Cargo.toml:6` 明确写了 `rust-version = "1.85"`，低于这个版本编译必失败。

```
winget install --id Rustlang.Rustup -e
```

国内网络下用清华镜像更快（装前设这两个环境变量即可，rustup 会自动写进配置）：

```
set RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup
set RUSTUP_UPDATE_ROOT=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup
```

安装时主机三元组选 `x86_64-pc-windows-msvc`（默认项，直接回车）。安装后**重开一个终端**让 PATH 生效。

### 3.3 判据：工具链就绪

```
cargo -V          :: 需要 ≥ 1.85
rustc -V
rustup show       :: host 必须是 x86_64-pc-windows-msvc
```

三条都出来才算装好。装不上就报错原文，不要绕。

### 3.4 建议：把仓库目录加进 Defender 排除路径

`HANDOVER.md` 第 128 行附近有说明：实时扫描会让 Rust 构建慢好几倍。排除 `E:\程序开发\think` 与 `E:\程序开发\think\src-tauri\target`。

### 3.5 首次编译基线（预计 30–60 分钟）

```
pnpm gate:core     :: cd src-tauri && cargo test -p thought-forge-core && cargo test -p thought-forge-core --examples && cargo clippy -p thought-forge-core --all-targets -- -D warnings
pnpm gate:shell    :: cd src-tauri && cargo test -p thought-forge-desktop --lib && cargo clippy -p thought-forge-desktop --all-targets -- -D warnings
```

**这两条必须全绿才算工具链就绪。** 这是本仓库第一次在本机编译 Rust 侧，慢是正常的，不是卡死。

若报错，先判断属于哪一类：
- 环境类（找不到链接器、找不到 Windows SDK）→ 回 3.1 补装。
- 代码类（编译错误指向 `council/` 下文件）→ 见第四节的"未验证改动"，那是上一轮盲写的三处，按语义修正，不要删功能。

### 3.6 更省事的另一条路：先走云端验证

只想先确认"上一轮那三处内核改动能不能编译、用例过不过"，**不必先在本机装工具链**：
在 GitHub 仓库页 → Actions → 选 `verify-thought-forge-windows.yml` → Run workflow。
它在 `windows-latest` 上编译内核、跑 `cargo test -p thought-forge-core` 与只读检查器用例，几分钟出结果。

两条路的区别：云端只能告诉你改对没改对，**改不动代码**；要反复瞎改改，还是得有本地工具链。
建议顺序：先跑云端确认基线，再装本地工具链准备干后面的活。

---

## 四、第二步：验证上一轮留下的三处内核改动（**未编译，重点看这里**）

上一轮（提交 `8e68f19`）改了三个文件，目标是"让选角用上六题积累的深浅"。**这三处从未编译过**，
所以你要把它们当"待验证"看，而不是当"已完成"看。改动语义如下：

| 位置 | 改了什么 | 语义 |
|---|---|---|
| `src-tauri/crates/core/src/council/mod.rs`（约 74–92 行，`Candidate` 结构体） | 新增字段 `layer_depth: BTreeMap<Layer, usize>` | 候选大师每题（层次）的技能单元数 |
| `src-tauri/crates/core/src/council/pool.rs`（约 85–113 行） | 单元分组时顺带计数；候选构造处填入 `layer_depth` | 数据来源：`master_units` 当前版本，与六题档案同源 |
| `src-tauri/crates/core/src/council/select.rs`（约 98–120 行） | 新增 `deepest_layer()`，两处落座层调用（保留席位约 164 行、自动补位约 290 行）改用它 | 落座层 = 他有料且本阵容未覆盖的题里积累最深的一题；都被覆盖时退回他最深的一题；没有单元时退回声明层；并列按道法术气器势取靠前 |
| `src-tauri/crates/core/src/council/select.rs`（约 349–379 行，`compare()`） | 补某一题时先比该题积累深浅，再比策略分；换批排除改为显式第一比较键 | 语义与原实现（减 1e6 分实现排除）等价，但"料多者优先"现在生效 |

要做完的事：
1. 编译并跑 `pnpm gate:core`。有错就按上表的语义修正。
2. **补内核断言**（任务 20.1 要求）：在 `src-tauri/crates/core/tests/council.rs` 增加用例，至少覆盖
   —— `layer_depth` 的每题计数与单元数一致；同一题上料多者优先入席；保留席位落在该大师积累最深且未被覆盖的那一题；全无单元的大师落回声明层。
3. 三处都过了，才把 `tasklist.md` 里 20.1–20.3 的 `⏳ 代码已写，未编译验证` 去掉并勾选。

---

## 五、第三步：按任务清单推进（顺序与文件指引）

任务清单是唯一口径：`.monkeycode/specs/2026-09-19-thought-forge-casting-roster/tasklist.md`
（需求 `requirements.md`、技术设计 `design.md`、接通审计 `wiring-audit.md` 同目录，**带精确行号**）。

顺序建议（先做能本地验证的，再做依赖密钥的）：

| 顺序 | 任务 | 主要文件 | 门禁 |
|---|---|---|---|
| 1 | 20.4 `master_list` 带上六题积累计数 | `src-tauri/src/commands.rs`（`master_list` 实现）+ `src/ipc/commands.ts`（类型）+ `src/ipc/client.ts`（演示桩） | `gate:core` + `gate:front` |
| 2 | 20.5–20.11、20.14 点将面板与留痕 | `src/realms/CouncilRealm.tsx`（`待选角` 按钮、换人）、新建 `src/components/SeatPicker.tsx`、`src/styles/shell.css` | `gate:front` |
| 3 | 20.12–20.13 大师包安装入口与六题体检 | `src/realms/VaultRealm.tsx`（安装按钮与档案区） | `gate:front` |
| 4 | 23.1–23.3 采集出口与两个入口 | `src/realms/SelfRealm.tsx`（采集列表）、陪练与定时发现面板（命令已存在，只缺界面，行号见 `wiring-audit.md` 第三节） | `gate:front` |
| 5 | 23.4 连接器超时接通 | `src-tauri/crates/core/src/connector/service.rs`（约 59–60、136–137 行）读 `connector.timeout_secs` | `gate:core` |
| 6 | 21.1–21.4 蒸馏六题收口 | `src-tauri/crates/core/src/distill/`（依赖平台密钥，先用桩客户端覆盖收口逻辑） | `gate:core` |
| 7 | 设置页剩余两步（`4 分区导航与入口`、`5 面板归位重排`） | `src/realms/SelfRealm.tsx`、`src/styles/shell.css` | `gate:front` |

**不归你做、也不要动**：
- **P22 种子六位补料**（7 项）：内容是 Alex 的材料，只有他能写。
- **P16.3–P16.10 真机验收**（8 项）：需要真机、模型密钥、签名密钥，步骤在 `.monkeycode/specs/2026-09-15-thought-forge-deepening/windows-verification.md`（清单 V1–V18）。
- **贯穿性维护 X.3、X.4**（`.monkeycode/specs/thought-forge-workbench/tasklist.md` 末尾）：跟着前两块走。
- **待定项**（名额超限是否硬拦、空位推荐、招募向导）：没有拍板，不要自行实现。

---

## 六、已知坑清单（每条都踩过，别重踩）

| 现象 | 原因 | 对策 |
|---|---|---|
| 编辑内核文件时"原文找不到" | 工作区 CRLF、索引 LF，多行锚点不匹配 | 用单行锚点；或改完立刻 `git diff` 核对 |
| `git add -A -- . ":(exclude)..."` 报 outside repository | cmd.exe 把引号当字面量传进去 | 先 `git add -A`，再 `git reset -q -- .ohmyagent` |
| `git push` 连接被重置 | GitHub 不通（无代理） | 等 Alex 开代理；或改推 Gitee。**不许因此跳过推送前的门禁** |
| 中文提交信息在 cmd 里变乱码 | 控制台编码 | 把提交信息写成 UTF-8 文件，用 `git commit -F 文件` |
| `pnpm gate:core` 第一次跑很久 | 本仓库首次编译 Rust 侧 | 30–60 分钟属正常，别中断 |
| 内核性能用例被忽略 | 十万节点用例带 `#[ignore]`，已移出默认门禁 | 单独跑 `pnpm gate:perf`；两个云端工作流各有一步覆盖它 |
| 桌面壳文件监听用例偶发失败 | 旧的"队列为空"断言在 Windows 上不稳 | **已修**（改为等一拍后排空）。若仍偶发，先确认改动没被退回 |
| 前端有两套调用方式 | `client.call` 与 `useCommand` 并存 | 查"有没有接上"时必须两种都查，只查一种会漏一半 |
| 别用 `README.md` 当唯一口径 | 进度会滞后 | 口径以规格目录的 tasklist 为准，README 只在收尾时同步 |

---

## 七、交付与提交纪律

- **小步提交**：一条任务一提交，提交信息三段式——改了什么 / 验证到什么程度 / 哪些没验。
- **推送前必须**：`pnpm gate:front`（前端改动）或 `gate:core`（内核改动）跑绿；两条都绿跑 `gate:all`。
- **推送**：网络恢复后 `git push`（本地已有 3 个提交在等）。**不要**因为推不上去就把提交合并或改写。
- **收尾**：更新 `README.md` 的规格表与当前进度、`tasklist.md` 勾选、`任务记忆.md` 登记一条
  （日期 / 做了什么 / 涉及文件 / 关键结果 / 下一轮接着做什么）。
- 汇报给 Alex 时：结论先行、说人话、术语当场解释、**没验证的明确说没验证**。

---

## 八、需要 Alex 本人做的（不要代劳）

1. 装工具链时可能弹 UAC 提权，需要他点确认。
2. 模型平台密钥（端点、模型名、key）——会诊、蒸馏、检索没有它一项都试不出来。
3. P22 六位大师的补料材料。
4. P16 真机验收（含签名密钥与升级域名）。
5. 代理（GitHub 不通）。

---

## 九、文件编号索引

| 编号/名称 | 位置 |
|---|---|
| 本轮规格（需求/设计/任务清单/审计） | `.monkeycode/specs/2026-09-19-thought-forge-casting-roster/` |
| 上一轮规格（P16 真机手册 V1–V18） | `.monkeycode/specs/2026-09-15-thought-forge-deepening/` |
| 维护项 X.3、X.4 | `.monkeycode/specs/thought-forge-workbench/tasklist.md`（约 272–273 行） |
| 人工实测路线图 | `人工实测路线图.md`（仓库根） |
| 人看的移交清单 | `HANDOVER.md` |
| 门禁脚本 | `package.json` 第 15–19 行（`gate:front` / `gate:core` / `gate:shell` / `gate:perf` / `gate:all`） |
| 云端工作流 | `.github/workflows/verify-thought-forge-windows.yml`、`release-thought-forge.yml` |
| 内核选角 | `src-tauri/crates/core/src/council/{mod,pool,select}.rs` |
| 命令实现与注册 | `src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`（约 29 行起的注册表） |
| 命令名与前端类型 | `src/ipc/commands.ts`；演示桩 `src/ipc/client.ts` |
| 会诊界面 | `src/realms/CouncilRealm.tsx`；大师库 `src/realms/VaultRealm.tsx`；自我与采集 `src/realms/SelfRealm.tsx`；采集层 `src/components/EmberLayer.tsx` |
| 样式 | `src/styles/shell.css` |
| 调参面板口径 | `src-tauri/crates/core/src/council/tuning.rs`（约 57 行起的 28 项、607–622 行快照） |

---

## 十、给接手者的第一句话

先做第三节（装工具链 + 跑绿两条内核门禁），**做完停下来汇报**，不要顺手写业务代码。
理由：本仓库的 Rust 侧从来没在这台机器上编译过，工具链与基线是全部后续工作的前提；
在它绿之前写的任何内核代码都属于盲写，攒到最后一起爆。
