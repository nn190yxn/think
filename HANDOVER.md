# 交接文档

写给接手继续做的人。读完这份再动代码，能少走很多弯路。

项目是什么、需求全貌、门禁命令见 `README.md`，这里只讲现状、待办、硬约束与坑。

## 一、一句话现状

功能开发已经做完（P1 至 P19 全部完成并通过门禁），代码可以构建、可以跑测试。剩下的工作集中在 Windows 真机验收（P16.3 至 P16.10），以及发布链路上两个必须由你填的真实配置。

## 二、已完成

| 阶段 | 范围 | 状态 |
|---|---|---|
| P1 至 P9 | 骨架、大师库、骑士团会诊、思维网络、主动助理、蒸馏流水线、采集与知识地形、自我蒸馏与发布、资产统计 | 完成，门禁通过 |
| P10 至 P15 | 自适应轮次与收敛曲线、调参、追问与逐席发言、连接器与联网检索、检索安全与分歧判定、成本与凭据与备份、自我约束与运行可恢复 | 完成，门禁通过 |
| P16.1、P16.2 | 真机验证方案与检查器、外壳自检命令 `model_probe` | 完成 |
| P17 | 六题档案与席位锚定到题 | 完成 |
| P18 | 同题对立与按题换批（含缺口题前端呈现、一题可站两位） | 完成 |
| P19 | 记录按题累积（席位立场摘要与立场变化对比） | 完成 |

当前规模：131 个命令，17 个迁移（`0001` 至 `0017`），六位种子大师，前端 14 个测试文件 129 个用例。

迁移到本仓库后已复核：

- 前端 `pnpm test` 在本仓库新布局下通过，14 文件 129 用例全绿。
- 内核 `tests/seed_packs.rs` 与 `tests/council.rs` 用的 `CARGO_MANIFEST_DIR.join("../../../seed-packs")` 在仓库根布局下正指向根目录的 `seed-packs`。
- 两个 workflow 的 `working-directory`、缓存路径与产物上传路径都已按仓库根调整，YAML 解析通过。

Rust 全量用例与 clippy 在迁移前是绿的，内容逐字节未变。请在你的 Windows 机器上先跑一遍 `README.md`「质量门禁」里的全部命令确认一遍。

> 给 AI 执行者的作业单见 `HANDOFF-AI.md`：工具链安装步骤、环境现状、未编译内核改动的验证要求、任务顺序与文件指引、已知坑清单都在里面。

## 三、还没做完的部分

### P16.3 至 P16.10 真机验证

这八项必须在一台真实的 Windows 机器上做，逐项步骤与判据在 `.monkeycode/specs/2026-09-15-thought-forge-deepening/windows-verification.md`，验证清单是 V1 至 V18。对应关系：

| 任务 | 覆盖的验证项 |
|---|---|
| 16.3 模型平台连通与密钥不落库 | V1、V2、V3、V4、V5、V15 |
| 16.4 会诊全链路与调用审计 | V6、V12 |
| 16.5 蒸馏全链路 | V7 |
| 16.6 三类连接器及其审计 | V11、V13、V14、V18 |
| 16.7 操作系统凭据库读写 | V15 |
| 16.8 备份创建、恢复与迁移前备份 | V16、V17 |
| 16.9 安装包安装、自动升级与签名 | V9、V10 |
| 16.10 记录结论、偏差与后续动作 | 填写手册第四节的记录表 |

### 云端已经覆盖的部分

触发 `.github/workflows/verify-thought-forge-windows.yml`（手动触发，约 30 分钟）可以自动判定：

- V1：`#[cfg(windows)]` 代码在真机编译，内核、桌面壳、检查器的用例全部跑通。
- V9 的安装与卸载：NSIS 与 MSI 静默安装、启动、卸载，并确认卸载后用户数据仍在。还会校验产物格式（NSIS 是 PE，MSI 是 OLE 复合文档）与安装落位。
- V16、V17：由内核 `cargo test -p thought-forge-core` 覆盖。
- 额外一步：用只读检查器校验应用真实建出的数据库，断言退出码 0。全新库上应为 14 项中通过 4（S1、S2、V2、V8）、跳过 10、未过 0。

V9 的签名与 V10 的自动升级不在这条工作流里，因为它用 `--no-sign` 生成未签名包。

### 必须人工判定的部分

云端 runner 没有交互桌面，下面这些只能你在真机上做：

- V8：剪贴板与活动窗口采集，以及「关闭后不再产生事件」。
- V2：界面里回填平台配置后的实际表现。
- V3：探针的实际耗时观感。
- V6：逐轮发言质量。
- V7：检查点续跑的手感。
- V12：提示词隔离，某席位看不到他人的补充检索结果。
- V13：MCP 工具清单的可读性。
- V10：自动升级，依赖真实签名与托管域名。

### 两项持续性任务

基线清单里的 X.3 与 X.4 不会一次性完成：

- X.3：每引入一位大师，同步更新覆盖矩阵与候选池统计。
- X.4：每次模型能力或平台变更，同步更新联网能力清单与调用审计口径。

## 四、接手后的执行顺序

1. 在本机跑一遍 `README.md`「质量门禁」的四组命令，确认基线是绿的。
2. 配好 `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`，然后在 GitHub 上手动触发 `Verify Thought Forge (Windows)`，先拿下 V1、V9、V16、V17。
3. 按 `windows-verification.md` 第三节逐项做 P16.3 至 P16.9，每做一项就把检查器输出与截图填进第四节的记录表。
4. 做 P16.10：把结论、偏差、影响范围、后续动作填全，并在 `.monkeycode/specs/2026-09-15-thought-forge-deepening/tasklist.md` 勾掉对应条目。
5. 若要完成 V10，先生成更新签名密钥对，把公钥填进 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`，把 `endpoints` 换成真实托管地址，再发布一个更高版本做升级验证。

## 五、待你补充的配置

| 项目 | 位置 | 说明 |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | 仓库 Actions 密钥 | 更新签名私钥，勿提交进仓库 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 仓库 Actions 密钥 | 私钥口令 |
| `plugins.updater.pubkey` | `src-tauri/tauri.conf.json` | 目前是 `REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY` |
| `plugins.updater.endpoints` | `src-tauri/tauri.conf.json` | 目前指向 `https://releases.thought-forge.app/...`，需换成你实际托管的更新清单地址 |
| 模型平台密钥 | 应用界面或环境变量 | 由使用者自行准备，`THOUGHT_FORGE_API_KEY` 只是回退路径 |

生成密钥对的命令：

```powershell
pnpm tauri signer generate -w $env:USERPROFILE\.tauri\thought-forge.key
```

私钥文件与口令留在本机。填好之前 `pnpm check:release-config` 会以退出码 1 失败，这是预期行为，不是缺陷。

## 六、硬约束（改动前务必确认）

这些约定已经在代码与测试里锁死，破坏它们会让门禁或运行时行为出错。

1. **错误码三处同步**：新增或修改内核错误变体，必须同时改 `src-tauri/src/protocol.rs` 的 `every_error`、`EXPECTED_CODES`，以及前端 `src/ipc/protocol.ts`。有个用例会读取前端清单逐项比对。`E_UNKNOWN` 是前端独有的兜底码。
2. **迁移不可修改**：`crates/core/src/migrations/` 下已发布的 SQL 不要改。新增能力要加新编号的迁移，并把 `db/migrations.rs` 的 `latest_version()` 同步更新。
3. **新增数据表要登记**：受 `data` 模块管辖的表必须补进 `crates/core/src/data/mod.rs` 的 `DATA_TABLES`，否则导出与清除会漏表，密钥残留扫描也会漏扫。
4. **密钥只进凭据库**：数据库只存引用名。检查器的 `--secret` 会扫描 `DATA_TABLES` 覆盖的全部文本列，命中即判未过。
5. **联网默认全关**：新增任何联网能力都要走内核的开关判定，内核侧用 `GatedClient` 保持纯逻辑可测，真实调用由外壳注入。
6. **采集职责边界**：外壳只产 `RawSample`，脱敏、去重、合并、落库一律在内核 `capture::pipeline`。
7. **静态检查**：两个 crate 的 clippy 必须保持 `-D warnings` 归零。命令层的 `#[tauri::command]` 因参数与前端 IPC 字段一一对应而保留多参数，已用 `#[allow(clippy::too_many_arguments)]` 标注，不要为消警把参数合并成结构体。
8. **不用 rustfmt**：core crate 未采用 rustfmt 约定，不要为通过 `cargo fmt --check` 做全量空格级改动。
9. **示例里的测试 helper 要标 `#[cfg(test)]`**：普通 `cargo test` 与 `cargo clippy --all-targets` 都按非测试目标编译示例，未标注会以 dead_code 在 `-D warnings` 下失败。
10. **提示词版本**：会诊提示词改动时要递增 `PROMPT_VERSION`，目前是 `2026-09-17.1`。检查器会核对成功发言锁定的大师与版本。
11. **六题口径**：层次既是颜色与几何标记，也是六道题。层的名字与枚举保持 `Layer` 与道法术气器势，题面取 `Layer::question()`，与前端 `src/domain/layers.ts` 的 `question` 逐字一致。
12. **一题可站两人**：补位会把多余人放到已有人站上的题，这是同题对立的来源。席位卡必须逐人列出，不能按题取第一位。

## 七、容易踩的坑

1. **pnpm 版本**：`package.json` 的 `packageManager` 必须填 npm 上真实存在的版本，且与 workflow 里 `pnpm/action-setup` 的 `version` 完全一致，否则会因重复指定版本报错。用 corepack 激活，别用 `npm i -g pnpm`。
2. **Rust 版本**：不要降到 1.77.2 一类旧版本，依赖树含 edition 2024 的 crate，下载阶段就会失败。
3. **别用 WSL 出 Windows 安装包**：WSL 只能编 Linux 目标，NSIS 与 MSI 必须在原生 Windows 工具链下打包。
4. **Defender 排除**：把仓库目录与 `src-tauri/target` 加进排除路径，实时扫描会让 Rust 构建慢好几倍。
5. **磁盘与内存**：`target/` 膨胀很快，debug 加 release 加多目标很容易吃掉二三十 GB。建议 8 核、32 GB 内存、SSD 留 60 GB。内存只有 8 GB 时要设 `CARGO_BUILD_JOBS=2`。
6. **内核测试含性能用例**：`cargo test -p thought-forge-core` 会连十万节点的 `tests/network_perf.rs` 一起跑，慢是正常的。只想跑单项用 `--test <模块>`。
7. **文件监听用例已改为稳定判据**：桌面壳的 `capture::tests::file_watch_reports_created_file` 原先断言「排空后队列为空」，但 Windows 上写一个文件会同时产生 create 与 modify 两个事件，稍晚到达的 modify 会让它随机失败（本机实测约一半概率）。现改为等 500ms 后排空，只断言同一路径的同一事件类型不再出现第二次；修前 4 跑 2 败，修后连跑 8 次全过。
8. **前端演示数据是模块级可变状态**：`src/ipc/client.ts` 为各功能保留了 `demo*` 状态，同一测试文件内的用例共享它。跨用例断言要按卡片标题定位，不要依赖索引与总数。
9. **跨测试文件重名的模块**：`capture`、`kb`、`self_distill`、`data`、`asset` 下都有 `pipeline` 或 `repo` 或 `service`，命令层与测试引用时用 `capture_pipeline`、`kb_service`、`self_service` 这类别名。`self` 是 Rust 关键字，自我蒸馏模块注册为 `self_distill`。
10. **Tauri 打包目标取值**：`bundle.targets` 只接受 `deb`、`rpm`、`appimage`、`msi`、`nsis`、`app`、`dmg`。Windows 的 WiX 产物对应 `msi`，写成 `wix` 会让构建脚本直接失败。
11. **`--no-sign` 够用的原因**：tauri-cli 的 `sign_updaters` 在读取 `pubkey` 与私钥之前就先判断 `no_sign` 并返回，所以占位 `pubkey` 不会让未签名构建失败。
12. **前端开发服务器端口固定 1430**：配了 `strictPort`，端口被占用会直接启动失败。

## 八、参考位置

| 要做什么 | 去哪里看 |
|---|---|
| 逐项真机步骤与判据 | `.monkeycode/specs/2026-09-15-thought-forge-deepening/windows-verification.md` |
| 验证记录表 | 同上，第四节 |
| 剩余任务勾选 | `.monkeycode/specs/2026-09-15-thought-forge-deepening/tasklist.md` 的 P16 |
| 持续性任务 | `.monkeycode/specs/thought-forge-workbench/tasklist.md` 的「贯穿性任务」 |
| 需求与验收标准 | 三份规格各自的 `requirements.md` |
| 只读检查器用法 | 不带参数运行 `cargo run -p thought-forge-core --example forge_verify` 会打印用法（退出码 2），或看验证手册第二节 |
| 演示数据与前端契约 | `src/ipc/demoData.ts`、`src/ipc/commands.ts`、`src/ipc/protocol.ts` |
