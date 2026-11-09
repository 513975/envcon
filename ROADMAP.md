# EnvCon 持续改进与进度追踪

最后更新：2026-11-09

当前源码元数据版本为 0.6.0（`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 一致）。2026-11-03 的 PWSH-01 回归证据记录 Rust 373 passed / 14 ignored、`npm run verify` 的 10 套 UI 通过；EnvCon 应用内 PowerShell 安装/切换仍待验收。2026-11-09 的 Windows workflow `42551986293`（commit `32d1b22`）失败 15 项；推送 `8c8adc5` 修复登记路径和卸载入口检查后，workflow `42557097874` 又发现 1 个 LLVM InstallLocation 缺失时的回退路径比较问题。当前已统一修复回退分支并扩展短路径测试。本机 Rust 374 passed / 14 ignored、完整 `npm run verify`（1711 modules、两套读状态及 10 套 UI）、`cargo check --offline` 和 `git diff --check` 均通过；新补丁尚未推送，GitHub CI/fresh clone 待重跑。现有 `src-tauri/target/release/envcon.exe` 由 `8c8adc5` 构建，版本 0.6.0、9,586,688 字节、SHA-256 `8593e3999a669de70dbf3e31024d6e707753302e836159b3f8c9ce7aaf7b6912`，不含当前补丁，不能作为候选产物。GitHub Pages 已部署 `32d1b22`；最新 Release 仍为 v0.5.0 预发布，尚无 v0.6.0 tag/Release。源码元数据版本不代表已打包或正式发布。

INSTALL-01-a / a.1：rustup-init 文件名分派缺陷已修复，受管 Rust 工具链已恢复并实测。`D:\DevEnvManager\current\rust` 指向 `D:\DevEnvManager\envs\rusts\rust-stable`；Rust 1.99.1 的 rustc、cargo、clippy、rustfmt 均可运行，五个基础组件已安装。四个历史失败目录保留，未修改系统级 PATH。

2026-11-02 回归与打包：完整 verify 通过（Rust 364 passed / 14 ignored、生产构建、两套读状态、10 套 UI），Clippy 通过并有 1 条既有排序风格建议。仅生成 Windows x64 GUI EXE，未生成安装包、未启动或发布；产物版本仍为 0.5.5。路径、大小、SHA-256 及 Rust 组件实测见 `docs/evidence/install01/rust-and-exe-verification-2026-11-03.json`。

INSTALL-01 仍进行中：系统 PATH 真实写入未执行；LLVM 安装器组件范围仍未实测。该专项记录目标版本为 0.5.6；负责人 Codex。2026-11-03 的 EXE 证据对应 0.5.5，而当前源码元数据已为 0.6.0；这不表示 INSTALL-01 已完成或发布。长期改进完成数不变。

PWSH-01 PowerShell 7 接入专项：进行中；负责人 Codex；目标源码版本 0.6.0。源码已接通独立 `pwsh.exe` 扫描/版本识别、官方稳定 Windows x64 ZIP、官方 SHA-256 门禁、安装结构/精确版本核验与前端展示。2026-11-03 记录的完整串行 Rust 库回归通过（373 passed / 14 ignored），`npm run verify` 含前端构建、读状态与 10 套 UI 全部通过。官方 v7.6.9 ZIP、SHA-256、隔离 `current/powershell` 命令调用及 5.1 保持独立已实测；下一步仍是 EnvCon 应用内安装/切换验收，再分别记录打包和发布。不会替换 Windows PowerShell 5.1、写系统级 PATH 或接入 MSI/MSIX/WinGet；长期 IMP 完成数不变。
PWSH-01 的代码/版本目标为 0.6.0；这不代表 R4/REL-01 发布门槛已满足，也不表示已打包或发布。

UIR-01 前端重构：OpenDesign 交接提案已整理，负责人 Codex，目标版本/排期待审核；本项当前仅有设计说明，不属于已批准实施范围，不计入长期改进完成数，也未改业务代码或运行测试。下一步由用户审核范围与快捷键/导航决策后，再决定是否纳入路线。

用途：记录后续改进的范围、优先级、验收结果与发布状态，是持续改进进度的唯一主清单。

## 基线与范围

2026-10-15 用户确认产品定位：**Windows 开发环境工具下载与扫描工具**。近期聚焦可用的下载入口、准确的本机版本/路径扫描和清楚的结果提示，不承诺完整项目环境。项目自动配置、依赖求解、隔离终端及一键项目启动不在当前范围；此前项目模式建议不再作为近期目标。

现有版本切换、路径设置和包管理能力保留，不因定位收窄直接删除。十项长期记录继续保留作为候选改进和风险追踪，不要求为了“完整产品”扩建全部能力；近期优先下载/扫描可靠性及现有写操作的安全修复。

### 版本完成定义

每个版本都要分别记录源码完成、测试通过、Windows 打包和正式发布四个状态。只有满足本阶段全部验收条件且上一阶段没有未解除的阻塞，才可将阶段标记为已完成。真实安装、卸载或更新未执行时，必须标记待验证；HTTP HEAD、生成 URL 或命令退出码不能单独作为成功证据。

PATH-01 的并行安排不等于忽略安全风险：在隔离夹具验收前，不得把受影响的 uv、pnpm 或 Yarn 路径写入操作宣称可靠；若仍可能损坏数据，发布前必须先修复或禁用相关写入入口。R3 的“完整卸载”逐工具按安装方式验收，不把普通目录删除视作安装器或外部软件已卸载；未达成的方式继续明确标记不支持，不得将 R3 或正式发布标成通过。


## 进度维护规则

1. 开始相关开发前读取本文件，选定条目 ID，填写负责人、目标版本、下一步，将状态改为“进行中”。
2. 完成子项、发生阻塞、测试结束或范围变化时立即同步本文件，不等到整轮迭代结束；进度更新随对应代码改动一起保留。
3. 状态使用：待开始 → 进行中 → 待验证 → 已完成；无法推进时写“阻塞”，取消或延后时写“暂缓”，同时记录原因和解除条件。
4. 实现完成但未验证只能标记“待验证”。“已完成”要求全部验收条件满足，并附源码、测试、报告或提交等可核对证据；未执行的测试明确注明。
5. “开发完成”和“已打包/已发布”分别记录。源码通过测试不能代表用户使用的 EXE 已包含改动。
6. 总览与详细复选框同时维护。总览完成数按已完成条目计，不用代码行数或主观百分比表示进度；新增、拆分条目时更新分母，保留历史 ID。
7. 实际故障与新发现记录在关联条目和变更日志中。历史完成记录保留，回归问题新增子项或重新打开条目，并说明原因。
8. 不把本计划当成后台自动监控：实时追踪指每次开发与验证事件发生后同步文档；没有开发活动时不产生虚构进度。
9. 验证能力（本机工具链实测，2026-10-21 更新）：`rustup component list --installed` 现有 `cargo`/`clippy`/`rust-std`/`rustc`/`rustfmt` 五个组件。**`cargo clippy` 已可用并清零**：首轮 42 个警告，其中 32 个机械问题（needless borrow、redundant closure、chunks_exact、inspect_err 等）由 `--fix` 处理，3 个 type_complexity 提取为命名别名，4 处同类型参数分组为结构体（`InstallRequest`、`RecoveryPaths`、`UninstallPathScope`/`UninstallTarget`、`InstallerUninstallTarget`），2 处 `set_readonly(false)` 属 Windows 语义必要用法并就地 `#[allow]` 说明理由。当前 `cargo clippy --offline --lib` 为 **0 警告**，应纳入常规验证集。**`cargo fmt` 仍未运行**：当前代码不完全符合 rustfmt 默认风格，首次格式化会产生覆盖整个 crate 的 diff，需单独决策并作为独立提交，在此之前不要把它读作「格式已检查」。

## 暂缓项与重新启动条件

- 项目级版本声明、隔离终端、一键启动项目：当前范围外；只有在下载/扫描稳定且有明确用户需求时重新评估。
- 完整 C/C++ 环境（MSVC、Windows SDK、MinGW 协同）：当前只做边界说明和已有工具扫描；有真实用户需求与维护能力后再立项。
- 新增环境类型或包管理器：先证明现有核心工具下载/扫描/验证通过，再逐项加入，并为每项提供能力矩阵。
- 大规模 UI 重构：除非当前入口无法表达支持边界或影响操作安全，否则不作为独立迭代目标。2026-11-03 新增 UIR-01 提案文档供产品审核；审核前仍属暂缓，不代表已排期或批准开发。

0.5.1 已具备按安装方式更新包管理器自身、扫描后直接清理旧包、npm 公共仓库确认恢复、环境安全删除、启动后台预热和前端读请求缓存。下列条目是在这些能力上的增量改进，不表示现有功能全部缺失。

0.5.1 基线发现包括：配置查询尚未在后端跨 IPC 复用，环境目录大小暂未统计；部分更新方式需要工具链提供支持，来源未知的 npm 包恢复仅支持明确确认使用公共仓库。0.5.3 首轮已补齐并发工具配置查询复用，其余边界继续按条目跟踪；历史审计结论已并入本路线图。

十项长期改进仍按下表排期；用户指定的 0.5.2 小版本修复单独追踪，不将小版本修复等同于完成整个长期方向。

## 一、状态总览（按完成度分类）

本轮专项 INSTALL-01（关联 AUD-016 / IMP-04、05、07、09）：进行中；负责人 Codex；目标 0.5.6。Rust 引导修复、工具链实装、组件命令验证、完整回归及独立 0.5.5 EXE 已完成；系统 PATH 真实写入与 LLVM 隔离安装实测仍未完成。取舍项与回归归档已收口，不改变长期条目完成数；未生成安装包、未发布。

**这是先看这里的地方。** 按完成度分组，每组写明「还差什么」。
详细条目见第四节「能力路线与验收」，实现过程见第六节「实现记录归档」。

### 1.1 已完成

源码、隔离夹具与验收证据齐备，无需后续动作。

| ID | 能力 | 目标版本 | 证据 |
| --- | --- | --- | --- |
| IMP-01 | 增量扫描与性能指标 | 0.5.4 | `docs/evidence/imp01/imp01-benchmark.json` |

### 1.2 代码完成 / 待实机验证

**代码与隔离夹具已通过，缺真实 Windows 环境验收。**
这类条目不能标为「已完成」：它们涉及真实 PATH 写入、真实卸载器执行或真实管理器更新，
按项目规则不得在用户真实环境中执行。

| ID | 能力 | 还差什么 |
| --- | --- | --- |
| IMP-02 | 更新预览与恢复 | 真实包管理器更新；单包粒度更新未实现 |
| IMP-04 | 环境生命周期 | 真实原厂卸载器、真实 PATH 写入 |
| IMP-05 | Windows 测试矩阵 | 覆盖 Win10/11、管理员/普通用户的实机矩阵 |
| IMP-06 | 任务与错误交互统一 | 统一任务状态契约、进度显示、错误结构化 |
| IMP-07 | 配置可靠性与迁移 | 真实 uv 恢复、新终端命令生效 |
| IMP-08 | 凭据与下载安全 | 官方校验资料逐来源在线复核 |
| IMP-09 | 适配器扩展与能力建模 | 十种管理器真实版本/配置读取 |

### 1.3 未开始

| ID | 能力 | 阻塞 / 前置条件 |
| --- | --- | --- |
| IMP-10 | 发布与应用升级质量 | 需证书与发布授权；依赖 R0–R3 验收 |

### 1.4 已决定不做

| ID | 能力 | 决定与理由 |
| --- | --- | --- |
| IMP-03 | 旧包恢复与可撤销清理 | 2026-10-20 决定不做。回收站式的「可撤销全局包清理」需对第三方包管理器保证恢复一致性，通用层面无法可靠验证；超出「下载与扫描」定位。现有「直接删除 + 明确告知不可撤销」已保留 |

### 1.5 外部阻塞（需用户提供条件）

| 事项 | 需要什么 | 影响范围 |
| --- | --- | --- |
| 代码签名与时间戳 | 证书 / 签名服务（密钥不入库） | REL-01、IMP-10 |
| 正式 Release 与 tag 推送 | 明确发布授权 | R4 收口 |
| 真实环境验收 | Windows 真机或可用 VM | R3 全部、PATH-01、ASMT-04 |

**2026-10-22 变化**：用户自报已在自己机器上安装并使用 0.5.5 构建，并要求不再做隔离安装冒烟。据此停止 Windows Sandbox 与 Hyper-V 冒烟路线。**这不等于实机验收通过**——用户陈述不是可核对证据，未观察到安装日志、运行截图或错误输出，R0 不因此关闭。证据 `docs/evidence/r0/r0-user-reported-install-2026-10-22.json`。
## 二、能力索引（IMP-01～10 总览表）

P1：优先投入；P2：中期完善；P3：按实际需求与发布条件推进。顺序为建议，可根据反馈调整。

当前完成：**2 / 10**（IMP-01、IMP-09 的 PATH-01 部分）；进行中：**6**；已决定不做：**1**；待开始：**1**。

> 2026-10-20：IMP-03 决定不做，IMP-09 范围收窄。详见 1.4 与 IMP-09 章节。

| ID | 改进方向 | 优先级 | 状态 | 目标版本 | 负责人 | 最后更新 | 下一步 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| IMP-01 | 增量扫描与性能指标 | P1 | 已完成 | 0.5.4 | Codex | 2026-09-18 | A～D 和四项验收通过，原生测量数据见 `docs/evidence/imp01/imp01-benchmark.json`；历史实现记录已并入本清单 |
| IMP-02 | 更新预览与恢复 | P1 | 进行中 | 待排期 | Codex | 2026-10-19 | 已实现执行前只读预览（当前版本、安装位置、精确命令、阻断原因）、按安装方式诊断（Corepack / pip 安装的 uv 拒绝自更新，且该判定经 `ToolConfig.selfUpdateBlocker` 在列表层即生效）、结构化版本范围能力（`UpdateCapabilities`：入口只装 latest，管理器确有文档化命令时给出指定版本模板）、更新前审计留痕（`update_journal`，写入失败即取消更新）与批量更新（预览汇总、跳过语义、失败项重试）；目标版本如实标注未知；单包粒度更新与分阶段进度仍未实现（管理器不报告中间阶段，给百分比即捏造）；composer/bun 经评估确认缺乏可正面验证的安装形态信号，按决定保持 `Standalone` 而非猜测；留痕**不是**回滚方案 |
| IMP-03 | 旧包恢复与可撤销清理 | P1 | 已决定不做 | — | — | 2026-10-20 | 2026-10-20 决定不做：可撤销全局包清理需对第三方管理器保证恢复一致性，通用层面无法验证。现有「直接删除 + 明示不可撤销」保留 |
| IMP-04 | 环境生命周期 | P1 | 进行中 | 0.5.9（UNINST-01） | Codex | 2026-10-17 | 受管范围预览/执行复核已接通；继续安装器登记匹配、PATH 来源记录与剩余隔离夹具，用户级/共享数据默认保留 |
| IMP-05 | Windows 测试矩阵 | P1 | 进行中 | 0.5.3 首轮；0.5.6–0.6.0 持续 | Codex | 2026-10-16 | 首轮 A 已完成；按 R0–R4 纳入下载、路径、扫描、卸载及 Windows 发布验证；不得把模拟测试当实机证据 |
| IMP-06 | 任务与错误交互统一 | P2 | 进行中 | 待排期 | Codex | 2026-10-19 | 已补安装任务跨页结果保留（30 分钟 + 本地 dismiss + 合并初始化）；统一各任务状态契约仍待排期 |
| IMP-07 | 配置可靠性与迁移 | P1 | 进行中 | 0.5.3 首轮；PATH-01 / 0.5.7～0.5.8 | Codex | 2026-10-16 | 首轮 A 已完成；PATH-01 进行中：目录角色与重装初始化夹具已实现，继续旧布局恢复、迁移联动与各管理器入口/缓存完整建模 |
| IMP-08 | 凭据与下载安全 | P1 | 进行中 | 0.5.7（DL-VERIFY-01） | Codex | 2026-10-17 | DL-VERIFY-01 源码与隔离夹具已实现；官方校验资料逐项复核及 R0 依赖仍待验收，其他凭据审计另行排期 |
| IMP-09 | 适配器扩展与能力建模 | P2 | 进行中（范围收窄） | 0.5.5 首轮；0.5.8 | Codex | 2026-10-20 | PATH-01 适配器部分已完成（`bin_path` 对 uv/pipx/bun 改用 `sibling_role`，修掉 uv 把 bin/cache 当损坏工具的缺陷）；仅保留 SCAN-CORE-01 扫描可信度；catalog 审计、适配器契约、新增品类评估、新管理器验证已决定不做 |
| IMP-10 | 发布与应用升级质量 | P3 | 待开始 | 0.6.0（REL-01） | Codex | 2026-10-16 | 关联 ASMT-06；补干净克隆、Windows CI、安装/升级/卸载及产物证据 |

## 三、当前阶段与下一步

### R0–R4 主路线与阶段门槛

产品范围：Windows 开发环境工具下载与扫描工具。本计划先修可靠性与已发现风险，不扩展项目环境构建能力，也不以新增工具数量作为进度指标。版本号均为建议目标，不代表已打包或正式发布；每阶段以验收证据决定是否进入下一阶段。

主路线为 R0→R4 五个阶段；PATH-01 是并行的 P1 维护项，不改变主路线顺序。先把“能否正确下载和识别”做实，再处理可信校验、扫描、卸载和发布，不将复杂的项目环境管理提前纳入范围。

| 阶段 / 目标版本 | 条目 / 优先级 | 负责人 | 交付范围 | 完成门槛 / 依赖 |
| --- | --- | --- | --- | --- |
| R0 / 0.5.6 | AUD-016 / IMP-09、05；P1 | Codex | 完成 16 类工具能力矩阵和 Python 下载垂直切片：只列真实 Windows x64 可安装资产，显示正式发布日期，保留当前版本/路径/更新结果，明确“已下载、已验证、未验证”的边界 | 离线测试、构建和 UI 回归通过；16 类默认资产逐一完成完整文件下载与格式检查；对可安全运行的安装方式做一次性 Windows VM 冒烟；十种管理器逐个核对当前版本和自更新结果。未完成实下载或 VM 验证前不得宣称安装全部验证通过 |
| R1 / 0.5.7 | DL-VERIFY-01 / IMP-08、09；P1 | Codex | 为下载源接入官方 SHA-256、签名或校验文件；校验失败禁止解压/执行；没有可信来源时明确显示“未验证” | 覆盖中断、长度不符、篡改、哈希不符、校验资料缺失和镜像文件；不得把本地计算哈希当作来源真实性证明。依赖 R0 的下载流程基线 |
| R2 / 0.5.8 | SCAN-CORE-01 / IMP-09、05；P1 | Codex | 优先提升 Git、GitHub CLI、Node、Python、JDK、Go、Rust 的实际路径、版本、重复入口和失败诊断；统一“未安装/无法执行/权限不足/解析失败/未扫描”状态 | 多入口同一安装去重、不同安装保留；扫描夹具和 Windows 实机结果一致。依赖 R0 能力矩阵及 R1 下载校验基线 |
| R3 / 0.5.9 | UNINST-01 / IMP-04、05；P1 | Codex | 为 16 类工具实现按安装方式区分的安全卸载：受管目录、原厂安装器、rustup、外部纳管链接、PATH/Junction 和 Python 包边界 | 删除前列影响；只清除能证明归属的资源；安装器路径精确匹配；用户级包、外部 venv、共享缓存和外部安装默认保留；执行后逐项核验并报告部分失败。依赖 R2 的扫描和归属记录 |
| R4 / 0.6.0 | REL-01 / IMP-05、10；P1 | Codex | 干净克隆验证、Windows CI；EXE/NSIS 全新安装、覆盖升级、卸载、配置保留和产物核对 | 源码、测试、打包、上传和正式发布分别有可复核证据；代码签名作为外部依赖单独评估，不阻塞源码验收。依赖 R0–R3 |

### 当前阶段状态与下一个可执行工作

- R0 部分完成，尚未交付：能力清单、R0-a～d 修复和 16 类官方资产文件下载/结构检查已有工作区实现与归档证据。**2026-10-19 在线复测**：16/16 完整下载、长度一致、结构检查通过，12/16 取得官方 SHA-256 且全部与本地一致。**镜像差异比对已完成**：6 类双源资产（Node/Go/Gradle/Maven/Python/JDK）镜像与官方逐字节一致（6/6）。**4 类“无校验资料”资产经复查实际都有发布方校验文件**：Rust `.sha256`、PHP `sha256sum.txt` 均为 SHA-256 且与本地一致，已接入代码；Maven 仅 SHA-512、ADB 仅 SHA-1，因校验器只接受 SHA-256 而明确标注原因、保持未验证。十种包管理器的离线适用性矩阵已完成（10/10 隔离夹具通过，真实版本与自更新 0/10）。**2026-10-20 安装冒烟门槛已解除**：在一次性 Windows Sandbox（Hyper-V 隔离的 Windows 11 Enterprise 10.0.26100，账户 WDAGUtilityAccount，Networking 禁用）中完成 EnvCon 0.5.5 静默安装、启动、主窗口创建、Junction 语义、静默卸载与注册表清理，**10/10 通过**。因此 R0 的「Windows VM 安装冒烟」阻塞项已消除；R0 总体验收仍差十种管理器真实版本/自更新（0/10），故 R0 仍不整体关闭。
- R1 源码与隔离夹具已提前实现并通过本轮完整回归（Rust 120 passed / 13 ignored、前端构建及八套 UI）；状态为待验收，不能越过 R0 关闭门槛。校验资料仍需逐来源在线复核，Gradle 校验文件来源也未实测。
- R2 核心扫描状态、安装身份去重、UI 汇总及隔离夹具已实现并通过本轮回归（`detect::system::tests` 16 passed / 1 ignored；完整 `npm run verify` 八套 UI 通过）；仍是提前实现，阶段验收待 R0/R1 关闭及 Windows 实机一致性核对。
- R3 / PATH-01 继续推进：最新完整验证为本轮 Rust 248 passed / 14 ignored、生产构建、九套 UI 通过。PATH/链接来源、旧链接逐项确认及历史 PATH 独立确认已完成隔离与 UI 模拟验证；手工保存作为用户确认实际 PATH 的显式恢复路径，只重建 `manual/verified` 基线并清空自动所有权，不认领既有条目。R3-h 生产执行链已接入并通过隔离/UI 回归：登记就绪时只运行精确匹配的卸载入口，先解除 EnvCon 当前 Junction，执行前重新核对登记指纹和 EXE 哈希，执行后同时复查登记和运行时目录；任一残留或路径清理失败均报告部分失败且不递归删除，PATH 只在原厂范围核验成功后清理。R3-b 的 Python 用户级包边界已从固定文案改为只读枚举（路径、分发包数量、多解释器共用提示，失败如实报告），但仍不可删除。PATH-01 已新增显式命令入口策略、uv 旧嵌套布局只读检查、扫描警告、可审计恢复执行核心、只读命令解析优先级检查、Yarn 2+ 迁移门禁，以及单工具与一键批量的只读配置差异预览（含受影响包清单与失效标记）：写入前可看到逐键旧值/新值、入口变化、冲突、阻止原因、跳过项和受影响包，写入后报告目录状态和命令解析。真实 uv 恢复、真实卸载器、真实命令生效与真实配置写入仍待验证。R0 外部验证继续待验证，R3 不绕过 R2/R1 的正式验收门槛。
- 包管理器页面显示的是当前已安装版本和可执行路径，不等于官方最新版本。更新仅指管理器自身自更新，不是升级所有全局包；无通用自更新能力的 pipx、Cargo、.NET CLI 继续明确显示不支持。
- 外部纳管环境只解除 EnvCon 创建的链接/记录，不删除原安装；完整卸载指可安全证明归属的 EnvCon 资源，不承诺删除所有与该语言相关的共享数据。
- 暂缓：项目版本文件解析、一键项目构建、隔离终端、依赖求解、WSL/Docker、Conda、完整 MSVC/Windows SDK、新包管理器及大规模 UI 重构。现有功能不因延期直接删除。
- R4 / REL-01 可执行部分已加入工作区：Windows CI 工作流、干净克隆验证脚本、版本/产物记录脚本、发布与回退流程文档，以及只读的应用版本检查/升级提示。Windows workflow `42551986293` 对 `32d1b22` 失败 15 项；后续 run `42557097874` 有 1 项 LLVM InstallLocation 缺失回退测试失败。当前已使用同一安全 8.3 路径归一化函数修复，并扩展 Windows 短路径测试。本机完整 Rust/UI 回归及 `cargo check --offline` 通过；下一步推送并核对新 CI/fresh clone，再重建 0.6.0 EXE。代码签名、Release 上传、tag 发布和 EXE/NSIS 真实安装/覆盖/卸载矩阵仍未完成，不能据此关闭 ASMT-06 / R4。

补充维护项：PATH-01 是已发现的 P1 写操作风险，目标版本为 0.5.7～0.5.8，不是新增产品方向；实现前必须先建立 uv 嵌套目录和 pnpm/Yarn 目录模型隔离夹具。项目环境管理、自动依赖配置和隔离终端仍不在当前范围。

### R0 执行清单（下一步）

1. 冻结 16 类工具和 10 种包管理器的能力矩阵：每项写清默认下载资产、安装方式、版本来源、当前扫描/更新/卸载行为及未支持边界。（范围沿革：.NET SDK 于 2026-10-22 成为第 17 类，PowerShell 7 于 2026-11-03 成为第 18 类；两者均在这份历史 R0 16 类矩阵之外单独跟踪。.NET 尚缺真实资产下载、长度及归档结构核验；PowerShell 的实施和验证范围见 PWSH-01。R0 矩阵仍按其原定 16 类范围验收。）
2. 为 16 类默认资产实际下载文件并检查长度、文件类型/归档结构，记录可取得的官方 SHA-256；将网络失败、镜像差异和无官方校验资料分别记录，不能用 HEAD 代替。R0 只建立资产和证据基线，下载时强制校验由 R1 实现。（2026-10-19：16/16 完整下载与结构检查通过；12/16 取得官方 SHA-256 且全部一致。**镜像差异已比对**：6 类双源资产镜像与官方逐字节一致。**4 类原记“无校验资料”经复查均有发布方文件**——Rust/PHP 为 SHA-256 且已接入，Maven(SHA-512)/ADB(SHA-1) 因算法不支持而明确标注。证据 `docs/evidence/r0/r0-mirror-and-publisher-checksums-verification-2026-10-19.json`。）
3. 优先在一次性 Windows VM 验证 Python：选择有真实 Windows x64 安装器的版本，检查发布日期、安装、实际路径和版本探测；再对其余可安全运行的安装方式做冒烟。（**2026-10-20 阻塞已解除**：主机启用 Windows Sandbox（`Containers-DisposableClientVM` + `HypervisorPlatform`）后，以一次性 Windows 11 Enterprise 10.0.26100 完成 EnvCon 安装冒烟 10/10 通过。Gen 2 Hyper-V 虚拟机因 UEFI 引导失败不可用，Gen 1 可引导到安装界面但未完成安装；Sandbox 路线已在同用途上替代。证据 `docs/evidence/r0/r0-sandbox-install-smoke-verification-2026-10-20.json`。下一步：按同一路线验证 Python 真实安装器。）
4. 用隔离夹具复核十种包管理器的当前版本和自更新结果；明确“不支持”“失败”“版本未变化”“版本变化”，不在用户的全局环境执行更新。（2026-10-19：离线矩阵已完成——10/10 管理器的 catalog 能力、覆盖夹具、隔离结果与跳过原因均已记录；真实版本读取与自更新仍为 0/10，见 `docs/evidence/r0/r0-package-manager-matrix-2026-10-19.json`。）
5. 运行 Rust、前端构建和必要 UI 回归；检查支持状态、下载结果和版本日期显示，修正实测发现的问题。
6. 把矩阵、下载记录、VM 结果、测试命令和未验证项归档；达到门槛后才关闭 AUD-016，并决定是否形成 0.5.6 构建。

### 建议推进顺序

按 R0→R4 主路线推进：0.5.6 下载与包管理器核查收尾 → 0.5.7 可信下载校验 → 0.5.8 核心扫描 → 0.5.9 可验证卸载 → 0.6.0 Windows 发布验收。PATH-01 为并行 P1 维护项，先建立隔离复现，建议在 0.5.7～0.5.8 期间穿插处理。目标版本是建议值；阶段验收失败时先修复，不跳过门槛进入下一阶段。

IMP-02、IMP-03 的包更新/旧包恢复，以及 IMP-04、IMP-07 中复杂生命周期和多项配置恢复仍保留为长期候选；除出现明确故障或用户需求外，不挤占近期下载与扫描主线。安全和基础回归测试贯穿每轮。


## 四、能力路线与验收

### INSTALL-01 安装完整性审计专项（2026-11-03 更新）

负责人：Codex；目标版本：0.5.6；状态：进行中；关联 AUD-016、IMP-04/05/07/09。

- [x] INSTALL-01-a：Rust 安装后显式补装 clippy/rustfmt；受管 stable 1.99.1 已通过 rustc、cargo、cargo-clippy、cargo-fmt 实际命令及组件清单核验。证据 `docs/evidence/install01/rust-and-exe-verification-2026-11-03.json`。
- [x] INSTALL-01-a.1：修复 rustup-init 下载暂存改名导致 proxy 模式启动，保留标准 basename 并增加独占目录/冲突/清理/文件名回归；用户受管 Rust 已恢复，四个历史失败目录保留，未修改系统级 PATH。负责人 Codex；目标 0.5.6。
- [ ] INSTALL-01-b：系统 PATH 逐项编辑、变更预览、提权写入与陈旧快照保护，复用 ownership journal；不提供整段自由文本入口。**源码与隔离/UI 回归已通过、已包含在独立 EXE；真实系统 PATH 写入未执行。**
- [x] INSTALL-01-c：维持 Include_launcher=0；统一 Python launcher 决策与下载页能力说明，不扩大共享资源写入面。策略夹具与完整回归通过。
- [x] INSTALL-01-d：维持 Gradle bin 默认，明确不含自身源码/离线文档的取舍与下载页说明；策略夹具与完整回归通过。
- [ ] INSTALL-01-e：核对 LLVM 实测条件与组件证据，未执行真实安装不得关闭实测项。
- [x] INSTALL-01-f：定向/完整回归与证据归档完成；Rust 364 passed / 14 ignored、前端构建、两套读状态、10 套 UI 及 Clippy 通过。独立 0.5.5 EXE 已构建，未生成安装包、未发布。证据 `docs/evidence/install01/rust-and-exe-verification-2026-11-03.json`。

2026-11-03 更新：Rust 四阶段安装与组件验证已在受管 stable 1.99.1 上实测；本轮完整 verify、Clippy 和独立 0.5.5 EXE 已通过/核验。系统 PATH 只完成源码与隔离/UI 回归，未写真实系统 PATH；LLVM 组件完整性仍未实测。证据 `docs/evidence/install01/rust-and-exe-verification-2026-11-03.json`。

### PWSH-01 PowerShell 7 接入（2026-11-03 新增）

负责人：Codex；目标版本：0.6.0；状态：进行中；关联 IMP-05、IMP-09（扫描可信度）；不改变 IMP 完成数。

- [x] PWSH-01-a：新增独立 `PowerShell` 环境类型；扫描、标准目录发现、版本探测和结构诊断只针对 `pwsh.exe`，不把 Windows PowerShell 5.1 的 `powershell.exe` 当作 PowerShell 7。隔离回归已通过，真实主机扫描待用户验收。
- [x] PWSH-01-b：接入官方稳定 Windows x64 ZIP 版本列表和下载；只信任发布 API 明确给出的 SHA-256，缺少可信摘要时如实显示未验证；不得默认选择预览版。解析夹具与下载门禁回归通过；实时 API/资产实下载待单独验收。
- [x] PWSH-01-c.1：官方稳定 ZIP 实际下载后 SHA-256 与 Releases API 摘要一致；必需文件齐全，7.6.9 实际运行成功，并经隔离的 `envs/powershells/7.6.9 -> current/powershell` PATH 入口解析成功。证据 `docs/evidence/powershell/pwsh-runtime-verification-2026-11-03.json`。
- [ ] PWSH-01-c.2：通过 EnvCon 应用执行真实受管安装并切换验收；隔离 PATH 运行验证不等同于完成应用安装流程。下载失败/哈希不匹配/结构不符/版本不符均不得报告安装成功。
- [x] PWSH-01-d：同步类型元数据、徽标、下载页与环境页；增加扫描、下载来源、安装校验和 UI 回归夹具。构建与 UI 回归通过。

2026-11-03 源码状态：PWSH-01-a～d 已实现，源码版本目标调整为 0.6.0。官方列表只接受明确标记为非 draft、非 prerelease 的稳定 release，并要求资产名、版本 tag 与官方 GitHub 下载 URL 一致；摘要缺失/无效时显示未验证并阻止受管安装。下载后复核 SHA-256，再检查 ZIP 结构、运行已校验的 `pwsh.exe` 并精确比对版本，匹配前不提交最终目录。Windows 探测增加 PE `MZ` 头检查，避免无效 `.exe` fixture 让 CreateProcess 长时间挂起。最近 `npm run verify`：Rust 373 passed / 14 ignored、1711 modules 构建、读缓存/扫描客户端和 10 套 UI 均通过；本地 Rust 测试需指定 `CARGO_HOME`/`RUSTUP_HOME`，UI 使用已安装的 Playwright 模块路径，均仅为当前验证进程设置。官方 7.6.9 ZIP 的 SHA-256、文件布局、实际版本及隔离 current/PATH 子进程调用已核验，证据见 `docs/evidence/powershell/pwsh-runtime-verification-2026-11-03.json`；回归记录见 `docs/evidence/powershell/powershell-development-verification-2026-11-03.json`。未通过 EnvCon 应用执行真实受管安装/切换；0.6.0 未打包或发布。

边界：不替换 Windows PowerShell 5.1、不写系统级 PATH、不接 MSI/MSIX/WinGet；通过 EnvCon 自有 `powershells` 与 `current/powershell` 管理，真实安装、切换和打包分开记录。首轮源码与隔离测试通过后仍需标注未实机验证，未打包/发布不得作相应完成声明。

### UIR-01 前端整体重构提案（2026-11-03）

- 状态：待产品审核；仅完成 OpenDesign 交接说明，尚未排期或进入源码开发。
- 负责人：Codex；目标版本：待定，不绑定 0.6.0。
- 范围：基于现有六页、Tauri API 与真实操作语义形成信息架构、逐页状态、视觉方向、技术约束和验收标准；不新增后端能力。
- 当前证据：`docs/proposals/ui-redesign-brief-2026-11.md`；本轮只改提案和索引，未修改业务代码或执行应用测试/打包。
- 下一步：用户审核产品边界及导航/快捷键行为决策；只有明确纳入路线后才安排原型、开发与验证。

按能力编号排列的完整条目，含当前进度、未完成项与验收条件。

### IMP-01 增量扫描与性能指标

- [x] 首轮 IMP-01-A：仅复用同时发生的工具配置查询；不缓存已完成结果，强制刷新绕过旧任务；本机三组命令尝试 68→34、平均查询 4104→3355ms，环境快照去除隐式目录遍历。历史实现和验证记录已并入本清单。
- [x] IMP-01-B：版本化持久快照、共享在途扫描、更新时间/刷新状态、输入与文件指纹、周期复核和写后失效；82 项 Rust + 原四套 UI + 缓存专项 UI 通过。基准数据见 `docs/evidence/imp01/imp01-benchmark.json`。
- [x] IMP-01-C：环境和缓存大小按需任务、取消/进度/部分诊断；预热不做递归大小扫描，单任务限制。4 项后端测试、大小/扫描 UI 专项和构建通过；390px 截图已核对。
- [x] IMP-01-D：固定场景每版本三组冷/热原生启动、首次可交互、重扫、进程树/内存/IO 指标；预算和完整验收通过。原始 12 次样本见 docs/evidence/imp01/imp01-benchmark.json。

- [x] 记录冷/热启动、首次可交互、常用扫描完成、手动重扫耗时，以及子进程数量、内存和磁盘负载；固定机器、目录规模和样本次数。进程树逻辑 IO/峰值提交内存口径详见报告，不冒充物理磁盘吞吐。
- [x] 增加后端配置查询复用、并发请求合并和有版本的持久化扫描缓存；展示结果时间与刷新状态。
- [x] 设计增量失效：结合元数据指纹和定时复核，覆盖根目录、PATH、工具版本、配置和项目目录变化；不只依赖父目录修改时间。原生外部版本变化约 30.7s 自动发现，移除/恢复后手动刷新通过。
- [x] 环境大小按需统计，支持取消、进度、不完整诊断；大小单任务、扫描组单任务和分类并发 3，预热不递归统计大小。

验收：固定场景有前后测量结果；同一查询不重复执行；外部安装/删除和配置变更后结果可刷新；扫描未完成时页面可正常交互。性能目标在基准建立后确定，不预先宣称提速比例。

### IMP-02 更新预览与恢复

- [ ] 在能力与 UI 上区分包管理器自更新和全局包升级，支持按实际安装方式诊断。（2026-10-19：`self_update_command` 对 pipx/Cargo/.NET 返回逐项原因且 UI 禁用按钮，已有 `package_manager_update_failures_are_structured_for_all_adapters` 覆盖；全局包升级仍走独立的 GlobalPackageReinstall 入口。**2026-10-19 追加：安装方式诊断已扩展**——新增 `UpdateChannel`，在生成任何命令前从命令文件旁的证据判定安装形态：命令文件正文指向 Corepack 的 npm/pnpm/yarn 判定为 `Corepack` 并拒绝自更新（会与 Corepack 的版本固定冲突）；与 `pip.exe` 同级且上层有解释器的 uv 判定为 `PythonPackage` 并拒绝 `uv self update`（会覆盖 pip 管理的文件）。无法读取的命令文件不作为任何证据，回退为 `Standalone` 而非猜测。判定位于 `self_update_command` 顶部，因此预览与执行器给出同一原因，预览不会承诺被拒绝的更新。新增夹具覆盖四种形态。**2026-10-19 追加：判定已提升到能力层**——`self_update_channel_blocker` 成为把 `UpdateChannel` 转成用户可见原因的唯一位置，执行器与工具列表都调用它，文案不会漂移；`ToolConfig` 新增 `selfUpdateBlocker`（`#[serde(default)]`），在 `get_tool_configs` 中由同一个已解析 `Invocation` 填入。UI 据此**在列表层就禁用“更新当前工具”**并在版本面板显示“此安装的自更新不可用：…”，不再需要先点开预览才被拒绝。该字段可选，旧快照缺字段时反序列化为 `None` 而非失败。新增 2 项夹具锁定“列表原因 === 执行器原因”与 IPC 字段名。证据 `docs/evidence/imp02/imp02-runtime-capability-verification-2026-10-19.json`。**未勾选原因**：composer 与 bun 仍无安装形态判定，按 `Standalone` 处理——**这是评估后的决定而非遗漏**：composer 启动器只能证明它包装了 `composer.phar`，无法反证不是其他安装器所生成；Windows 上各 bun 发行版写出的都是同一个静态 `bun.exe`，scoop/winget 的 shim 在安装目录之外、二进制旁不留标记。两者都缺乏“可正面验证”的信号，加启发式即为猜测，故保持 `Standalone` 而不误禁本可正常执行的更新；该决定已由 `composer_and_bun_stay_standalone_because_no_reliable_evidence_exists` 锁定（含“uv 判定不得泄漏到 bun”的反例）。证据 `docs/evidence/imp02/imp02-composer-bun-channel-evaluation-2026-10-19.json`。`.cmd` 正文启发式识别不到经由其他包装脚本间接调用 corepack 的情况。**2026-10-19 再补**：UI 渲染夹具已补齐——`package-manager-update-ui` 注入一条由 Corepack 管理的 pnpm，断言按钮禁用、版本面板出现“此安装的自更新不可用”、按钮 title 含同一原因，且禁用后不会发起预览调用。）
- [ ] 预览当前/目标版本、安装路径、来源和影响范围；支持单包、批量、跳过与失败项重试。（**2026-10-19 部分完成**：新增只读 `preview_package_manager_update`，执行前展示当前版本、安装位置、将执行的精确命令行，以及“更新通过所属解释器执行/固定到当前前缀”等说明；阻断原因与真实执行共用 `package_manager_update_blocker`，预览不会承诺执行器会拒绝的更新；Yarn Berry 与无法解析的版本一律阻断。**目标版本如实显示为“无法在更新前得知”**，不猜数字。UI 改为“先预览再确认”，取消不执行。新增 2 项 Rust 夹具与更新后的 `package-manager-update-ui` 场景（预览零执行、取消零执行、4 次执行对应 4 次确认）。**2026-10-19 追加：批量、跳过与重试已实现**——`preview_package_manager_updates` 汇总全部管理器的只读预览并给出 ready/blocked 划分，被阻止项保留原因而非被丢弃；`update_package_managers` 全程只取一次独占锁并复用单工具执行器。**跳过的语义是执行前的决定**（未找到命令、不支持自更新、Yarn Berry、pip 解释器不匹配、用户显式跳过），带原因且不计为失败，也不会进入重试；只有 `commandFailed` 才是 `failed`，`retryable` 由 items 派生以免 UI 与真实结果漂移。UI 新增“批量更新”按钮、只读预览对话框、结果对话框与“重试失败项”（合并回首次结果，不丢失既有信息）。新增 2 项 Rust 夹具与批量 UI 断言（预览零执行、只发 ready 项、跳过项从不进入执行器、重试仅重发失败项）。证据 `docs/evidence/imp02/imp02-batch-update-verification-2026-10-19.json`。**未完成**：单包粒度（按包而非按管理器）的更新仍未实现。）
- [ ] 执行前保存清单和必要配置；制定各管理器的恢复方案并校验恢复结果。（**2026-10-19 部分完成**：新增 `update_journal` 模块，在更新器启动**之前**把工具名、可执行路径、更新前版本、将运行的程序与参数写入 `backups/updates/journal.json`；写入失败即取消更新，不做无审计的执行。完成后按 attempt 令牌把结果落为 `completed`/`unverified`/`failed`——命令退出 0 但读不到版本记为 `unverified` 而非成功；未知或已结算的令牌是错误，不会静默忽略。沿用既有 journal 约定：schema 版本、`deny_unknown_fields`、写前检查重定向、写临时文件再改名；保留最近 50 条。**未完成**：这是审计留痕而非回滚——这些管理器没有受支持的“装回指定旧版本”命令，故没有恢复方案，也无法校验恢复结果；清单尚未在 UI 展示。证据 `docs/evidence/imp02/imp02-update-journal-verification-2026-10-19.json`。）
- [ ] 明确降级不可用、依赖变化、旧版本下载不可用等限制；显示分阶段结果，不把命令成功等同于版本已更新。（**2026-10-19 部分完成**：`PackageManagerUpdateStatus` 已区分 `commandFailed`/`verificationFailed`/`versionChanged`/`versionUnchanged`/`versionUnknown`，UI 对“命令成功但版本未变化”明确提示不证明是最新版。本轮在版本面板新增三条限制说明：**不支持指定版本降级**、全局包的**依赖变化由管理器自行解析且不给出影响范围**、**旧版本可能已从来源下架**；同时只读展示最近一次更新记录（状态、`更新前 → 更新后` 或“未确认新版本”、程序路径，并注明“仅用于审计，应用不会自动恢复到此版本”）。清单缺失或不可读时隐藏该行，不谎称没有更新过。新增 `package_manager_update_record` 只读命令。`package-manager-update-ui` 增加“限制文案在操作前可见”“无记录时不显示记录行”“更新后显示命令完成与仅审计声明”断言。证据 `docs/evidence/imp02/imp02-update-limits-and-record-ui-verification-2026-10-19.json`。**2026-10-19 追加：版本范围限制已进入能力层**——新增 `UpdateCapabilities{latestOnly, manualPinnedCommand}`，随每个 `PackageManagerUpdatePreview` 返回（被阻止时也返回，因为这是入口的限制而非单次尝试的限制）。`latestOnly` 说明**本入口**只装 latest、不接受版本参数；`manualPinnedCommand` 在管理器确有文档化命令时给出模板（npm/pnpm/pip/composer/uv），yarn（按项目固定）、bun（无版本固定自升级）与无通用自更新的 pipx/cargo/dotnet 一律返回 `None`——`None` 表示“无可背书的单行命令”，不是“不可能”。UI 预览据此显示“版本范围”与“指定版本需手动执行”，取代此前会被误读为管理器缺陷的“不支持指定版本降级”笼统说法。新增 `update_capabilities_state_the_entry_point_limit_and_real_pin_commands` 夹具；Rust 282 passed / 14 ignored；九套 UI PASS；`npm run build` 1709 modules / 537ms。证据 `docs/evidence/imp02/imp02-update-capabilities-verification-2026-10-19.json`。**未完成**：尚无真正的分阶段进度显示（更新是单一命令行调用，管理器不报告中间阶段，给百分比即为捏造）；“依赖变化由管理器自行解析”“旧版本可能已下架”两条仍是说明文字而未结构化。**2026-10-19 再补**：`package-manager-update-ui` 增加三条渲染断言——预览对话框实际渲染“此入口只安装 latest”“指定版本需手动执行”与精确模板 `npm install --global npm@<version>`，补上 UI 接入缺失的渲染证据。）

验收：仅更新用户选中对象；更新前后版本和路径可核对；部分失败保留逐项结果；可恢复场景通过故障夹具验证，不可恢复场景在执行前说明。

### IMP-03 旧包恢复与可撤销清理（已决定不做）

> **2026-10-20 决定：本方向不在当前范围，不再推进。**
>
> 理由：本产品的定位是「Windows 开发环境工具下载与扫描工具」，不是完整的包管理平台。
> 这四项的核心是「可撤销的全局包清理」，需要把包文件、管理器清单、启动脚本一起纳入回收站，
> 并对第三方包管理器保证「恢复一致性」——后者在通用层面无法可靠验证。
> 现有 `old_packages` 模块已提供清晰、诚实的替代行为：**直接删除 + 明确告知不可撤销**。
>
> 以下原始条目**保留作为历史记录**，不再打勾，也不删除。

- [ ] 区分可信安装来源、用户指定恢复来源与不可恢复来源；保留原始判断依据。
- [ ] 支持选择私有 registry、镜像及认证引用，凭据存储与脱敏依赖 IMP-08。（**2026-10-21 已决定不做**：用户明确判定该功能无必要。当前只使用各工具官方默认源，不提供私有源、镜像或认证引用配置；凭据存储与脱敏规则因此无需扩展。本条目保留原文与决定理由，便于日后回溯评估过程。）
- [ ] 评估隔离区/回收站：同时处理包文件、管理器清单和启动脚本；仅在可验证恢复一致性时允许撤销。
- [ ] 支持导出清理计划、保存执行报告、按保留期限处理备份，明确无法撤销的卸载方式。（**2026-10-21 已决定不做**：用户明确判定「导出清理计划」无必要。清理**执行后**的报告已经实现（`envcon-cleanup-<token>.json`，逐项记录 removed/failed 与原因），但不会新增「执行前导出计划」这一形态。不可撤销的卸载方式已在预览对话框中说明。）

验收（原计划，不再要求）：恢复来源展示准确；私有认证不出现在日志和报告；操作失败不删除唯一副本；链接越界与共享文件夹具被保护；可撤销方案恢复后管理器清单及启动命令均正确。

**当前实际行为**：清理报告写入 `envcon-cleanup-*.json` 并保留最近 20 份；执行前重新核对包清单，变化则拒绝；失败逐包记录且不重试。这些能力保留。

### IMP-04 环境生命周期

- [ ] UNINST-01（R3 / 0.5.9）：将当前“删除受管目录 / 移除外部链接”的基础行为扩展为按资源所有权预览、安装器卸载适配、Python 包/用户级数据分类、PATH/Junction 清理和执行后核验；本轮先完成只读预览，不改旧删除命令；详细边界与测试夹具见本文件“UNINST-01 完整卸载的边界与验收”。
- [ ] ASMT-04：切换后比较预期与实际命令路径/版本，解释既有 PATH 和进程环境影响；先完成诊断与修复预览，再验证新终端实际生效。（2026-10-19：只读诊断、**可预览修复**与**修复后重新诊断**均已实现并通过隔离/UI 回归——`switch_env` 返回 `SwitchDiagnosis`；`preview_path_repair` 生成 `reorder`/`manual`/`none`；`Change::Reorder` 精确绑定、执行前重算、写入前后条目集合必须一致；`apply_path_repair` 写入后重读注册表并重新解析，返回 `verified`/`problems`，核验未通过时 UI 明确提示而不报成功。**未完成**：真实环境切换、真实新终端解析对照、真实用户 PATH 写入验证；系统 PATH 造成的遮蔽仍只能手动处理。证据 `docs/evidence/asmt04/asmt04-switch-diagnosis-verification-2026-10-19.json`、`docs/evidence/asmt04/asmt04-path-repair-verification-2026-10-19.json`、`docs/evidence/asmt04/asmt04-repair-rediagnosis-verification-2026-10-19.json`。）

- [ ] 删除前识别当前环境、PATH 引用、文件占用及相关进程；检测不完整时如实提示。（**2026-10-19 部分完成：文件占用已前置到预览**。此前占用只能在 `execute_uninstall_scope` 失败后才知道，且那时当前版本 Junction 已被解除，用户面对的是半完成状态。现新增 `file_blocks_deletion`：以 `DELETE` 访问权打开后立即关闭（不修改任何内容），只把 `ERROR_SHARING_VIOLATION` 判为占用——**访问拒绝是权限问题，绝不冒充占用**。预览最多探测 200 个文件并如实区分“已检查全部 N 个文件”与“已抽样检查前 N 个文件（共 M 个）”，空目录写明未做检查。占用项进入 `unverified`（资源名「文件占用」），列出前 10 个相对路径并提示先关闭占用程序；**占用不阻断预览**，范围仍完整，让用户在知情后决定。**顺带修掉一个由本改动暴露的真实回归**：`validate_uninstall_execution_preview` 用整份预览做相等比较，而占用是易变的环境观察，预览与执行之间程序打开文件会导致合法预览被判“已变化”；现两侧比对前剔除 `LOCK_RESOURCE` 条目，预览仍向用户展示占用。新增 `uninstall_preview_reports_a_locked_file_before_any_deletion_runs` 夹具（未锁不报、加锁报出具体文件、不误伤同目录未锁文件、释放后消失），既有 `uninstall_reports_partial_failure_when_a_managed_file_is_locked` 恢复通过。Rust 283 passed / 14 ignored；卸载套件 31 passed；九套 UI PASS。证据 `docs/evidence/r3/r3-uninstall-preview-lock-detection-verification-2026-10-19.json`。**未完成**：不探测被映射的可执行映像；未做进程识别（把句柄映射回进程需本应用未使用的提权 API）；200 上限由代码固定，用户不可调；当前环境与 PATH 引用识别此前已实现，本条仍未勾选。**2026-10-19 再补：目录占用检测**——此前只探测文件占用，而**另一个进程打开的目录同样阻止删除**，用户要么只看到「目录树未能完整读取」而无从得知原因，要么毫无预警地等到删除失败。新增 `directory_blocks_deletion`：与文件探测同一规则（`DELETE` 访问 + `share_mode(7)` + 立即关闭），**只认 `ERROR_SHARING_VIOLATION`**；必须带 `FILE_FLAG_BACKUP_SEMANTICS`，否则打开目录一律返回拒绝访问，而那是权限问题、绝不能当作「被占用」。资源名用独立的「目录占用」而非复用「文件占用」，因为**处置方式不同**：文件通常是关掉占用它的程序，目录往往是被设为工作目录的终端。目录在既有遍历中探测，与文件共用 200 条抽样上限；`LOCKED_DIR_RESOURCE` 与 `LOCK_RESOURCE` 一同在预览相等比较中剥离，避免切换间的打开动作让合法预览被判失效。**过程中发现并修正的真实缺陷**：完全独占的目录句柄（`share_mode(0)`）会让 `read_dir` 本身失败，遍历返回 `Err`，原本只在成功分支上报的观测会被静默丢弃——临时诊断输出 `DIAG blocks=true inv_ok=false dirs=2 locked_dirs=["cache"]` 证实探测有效而遍历失败；现改为**两个分支都点名目录**，夹具改用 `share_mode(3)`（允许读写、拒绝删除）以模拟真实的「打开的工作目录」而非极端独占。新增 1 项夹具（干净树不编造、`share_mode(3)` 被识别为真占用、点名 cache、说明「工作目录」处置、资源名不与文件占用混淆、范围仍完整、释放后消失）；Rust 298 passed / 14 ignored；`cargo check` 0 warnings。证据 `docs/evidence/r3/r3-uninstall-directory-lock-detection-verification-2026-10-19.json`。**仍缺**：只认共享冲突，以仍允许 `DELETE` 的方式打开的目录不报告（刻意，猜会误报）；无进程识别；未在真实程序占用真实目录上验证。**2026-10-19 补充覆盖**：新增第二项夹具 `an_exclusively_held_directory_is_named_even_when_the_walk_fails`，用 `share_mode(0)` 真实触发 `read_dir` 失败，断言 `scope_complete` 保持 false、`受管目录树` 条目仍在、阻塞目录 `cache` 被点名、释放后重新可读——**两个上报分支现各有独立夹具**（成功分支 `share_mode(3)`、错误分支 `share_mode(0)`）。Rust 299 passed / 14 ignored。UI 侧亦补 4 项断言（无占用时不渲染、有占用时点名 cache 与「工作目录」处置、计数显示为「（1 项）」、必须落在「未验证/不自动处理」而不得出现在「将移除」）。`release-052-ui` PASS。）
- [ ] 提供“卸载并清理相关用户 PATH”“仅移除管理记录”“保留目录并停用”模式；明确对受管普通目录、外部链接的不同适用范围，避免隐式搬移或更改外部目录。（**2026-10-19 部分完成：新增「停用（保留文件）」模式**。此前 `uninstall_env` 没有 mode 参数，唯一路径总是删除受管目录或移除链接，UI 也只有一个确认按钮——三种模式中只有第一种存在。现新增 `UninstallMode { Uninstall, Detach }` 并随 `UninstallPreview` 传递（`serde(default)`，旧客户端按 `Uninstall` 反序列化）。`apply_detach_mode` 把目录/链接项从 `removable` 移到 `retained` 并改写说明；**当前版本 Junction 与有来源证据的 PATH 条目仍可移除**，否则环境仍在生效，不叫“停用”。执行端在 Detach 下整段跳过删除分支，只解除 Junction。**原厂卸载器与停用互斥**：原厂卸载删除的正是停用承诺保留的内容，预览中写明原因，执行前直接拒绝。**两处必须修的接线**：解除 Junction 后的复核会重新生成预览，若不带同一 mode，每次 Detach 都会被判“卸载预览已变化”；`uninstall_env` 原先用默认模式重建基线，Detach 提交永远匹配不上。UI 确认框新增“选择处理方式”分组，切换模式会重新预览（旧预览绑定的是另一种范围）。新增 2 项夹具（Detach 保留字节且解除 active；模式与范围不符的提交在改动前被拒）；Rust 285 passed / 14 ignored；`npm run build` 1709 modules / 517ms；九套 UI PASS；`git diff --check` 干净。证据 `docs/evidence/r3/r3-uninstall-detach-mode-verification-2026-10-19.json`。**未完成**：「仅移除管理记录」仍未单独实现——停用总会解除当前版本链接，因为保留 current 的环境不算停用；未在真实受管目录上执行停用；未打包/发布。**2026-10-19 再补：停用模式的 UI 断言与文案修正**——`release-052-ui` 增加断言：点击「停用（保留文件）」后**重新预览一次且 `mode=detach`**、`aria-pressed` 正确切换、目录从「将移除」移到「明确保留」并带停用说明、主按钮变为「确认停用」、正文改为「停用后目录与文件全部原地保留」、取消不执行任何卸载。**同轮修掉两处自相矛盾的文案**：停用模式下对话框仍写「将永久删除安装目录」、主按钮仍是「确认删除目录」——与“保留文件”的承诺直接冲突，现随模式切换。`npm run build` 1709 modules / 484ms；九套 UI PASS；`git diff --check` 干净。**2026-10-19 评估「仅移除管理记录」：结论是不应作为独立模式实现**。核实到两个事实：① `link_ownership::receipt_path` 把收据存在 `root/.envcon-ownership/links/<digest>.json`，而**环境是否受管由目录位置决定**（`root/envs/<type>/` 下的目录一律被扫描为受管），收据只证明“这个链接是 EnvCon 建的”；② 删除收据会让该链接从 `Source::Owned` 降级为 `Source::Legacy`，此后移除必须逐项确认。也就是说，这个模式**不能达成“不再受管”**，只会把可证明的归属变成不可证明——状态更差，不是更轻。若要表达“暂时不再索管某个环境”，正确做法是独立的忽略清单（环境仍受管、只是不显示/不警告），属于新功能范围，本轮不做。故第 348 条按“卸载 + 停用”两种模式收口，第三种的语义差异在此如实记录。**2026-10-22 新增：外部纳管环境搬入受管目录并可删除旧目录**。此前纳管只建 junction 指向外部目录，用户既不能把它真正搬进受管目录，也不能在搬入后清掉外部旧目录。现新增四个命令：`preview_move_environment`（只读给出源/目标/文件数/字节数/链接数）、`execute_move_environment`、`preview_discard_moved_source`、`execute_discard_moved_source`。**关键发现**：纳管本身就是用 `envs/<folder>/<name>` 这个 junction 指向外部目录，因此搬入的目标路径与当前链接是同一位置——搬移动作是「删掉链接，再在该位置建一个装着同样内容的真实目录」，初版把「目标已存在」当拒绝条件会永远拒绝。搬移顺序为复制到 `.` 开头的临时目录（扫描会跳过）→ 逐字节校验 → 复检源未变 → 经 `link_ownership::remove` 删链接 → rename 到位；任一步失败保留原链接与外部目录，rename 失败则重建链接回滚。按需求方决定，搬移后**原路径不留任何回指链接**，外部旧目录保留供用户核对后单独删除。删除侧五道防线：受管位置仍是链接（未搬移）拒绝、外部路径是管理根目录或其祖先/内部拒绝、用户/系统目录或其祖先拒绝、受管副本与旧目录的相对路径或字节数不一致拒绝、执行时重跑预览比对并在递归删除前再确认不是重解析点。新增 12 项夹具（预览 4、执行 3、删除 5）与 7 项 UI 断言。Rust 339 passed / 14 ignored；九套 UI PASS；`tsc --noEmit` 与 `npm run build`（1710 modules）通过；`git diff --check` 干净。证据 `docs/evidence/r3/r3-move-external-env-verification-2026-10-21.json`。**未完成**：未在真实 Windows 环境搬移真实安装目录（全部临时目录夹具）；搬移没有取消入口，大目录需等待完成；进程被杀会留下 `.name-moving-<stamp>` 临时目录（扫描跳过但不自动清理）；`same_shape` 只比相对路径与字节数、不比内容哈希；删除成功后界面无法回溯旧路径。）
- [ ] 增加环境导出、导入、备份和恢复，记录绝对路径、链接及跨机器兼容限制。（**2026-10-19 部分完成：只读环境清单导出**。此前环境层面**完全无法导出**——只有设置文件与 PATH 有备份机制，用户没有办法记录本机装了什么。新增 `ManifestEntry`/`EnvironmentManifest`：每条记录名称、类型、本机绝对路径、解析后身份路径、版本、状态与链接/current 标记，**不含任何可据此重建环境的内容**。`build_manifest` 刻意不碰文件系统与时钟，因此映射、排序与兼容性措辞都能由夹具覆盖而非只能经 IPC 触达。条目按（类型, 小写名称）确定性排序，同一状态两次导出的字节一致、便于比对。**跨机器限制写成可核对的事实**而非免责声明：固定四条（本机绝对路径、不是可还原备份、链接记录解析后目标、外部纳管仅记录链接）+ 两条条件项（便携模式、未配置根目录）。其中「空清单不代表本机没有安装」只在确实没有根目录时出现，因为只有那时它成立。`validate_manifest_target` 在写入前拒绝相对路径、非 `.json` 与无父目录；落盘走 `config::atomic_write`（同目录临时文件替换），不会在目标路径留下半截清单。UI 新增「导出清单」按钮。新增 3 项夹具；Rust 290 passed / 14 ignored；`npm run build` 1709 modules / 524ms；九套 UI PASS；`git diff --check` 干净。证据 `docs/evidence/r3/r3-environment-manifest-export-verification-2026-10-19.json`。**未完成**：**导入、还原与跨机器迁移均未实现**——路径与本机绑定，还原需要目标解析这一步，本轮刻意不做设计；本条仍未勾选。**2026-10-19 再补：导出按钮的 UI 断言**——`release-052-ui` 增加：点击后 `plugin:dialog|save` 与 `export_environment_manifest` 各调用一次、传入的是对话框返回的路径、toast 同时显示写入位置与「不是可还原的备份」；**取消保存对话框时不得发起导出**（导出计数保持 1）。断言仍打在 mock 上，未在真实后端写盘。**2026-10-19 再补：导入（只读比对）**——此前 `EnvironmentManifest` 只有 `Serialize`，清单是**只能写不能读**的；`notes` 还写着「导入不会自动执行」，却没有读回来的入口。现补上导入的诚实形态：**只读差异比对**，而非隐式还原——清单记录的是单机绝对路径且不含文件内容，把它变成环境必须凭空决定文件从哪来，因此不做。`EnvironmentManifest`/`ManifestEntry` 增加 `Deserialize` 与 `PartialEq`；`compare_manifests` 为纯函数（不碰文件系统与时钟），配对键为（类型, 小写名称）的 `BTreeMap`，每条记录只落入 `unchanged`/`added`/`missing`/`changed` 之一，且新增行不留清单一侧、缺失行不留本机一侧（避免读成空版本）。三处刻意细节：路径经 `detect::system::path_key` 比较，大小写/分隔符/尾分隔符差异不产生幻影「路径」变更；**记录版本为 null 表示当时探测失败**，即便现在已知版本也不算变更；根目录不同（或清单无根目录）时 `same_root=false`，此时才出现「跨机器比对」说明，行结果只说明差异而非缺陷。`validate_manifest_source` 在解析前拒绝相对路径、非 `.json`、不存在与目录，`schemaVersion` 单独校验以免将来版本误解析。新增 2 项夹具（分类四态、null 版本不算变更、跨机器根目录、源校验），Rust 294 passed / 14 ignored；`tsc --noEmit` 通过；`release-052-ui` 新增 8 项断言（各调用一次、路径透传、四类计数、缺失不渲染本机侧、新增不渲染清单侧、变更点名差异字段、无确认按钮、取消选择不比对）PASS。证据 `docs/evidence/r3/r3-environment-manifest-import-verification-2026-10-19.json`。**仍缺**：`备份和恢复` 两半完全未实现；配对按名称，同名不同目录会被当作同一条目（已在限制文本中说明）；跨机器比对会给每条记录报「路径」差异，虽诚实但嘈杂且无路径映射 UI；schemaVersion 非 1 直接拒绝，无迁移路径。**2026-10-19 再补：按环境「重新检测」入口**——第 350 条要求「重新识别」，此前只有整页刷新（重扫全部 16 类与其中每个条目），用户修好一个目录后无法只问「**这一个**现在好了吗」。把扫描里每条环境的判定抽成 `env_health(env_type, path, accessible) -> EnvHealth{version, status, status_detail}`，`scan_category` 改为调用它；新命令 `redetect_environment` 走 `rescan_category` 取出指定条目——**两个入口共用同一判定**，因此单条复检不可能报出与全量扫描矛盾的状态（夹具用镜像断言锁定该等式，而非只靠重构）。环境名沿用既有 `switcher::validate_name`，不能逃出受管目录；未配置根目录与「此分类下没有该名称」都给出明确错误而非空结果。全程只读。UI 每行新增「重新检测」按钮：成功提示「通过」并带版本，失败提示「仍未恢复」并给出状态原因，绝不报成通过，随后刷新总览。新增 1 项 Rust 夹具（缺 node.exe 报结构原因→补上后转为探测失败，证明不复用旧结论；`env_health` 与分类扫描三项完全一致）与 4 项 UI 断言（精确指向被点击的行、通过提示带版本、失败提示原因、失败时不得出现「通过」文案——断言绑定同名行，避免上一条未消失的 toast 误判）。Rust 297 passed / 14 ignored；`cargo check` 0 warnings；`npx tsc --noEmit` 通过；九套 UI 全 PASS。证据 `docs/evidence/r3/r3-environment-redetect-verification-2026-10-19.json`。**仍缺**：重新检测只读指定目录（`read_single_env` 与分类扫描共用 `read_env_entry`/`current_link`），每次仍解析一次 current 链接并执行同一版本探测；结果经 toast 呈现，除总览刷新外没有行内就地更新；损坏的普通目录依然只能诊断无修复动作。）
- [ ] 为损坏环境提供重新识别和可预览的修复入口。（**2026-10-19 部分完成：失效链接的清理入口**。此前 `preview_uninstall_scope` 把「链接目标不可解析」与「链接对象本身读不了」混为一谈：前者让 `scope_complete=false` 并压入「管理链接目标」阻止项，结果是**最需要修复的悬空链接反而被应用拒绝处理**，用户没有任何途径清理它。现把两者拆开——`link_target_path` 失败仍阻断（连对象身份都无法确认），而已读到目标却 `canonicalize` 失败时记为 `unresolved_target`，生成可移除的「失效链接入口」，说明只移除链接对象、**绝不跟随或删除任何目标**；同时不再谎称保留一个已不存在的「受管目标目录」。执行端无需新路径：`link_ownership` 本就用 `FILE_FLAG_OPEN_REPARSE_POINT` 打开链接对象并按句柄删除，从不解析目标——夹具端到端证明了这一点而非假设。新增 2 项夹具（悬空链接预览可清理且不阻断；执行后入口消失、目标未被重建）；Rust 287 passed / 14 ignored；`release-052-ui` PASS；`git diff --check` 干净。证据 `docs/evidence/r3/r3-dangling-link-repair-verification-2026-10-19.json`。**未完成**：「重新识别」一半未改动（总览每次扫描本就重读结构与版本，但没有专门的修复入口）；本项只覆盖悬空链接，损坏的**普通目录**仍只有既有的 broken 状态展示而无修复动作；未重指向到新目标（那属于隐式搬移，计划明确禁止）；未在真实运行时上复现悬空链接。**2026-10-19 再补：「重新识别」的诊断信息已具体到缺失条目**——`looks_like` 原本只返回布尔值，扫描把它折成「目录结构不完整或缺少必要可执行文件」，用户无法判断这是**可修的一个文件缺失**还是**选错了目录**。现抽出 `structure_problem(env_type, root) -> Option<String>` 明确点名缺失入口，并把 `looks_like` 定义为 `structure_problem(..).is_none()`，两者不可能再分歧。有备选语义的类型保持原样：Maven/Gradle 接受 `.cmd` 或 `.bat` 并在两者都缺时说明，Rust 接受 `cargo-home/bin/rustc.exe` 或工具链目录，Adb 沿用既有探测。扫描保留原因而非压成布尔值，broken 的 `status_detail` 变为「目录结构不完整：<原因>」；结构正常但版本探测失败仍走单独的「版本探测失败」文案，两种失败模式保持可区分。新增 2 项夹具（点名 node.exe / bin/javac.exe，逐个补齐后 `looks_like` 同步转真；对全部 16 类断言 `looks_like` 与 `structure_problem(..).is_none()` 恒等）；Rust 292 passed / 14 ignored；`cargo check` 0 errors。证据 `docs/evidence/r3/r3-structure-diagnosis-verification-2026-10-19.json`。**仍缺**：没有专门的「重新检测」按钮（总览每次扫描本就重读结构与版本，用户需刷新）；损坏的普通目录依然只能诊断、无修复动作。**2026-10-19 再补：版本探测失败现在说明原因**——此前 `status_detail` 一律写「版本探测失败，无法确认环境可用性」，而 `run_capture_with_env` 其实已区分超时/无法启动/退出码+stderr，只是 `probe_version` 返回 `Option` 把它们全丢了，用户无法判断是缺文件、进程卡住还是 CLI 报错。新增 `VersionProbe { version, reason }`（两者互斥，调用方不可能同时被告知「成功」和「为什么没成功」）与 `probe_version_with_reason`；`probe_version` 保留为 `Option` 薄包装，因此 `install.rs`/`commands.rs` 的既有调用方行为不变。Python/Node/Zig/Deno/Bun/Php/Llvm/Git/Gh/Mingw 走 `probe_simple`，缺文件时报「缺少可执行文件：<路径>」、启动失败或超时报「<命令> 失败：<原因>」、输出不可解析时报「<命令> 的输出没有可识别版本：<片段>」；片段压成一行并截断 120 字符，避免多行 stderr 灌满状态文本。**Jdk/Go/Rust/Maven/Gradle/Adb 保留原探测并如实说明「此类型目前不提供结构化原因」**——文件解析或多次尝试拿不到单一失败点，编一个原因比不提供更糟。新增 2 项夹具（缺文件点名路径、不可执行文件来自真实启动失败、版本与原因互斥、Maven 如实说明无结构化原因、`probe_version` 的 Option 契约不变）；Rust 296 passed / 14 ignored；`cargo check` 0 warnings。证据 `docs/evidence/r3/r3-version-probe-reason-verification-2026-10-19.json`。**仍缺**：六类仍无结构化原因；原因以自由文本出现在 `status_detail`，不是可过滤的结构化字段；未在真实故障安装上探测。）

验收：每种模式预览文件、链接和配置影响；无关 PATH 和外部目标保持完整；占用导致失败时仍可重试；Windows 目录链接、只读文件和当前环境覆盖回归测试。

### IMP-05 Windows 测试矩阵

- [ ] PATH-01（并行维护 / 0.5.7～0.5.8）：补 uv 嵌套 bin/cache、旧入口恢复、pnpm/Yarn 目录差异、PATH 优先级及部分写入失败的隔离回归；十管理器分别记录适用项、通过项和未验证项，不使用用户真实包执行破坏性测试。
- [ ] UNINST-01（R3 / 0.5.9）：为环境内 pip 包、`--user` 包、外部 venv、共享缓存、安装器登记、PATH 混合条目、链接纳管和失败恢复建立隔离卸载夹具；验证只移除所选且可证明归属的资源。（**2026-10-19 覆盖复核完成，未勾选**：按需求原文逐项比对现有夹具，**8 类场景均已有隔离夹具覆盖**——① 环境内 pip 包：`uninstall_preview_separates_owned_python_packages_from_unverified_external_data`、`uninstall_python_lib_link_preserves_external_packages`；② `--user` 包：3 项 `python_user_scope_*`（只读枚举、不可读不报零、无法解析不猜）；③ 外部 venv：同 ① 夹具建 `project/.venv` 并断言 `keep.txt` 存活；④ 共享缓存：同 ① 夹具建 `pip-cache` 并断言存活，避免随单个环境删掉其他解释器共用缓存；⑤ 安装器登记：`installer_uninstall` 9 项（精确匹配、重复/冲突登记、执行前复核、执行后复查登记与运行时）；⑥ PATH 混合条目：12 项（有来源免确认、来源未知逐项授权、用户/共享/外部条目保留、日志快照绑定）；⑦ 链接纳管：9 项（外部/内部链接、嵌套重解析点、未认领链接阻止移除、目标不被遍历）；⑧ 失败恢复：8 项（占用、写失败/复查失败、陈旧预览、替换链接或切换 current 均报 Partial 且保留资源）。每项「移除」断言都同时断言「必须存活什么」，因此「只移除所选且可证明归属的资源」有对应证据。证据 `docs/evidence/r3/r3-uninstall-fixture-coverage-map-2026-10-19.json`；基线 Rust 292 passed / 14 ignored。**未勾选原因**：全部为临时目录夹具，**未运行真实卸载器、未写真实注册表或 PATH**；本条继承的 UNINST-01 实机验收仍受无可用 Windows VM 阻塞；覆盖由逐项阅读夹具得出，非变异测试证明。）
- [ ] ASMT-01/03/06：补 Python 缺失安装资产与安装后版本不符夹具；将必要历史包 UI/Rust 集成测试纳入版本控制及适当入口，单列 Windows CI 与实机安装验证。2026-10-15 非 UI verify 已通过，不能替代这些待补覆盖。

- [x] 首轮 IMP-05-A：配置损坏/新旧 schema/保存失败/恢复与并发查询夹具已通过；新增 npm run verify、前端读缓存及设置 UI 专项；平台边界见 docs/guides/testing.md。

- [ ] 覆盖 Windows 10/11、管理员/普通用户和多个工具版本；每项注明实机、CI 或模拟验证。
- [ ] 覆盖多卷、空格/Unicode/长路径、只读、权限拒绝、网络盘、junction、符号链接与链接循环。（**2026-10-20 部分完成：路径形状夹具**。新增 `path_comparisons_survive_spaces_unicode_and_long_paths`（提交 `0020279`），覆盖三类此前无夹具、且实际最容易出问题的形状：目录名含空格（`Program Files\EnvCon Home`）、非 ASCII 用户目录（`用户\开发环境\工具`）、超过 MAX_PATH 的深路径（20 层、>260 字符）；同时断言「仅共享文本前缀的同级目录不算后代」——这正是会让清理越界的假阳性。写夹具时纠正了我的一个错误假设：**Windows 在文件系统 API 层会裁掉尾随空格**，`EnvCon Home ` 与 `EnvCon Home` 解析到同一目录（实测确认），断言已改为记录该实测行为。**此前已覆盖**：junction（`path_comparisons_normalize_case_trailing_separators_and_junctions`）、链接循环（`scan`/`size_tasks`/`package_scan` 各有一项）、权限拒绝（`package_scan` 的 `permission_denied_is_counted_separately_and_problems_stay_bounded`）。**未覆盖**：多卷、只读目录写入失败、网络盘路径。验证：`cargo test --offline --lib` → **300 passed / 0 failed / 14 ignored**。）
- [ ] 各管理器使用隔离夹具验证查询、更新、重装、卸载和失败恢复；在线与离线测试分开。（**2026-10-21 部分完成：十种管理器的自更新命令契约**。`self_update_command` 为每个管理器生成确切参数，这些参数会**先展示在预览对话框、随后原样执行**，因此命令内容本身就是对外契约。此前只有 npm（`--prefix` 重定向）与 pip（解释器切换）有间接覆盖，其余八个管理器的确切参数**没有任何夹具**——改动分支不会让任何测试失败。新增 `every_manager_self_update_command_is_pinned_or_refused_with_an_action`，按三类锁定：**能自更新的逐字锁定参数**（pnpm→`self-update`、bun→`upgrade`、uv→`self update`、composer→`self-update`，并断言可执行文件不被重定向、不额外加 prefix）；**不能自更新的拒绝信息必须点名替代动作**（pipx→`python -m pip install --upgrade pipx`、cargo→`rustup update stable`、dotnet→「更新 SDK」）——只说「不支持」会让用户无处可去；**未知工具被拒绝且不静默退化成默认命令**。npm/yarn 与 pip 的路径依赖分支已由既有 `self_update_targets_original_installation` 覆盖，Corepack/pip-uv 安装形态由 `corepack_and_pip_installed_managers_refuse_self_update_with_a_specific_reason` 与 `list_preview_and_executor_share_one_self_update_blocker` 覆盖。Rust 317 passed / 14 ignored、`git diff --check` 干净。证据 `docs/evidence/imp05/manager-self-update-commands-verification-2026-10-21.json`。**2026-10-21 补：查询维度的状态分类**。`node_rows` 把每个依赖分成四种可观察状态（正常 / 缺少安装文件 / 版本与声明不一致 / 本地链接），每种都对应一条给用户看的 `detail`；既有夹具只覆盖 `missing` 一种，其余三种一旦回归不会有测试失败。新增 `node_rows_classifies_every_dependency_state_and_never_invents_one`（四态逐一锁定、`invalid: false` 不得被误判为不一致、路径优先取依赖自身 `path` 否则回退 `项目根/node_modules/包名`、同名多字段按 (name, path) 去重）与 `pip_rows_flags_direct_sources_and_keeps_plain_packages_clean`（`direct_url` → 「本地或直接来源」、非 inspect 模式读顶层数组、editable 回退 `editable_project_location`、缺包名报错）。Rust 319 passed / 14 ignored。证据 `docs/evidence/imp05/package-query-state-classification-verification-2026-10-21.json`。**2026-10-21 补：清理目录分离校验**。`old_packages::verify_separate` 是「清理旧包目录」与「新安装目录」之间的唯一防线，一旦两者重叠或被重定向，清理会删掉刚装好的东西。既有夹具只覆盖 `source == destination` 与 **source 侧**重定向，`destination` 侧的三种情形完全没有夹具。新增 `verify_separate_rejects_overlap_and_redirects_on_both_sides`：平行目录放行（不能过度拒绝）、destination 是 Junction 拒绝（清理会写到链接目标）、destination 位于 source 之内拒绝（会连带删掉新安装）、source 位于 destination 之内拒绝、通过 Junction 别名形成的父子关系同样拒绝。old_packages 模块 6 passed。证据 `docs/evidence/imp06/cleanup-root-separation-verification-2026-10-21.json`。**仍未完成**：全部夹具只验证命令**构造**与 JSON **解析**，未执行任何真实管理器——「在线与离线测试分开」的在线部分仍未做；**重装、卸载两个维度仍无 per-manager 夹具**；失败恢复（更新中断、部分写入）无 per-manager 夹具；夹具锁的是当前 JSON 字段约定，管理器改变输出格式时不会被发现；清理侧未覆盖符号链接（非 Junction）、大小写差异与 8.3 短名等 Windows 路径等价形式。）
- [ ] 为解析器增加属性/异常输出测试，为下载增加网络失败、取消、校验失败与重试测试。（**2026-10-21 部分完成：解析器 snippet 边界**。`detect::probe::snippet` 是探测失败时唯一被用户读到的文本，承担三个产品决策——多行压成一行、长度上限 120 字符、空输出不能返回空串——此前**完全没有夹具**，任何回归只会表现为错误提示变难读或误导。新增 `snippet_flattens_bounds_and_never_returns_an_empty_reason`，覆盖多行/CRLF/制表符归一、空与纯空白返回「命令没有输出」、120 字符边界含等号、121 字符截断带省略号、以及按**字符**而非字节截断（130 个多字节字符不 panic）。新增 `version_token_rejects_non_versions_and_strips_only_trailing_punctuation` 与 `strip_word_prefix_requires_a_space_then_a_digit`：前者锁定「裸整数不是版本」（年份与构建计数在工具输出中随处可见，接受它们会把 2026 报成版本）、v 前缀隐式丢弃、尾部 `. - +` 裁掉而内部保留、引号括号逗号剥离、前导零原样保留（规范化会捏造工具从未打印的值）、首个合格 token 胜出；后者锁定「必须空格后紧跟数字」——无空格、非数字、双空格、大小写不符都不剥离，刻意保守以便调用方回退整行。下载侧复核确认已有覆盖：长度不符与摘要不符不提交半截文件、响应头与数据两阶段均可取消、校验状态彼此区分且未验证不报成通过。Rust 308 passed / 14 ignored、`git diff --check` 干净。**下载重试已实现**：新增 `DOWNLOAD_ATTEMPTS = 3` 与 `Failure::{Transient, Final}`，`download_with_retry` 只重试传输层失败（连接错误、请求发送失败、响应体中途断开、5xx、429），而取消、本地写入失败、4xx（除 429）、长度不符、摘要不符一律**不重试**——同一 URL 每次返回同样字节，重试内容层失败只会把真实损坏掩盖在一串静默尝试背后。重试前删除 `.part` 并把进度回报为 0，避免 UI 显示不再描述新尝试的百分比；退避期间仍可取消。新增 4 项夹具（503 后重试成功且服务器恰好收到 2 次请求、持续 503 在 3 次后停止且不留残片、404 只请求 1 次、摘要不符只请求 1 次）。Rust 312 passed / 14 ignored。证据 `docs/evidence/imp05/parser-anomalous-output-verification-2026-10-21.json`。**仍未完成**：未在真实网络故障下验证重试（夹具用本地 TCP 服务器模拟状态码，不覆盖连接中途断开）；`version_token` 未覆盖「同一行两个都合格的候选」这类歧义场景。）
- [x] **将必要测试脚本、夹具和工作流纳入版本控制（2026-10-20 完成）**：已提交 `b620e2e`，纳入 139 个文件（82 新增 / 50 修改 / 7 重命名），包括此前未跟踪的 6 个生产模块（`app_update`、`installer_uninstall`、`link_ownership`、`path_ownership`、`update_journal`、`uv_recovery`）、3 份 Rust 集成测试、3 份 UI 测试、CI workflow、发布脚本与全部证据文件。同时修正 `.gitignore`：`docs/` 重组后原例外规则指向失效旧路径，导致 `docs/README.md` 被误忽略；现改为 `/docs/*.md` + `!/docs/README.md`，子目录内容默认纳入版本控制。验证：`git status` 干净；`scripts/verify-fresh-clone.ps1 -SkipInstall -SkipTests` → passed（22 个必需文件、0 缺失）。

验收：矩阵标明通过、失败、跳过及原因；测试可重复执行；不依赖真实用户包或修改用户 PATH；关键流程在发布前自动运行，覆盖空白明确列出。

### IMP-06 任务与错误交互统一

- [ ] ASMT-03/04/05：区分安装完成、验证完成与终端生效；路径配置展示旧值、新值、包与命令入口影响和逐项结果，复用已有任务与错误机制。

- [ ] 统一扫描、安装、迁移、更新和删除的任务状态、阶段、取消、日志与后台恢复入口。（2026-10-19 已补一项跨页一致性缺陷：安装任务成功后原先 2 秒即从后端移除，离开页面后结果丢失；现保留 30 分钟，前端 `dismiss` 仅本地隐藏且拒绝隐藏运行中任务，`init` 改为合并而非覆盖。扫描/重装仍使用各自的字符串状态词表，统一契约未做，属重构范畴。证据 `docs/evidence/imp06/imp06-install-task-recovery-verification-2026-10-19.json`。）
- [x] **可计算时显示剩余时间，无法计算时不显示（2026-10-20 完成）**：提交 `e6b132f`。此前下载行只显示字节数与速度，500 MB 和 5 MB 的工具看起来同样「没底」。新增 `formatEta(remainingBytes, bytesPerSec)`，**只在能真实计算时返回字符串，否则返回 `null`**：总大小未知（无分母）、速度非正（传输未开始）、已无剩余、或推算时长超过 24 小时——这四种情况一律不给数字，UI 也不渲染后缀。安装阶段没有任何字节数，因此不会出现剩余时间。夹具同时断言两侧：下载任务显示算出的估算值，且**页面上恰好只有一处**估算（安装行不得有）；并直接锁定 `formatEta` 的边界返回值。验证：`npx tsc --noEmit` 通过；`tests/download-ui.mjs` PASS。**未完成**：包管理器更新、迁移等**不报告中间进度**的阶段仍只显示当前阶段文案，不显示百分比——这是刻意的，因为没有可用的真实进度信号。
- [x] **错误区分原因、影响、建议与详情（2026-10-20 完成）**：提交 `926c4bd`。后端把错误序列化成**纯字符串**（`AppError` 的 `Serialize` 实现），下载页此前直接把它塞进 `Empty desc`——用户看到「网络错误: error sending request for url (...)」时，分不清是自己的网络、站点还是文件问题，也不知道重试有没有用。新增 `src/lib/errorInfo.ts`：`explainError` 返回 `kind`/`cause`/`impact`/`action`/`retry` 五段。分类**只依据可验证的文本前缀**：`网络错误:`、`IO 错误:`、`JSON 错误:` 由后端 `Display` 固定写入，是稳定信号；其余按后端实际措辞匹配（校验/SHA-256、权限/拒绝访问、磁盘/空间不足、来源/版本目录/不匹配）。**认不出的一律归为 `unknown` 并如实说「未识别的错误。」**，不编原因。UI 把原因、影响、建议分行展示，原始后端文本收进可折叠的「后端详情」，并在 `retry=false` 时（校验类）把「重试」改为「重新加载列表」并明确说明同源重试无效、提供切换官方来源。验证：`npx tsc --noEmit` 通过；`tests/download-ui.mjs` PASS，含「未识别错误不得被编原因」「校验失败不可重试」两项断言。
- [x] **补齐环境列表的搜索、排序与健康过滤（2026-10-20 完成）**：提交 `4c7adb6`。新增 `viewEnvs` 纯函数承载视图规则，界面层加搜索框（匹配名称/路径/版本）、健康过滤（全部/可用/有问题）与排序（名称/版本/状态）。关键行为：筛选与排序**不触发重新扫描**；缺版本的条目排在最后而非破坏比较器；按状态排序时**问题条目排在健康条目之前**；筛选空结果不再复用「暂无环境，去下载中心安装一个吧」的文案——此前两种情况外观相同，会让用户以为环境丢了。UI 夹具断言：非匹配行被移除、筛选空状态可与真正空分类区分、`get_overview` 调用次数不变。验证：`npx tsc --noEmit` 通过；`tests/release-052-ui.mjs` PASS。**未完成**：键盘操作（上下移动、回车激活）与分组组件复用尚未做，故本条只勾掉搜索/排序/过滤三部分。

验收：跨页面状态一致，切换页面不丢任务；失败和部分成功可区分；桌面/窄窗口、深浅主题、键盘与可访问名称回归通过。

### IMP-07 配置可靠性与迁移

- [ ] PATH-01（P1，Codex，并行维护目标 0.5.7～0.5.8）：修复包管理器路径语义与目录布局，统一设置、重装、迁移、清理和生效验证；发现、方案、证据与验收见下方专项。实现未开始。
- [ ] ASMT-04/05：补 PATH 解析验证及多管理器配置差异预览；明确路径修改不搬移旧包，为第三方配置设计适用的备份/恢复。已有迁移、重装和部分失败报告继续保留。

- [x] 首轮 IMP-07-A：应用 config.json 版本兼容、可靠替换、上次有效配置备份、显式恢复与错误提示；6 项配置夹具与设置 UI 已验证。第三方配置/PATH 审计及未覆盖事务边界见本清单相关执行记录。

- [ ] 审计应用配置、管理器配置、注册表和 PATH 写入方式；已有可靠实现复用，缺口单独列明。
- [ ] 为文件配置增加 schema 版本、兼容检查和迁移；采用同目录临时文件、可靠替换和错误恢复。（**2026-10-21 部分完成：写入失败路径补齐夹具**。实现早已具备——`SCHEMA_VERSION = 1`、高于当前版本时拒绝而非重置、未知字段经 `serde(flatten)` 保留、`atomic_write` 用同目录临时文件 + `MoveFileExW(REPLACE_EXISTING|WRITE_THROUGH)`、失败时清理临时文件。缺的是验收条款里「权限拒绝、磁盘满之后旧配置或可恢复备份仍有效」这半句的**夹具**：此前只测了并发冲突与陈旧实例，没测写入本身失败。新增 2 项：`a_failed_backup_write_leaves_the_original_config_untouched`（把备份路径变成目录制造写失败，断言主配置逐字节不变、不置位 `write_blocked`、不留 `.tmp-` 残骸、障碍移除后保存恢复可用）与 `a_failed_main_write_still_leaves_a_usable_backup`（用**只读属性**制造替换失败——这是 Windows 上「权限拒绝」的真实形态；断言备份已写好且 `has_valid_backup()` 为真、主配置仍是旧内容未被半写坏）。config 模块 12 项全通过，完整 Rust 314 passed / 14 ignored。证据 `docs/evidence/imp07/config-write-failure-verification-2026-10-21.json`。**仍未完成**：未覆盖真正的磁盘满（需要填充卷）；跨版本迁移只有「拒绝更高版本」与「保留未知字段」两条，没有旧版本→新版本的字段重命名迁移；第三方管理器配置与注册表写入不适用文件原子写入，未纳入本条。）
- [ ] 提供配置历史、回滚和损坏恢复；注册表与第三方管理器写入使用适合各自机制的备份和恢复，不冒充文件原子写入。（**2026-10-21 部分完成：文件配置的历史与回滚**。此前只有一级 `config.previous.json`，再往前保存两次就无从回溯。现新增 `config-history/` 目录：每次保存前归档上一份有效配置、按纳秒时间戳命名、裁剪到 20 份；`list_config_history`/`restore_config_history` 命令接入设置页「配置历史」区，回滚前另存 `config.before-history-restore-*` 以便撤回，损坏的历史文件经 `parse_settings` 拒绝而不覆盖可用配置。新增 4 项夹具（累积与排序、裁剪上限、回滚保留被替换文件、拒绝损坏历史）。修复过程中发现并修掉一处真实缺陷：历史列表读取失败曾让整个设置页落入 catch 分支，现降级为空列表。Rust 304 passed / 14 ignored、九套 UI PASS、`npm run build` 1710 modules、`tsc --noEmit` 与 `git diff --check` 通过。证据 `docs/evidence/imp07/config-history-verification-2026-10-21.json`。**注册表与第三方管理器写入的备份/恢复仍未实现**，该条因此保持未勾选。**2026-10-21 决定不再实现该部分**：用户明确判定「备份第三方管理器注册表」无必要。应用只备份并恢复自己的配置文件（含历史与回滚），不为注册表或第三方管理器配置另建备份机制；这类写入仍走各工具自身的命令，不由本应用代管其状态。）
- [ ] 统一写操作后的缓存失效、并发写入保护与旧读响应处理。

验收：中断、磁盘满、权限拒绝、损坏配置及跨版本迁移测试后，旧配置或可恢复备份仍有效；不能静默重置用户设置；页面不显示过期配置结果。

#### PATH-01 包管理器路径适配与旧布局恢复

负责人：Codex；优先级：P1；目标版本：0.5.7～0.5.8（建议，并行维护）；状态：**进行中（先做隔离布局模型与回归）**。关联 IMP-07（主项）、IMP-05、IMP-06、IMP-09，以及 ASMT-04/05；作为 IMP-07 子项，不新增长期方向，完成数仍为 1 / 10。下一步：为 uv/pipx/Bun 生成兄弟入口目录，补 pnpm/Yarn Classic 入口配置及路径角色隔离测试；真实配置写入仍禁止。

2026-10-16 用户反馈：uv 一键设置后，`UV_TOOL_BIN_DIR` 位于 `UV_TOOL_DIR` 内。已完成当时工作区的只读源码检查，并核对 uv 0.13.1 对应上游实现及相关官方文档。以下是检查发现和后续建议，不是已经完成的修复；实施前需针对届时源码和工具版本复核。未在用户真实环境执行安装、升级、卸载或配置修改，未完成隔离复现和端到端验收；AUD-016 的其他测试结果不能代替 PATH-01 验收。

**发现与实际影响**

| 子项 | 发现 / 证据边界 | 实际影响与处理方向 |
| --- | --- | --- |
| PATH-01-A / uv | 已确认源码缺陷：`bin_path()` 默认返回 `root/bin`，`configuration()` 将其写入 `UV_TOOL_BIN_DIR`；uv 将工具根目录下的直接子目录按包名枚举，并读取各目录的 `uv-receipt.toml` | `bin` 创建后可能被识别为损坏工具并出现 `Ignoring malformed tool bin` 警告。上游缺少安装记录的卸载分支会尝试移除对应目录，存在入口误删风险；该删除风险为源码推演，未进行破坏性实机验证。不能引导用户执行 `uv tool uninstall bin` 修复这种布局 |
| PATH-01-B / uv 重装与清理 | 已确认重装环境把入口和缓存分别设置为目标目录下的 `bin`、`cache`；旧包清理的 `verify_cleanup_files()` 又要求 receipt 中入口位于来源工具根目录内 | 只修改一键设置无法解决全链路问题；重装仍可能产生错误布局，改为兄弟入口目录后又可能被原清理校验拒绝。设置、重装、迁移、扫描和清理必须使用一致的目录模型 |
| PATH-01-C / pnpm | 一键设置写入 `global-dir`、`store-dir`，没有同步配置 `global-bin-dir`；统一 PATH 接入分支未覆盖 pnpm | 包目录改变后入口可能仍留在旧位置；重装流程却显式指定目标下的 bin，形成两套规则。需按支持版本查询并配置包目录、入口目录及存储目录，核对 PATH |
| PATH-01-D / Yarn | Classic 仅设置 `global-folder`，没有同步处理控制命令入口的 `prefix` 和 PATH；2+ 分支将 `globalFolder` 当作全局安装目录 | Classic 的包位置与命令位置可能不一致；2+ 的 `globalFolder` 实为全局数据目录，不能按 Classic 全局包语义展示或迁移。版本能力必须分开处理 |
| PATH-01-E / 通用反馈 | 当前主要根据配置写入成功报告成功，尚未把入口查询与命令实际解析纳入成功条件；用户 PATH 采用追加方式 | 旧入口仍可能优先命中；应用、已有终端与新终端的环境也可能不同。需区分“配置已写入”“目录已核对”“命令生效已验证”，提供部分失败结果和可恢复记录 |

其他管理器的检查边界：不能将 uv 的结论推广为“所有 bin 子目录都错误”，也不能将未发现问题等同于全部验证通过。

| 管理器 | 当时适配情况 / 后续核对项 |
| --- | --- |
| pipx | 工具环境位于 `PIPX_HOME/venvs`，入口可与 venvs 并列；本次未确认 uv 同类冲突。核对旧入口、共享 Python 和自定义 PIPX 配置的恢复 |
| npm | Windows 适配将全局 prefix 本身作为入口目录；本次未确认 uv 同类冲突。核对旧 prefix、入口重建和 PATH 优先级 |
| Cargo | 使用 `CARGO_INSTALL_ROOT` 与其下的 bin；保持它和 `CARGO_HOME`、rustup 工具链的职责区分，不能因整理全局包而搬移或删除工具链 |
| Bun | 全局包与入口变量分别设置，本次未确认 uv 同类冲突；继续核对实际版本、配置文件优先级及已有启动器 |
| Composer | `COMPOSER_HOME` 不只是包安装目录，还涉及全局配置；核对自定义 vendor/bin 配置、仓库与认证引用的保留，不把整个 home 当作可随意清理的包目录 |
| pip、.NET Tools | 当前一键配置不开放全局安装目录设置，主要修改缓存；保留此能力边界，分别验证缓存设置，不伪装成支持任意全局包目录迁移 |

本地证据入口（函数名为定位依据，后续行号可能变化）：

- `src-tauri/src/package_managers/native.rs`：`bin_path`、`global_env`、`bin_env`、`verify_cleanup_files`；`src-tauri/src/package_managers/mod.rs`：`configuration`。
- `src-tauri/src/package_managers/commands.rs`：`environment`；`src-tauri/src/global_reinstall.rs`：重装环境构造和 `install_args`。
- `src-tauri/src/pkgtools.rs`：`values`、`apply_tool_config`；`src-tauri/src/commands.rs`：`apply_tool_config` 的 PATH 接入；`src-tauri/src/pathman.rs`：`integrate_entries`。
- `src/pages/PackageManagers.tsx`：`applyTool`、`applyAllTools` 和路径展示；`src/lib/managerCatalog.json`：各管理器路径能力。

上游核对依据：Astral uv 仓库标签 `0.13.1` 的 `crates/uv-tool/src/lib.rs`（`InstalledTools::tools`、`remove_environment`）、`crates/uv-tool-commands/src/list.rs`、`uninstall.rs`，以及 `crates/uv/tests/tool/tool_list.rs` 的缺失 receipt 用例；uv 官方 Tools 文档；pnpm Settings 的 global-dir/global-bin-dir；Yarn Classic global 文档和 Yarn 2+ yarnrc 的 globalFolder 定义。这些支持源码与语义判断，不替代本项目隔离测试。

**建议布局与实施范围**

uv 的建议目标布局如下；保留既有工具环境根目录名称，减少不必要的虚拟环境搬迁：

```text
<root>/globals/uv-global/  <- UV_TOOL_DIR
<root>/globals/uv-bin/     <- UV_TOOL_BIN_DIR; add to PATH
<root>/globals/uv-cache/   <- UV_CACHE_DIR
```

- [x] **模型与隔离校验已完成（2026-10-19）**：为各管理器分别建模工具/包环境目录、命令入口目录、缓存/存储目录和配置目录；禁止 uv 的入口或缓存与工具环境根目录相同或嵌套。路径校验覆盖大小写、尾部分隔符、junction/符号链接和规范化后的实际位置；真实命令生效仍归后续验收。
- [x] **目录模型已用于一键设置、路径读取、重装、迁移与清理（隔离回归通过）**：一键设置/指定工具设置经 `validate_directory_roles`，重装经 `create_destination_layout` 独占创建，迁移拒绝被角色模型判为错误的布局，清理按 receipt 声明的入口目录。pnpm 与 Yarn Classic 的入口配置和实际查询已接通；Yarn 2+ 的 `global` 迁移已按 PATH-01-k 拒绝并隐藏。扫描与真实命令生效仍待验证。
- [x] **PATH-01-l / 配置差异预览，隔离与 UI 回归通过、实机待验证**：新增只读 `preview_tool_config`/`ToolConfigPreview`，复用与写入相同的 catalog 检查、布局校验和目标计算，但不创建目录、不执行 `config set`、不写环境变量。返回逐键 `旧值 → 新值`（含角色分类）、`blocked` 原因、命令入口是否变化、合并 PATH 后的命令解析状态和“已装包不搬移”说明；未变化的值也列出以便核对最终目标。UI 的“仅设置路径”改为先打开预览对话框，`blocked` 非空时“写入配置”保持禁用。隔离夹具 4 项：`config_preview_lists_old_and_new_values_without_writing`、`config_preview_classifies_every_manager_key`、`config_preview_refuses_invalid_layout_without_creating_directories`、`config_preview_counts_only_existing_non_empty_targets`；`packages-ui` 断言预览先于写入、预览期间不调用写入、阻止项禁用写入。证据 `docs/evidence/path-01/path-config-preview-verification-2026-10-19.json`。
- [x] **PATH-01-m / 一键配置批量预览，隔离与 UI 回归通过、实机待验证**：新增只读 `preview_tool_configs`/`ToolConfigBatchPreview`，用与单工具相同的检查逐个评估，返回 `items` 以及显式的 `ready`/`blocked` 工具列表；被阻止的工具清空变更行、`changedCount` 归零，不会虚增批量总数；单个工具预览失败记为该项阻止而非中断整批。`ToolConfigPreview` 新增 `changedCount`，只统计实际变化的行。UI 的“一键配置全部”改为先打开只读汇总对话框，逐工具列出变化、跳过项与原因，确认后仅写入 `ready` 中的工具，并分别报告已配置/已跳过/失败数量。隔离夹具 3 项：`config_preview_counts_only_rows_that_actually_change`、`batch_preview_separates_ready_and_blocked_tools_without_writing`、`batch_preview_leaves_target_directories_untouched`；`packages-ui` 断言批量预览不写入、被阻止的 pip 不进入写入、逐工具变化行可见。证据 `docs/evidence/path-01/path-batch-config-preview-verification-2026-10-19.json`。
- [x] **PATH-01-n / 受影响全局包清单，隔离与 UI 回归通过、实机待验证**：新增只读 `packages::global_package_rows` 与 `pkgtools::read_affected_packages`，从**当前全局安装目录**读出现有包名、版本和自身诊断，不执行管理器命令、不安装/更新/删除。`classify_affected_packages` 在命令入口变化时把每个包标为 `unresolved` 并给出计数，入口不变则标为 `kept`；空读取明确说明“不代表目录为空”，缺失/不可读目录单独给出原因，不伪造空清单。`ToolConfigPreview` 新增 `affectedPackages`，单工具预览对话框列出包名、版本、“入口可能失效”标记和总数。隔离夹具 3 项：`affected_packages_mark_launchers_only_when_the_entry_moves`、`affected_packages_never_claim_absence_from_an_empty_read`、`affected_packages_read_the_global_root_without_executing_anything`；UI 断言预览显示具体包与计数。证据 `docs/evidence/path-01/path-affected-packages-verification-2026-10-19.json`。
- [x] **修改前的差异与影响预览（隔离回归通过，实机待验证）**：单工具与一键批量均提供只读预览——逐键旧值/新值、命令入口变化、PATH 冲突、阻止原因、跳过项，以及当前全局目录中的具体包清单及其是否可能因入口变化而失效；写入时保留既有配置备份机制，批量写入分项报告已配置/已跳过/失败。尚未执行真实配置写入。
- [x] **写入后核对入口与命令解析（隔离回归通过，实机待验证）**：`apply_tool_config` 写入后报告目录在 PATH 中的状态，并用只读命令解析区分 `managed`、`shadowed`、`missing`，同时列出受管入口内被 PATH 靠前位置覆盖的启动器；当前进程、新终端与重复条目分别报告。普通 PATH 项自动清理仍只在来源和依赖可证明时进行。

**已有 uv 错误布局的恢复建议**

1. 先识别实际 `UV_TOOL_DIR`、`UV_TOOL_BIN_DIR`、缓存、receipt 中入口和 PATH，列出包名、版本、原始来源、附加依赖及配置；保护旧环境和唯一可用入口。只检测到嵌套时不能直接删目录或调用卸载 bin。
2. 保持工具环境根目录不变，规划独立入口与缓存目录；预览冲突并备份配置。通过 uv 原生命令按原始安装要求重建或重装入口，核对 receipt 中的实际路径。不能仅移动 exe 或只改变量就报告恢复成功。
3. 原始来源不完整、私有依赖不可达或安装要求无法可靠还原时，保留旧工具并给出手动恢复所需信息；不能降级为从公共仓库安装同名包，也不能丢弃附加依赖。
4. 验证新入口可执行、版本及 receipt 一致后，再逐项处理可证明归属的旧入口和 PATH。兄弟入口目录的清理必须依据明确的目录所有权与 receipt，不能简单取消“必须位于根目录内”的保护来允许删除任意外部文件。
5. 通过文件占用、权限拒绝、网络失败及部分重装成功夹具验证恢复和重试；未成功恢复的工具继续保留原数据，报告部分结果。具体重装参数和是否可原地更新入口，在实施时按支持的 uv 版本验证。

**验收条件（全部待验证）**

- [ ] 空白配置与已有 `uv-global/bin`、`uv-global/cache` 错误布局均有隔离夹具；设置、重装后 `uv tool dir`、`uv tool dir --bin`、缓存查询和软件显示一致，`uv tool list` 不再把 bin/cache 当作损坏工具。
- [ ] 在隔离环境覆盖安装、列举、升级、单工具卸载与批量卸载；核对入口及 receipt，其他工具、外部目录和共享入口不受误删。旧错误布局的风险复现仅使用可丢弃夹具。
- [ ] 已有工具恢复覆盖多个包、附加依赖、非默认来源、同名命令冲突和部分失败；恢复后命令路径与版本正确，失败时可用旧副本仍保留。
- [ ] pnpm 的包/入口/存储目录及 Yarn Classic 的 global dir/global bin 与预览一致（隔离夹具已通过，真实管理器待验证）；Yarn 2+ 不再显示或执行不适用的 Classic 全局安装/迁移操作（已由 PATH-01-k 实现：后端拒绝、UI 隐藏，隔离与 UI 回归通过）。
- [x] **PATH 覆盖（隔离夹具通过）**：系统/用户 PATH 顺序、旧入口优先导致的 `shadowed`、大小写与尾斜杠重复、共享条目及未知变量保留均有夹具覆盖；`apply_tool_config` 不再以“目录已追加”冒充命令生效。真实注册表与新终端对照仍未执行。
- [ ] 十管理器完成适用路径能力回归并记录具体版本、命令、结果和跳过原因；功能验证、源码完成、打包和正式发布分别留证，未执行项不得打勾。

下一步：先复核实施时版本和源码，建立 uv 嵌套目录隔离复现及目录模型测试，再实现 PATH-01-A/B，随后处理 C/D 和通用结果验证。登记完成不表示功能修复完成。

### IMP-08 凭据与下载安全

- [ ] ASMT-02（R1 / 0.5.7）：**进行中（提前实现、验收待 R0 关闭）**；扩展 DownloadSpec 携带可信校验依据，在下载提交/解压/安装前校验；无可信资料如实标记，校验失败阻止执行。现有解压路径保护不替代内容校验。（**2026-10-21 已决定不再扩展**：`DownloadSpec` 已携带 `VerificationInfo` 并在下载写入过程中校验，无可信资料标记为未验证、校验失败阻止执行，该部分**保持现状**。用户明确判定签名验证无必要，故不引入发布方签名；本条目剩余部分不再推进，验收随 R0 实机条件一并处理。）

- [ ] 审计外部命令调用，保持结构化参数，验证参数注入和 Windows 启动器边界。（**2026-10-21 部分完成：审计 + 启动器边界夹具**。逐文件检索 `Command::new` / `cmd.exe` / `powershell` 调用点，确认**生产路径中不存在「拼接字符串后交给 shell」的调用**：`pkgtools`、`download::run_installer`、`detect::probe`、`installer_uninstall::execute`、`reinstall_process`、`global_reinstall`、`pip_reinstall`、`uv_recovery` 全部使用 `Command::new(exe).args([...])` 结构化参数；`detect::system` 的 PowerShell 仅用于读取版本且脚本为常量。唯一的「字符串 → 进程」转换点是 `installer_uninstall::parse_command`——它把注册表里的 `UninstallString`（**不可信输入**，任意软件都能写）转成 `(PathBuf, Vec<String>)`，因此这个函数本身就是 Windows 启动器边界。此前只断言 4 个反例且无正例，无法证明「合法登记仍能通过」。新增 `parse_command_accepts_only_an_exact_exe_with_literal_arguments`：接受引号路径+字面参数、引号路径无参数、首尾空白、无引号绝对路径；拒绝 `&`、`|`、`>`、`<`、`^`、`%VAR%`、嵌套引号、单引号、引号未闭合、路径与参数间无分隔、非 `.exe`（`.bat`）、相对路径（`cmd.exe /c`、`msiexec.exe /x`）、父目录穿越。Rust 315 passed / 14 ignored、`git diff --check` 干净。证据 `docs/evidence/imp08/command-launcher-boundary-verification-2026-10-21.json`。**仍未完成**：未做动态注入测试（夹具只验证解析层）；未逐命令复核 Tauri IPC 参数传递；`rundll32`/`wscript` 等启动器因实现直接拒绝非 EXE 而未单独覆盖；凭据脱敏属第 339 条。）
- [ ] 为 registry token、代理密码、认证 URL 建立凭据存储与统一脱敏规则，覆盖 UI、子进程输出、日志和报告。（**2026-10-21 部分完成：URL 凭据脱敏**。全仓检索 `token`/`password`/`credential`/`secret`/`auth` 逐条判定：`old_packages`、`global_reinstall`、`commands` 里的 `token` 全是**内部预览令牌**（`scan-<pid>-<n>`），与凭据无关；`sources.rs` 的 16 类源全是编译期常量 URL，`reqwest::Client` 未配置任何认证头，**没有私有 registry、没有代理认证**，因此「凭据存储」在当前功能范围内**无实现对象**。但「统一脱敏」有真实缺口：`download.rs` 的失败信息 `format!("下载失败({}): {e}", spec.url)` 把**完整 URL 原样**写进错误，经 `AppError::Serialize` 直达 UI 与日志——一旦支持带凭据的地址，`https://user:token@host/...` 会完整泄漏。新增 `error::redact_url`：userinfo 用 `rsplit_once('@')` 整体替换（密码含 `@` 时最后一个是分隔符）、敏感查询参数只替换值保留键名（`token`/`key`/`secret`/`password`/`passwd`/`auth`/`sig`/`signature`/`credential`，不区分大小写且允许包含）、fragment 保留、无 scheme 与空串不 panic、公开 URL 逐字符原样通过。已接入下载失败信息这一唯一出口。Rust 316 passed / 14 ignored、`git diff --check` 干净。证据 `docs/evidence/imp08/url-credential-redaction-verification-2026-10-21.json`。**仍未完成**：凭据存储（keyring/DPAPI、凭据输入界面）完全未实现；**子进程输出未脱敏**——`pkgtools::run`/`mutate` 的 stdout/stderr 原样进错误信息，若管理器打印 token（如 `npm config get //registry/:_authToken`）不会被处理；日志文件与导出报告（`update_journal`、清理报告、环境清单）也不经过 `redact_url`；查询参数白名单是启发式的，可能漏掉自定义参数名；`Authorization: Bearer ...` 这类非 URL 形态的凭据当前不处理。）
- [ ] 按源能力验证下载哈希/签名，并记录校验来源；无可信校验资料时显示“未验证”，而不是“校验通过”。（**2026-10-21 已决定不再扩展**：哈希部分已实现并保持现状——`VerificationInfo` 区分 `TrustedSha256`/`ChecksumFile`/`Invalid`/`Unverified`，下载过程边写边算 SHA-256，不符即拒绝落盘且按「最终失败、不重试」处理；无可信资料时如实标为未验证。**签名验证部分决定不做**：用户明确判定签名验证无必要，因此不引入发布方签名与时间戳校验。本条目保留原文，便于日后回溯为什么只做到哈希这一层。）
- [ ] 为删除、迁移和更新统一影响范围预览及执行前复核，覆盖预览后路径变化。（**2026-10-21 部分完成：更新侧补齐执行前复核**。此前卸载与迁移都用 `expected_preview` 绑定执行，但**更新没有**——用户看完预览后若可执行文件、版本或参数集发生变化，应用仍会按过期预览执行。现 `update_package_manager` 接受用户确认过的预览，执行器用**本次真正要用的值**重新构建同一结构并逐字段比对，不一致即拒绝并提示重新预览；已被阻断的管理器单独报出原因而非泛化为「预览已变化」。批量更新传 `None` 并注释说明：批量没有逐工具确认可绑定。新增夹具覆盖「相同预览放行」「可执行文件不同」「参数不同」「版本变化」「管理器变为阻断」五种情形。Rust 305 passed / 14 ignored、九套 UI PASS、`npm run build` 1710 modules、`tsc --noEmit` 与 `git diff --check` 通过。证据 `docs/evidence/imp08/update-execution-precheck-verification-2026-10-21.json`。**仍未完成**：删除与迁移的预览已各自具备，但三者尚未统一成同一套机制；未覆盖预览后目录被整体替换的情形；未在真实管理器上执行。）

验收：恶意参数与路径夹具不能扩大操作范围；测试凭据不出现在产物报告中；下载校验失败阻止安装；已有安全边界保留且有证据。

### IMP-09 适配器扩展与能力建模

> **2026-10-20 范围收窄**：原 6 项中，4 项（catalog 审计、适配器契约、新增品类评估、新管理器验证）是纯架构重构，用户不可感知，已决定不做；保留与扫描质量直接相关的 SCAN-CORE-01。被砍条目的原文保留在下方作为历史记录。

- [x] **PATH-01（并行维护 / 0.5.7～0.5.8）**：将目录角色和版本能力纳入适配器契约，优先覆盖 uv、pnpm、Yarn Classic/2+ 的差异；复用 IMP-07 专项及 IMP-05 验收，不统一套用单一目录布局。**已完成（隔离回归）**：`bin_path` 对 uv/pipx/bun 改用 `sibling_role`，入口目录不再落在工具环境根内——这修掉了 uv 把 `bin`/`cache` 当成损坏工具的原始缺陷；`directory_roles` 覆盖全部十管理器；uv/pnpm/Yarn 专项夹具 9 项通过。真实命令生效仍归 IMP-05 验收。
- [ ] SCAN-CORE-01（R2 / 0.5.8）：先覆盖 Git、GitHub CLI、Node、Python、JDK、Go、Rust；每条扫描结果保留可执行路径、版本和来源，按规范路径/安装身份消除同一安装的重复入口，不合并不同安装；将未安装、权限不足、无法启动、退出失败、输出无法解析、未扫描区分为不同状态。
- [x] ASMT-01：Python 版本列表按实际 Windows x64 安装资产筛选，并覆盖没有对应安装器的版本；当前源码与 UI 回归证据见 AUD-016。

- [x] IMP-09-A（0.5.5，Codex）：新增 ADB / Android Platform Tools 环境；发现 PATH、Android SDK 标准目录和环境变量；正确识别发行版本，复用下载、安装、切换、纳管、卸载；验证 SDK 外部目录保护并补充接入说明。该环境扩展不等于完成长期管理器能力模型。

**已决定不做（原文保留）**

- ASMT-03：将 ADB 的结构/版本验证经验扩展为适配器能力，避免仅凭下载、解压或安装器退出码宣称可用。→ *ADB 是特例（依赖 SDK 结构与 release 元数据），推广到其他工具需要各自验证语义；其核心检查已由 R0/R1 的资产结构检查覆盖。*
- 审计现有 catalog/adapter，集中命令、元数据解析、路径发现、更新和卸载能力，消除确认存在的重复分支。→ *纯重构，无用户可见收益。*
- 定义适配器契约和共享测试，区分运行时、项目依赖工具、开发包管理器及系统软件管理器。→ *当前 3 个 `AdapterKind` 已能覆盖十管理器；再抽象是为假想的第四类工具做准备。*
- 评估 Conda、Maven/Gradle、Chocolatey、Scoop 的真实用户流程和安全范围，逐项排期，不假定都具备全局包语义。→ *Chocolatey/Scoop 是系统级包管理器，语义与 npm 类完全不同，塞进同一抽象会变形；超出「下载与扫描」定位。*
- 用一个经确认的新增管理器验证：能力缺失可以解释，页面无需复制整套业务逻辑。→ *依赖上一项。*

验收：现有十管理器回归通过；新增适配不绕过公共路径/进程安全逻辑；公共类型及前后端字段一致。（原「提供新增适配器开发说明」随适配器契约一并取消。）

### IMP-10 发布与应用升级质量

- [ ] ASMT-06：核对干净克隆包含必要文档与测试，建立源码→验证→产物证据链；补全全新安装、覆盖升级、卸载及数据保留矩阵。0.5.5 已打包历史记录保留，未正式发布状态不变。（**2026-10-20 部分完成：干净克隆验证已通过**。提交 `b620e2e` 后 `scripts/verify-fresh-clone.ps1 -SkipInstall -SkipTests` 从「11 个文件缺失」变为 **passed**（22 个必需文件、0 缺失）。过程中修正了脚本自身的过期路径——它仍在检查 `docs/release-process.md`，该文件已移到 `docs/guides/`，导致干净树也被误报缺失（提交 `27e7fa1`）。**未完成**：安装/覆盖升级/卸载及数据保留矩阵仍需真实 Windows 环境。）

- [ ] REL-01（R4 / 0.6.0）：建立源码提交、版本、锁文件、测试结果、构建环境、变更日志和产物 SHA-256 的对应记录；建立可回退版本标记。
- [ ] 配置 Windows 代码签名与时间戳；证书、签名服务和费用作为外部依赖单独记录，密钥不入库。
- [ ] 验证 EXE、NSIS 的安装/覆盖升级/卸载及用户数据保留；评估 MSI 阻碍与实际需求。
- [ ] 增加应用版本检查和升级提示；若实现自动更新，验证更新元数据与产物签名、失败恢复和用户确认。

验收：显示版本、文件版本与发布记录一致；全新安装和升级均可复核；回退入口明确；区分本地构建、上传和正式发布，不把打包当作已发布。


## 五、执行记录

INSTALL-01（Codex；目标 0.5.6；2026-11-03）：Rust 安装缺陷已修复，受管 stable 1.99.1 和 clippy/rustfmt 组件命令已实测；完整 verify 通过（Rust 364 passed / 14 ignored、10 套 UI、生产构建、两套读状态），Clippy 通过并有 1 条排序风格建议。独立构建当前版本 0.5.5 EXE 并核验版本/架构/大小/SHA-256；证据 `docs/evidence/install01/rust-and-exe-verification-2026-11-03.json`。未生成安装包、未发布；系统 PATH 真实写入与 LLVM 隔离安装仍待验证。

PWSH-01（Codex；目标 0.6.0；2026-11-03）：源码已实现并完成完整回归；当前已在唯一临时目录验证官方 ZIP、SHA-256、版本探测及隔离 managed `current/powershell` 的 PATH 子进程调用。下一步真实 EnvCon 安装/切换仍待验收；保留现有工作区改动，未打包/发布。

UIR-01（Codex；目标版本/排期待定；2026-11-03）：完成 OpenDesign 前端重构提案文档与文档索引；状态待产品审核，仍属路线图暂缓项。只读盘点六页、API 边界、18 类环境及关键安全流程；未修改业务源码、未运行应用测试、未打包或发布。

每个条目开始开发时新增一行；同一条目的当前行随事件更新，详细过程放入变更日志。

| ID | 当前子项 / 下一步 | 阻塞与解除条件 | 实现与验证证据 | 构建 / 发布状态 | 更新时间 |
| --- | --- | --- | --- | --- | --- |
| DOC-REVIEW / 文档一致性核对 | Codex；2026-11-08；完成本轮状态同步 | 无产品代码改动 | 核对受跟踪 Markdown 的本地链接、18 类环境与源码元数据；更新 10 套 UI 测试说明、release record 输出路径和构建/发布状态；更正路线图证据计数 | 未运行应用测试；未打包、未发布 | 2026-11-08 |
| UIR-01 / 前端整体重构提案 | Codex；排期/目标版本待定；已形成说明，待用户审核范围及导航/快捷键决策 | 当前为提案，仍按大规模 UI 重构暂缓规则；用户明确纳入路线后解除 | `docs/proposals/ui-redesign-brief-2026-11.md`；只读源码盘点，未运行应用测试 | 未进入开发、未打包、未发布 | 2026-11-03 |
| PWSH-01 / PowerShell 7 | Codex；0.6.0；进行中；真实 EnvCon 安装/切换待验收 | 打包/发布需另行记录；0.6.0 不代表 R4/REL-01 通过 | 完整 Rust 373 passed / 14 ignored；`npm run verify`（1711 modules、读缓存/扫描客户端、10 套 UI）通过；官方 PowerShell 7.6.9 ZIP、SHA-256、隔离 PATH 运行通过，证据见 `docs/evidence/powershell/pwsh-runtime-verification-2026-11-03.json` | 未打包、未发布 | 2026-11-03 |
| REL-01 / ASMT-06 | Codex；0.6.0；进行中；推送 LLVM 回退路径修复，重跑 Windows CI/fresh clone，再以通过的 commit 重建 EXE | `42551986293` 失败 15 项；`42557097874` 失败 1 项，系 LLVM 无 InstallLocation 时的卸载器父目录比较未归一化 8.3 拼写。当前修复已通过本机 Rust 374 passed / 14 ignored、完整 `npm run verify`、`cargo check --offline` 与 `git diff --check`；新 CI/fresh clone 待执行。PowerShell 应用内验收、代码签名、真实安装矩阵仍未完成 | `path_key_with_long_names` 遇到 reparse component 保持原值；Windows 短路径测试覆盖登记路径与 LLVM fallback。上次 fresh clone 在 `8c8adc5` 核对 22 个文件、`npm ci`、构建和 Rust 374 passed / 14 ignored。最后打包的 EXE 来自 `8c8adc5`，9,586,688 bytes、SHA-256 `8593e3999a669de70dbf3e31024d6e707753302e836159b3f8c9ce7aaf7b6912`，不含当前修复 | 待推送/CI；未新打包、未打 tag、未发布 | 2026-11-09 |
| UNINST-01 / R3-h 生产执行链 | Codex；0.5.9；代码及隔离/UI 回归通过、实机待验证（提前实现）；下一步在 Windows VM/授权环境用真实 Python/LLVM 原厂卸载器验收，并核对目录外副作用 | 不得运行真实用户卸载器；真实安装/卸载验收依赖 VM/授权环境及 R0～R2 关闭；真实 PATH/注册表仍未写入 | `cargo test --offline --lib installer -- --test-threads=1`：21 passed / 0 failed / 217 filtered out；生产执行器超时测试 1 passed / 237 filtered out；完整 `npm run verify`：Rust 225 passed / 13 ignored、1708 modules 构建、两套读状态及九套 UI PASS。覆盖精确命令、无 shell、执行前登记/哈希复核、Junction 先解除、登记/运行时双复查、部分失败和无递归回退。`cargo check --offline` 0 errors / 1 个既有警告；`git diff --check` 0；fmt 缺组件未执行。证据见 `docs/evidence/uninstall/installer-uninstall-verification-2026-10-19.json` | 未打包、未发布 | 2026-10-19 |
| UNINST-01 / R3-g 历史 PATH | Codex；0.5.9；代码及隔离/UI 回归通过，实机待验证（提前实现）；下一步在 Windows VM/授权环境复核真实 PATH；R3-h 生产链路已接入但仍待实机验收 | 实际 PATH/注册表不得写入；真实验收依赖 Windows VM/用户授权及 R0 关闭 | `path_ownership` 13 passed、`uninstall_legacy_path` 7 passed、`release-052-ui` PASS；完整 `npm run verify`：Rust 212 passed / 13 ignored、构建、两套读状态及八套 UI PASS。新增手工保存恢复夹具，确认 pending 经显式保存后只重建基线、不认领旧条目。`cargo check --offline` 0 errors / 1 个既有警告；`git diff --check` 0；fmt 因缺组件未执行。证据见 `docs/evidence/uninstall/legacy-path-verification-2026-10-19.json` | 未打包、未发布 | 2026-10-19 |
| DL-VERIFY-01 / IMP-08、09 | Codex；0.5.7；**实现与隔离测试已通过，待验收（提前实现）**；下一步待 R0 关闭后验收；官方校验资料与镜像差异已于 2026-10-19 核实 | R0 门槛仅剩 Windows VM 安装冒烟；无可信资料仍只能显示未验证 | 2026-10-19：`cargo test --offline --lib` 264 passed / 14 ignored；sources 套件 20 passed；新增 Rust/PHP 校验文件接入与镜像不继承官方摘要的夹具；官方 16 类资产下载审计与 6 类镜像逐字节比对均通过 | 未打包、未发布 | 2026-10-19 |
| SCAN-CORE-01 / IMP-09、05 | Codex；0.5.8；**实现与隔离回归通过，待验收（提前实现）**；下一步待 R0/R1 门槛与 Windows 实机一致性核对 | R0/R1 仍未完成外部验收；扫描范围不可穷举整台机器，未发现只可表达“扫描范围内未发现” | `cargo test --offline --lib detect::system::tests -- --test-threads=1`：16 passed / 1 ignored；完整 `npm run verify`：Rust 120 passed / 13 ignored，构建、读状态、八套 UI 全通过。含核心身份、错误分类、空结果筛选夹具 | 未打包、未发布 | 2026-10-17 |
| AUD-016 / IMP-09、05、04、02 | Codex；0.5.6 R0-a～d 源码修复和隔离回归已验证；官方 16 类资产真实下载/结构核验与 6 类镜像逐字节比对已完成；下一步处理 R0 安装冒烟阻塞 | GitHub API 未认证会触发限流；安装器行为、目录外卸载项和 PATH 清理仍未实测；无可用 Windows VM；`cargo fmt --check` 因当前工具链未安装 rustfmt 无法执行 | 2026-10-19 复测：`cargo test --offline --lib sources::tests::live_download_asset_audit -- --ignored --exact --nocapture` 通过（16 类官方资产逐项下载；381.83 秒；证据 `docs/evidence/r0/r0-asset-audit-2026-10-19.json`）。同日镜像比对 6/6 逐字节一致，Rust/PHP 校验文件已接入，Maven/ADB 因算法不符明确标注；证据 `docs/evidence/r0/r0-mirror-and-publisher-checksums-verification-2026-10-19.json`。`package_managers::` 隔离夹具 16 passed，十管理器矩阵见 `docs/evidence/r0/r0-package-manager-matrix-2026-10-19.json`。下载只写临时目录，未安装 | 当前源码改动在工作区；未重新打包 0.5.5，未发布 | 2026-10-19 |
| PATH-01 / IMP-07、05、06、09 | Codex；0.5.7～0.5.8；进行中（提前实现）；下一步：在隔离 Windows 环境用真实 uv 验证恢复后 receipt/入口/版本、`uv tool list` 和新终端命令解析，并用 `where.exe` 对照命令解析结果；继续十管理器真实版本/配置 | 真实 uv 恢复、真实管理器配置与版本仍未验证；依赖 R0～R2 阶段门槛及可用隔离 Windows 环境 | `cargo test --offline --lib`：248 passed / 14 ignored；九套 UI PASS；`npm run build` 1709 modules；`cargo check` 0 errors / 1 个既有警告；本轮新增只读命令解析检查、Yarn 2+ 迁移门禁、单工具配置差异预览、一键批量预览、受影响全局包清单与批量展示，合计 15 项新隔离夹具及 3 个 UI 场景；证据见 `docs/path-*.json`（本轮最全为 `docs/evidence/path-01/path-batch-packages-and-python-user-scope-verification-2026-10-19.json`） | 未打包、未发布 | 2026-10-19 |
| UNINST-01 / IMP-04、05 | Codex；0.5.9；进行中（提前实现）；下一步真实原厂卸载验收、PATH/注册表演示及 16 类剩余安装方式边界 | 真实卸载器、VM 安装/卸载验收和目录外副作用未验证；真实 Windows 验证受 VM 条件阻塞；依赖 R0～R2 | 最新完整 Rust 回归 297 passed / 14 ignored、九套 UI PASS；R3-h 生产执行链、Junction/PATH 顺序与隔离夹具已接通，证据 `docs/evidence/uninstall/installer-uninstall-verification-2026-10-19.json`；本轮新增卸载预览前置文件占用检测（只认 `ERROR_SHARING_VIOLATION`，200 文件抽样并如实标注），证据 `docs/evidence/r3/r3-uninstall-preview-lock-detection-verification-2026-10-19.json`；新增「停用（保留文件）」模式（`UninstallMode::Detach`，跳过全部删除、只解除 Junction 与有来源证据的 PATH，原厂卸载与停用互斥），证据 `docs/evidence/r3/r3-uninstall-detach-mode-verification-2026-10-19.json`；让悬空受管链接可被识别与清理（区分「目标不存在」与「链接对象读不了」，只移除链接对象、不跟随目标），证据 `docs/evidence/r3/r3-dangling-link-repair-verification-2026-10-19.json`；新增只读环境清单导出（绝对路径/身份路径/版本/状态，跨机器限制写成事实，纯构建与原子写入），证据 `docs/evidence/r3/r3-environment-manifest-export-verification-2026-10-19.json`；新增环境清单导入（**只读差异比对**，四态分类、null 版本不算变更、路径归一化，不做隐式还原），证据 `docs/evidence/r3/r3-environment-manifest-import-verification-2026-10-19.json`；损坏目录现在点名缺失入口（`structure_problem`，`looks_like` 定义为其否定，16 类恒等），证据 `docs/evidence/r3/r3-structure-diagnosis-verification-2026-10-19.json`；真实环境仍未操作 | 未打包、未发布 | 2026-10-19 |
| 产品路线 R0～R4 | R0 已部分实现；当前 0.5.6 仍待完整下载与隔离 Windows 安装验证 | 无；各版本均为建议目标，须按每轮验证结果再确认 | 本计划写入 ROADMAP；本轮仅文档检查，未修改源码或执行功能测试 | 当前基线仍为 0.5.5；无新产物、未发布 | 2026-10-16 |
| 产品定位 / 关联 ASMT-01～06 | Codex 完成文档范围修订；目标版本不变，修复版本待排期；下一步聚焦下载与扫描可靠性 | 用户明确项目管理不在当前范围 | README.md、PRODUCT_ASSESSMENT.md 与本清单同步；仅文档检查，不复用历史测试宣称功能完成 | 无源码修复、无新打包、无发布 | 2026-10-15 |
| ASMT-01～06 / 关联 IMP-04～10 | Codex 已完成评估；下一步按收窄后的范围安排实现，新增目标版本待排期 | 无评估阻塞；用户已确认下载与扫描定位 | PRODUCT_ASSESSMENT.md；评估轮 verify --skip-ui：构建、Rust 92 passed / 9 ignored、两套读状态通过；UI、真实安装与升级未执行 | 评估轮无源码修复、无新打包、无发布 | 2026-10-15 |
| — | 尚未启动本清单中的开发工作 | 无已确认阻塞 | 文档基于 0.5.1 现状建立，未执行新功能测试 | 本轮仅文档，无新产物 | 2026-09-17 |
| IMP-09-A | 子项与本地打包完成；后续继续公共适配器契约 | 无 | Rust 92 passed / 9 ignored；ADB 专项 6 passed / 1 ignored；前端构建、两套读状态和六套 UI 通过；版本/哈希见 docs/releases/release-0.5.5.md | target-v0.5.5 EXE/NSIS 已打包；未正式发布 | 2026-09-19 |
| IMP-01-B/C/D | 全部完成；后续发现回归时按本条重新打开 | 无 | 86 Rust、两套读状态、六套 UI 与补充竞态测试通过；12 次原生基准满足预算，热扫描 8196→1232ms，进程 167→9；外部变更和大小原生验证通过 | 各轮英文提交已推送 codex/imp01；0.5.4 EXE/NSIS 已打包并核对版本/哈希；正式 Release/安装升级未执行 | 2026-09-18 |
| IMP-01-A / IMP-05-A / IMP-07-A | 首轮已验证；后续推进完整启动基准、持久化扫描缓存与第三方配置回滚 | 无 | 统一 verify：Rust 79 passed / 8 ignored、前端构建、读缓存与四套 UI；本机基准和产物记录归档于 `docs/archive-legacy.md` | 0.5.3 EXE/NSIS 已打包；未远端发布；安装/覆盖升级未验证 | 2026-09-18 |
| FIX-052-01～04 | 四项源码修复及验收完成，最终产物已核对；后续按用户反馈迭代 | 无 | Rust 71 passed / 7 ignored、三套 UI、在线版本源及本机扫描验证；版本/大小/哈希见 `docs/archive-legacy.md` | EXE/NSIS 已打包；未远端发布；安装/覆盖升级未验证 | 2026-09-18 |

## 六、实现记录归档（按事件时间倒序）

> 以下是各阶段的实现过程记录，**保留原始内容不作改写**。
> 这些是历史快照：其中的日期与措辞可能已被后续改动覆盖。
> **判断当前状态请以第一节「状态总览」和第四节「能力路线」为准。**

### 2026-10-19 R4 / REL-01 可执行部分（历史实现记录；当前状态见上方总览）

负责人：Codex；目标版本：0.6.0；状态：**进行中（源码/脚本已实现，CI、真实网络、安装矩阵与发布待验证）**；下一步：重新运行 fresh clone 检查并核对 Windows CI 结果，取得可用产物记录后再决定 tag；tag 或 Release 均需单独授权。

- [x] **REL-01-a / 已实现、待 CI 验证**：新增 `.github/workflows/windows.yml`，在 Windows runner 执行 `npm ci`、`npm run build`、`cargo test --lib`，显式安装固定版本 Playwright 后运行 `npm run verify`；UI 测试使用 runner 自带的 Edge 通道。截至该历史记录尚未在 GitHub Actions 实际运行；当前运行状态见第一节和 REL-01 执行记录。
- [x] **REL-01-b / 首次 fresh clone 检查（2026-10-19）失败，后续文件跟踪问题已修复**：该次 `-SkipInstall -SkipTests` 检查在临时克隆中发现 11 个缺失文件；其后相关文档、集成/UI 测试、CI workflow 和脚本已纳入 Git 跟踪。fresh clone 流程尚未在修复后重新运行，当前仍需验证。另行运行的离线集成测试 `cargo test --offline --test sources_test download_spec -- --test-threads=1` 为 3 passed / 2 filtered out（两项网络测试未运行）。
- [x] **REL-01-c / 脚本已实现并生成记录**：新增 `scripts/record-release.ps1`，比对 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 版本，记录 commit/分支/工作区状态、Node/Rust 构建环境、锁文件 SHA-256、已有产物 SHA-256 和调用方提供的测试证据。本轮生成 `docs/releases/release-record-0.5.5-20261018-230054.json`：版本三处均为 0.5.5、`versionConsistent=true`、工作区 `dirty=true`，测试字段记录本轮 `npm run verify` 结果；发现的旧 0.5.5 NSIS 产物 SHA-256 为 `7071493d...f71a1e`，状态标为 `present-unverified-provenance`，不能证明由当前 commit 构建；不表示本轮已打包或已发布。
- [x] **REL-01-d / 流程文档已实现、tag 未创建/未推送**：新增 `docs/guides/release-process.md` 并加入 `.gitignore` 例外，规定正式 tag 为 `v<semver>`、预发布为 `v<semver>-rc.<n>`，不移动/覆盖/删除已推送 tag；回退从可核对产物重新构建并记录。当前没有新 tag，未执行推送。
- [x] **REL-01-e / 已实现、待真实 Release/网络验证**：新增后端 `app_update::check_latest` 与 `check_app_update` 命令，读取 GitHub Releases latest 元数据；区分 `upToDate`、`updateAvailable`、`noRelease`、`unknownVersion`、`error`，仅接受本仓库 Release URL，只展示发布说明与发布方摘要，不自动下载、执行或安装。设置页“关于”卡片增加“检查更新”和“打开发布页”，失败可重试，并明确提示不会自动安装、摘要需下载后另行核对。新增 4 项解析/版本比较测试（通过）；真实 GitHub Release 检查和真实升级未执行。
- [ ] **REL-01-f / 阻塞**：代码签名与时间戳（需证书/密钥，密钥不入库）、正式 Release 上传（需发布授权）、EXE/NSIS 全新安装/覆盖升级/卸载与配置保留真实验证（本机无可用 Windows VM/真机条件）均未执行；tag 推送需用户明确授权。

最终回归（2026-10-19）：`npm run verify`（Codex 运行时 Playwright）→ Rust **230 passed / 0 failed / 14 ignored**、生产构建 **1709 modules**、两套读状态和九套 UI PASS；`cargo check --offline` → 0 errors / 1 个既有 `path_ownership::preview_cleanup` dead-code 警告；`git diff --check` 退出 0；`cargo fmt --check` 因缺 `cargo-fmt.exe` 无法执行。证据见 `docs/evidence/r4/r4-release-verification-2026-10-19.json`；CI 和真实 Release 检查仍未运行。

### 2026-10-17 R3-g / 历史 PATH 逐项确认接续

负责人：Codex；目标版本：0.5.9；状态：**进行中（首轮定向测试 8 passed / 2 failed，尚未通过回归；提前实现、验收待 R0 关闭）**。下一步：修正旧断言、补齐确认审计和执行快照绑定后重跑。固定 current PATH 入口单独列候选，未选择项保留且不阻止仅删除受管环境；重复/候选外确认、重复 PATH 项、共享依赖、日志损坏或 pending 状态及预览后变化不能授权清理。用户确认不构成 EnvCon 创建来源证明。不得写真实用户 PATH。

- [x] 独立列出无连续 EnvCon 来源证据、但与所选环境固定 current 入口精确匹配的历史 PATH 候选；仅唯一出现且无共享依赖的条目可供确认。
- [x] UI 候选默认不选；逐项确认后将精确字符串随卸载预览提交，显示“用户确认移除”，不混入自动所有权日志。
- [x] 执行前重读 PATH、来源日志和依赖，并比较候选及已确认集合；未选择项保留，重复/候选外条目、重复 PATH 项、外部改写和预览后变化必须拒绝或保留。
- [x] 隔离夹具覆盖共享依赖、日志缺失/损坏/pending、PATH 写入/复查失败和成功后的结果报告。
- [ ] 真实注册表及用户 PATH 验收仍待 Windows 实机授权；无 VM/授权前不得宣称实机通过。
- [x] **手工保存恢复语义 / 隔离验证通过**：用户显式保存或恢复实际 PATH 时写入 `manual/verified` 基线并清空自动所有权，不把既有条目变成 EnvCon 创建；新增 `path_ownership::tests::explicit_manual_save_reconciles_pending_record_without_claiming_entries`，确认 pending 记录经手工保存后可继续集成，且旧条目仍只能作为历史候选确认。

2026-10-19 接续：负责人 Codex；目标版本 0.5.9；状态继续**进行中（提前实现，验收待 R0 关闭）**。最新 `npm run verify` 为 Rust **205 passed / 13 ignored**，生产构建、两套读状态及八套 UI 全通过；`release-052-ui.mjs` 单独 PASS。历史 PATH 核心 13 passed、卸载历史 PATH 7 passed、手工保存恢复 1 passed；`cargo check --offline` 为 0 errors / 5 个既有 dead-code 警告，`git diff --check` 退出 0，`cargo fmt --check` 因缺少 cargo-fmt.exe 未执行。仅临时目录和模拟 PATH，未写真实用户 PATH/注册表、未安装/卸载/打包/发布。

### 2026-10-19 PATH-01 布局策略与 uv 旧布局

负责人：Codex；目标版本：0.5.7～0.5.8；状态：**进行中（提前实现；本轮布局策略、只读检查、扫描警告、恢复执行核心和实际入口查询已通过隔离/UI 回归，真实 uv 恢复与真实命令生效待验证）**。下一步：在隔离 Windows 环境用真实 uv 核对恢复后 `uv tool dir --bin`、`uv tool list`、PATH 和新终端命令解析，再继续十管理器真实版本/配置；不得在真实用户环境执行。

- [x] **PATH-01-c / 隔离与 UI 回归通过**：`command_entry_policy` 明确 npm/.NET 根目录入口、Cargo/Composer/Yarn 固定子目录和 uv/pipx/Bun/pnpm 兄弟入口；`validate_layout` 校验命令入口必须与固定布局一致，缓存/配置不得嵌套。Cargo/Composer 默认缓存改为兄弟目录，避免角色模型自身制造嵌套。
- [x] **PATH-01-d / 只读检查通过**：`inspect_uv_layout` 读取 uv receipt、site-packages 元数据和入口，区分 `clean`、`legacy`、`legacy-partial`、`legacy-empty`；损坏/缺失 receipt、多个要求、自定义选项、直接来源和外部入口不标记为可自动恢复。包管理器页面显示命令入口和只读警告。
- [x] **PATH-01-e / 清理与迁移边界通过**：uv 清理只接受 receipt 声明的兄弟入口或旧 `root/bin` 入口；旧嵌套布局阻止迁移和旧包清理，避免把错误布局继续复制或通过卸载命令误删入口。
- [x] **PATH-01-f / PATH 状态区分通过**：应用路径后报告 `active`、`requires-new-terminal`、`current-process-only`、`duplicate`、`missing` 和读取失败；只证明目录写在 PATH 中，不冒充已解析具体包命令。
- [x] **PATH-01-g / 恢复执行核心与隔离回归通过**：`recover_uv_layout` 只接受用户确认过的只读预览；执行前重新检查布局、receipt 哈希和旧入口内容哈希，按 receipt 中的精确 `name==version` 运行 `uv tool install --force`，设置独立 `UV_TOOL_BIN_DIR`，成功后复查新 receipt/入口文件/版本，再按旧入口快照清理；共享缓存只在所有可恢复工具成功且无待人工工具时迁移，失败/残留/预览后变化保留旧数据并报告部分结果。新增 5 项隔离夹具（成功迁移、预览过期、命令失败保留、部分恢复保留共享缓存、启动器名称限制）。
- [x] **PATH-01-h / 扫描警告接入通过**：扫描结果会检查有效 `UV_TOOL_DIR`、受管 `globals/uv-global` 和 uv 默认工具目录；发现旧嵌套布局时加入扫描警告并标为部分结果，不再把该布局当作正常状态。新增 `scan_warns_about_legacy_uv_layouts_seen_in_the_effective_environment`。
- [x] **PATH-01-i / 实际入口查询接入通过**：`pkgtools::values` 现在对 pnpm 查询 `global-bin-dir`、Yarn Classic 查询 `global bin`、uv 查询 `tool dir --bin`、pipx 查询 `PIPX_BIN_DIR`、Bun 查询 `pm bin -g`、Composer 查询 `bin-dir`；查询失败或返回非绝对路径时回退到声明的布局，不把回退值冒充实际查询结果。`ToolConfig.commandPathSource` 与包管理器页面把入口标为“实际查询”或“布局推导”，新增 `values_include_actual_command_entries_for_pnpm_yarn_and_uv` 与 `command_entry_source_distinguishes_queried_and_layout_fallback` 隔离夹具。
- [x] **PATH-01-j / 命令解析优先级检查，隔离回归通过、实机待验证**：新增只读 `command_resolution`/`launcher_statuses`，按“系统 PATH 在前、用户 PATH 在后”的合并顺序解析新终端将命中的启动器，区分 `managed`（命中受管入口）、`shadowed`（被靠前安装覆盖）和 `missing`；不做任何 PATH 写入，也不执行启动器。`PATH` 中的 `%VAR%` 按环境变量展开，未知变量保留原文不静默丢弃；相对 PATH 条目一律不认作命中。`apply_tool_config` 结果新增“命令解析”和“受管入口内命令被 PATH 靠前位置覆盖”两条，不再只凭目录已追加就报告生效。隔离夹具 3 项：`command_resolution_detects_shadowed_managed_entry_and_missing_command`、`launcher_statuses_report_each_entry_and_skip_shadowed_ones`、`registry_path_order_and_variable_expansion_match_a_new_terminal`。证据 `docs/evidence/path-01/path-command-resolution-verification-2026-10-19.json`。
- [x] **PATH-01-k / Yarn 2+ 迁移能力门禁，隔离与 UI 回归通过、实机待验证**：新增 `migration::Capabilities`，由实际运行的管理器版本推导 `yarn_berry`；`preview`/`execute` 在触碰任何目录前拒绝 Berry 的 `global` 迁移，命令层用 `tool_version` 读取精确可执行文件版本而非静态 catalog。`ToolMigration` 在 Yarn 2+ 时隐藏“全局包目录”单选并说明 `globalFolder` 只是全局数据目录，只保留缓存迁移。新增 `yarn_berry_never_migrates_global_folder_as_a_classic_install_root`、`capabilities_recognize_classic_and_berry_versions`；`packages-ui` 增加 Berry 迁移场景断言。证据 `docs/evidence/path-01/path-yarn-capability-verification-2026-10-19.json`。
- [x] **PATH-01-o / 批量对话框受影响包展示，UI 回归通过**：一键批量预览的每个可写工具下按工具分组展示受影响包（名称、版本、“入口可能失效”标记与逐工具计数）；无可列包时显示原因说明而非留空。`packages-ui` 新增断言：批量对话框出现“受影响的全局包”、`eslint 9.1.0（入口可能失效）` 与“2 个可能不再解析”。
- [ ] 真实 uv 恢复（含 `uv tool list`、新终端命令解析）和十管理器真实版本/配置仍待验证；不得将本轮隔离结果视为 PATH-01 验收完成。命令解析结果尚未在真实注册表/新终端对照，也未与 `where.exe` 的实际优先级逐条比对；真实 Yarn Classic/Berry 安装未用于迁移验证。

验证命令：`cargo test --offline --lib -- --test-threads=1` → **262 passed / 0 failed / 14 ignored**；九套 UI（`node tests/run-ui.mjs <全局 playwright 模块> ...`，含 Yarn Berry 迁移、单工具/批量配置差异预览、批量受影响包清单、切换后命令诊断、PATH 修复预览与写入后核验、扫描入口版本冲突渲染、安装任务跨页恢复场景）全部 PASS；`npm run build` → 1709 modules / built in 695ms；`cargo check --offline` → 0 errors / 1 个既有 `path_ownership::preview_cleanup` dead-code 警告；`cargo fmt --check` 仍因缺 `cargo-fmt.exe` 无法执行。新增夹具：`command_entry_policy_matches_real_manager_layouts`、`uv_legacy_layout_is_read_only_and_classifies_recovery`、`uv_cleanup_accepts_declared_sibling_or_legacy_entry_only`、`refuses_to_migrate_a_legacy_uv_layout_as_is`、`path_integration_distinguishes_registry_and_current_process`、`uv_recovery::tests::*` 5 项、`scan_warns_about_legacy_uv_layouts_seen_in_the_effective_environment`、`values_include_actual_command_entries_for_pnpm_yarn_and_uv`、`command_entry_source_distinguishes_queried_and_layout_fallback`、`command_resolution_detects_shadowed_managed_entry_and_missing_command`、`launcher_statuses_report_each_entry_and_skip_shadowed_ones`、`registry_path_order_and_variable_expansion_match_a_new_terminal`。证据见 `docs/evidence/path-01/path-layout-verification-2026-10-19.json`；仅临时目录/模拟 UI，未写真实 PATH/注册表、未执行真实恢复/安装/卸载、未打包/发布。

### 2026-10-17 PATH-01 接续实现记录

接续负责人：Codex；目标版本：0.5.7～0.5.8；状态：进行中。下一步为 PATH-01 迁移增加管理根目录双向边界检查，补根内源/Junction 夹具；随后继续四角色旧布局恢复与迁移联动。保留 npm 前缀/命令入口重合与 Yarn `prefix/bin` 的适配语义，未接入全角色联动，必须继续列为未完成。

- [x] **PATH-01-a / 隔离验证通过**：修复配置角色数组类型错误、旧配置路径夹具及 Composer `vendor/bin` 父目录创建；npm 重装缓存改为兄弟目录。九种全局重装适配器初始化、已有配置保留/回滚及 npm 别名冲突夹具通过；完整库测试 148 passed / 13 ignored。
- [x] **PATH-01-b / 四角色模型隔离回归通过**：新增工具环境目录、命令入口目录、缓存/存储目录、配置目录的统一视图；公共配置入口及 `pkgtools::apply_tool_config` 均使用同一角色校验。catalog 中支持全局目录配置的管理器、npm 前缀入口和 Yarn `prefix/bin` 语义均有隔离夹具；pip/.NET 没有被误当成支持全局根目录配置。dotnet 重装缓存改为兄弟目录。另拒绝位于或别名指向管理根目录的迁移源。定向 `pkgtools` 6 passed、`package_managers` 4 passed，完整 verify 193 passed / 13 ignored，证据 `docs/evidence/path-01/path-role-verification-2026-10-17.json`。
- [ ] npm/.NET 的入口与根重合、Cargo/Composer 固定子目录的真实工具语义、旧布局恢复、迁移联动和新终端命令生效仍未完成；不将隔离模型回归宣称为路径设置/重装/迁移验收。

验证入口复核（关联 ASMT-06）：发现 `verify.mjs` 引用的 `tests/scan-ui.mjs` 仍被 `/tests/*` 忽略。已添加精确例外，使该必要测试不再被忽略；2026-10-19 又将新增的 `tests/packages-ui.mjs` 纳入版本控制并加入 `npm run verify`，避免验证命令依赖被忽略文件。当前 `tests/scan-ui.mjs` 仍未加入 Git 索引（本轮沙箱禁止写 `.git`），干净克隆验收和其余历史测试跟踪仍归 REL-01。

负责人：Codex；目标版本：0.5.7～0.5.8；状态：**进行中（提前实现，验收待 R0 关闭及路径全链路验证）**；下一步：补齐各管理器完整目录角色、旧布局恢复、迁移联动及清理来源记录；已有初始化/配置隔离夹具与完整回归通过。

- [x] uv/pipx/Bun 兄弟入口/缓存、pnpm store/bin、Yarn Classic prefix/bin 与独立配置目录夹具通过；配置生成和 Berry 请求提前拒绝通过回归。此子项仅覆盖已列管理器，不代表全部十管理器路径全链路完成。
- [x] 临时预览采用独占创建及 marker 所有权复查；重装初始化拒绝已有角色目录，配置冲突回滚本轮空目录；已有目录/Junction/替换目录与配置保留夹具通过。
- [ ] 旧布局恢复、uv 兄弟入口清理所有权、迁移联动和真实命令生效仍未完成；不得宣称路径设置/重装/迁移已验收。

### UNINST-01 完整卸载的边界与验收（建议目标 0.5.9）

用户于 2026-10-16 明确提出，希望 EnvCon 完整实现卸载。这里的“完整”定义为：**安全、可验证地移除所选 EnvCon 管理环境拥有的内容和由 EnvCon 明确创建的关联项**，不是扫描并删除机器上所有与该语言有关的数据。

- **EnvCon 管理目录安装**：若工具以 ZIP/便携目录形式安装，预览并删除所选环境目录；目录内该 Python 解释器的 `site-packages`、`Scripts` 等也属于该环境，随目录一并移除。通常通过此 Python 执行 `python -m pip install` 的包会落在该解释器环境中；不需要逐个调用 pip uninstall 才能移除目录内的包。
- **Python 用户级安装**：`pip install --user` 使用当前用户的独立安装位置，Windows 默认位于 `%APPDATA%\\Python`，但可通过 `PYTHONUSERBASE` 改变；不一定在 EnvCon 的环境目录。[pip 文档](https://pip.pypa.io/en/stable/user_guide/#user-installs) · [Python `site.USER_BASE`](https://docs.python.org/3/library/site.html#site.USER_BASE)。扫描/预览时单独列出并标明可能被多个 Python 共用的风险；默认保留。不得因为某个解释器能访问它，就推定其归该解释器所有。仅在能核实路径、列出影响并由用户逐项单独确认时，才提供清理选项。
- **外部虚拟环境、其他 Python、全局包和缓存**：不因为卸载选中的 Python 而自动删除。它们不一定由该环境所有；缓存另设可选清理，不作为环境卸载的隐含副作用。
- **安装器型工具**：若 Python/LLVM 等安装器在目标目录外登记卸载项或创建系统关联，优先只调用与所选精确安装路径匹配的原厂卸载入口；无法可靠确认归属时不猜测执行，显示“目录已移除 / 安装器残留待处理”等部分结果与手动指引。
- **外部纳管环境**：继续使用“移除管理”语义，只删除 EnvCon 建立的链接/记录，不删除原安装目录；不能把外部环境的 pip 包当作 EnvCon 所有。
- **PATH、环境变量和 Junction**：为后续新增的 EnvCon 写入建立明确来源记录；卸载前识别使用关系。只删除可证明由 EnvCon 添加且已无其他受管版本依赖的精确条目；若共享 `current/<type>` 仍被其他版本使用则保留。旧版本时期写入但没有来源记录的条目，必须作为候选项预览并由用户单独确认，不能冒称可自动识别。不得以重写整条 PATH 的方式清理，也不得删除用户自建的虚拟环境入口。
- **预览与验证**：执行前列出环境目录、包/脚本数量、当前链接、关联 PATH/注册项、外部保留项；遇到路径重定向、所有权不明或无法安全匹配的注册项应停止相应清理。执行后核验目录、链接和可识别的卸载登记，并逐项报告成功、保留、失败及原因。

验收夹具至少包含：环境内 pip 包、`--user` 包及自定义 `PYTHONUSERBASE`、外部 venv、共享 pip 缓存、两个并存 Python 版本、当前/非当前版本、外部链接纳管、安装器登记路径匹配/不匹配、PATH 中混有用户自定义条目、无历史 PATH 来源记录、文件占用和中途失败。任何不属于被选环境且不能证明由 EnvCon 创建的内容必须保留。

### 2026-10-17 R3 / UNINST-01 实现记录

接续负责人：Codex；目标版本：0.5.9；状态：进行中。下一步复核已有排序快照、修正 Python `Lib` 祖先链接的包归属预览，运行最新隔离测试；快照仅比较路径/类型/大小/修改时间，不证明文件内容完整性。此前 143 passed / 13 ignored 的完整回归早于最近 `snapshotHash` / `config_path` 修改，不作为最新代码通过证据。

- [x] **R3-c / 隔离验证通过**：元数据快照稳定性、同数量文件改名/内容大小变化拦截，以及 Python `Lib` Junction 不枚举/删除外部包。最新库测试 148 passed / 13 ignored，前端构建通过。
- [x] **R3-d / 隔离回归通过**：`current_junction_targets` 保留所选链接的入口身份；并存别名夹具验证删除非当前链接保留 current、删除当前链接才解除 current，外部目标一直保留。

负责人：Codex；目标版本：0.5.9；状态：**进行中（提前实现；当前受管范围预览/执行与隔离回归通过，完整卸载未完成）**；下一步：实现安装器精确登记匹配、PATH 来源追踪及剩余边界夹具。原厂卸载器、PATH 来源追踪与目录外所有权尚未实现完整链路；不得把受管范围移除说成完整卸载。

- [x] **R3-a / 隔离回归通过**：只读预览、Python 环境内包计数、保留边界及必填 `expectedPreview` 已接通；执行前重新预览，移除后逐项核验。最新卸载定向 12 passed；完整验证 149 passed / 13 ignored，八套 UI 通过，尚不代表真实完整卸载验收。
- [ ] **R3-b / 部分实现**：内部受管链接和外部纳管链接均只移除入口；文件占用返回部分失败。PATH 来源记录/清理已按 R3-e 接通，新增 Junction 来源按 R3-f、旧链接及历史 PATH 的逐项确认按 R3-g 接通；安装器精确登记适配已按 R3-h 完成只读/模拟核心，但生产原厂命令执行尚未接入；其他目录外资源仍保留或列为未验证。**2026-10-19 补充**：Python 用户级安装位置（`PYTHONUSERBASE` 或 `%APPDATA%\Python`）现由 `python_user_package_scope` 只读枚举，预览保留项会给出解析路径、分发包数量和多解释器共用提示；相对路径/无法解析/读取失败分别报告原因，不伪造零。用户级包仍不可删除，仅改善报告。证据 `docs/evidence/path-01/path-batch-packages-and-python-user-scope-verification-2026-10-19.json`。
- [ ] **R3-e / 代码与隔离回归通过，实机待验证**：负责人 Codex；目标版本 0.5.9。PATH 集成/手工保存/备份恢复的来源日志、卸载只读预览、逐类型共享依赖判定和执行结果报告已接通。6 项新增隔离测试通过：循环覆盖 16 类固定入口、用户/共享条目保留、陈旧预览阻止、来源失败降级、目录失败不清理 PATH，以及写失败/复查失败/写前新依赖的部分结果；另有 8 项日志核心夹具。完整 verify：164 passed / 13 ignored、构建、两套读状态和八套 UI 通过；证据 `docs/evidence/uninstall/uninstall-path-verification-2026-10-17.json`。下一步补安装器与 Junction 来源边界，真实 PATH/注册表未操作，实机验收仍待验证。
- [ ] **R3-f / Junction 来源追踪，代码及隔离回归通过、实机待验证**：负责人 Codex；目标版本 0.5.9；提前实现、验收待 R0 关闭。新增切换/纳管链接记录创建来源、目标入口与 Windows 文件身份，卸载预览绑定指纹，删除前核对打开的链接对象。原核心 8 项、卸载定向 20 项通过；历史无来源链接单独确认已按 R3-g 接通，损坏/未完成收据及身份不匹配仍阻止。下一步按阶段安排实机验收；本地收据不构成防篡改来源认证。
- R3-f 核心定向验证：`cargo test --offline --lib link_ownership::tests -- --test-threads=1` → 5 passed / 0 failed / 177 filtered out。通过临时目录核验对象身份、同目标替换拒绝、来源缺失/损坏/pending/重定向拒绝与提交失败保留；此时尚未接入切换/纳管/卸载生产调用，下一步继续接入。官方 API 网页检索未返回内容，未作为成功证据；本地 windows-sys 绑定和隔离测试为当前实现依据。
- R3-f 接入验证：新增切换/纳管来源记录，卸载预览绑定当前链接和管理链接对象身份，执行时复查后按打开的链接对象移除；`cargo test --offline --lib commands::tests::uninstall -- --test-threads=1` → 20 passed / 0 failed / 164 filtered out。当前状态：**部分实现，完整回归待验证**；下一步跑完整 verify。历史无来源链接默认阻止移除，其单独确认流程未实现，不能关闭该子项。编辑夹具时一次 Node 命令因 PowerShell 引号转义失败，未写入文件，改用 apply_patch 完成。
- R3-f 首次完整回归：构建通过，Rust **172 passed / 1 failed / 13 ignored**；失败于 `locked_link_and_writer_lock_prevent_mutation` 的占用期间重新读取指纹（PermissionDenied），UI 尚未执行。状态继续进行中；下一步核对共享模式、实际移除结果与记录状态，保留失败日志 `tests/tooling/junction-ownership-verify-failed-1.txt`，不把此轮声明为通过。
- R3-f 故障定位：元数据专用句柄未阻止删除请求，但延迟入口消失；实际返回“不能确认成功”，收据保持 removing。改用读取句柄验证禁止删除，另加 `pending_delete_is_not_reported_as_verified_removal` 验证延迟删除不误报成功；定向 **8 passed / 0 failed / 179 filtered out**。保留首次失败记录，下一步重跑完整 verify。
- R3-f 最终回归：完整 `npm run verify` → **174 passed / 0 failed / 13 ignored**，两套读状态与八套 UI PASS；单独 `npm run build` → 1708 modules / built in 676ms；`cargo check --offline` → dev profile 成功（4.36s）；`git diff --check` 退出 0。完整输出、首次失败输出、夹具名与源码 SHA-256 见 `docs/evidence/uninstall/junction-ownership-verification-2026-10-17.json`。仅临时目录及 UI 模拟，未操作真实用户 PATH/注册表/工具安装卸载；fmt 本轮未执行（前轮确认缺 cargo-fmt.exe），未打包/推送/发布。
- [ ] **R3-g / 历史链接逐项确认，代码及隔离回归通过、实机待验证**：负责人 Codex；目标版本 0.5.9；提前实现、验收待 R0 关闭。只读预览单独列出无来源记录但身份可核验的固定管理/current 链接，用户逐项勾选才允许移除；执行重新核对预览和对象身份，记录“用户确认移除”而非“EnvCon 创建”。损坏/未完成/身份不符的既有收据继续阻止。完整 verify 178 passed / 13 ignored、八套 UI 通过，最终文案/窄屏增量的 build/UI 也通过；证据 `docs/evidence/uninstall/legacy-link-verification-2026-10-17.json`。下一步继续原厂卸载器匹配及历史 PATH 边界；仅隔离夹具及 UI 模拟，不操作真实环境。
- R3-g 定向验证：候选列表、逐项确认、执行时重验与 confirmed-removing/confirmed-removed 审计状态已接入；`cargo test --offline --lib legacy -- --test-threads=1` → **6 passed / 0 failed / 185 filtered out**（含 5 项相关链接测试及 1 项既有配置迁移测试）。覆盖未确认/漏选/重复/夹带路径拒绝、同目标替换拒绝、损坏/pending 收据不能绕过、外部数据保留及 PATH 清理联动。UI 勾选/取消重置场景已写入，完整 verify 待执行。历史 PATH 的单独确认仍未实现，当前继续默认保留。
- R3-g 完整验证：`npm run verify` → **178 passed / 0 failed / 13 ignored**，两套读状态和八套 UI PASS；单独 `npm run build` → 1708 modules / built in 673ms，`cargo check --offline` → dev profile 成功（3.07s）。之后仅新增确认面板 390px 窄屏断言及截图，定向 UI 待运行；业务源码保持本次通过版本。历史 PATH 及原厂卸载等未完成项继续保留。
- R3-g 最终前端复核：390px 确认面板无横向溢出、操作按钮可见，勾选取消及重新打开后重置通过；截图发现旧“不清理 PATH”提示，已修正文案。随后 `npm run build` → built in 686ms，`node tests/run-ui.mjs <运行时 Playwright> tests/release-052-ui.mjs` → PASS。Rust 在完整 verify 后未修改；完整输出、夹具名、最终源码哈希归档 `docs/evidence/uninstall/legacy-link-verification-2026-10-17.json`。fmt 未执行（缺组件）；无真实环境写入、打包或发布。
- [ ] **R3-h / 原厂卸载生产执行链，代码及隔离/UI 回归通过、实机待验证**：负责人 Codex；目标版本 0.5.9；提前实现、验收待 R0 关闭。只读登记采集、Python/LLVM 精确入口匹配、命令约束和模拟执行核心已有回归；本轮已将 `ready` 计划接入生产执行器：按固定参数直接启动 EXE（不经过 shell，超时终止进程树），执行前重新核对登记指纹和文件哈希；先解除 EnvCon 拥有的 current Junction，执行后同时复查登记和运行时目录。命令失败、退出码 3010、登记/目录残留、复查不可用及 PATH 写失败均报告部分结果；绝不回退到递归目录删除，PATH 只在原厂范围核验成功后清理。隔离夹具覆盖成功、命令失败、登记残留、运行时残留、预检变化、Junction 失败和 PATH 失败；真实卸载器仍未实测，不能视为完整卸载。
- R3-h 首次核心验证：`cargo test --offline --lib installer_uninstall::tests -- --test-threads=1` → **7 passed / 0 failed / 191 filtered out**。之后补齐组件登记过滤、冲突登记优先阻止、改名登记残留复查，并将只读审计预览接入产品；生产卸载器执行尚未启用，匹配/冲突/不可读登记会阻止直接删除目录以保留入口。HKCU Software 共享视图只读取一次，HKLM 读取 32/64 视图；此生产只读适配器未在真实用户注册表执行。最新增量待回归，下一步运行定向与完整 verify。
- R3-h 接入定向验证：`cargo test --offline --lib installer -- --test-threads=1` → **15 passed / 0 failed / 186 filtered out**（9 项新适配核心、1 项新命令预览、5 项既有安装器相关测试）。新增 UI 场景检查已匹配入口/参数可见、删除禁用及取消不执行；完整 verify 待运行。当前实现范围是只读预览和模拟执行核心；原厂命令启动、残留目录/包清理、PATH/Junction 顺序及后续产品结果联动仍待接入，不宣称安装器卸载完成。
- R3-h 本增量完整回归：`npm run verify` → **188 passed / 0 failed / 13 ignored**，两套读状态与八套 UI PASS；`npm run build` → 1708 modules / built in 695ms；`cargo check --offline` → dev profile 成功（4.59s），但有 4 条 dead_code 警告（Backend/Execution/registration_absence/execute 尚未接入生产执行链），未隐藏警告。证据 `docs/evidence/uninstall/installer-audit-verification-2026-10-17.json`。fmt 未执行（前轮确认缺组件），无真实环境操作/打包/推送/发布；R3-h 仍进行中。
- R3-h 生产执行链定向验证：`cargo test --offline --lib installer -- --test-threads=1` → **21 passed / 0 failed / 203 filtered out**；新增成功、命令失败、登记残留、运行时残留、预检变化、Junction 失败和 PATH 写失败夹具，并拒绝 `ready` 计划回退到递归目录删除。另跑 `reinstall_process::tests::blocking_timeout_terminates_descendant_processes` → 1 passed，验证生产同步执行器的超时进程树终止。
- R3-h 本增量完整回归：`npm run verify` → **212 passed / 0 failed / 13 ignored**，1708 modules 前端构建、两套读状态和八套 UI PASS；`cargo check --offline` → 0 errors / 1 个既有 dead-code 警告；`git diff --check` 退出 0；`cargo fmt --check` 无法执行（缺 `cargo-fmt.exe`）。只使用临时目录、模拟注册表和模拟执行器，未运行真实卸载器、未写真实 PATH/注册表、未打包/推送/发布。证据 `docs/evidence/uninstall/installer-uninstall-verification-2026-10-19.json`。真实安装器执行、Windows VM 验收和目录外副作用仍未验证，不能标记 R3-h 完成。
- R3-e 执行记录：完整 `npm run verify` 为 158 passed / 0 failed / 13 ignored；TypeScript/Vite、两套读状态和八套 UI 通过，实际完整输出与源码哈希见 `docs/evidence/path-01/path-ownership-verification-2026-10-17.json`。接续状态：**进行中**，负责人 Codex，目标 0.5.9；下一步将来源计划接入卸载只读预览和可注入执行器，覆盖共享版本、陈旧预览及部分失败，不操作真实注册表。
- R3-e 接入定向验证：`cargo test --offline --lib commands::tests::uninstall_path -- --test-threads=1` → 6 passed / 0 failed / 171 filtered out。首次编译因数组迭代模式多写 `&` 失败，修正后通过；尚未执行此增量的完整回归。仅对所选类型固定 current 入口核验，系统 PATH、其他变量及外部别名仍默认保留；日志和多次复查不构成与外部进程之间的原子事务。
- R3-e 最终接入验证：完整 `npm run verify` → 164 passed / 0 failed / 13 ignored；新增 PATH 预览/部分失败 UI 场景与其余七套 UI 均 PASS。单独 `npm run build` → 1708 modules / built in 701ms；`cargo check --offline` → dev profile 成功（37.97s）；`git diff --check` 无输出、退出 0。完整输出及文件哈希见 `docs/evidence/uninstall/uninstall-path-verification-2026-10-17.json`。本增量未重跑 fmt：前轮已确认缺 cargo-fmt.exe，未安装组件。下一步仍是 R3 原厂登记/Junction 来源，阶段不关闭。
- 本轮先前完整测试曾为 134 passed / 2 failed / 13 ignored：全局 `paths_eq` 规范化改变了拒绝重定向的语义，已修复直接 Junction 拒绝；祖先重定向仍在复核。随后一次完整测试为 136 passed / 13 ignored，但其后临时目录所有权实现继续变化，此结果不替代最终回归。

### 2026-10-17 R2 / SCAN-CORE-01 实现记录

负责人：Codex；目标版本：0.5.8；状态：**实现与隔离回归通过，待验收（提前实现，验收待 R0/R1 关闭及 Windows 实机一致性核对）**；下一步：保持 R0/R1 外部验收待办，之后按 R3 先做只读卸载范围预览。

- [x] **R2-a / 隔离回归通过**：扫描结果区分未扫描、范围内未发现、可用、权限不足、无法启动、命令失败、解析失败和超时；不把“未在扫描范围发现”表述为机器绝对未安装。
- [x] **R2-b / 隔离回归通过**：按规范化安装身份合并核心工具同一安装的重复入口，不合并不同安装；保留入口路径/版本/来源。
- [x] **R2-c / 隔离回归通过**：Rust 扫描夹具及扫描 UI 覆盖状态、身份合并、不同安装和失败原因；空结果仍可使用分类筛选。
- [x] **R2-d / 同安装入口版本不一致，隔离回归通过、实机待验证**：此前同一安装只保留一条记录，另一个入口即使报告不同版本也被静默丢弃。现在每个重复入口都会探测，差异写入 `ExternalEnv.versionConflicts`（入口路径 + 版本或探测错误），`scan_discovered` 为受影响工具生成一条警告（最多列 3 个入口并给出总数），结果状态降为 `Partial`，不再把扫描呈现为完全一致。新增 2 项夹具：`same_installation_entrypoints_with_different_versions_are_reported`、`identical_versions_across_entrypoints_produce_no_conflict`。证据 `docs/evidence/r2/r2-scan-entry-version-conflicts-verification-2026-10-19.json`。
- [x] **R2-f / 冲突数据 UI 渲染与窄屏，UI 回归通过**：`ScanResults` 在受影响条目内展示“同一安装的其他入口版本不同（N 个）”，逐项列出冲突入口路径与“报告 <版本>”或“无法读取版本（<错误>）”，并说明已保留一条安装记录；折叠状态下分组标题显示“N 个入口版本不一致”。路径使用 `break-all` 换行，390px 无横向溢出。`scan-ui` 新增断言（含版本与错误两种条目）并保存 `tests/scan-conflicts-390.png`。证据 `docs/evidence/r2/r2-scan-conflict-ui-verification-2026-10-19.json`。
- [x] **R2-g / 扫描 IO 失败的结构化分类，隔离回归通过、实机待验证**：包扫描此前把每个 IO 失败都写成中文自由文本，调用方无法区分“权限不足”“路径已消失”“包清单读取失败”，也没有统计扫描范围中有多少位置不可访问。现新增 `ScanProblem{kind,path,detail}` 与 `permissionDenied` 计数，kind 取 `permission-denied`/`not-found`/`unreadable`/`entry-unreadable`/`package-list-failed`/`link-skipped`/`depth-limit`/`limit-reached`；`classify_io_kind` 按 `ErrorKind` 判定，不解析错误文案。结构化项与自由文本由同一事件同时写入，两种视图不会互相矛盾；`permissionDenied` 只统计权限拒绝，不由告警数推断；两者同受 `MAX_PROBLEMS`(80) 上限约束而 `warningCount` 继续累加。旧包扫描页在 `permissionDenied>0` 时明确提示“这些位置未被扫描，不代表其中没有包”。新增 2 项夹具：`io_failures_are_classified_instead_of_only_free_text`、`permission_denied_is_counted_separately_and_problems_stay_bounded`。证据 `docs/evidence/r2/r2-scan-io-classification-verification-2026-10-19.json`。**未验证**：本机未扫描真实的 ACL 拒绝目录（权限分支由构造的 `PermissionDenied` 错误证明）；`tests/package-scan-ui.mjs` 被 `.gitignore` 忽略且在第 61 行既有断言处失败，故新提示尚无 UI 断言门禁。
- 验证：`cargo test --offline --lib detect::system::tests -- --test-threads=1`：16 passed / 1 ignored / 116 filtered out；`npm run verify`：TypeScript/Vite 构建、Rust 120 passed / 13 ignored、两套读状态与八套 UI 均通过；Playwright 使用 Codex 运行时模块。阶段门槛仍待 R0/R1 与 Windows 实机一致性验证；未打包/发布。
- R2-g 验证（2026-10-19）：`cargo test --offline --lib package_scan::tests -- --test-threads=1` → **4 passed / 0 failed / 1 ignored**；完整 `cargo test --offline --lib -- --test-threads=1` → **266 passed / 0 failed / 14 ignored**；`npm run build` → 1709 modules / built in 705ms；九套受跟踪 UI 全部 PASS；`git diff --check` 无空白错误。仅使用临时目录与构造的 `PermissionDenied` 错误；本机未扫描真实 ACL 拒绝目录，`tests/package-scan-ui.mjs` 因被忽略且存在既有失败断言而不作为门禁。证据 `docs/evidence/r2/r2-scan-io-classification-verification-2026-10-19.json`。

### 2026-10-17 R1 / DL-VERIFY-01 实现记录

负责人：Codex；目标版本：0.5.7；状态：**实现与隔离测试已通过，待验收（提前实现，验收待 R0 关闭）**；下一步：R0 门槛仅剩隔离 Windows 安装冒烟；官方校验资料与镜像差异已于 2026-10-19 核实（6/6 镜像逐字节一致，Rust/PHP 校验文件已接入）。R1 不提前关闭 R0，也不把本地哈希或隔离下载当作官方来源真实性证明。

- [x] **R1-a / 已验证**：`DownloadSpec` 携带校验状态/预期长度；下载提交前校验响应长度与可信 SHA-256，安装前重新核对磁盘文件，失败不会提交或进入安装。
- [x] **R1-b / 已实现、来源核验待验证**：版本元数据接入当前可取得的官方哈希/校验文件；列表缓存不能被后端当作当前来源证据，镜像无可信资料标为“未验证”，无效资料阻止执行。**2026-10-19 在线复测**：16 类中有 12 类取得官方 SHA-256，全部与本地下载一致——JDK、Python、Node、Go、Gradle、LLVM、Zig、Deno、Bun、Git、GitHub CLI、MinGW。**同日追加**：Rust 官方 `rustup-init.exe.sha256` 与 PHP `sha256sum.txt` 也已核实并与本地一致，现已接入 `rust_channels()` 与 `parse_php_releases()`；Maven(Apache SHA-512)、ADB(Google SHA-1) 因校验器只接受 SHA-256 而标注具体原因。修复了 `download_spec_with_verification` 无条件覆盖分支校验资料的缺陷（镜像 URL 不会再继承官方摘要）。证据 `docs/evidence/r0/r0-asset-audit-2026-10-19.json`、`docs/evidence/r0/r0-mirror-and-publisher-checksums-verification-2026-10-19.json`。
- [x] **R1-c / 隔离夹具已通过**：覆盖取消/中断、长度不符、篡改、哈希不符、校验资料缺失/无效、未验证镜像和安装前文件变化；最新完整回归见 2026-10-17 执行记录。
- [x] **R1-d / UI 已实现并回归**：下载页区分校验依据与实际已验证状态；安装任务不把来源有哈希误标成文件已核验。
- [x] **R1-f / 校验文件与资产替换边界，隔离回归通过**：`parse_sha256_checksum_file` 现在区分“校验文件列出了其他资产”与“文件无可解析条目”，前者报告条目数量并提示版本或资产名可能已变化；后者保留原文案。新增 2 项夹具：`checksum_file_listing_other_assets_is_not_a_silent_match` 锁定两类错误语义，`offline_cache_never_keeps_a_trusted_digest_for_a_replaced_asset` 锁定离线缓存必须把可信哈希降级为 `Unverified`。复核确认 `resolve_download_spec` 每次安装都重新拉取并解析校验文件，不存在复用旧哈希的生产路径。证据 `docs/evidence/r1/r1-checksum-and-config-injection-verification-2026-10-19.json`。
- [x] **R1-g / 配置路径写入边界，隔离回归通过**：`validate_tool_config_layout` 拒绝含分号、引号、空字符、CR/LF 的配置目录，理由是这些值会被写入管理器配置文件而不是仅作命令行参数；新增 `config_paths_cannot_break_out_of_manager_config_files` 覆盖 5 类注入字符及相对/父级路径。证据同上。
- [x] **R1-h / 外部命令审计复核（只读）**：已复核 14 处 `Command::new`，均使用结构化参数且不经过 shell；`installer_uninstall` 已有拒绝 `cmd.exe /c`、`msiexec /x {guid}` 与引号拼接的夹具，`node.rs` 的包名/版本白名单仍在位。本轮未新增代码，仅记录审计结论；真实安装器执行仍待实机验证。
- [x] **R1-e / 2026-10-19 资产审计解析路径修正（隔离测试通过，在线审计未执行）**：负责人 Codex；目标版本 0.5.7。`resolve_download_spec` 统一经 `resolve_verification` 解析 `ChecksumFile`，拒绝非 HTTPS 校验文件地址；忽略型 `live_download_asset_audit` 改用同一生产解析路径，并记录官方 SHA-256 是否与本地下载一致、校验元数据及解析失败原因，不再绕过校验文件解析。新增 `verification_resolution_keeps_trusted_metadata_and_rejects_insecure_checksum_files`。验证：定向 1 passed；`sources::tests` 16 passed / 3 ignored。在线资产审计本轮未执行（环境网络受限，未重复进行 16 类大文件下载）；既有 `docs/evidence/r0/r0-asset-audit-2026-10-17.json` 不包含本修正后的校验文件解析路径，不能据此宣称官方校验资料验收完成。

### 2026-10-16 全工具核查（AUD-016）

负责人：Codex；目标版本：0.5.6（建议）；状态：部分完成、待交付。已核查 16 类环境的版本列表、下载资产和安装/卸载边界，以及十种包管理器的版本显示和自更新路径；工作区源码与隔离回归已通过，但完整下载和隔离 Windows 安装冒烟尚未完成，不执行用户真实环境安装、更新或卸载。

- [x] 完成 16 类工具的静态代码核对及首选资产在线 HEAD 检查；分别记录隔离夹具、来源响应和未执行的实机验证，不以生成下载 URL 代替成功下载证明。正常权限下 10 类非 GitHub 源及其首选资产 HEAD 返回 200；六类 GitHub 工具使用官方 API 单次取清单后，官方/ghfast 镜像首选资产 HEAD 均返回 200。Rust 全源串行在线测试曾因 GitHub API 未认证限流失败，单次只读复核另行完成。HEAD 不证明文件内容、哈希、安装或卸载成功。
- [x] 修复已复现的 Python 版本日期误显，区分源目录修改时间与正式发布日期，并回归旧缓存显示。Python 官方 API 仅列稳定 Windows x64 安装器，旧缓存会过滤无对应安装器的记录；下载 UI 回归覆盖“年份+时间”旧值显示“日期未知”。
- [x] 展示十种包管理器已检测到的当前版本和精确可执行路径，区分不支持自更新、命令成功但版本未变、版本变化及执行失败；pipx/Cargo/.NET 不提供通用自更新。该版本显示不代表官方最新版本；本轮未在真实用户环境执行更新。
- [ ] **官方默认资产下载已实测，R0 资产验收仍待完成**：16 类官方默认资产均完整下载到隔离临时目录，Content-Length、版本目录大小、本地 SHA-256 与 ZIP/PE 结构检查通过。**2026-10-19 重新实测**：在线审计成功执行（381.83 秒），16/16 完整下载且长度一致；**12/16 有发布方 SHA-256 且全部与本地一致**。**2026-10-19 追加**：6 类双源资产的镜像文件与官方文件逐字节一致（6/6）；原记“无校验资料”的 4 类经复查均存在发布方校验文件——Rust `.sha256` 与 PHP `sha256sum.txt` 均为 SHA-256 且与本地一致，已接入下载校验；Maven 仅 SHA-512、ADB 仅 SHA-1，校验器只接受 SHA-256，故明确标注原因并保持“未验证”。证据见 `docs/evidence/r0/r0-asset-audit-2026-10-19.json` 与 `docs/evidence/r0/r0-mirror-and-publisher-checksums-verification-2026-10-19.json`（含逐项版本、URL、字节数、结构与哈希）。**未完成**：Windows VM 安装冒烟（**2026-10-20 已解除**：改用一次性 Windows Sandbox，以 `WDAGUtilityAccount` 受限账户、禁用网络、禁用 vGPU 的干净 Windows 11 Enterprise 10.0.26100 中完成静默安装、启动、主窗口创建、Junction 语义、静默卸载与注册表清理，10/10 通过，证据 `docs/evidence/r0/r0-sandbox-install-smoke-verification-2026-10-20.json`）；因此 R0 的安装冒烟阻塞已消除，但十种管理器真实版本/自更新仍为 0/10，R0 不整体关闭。
- [x] **R0-d4 / 十种包管理器适用性矩阵（离线部分）**：记录每个管理器的 catalog 能力（globalConfig/cacheConfig/migrateGlobal/selfUpdate）、覆盖它的隔离夹具、隔离结果与真实环境跳过原因。10/10 隔离夹具通过（`package_managers::` 16 passed）；真实版本读取 0/10、真实自更新 0/10，其中 pipx/Cargo/.NET 明确为无通用自更新。证据见 `docs/evidence/r0/r0-package-manager-matrix-2026-10-19.json`。**未完成**：真实管理器版本与自更新结果仍待隔离 Windows 环境。
- [ ] 将 UNINST-01 从 Python 例子扩展至全部 16 类（压缩包/便携包、安装器、rustup、外部链接与目录外关联项），给出验证边界与下一步。当前仅完成边界定义和准确提示；实现仍是删除受管目录/移除外部链接，未调用原厂卸载器，也未清理或核验目录外注册项、共享 PATH、用户包和缓存。（**2026-10-22 范围变化**：.NET SDK 已成为第 17 类，同样未纳入本条的卸载覆盖范围；其卸载目前走通用「删除受管目录」，未针对 `dotnet.exe` + `sdk/` 布局做专项校验。）

### 2026-10-16 AUD-016 复核与 2026-10-17 实现记录

本轮源码审查新增的 R0 项（负责人：Codex；目标版本：0.5.6；状态：进行中；下一步依 a→d 实现并验证）：

- [x] **R0-a / 已验证、待 R0 总验收**：`src-tauri/src/install.rs` 按安装方式区分失败恢复；PortableGit 失败目录移至 `.failed-*` 并允许同名重试，Python/LLVM 保留目标目录；`failed_portable_git_install_quarantines_files_and_allows_same_name_retry`、`failed_registered_installer_keeps_its_original_target` 通过。
- [x] **R0-b / 已验证、待 R0 总验收**：Python 官方列表按真实 Windows x64 安装器资产筛选；华为云镜像独立解析 `3.x.y` 目录和精确 `python-{version}-amd64.exe`，官方 API 仅补发布日期且失败时保留镜像资产；镜像解析/官方日期失败夹具通过。
- [x] **R0-c / 已验证、待 R0 总验收**：`update_package_manager` 拆分命令失败、复查失败、版本变化、版本未变化和版本未知；十管理器结构化故障夹具及 UI 场景通过；未执行真实全局更新。
- [x] **R0-d / 已验证、待 R0 总验收**：`tests/package-manager-update-ui.mjs` 已纳入跟踪，Python 下载页来源文案已更新；使用 Codex 运行时 Playwright 模块完成完整 `npm run verify`。

- 已撤回未完成的 LLVM MSI 执行分支。当前只列出并接受官方 `LLVM-<版本>-win64.exe`；LLVM 23.1.x 的 `win64.msi` 会被明确排除并在旧 EXE 条目提示“新版 MSI 暂不支持”，避免把未知 MSI 属性误当成可安装。
- 下载流新增 `Content-Length` 与实际字节数一致性检查；ZIP 继续在写入前拒绝越界路径、链接祖先和不安全文件名。
- 通用 ZIP 安装后的检查目前只验证必要文件结构；除 ADB 外不执行下载压缩包中的二进制来伪造“版本已验证”。安装器路径仍会在退出后做有限结构/版本检查，可信哈希/签名校验列入当前 R1（0.5.7），不能因此宣称完整下载验证。
- 环境管理页将普通受管环境的动作改称“删除受管目录/删除目录”，并明确安装器记录、目录外数据、用户级包、缓存和共享 PATH 可能保留；外部链接继续只提供“移除管理”。
- 验证证据：Rust `cargo test --lib` 101 passed / 10 ignored；`npm run build` 通过；已跟踪 UI 回归（设置、启动、卸载/纳管、下载、扫描、大小）通过；新增包管理器版本/更新状态 UI 回归通过。完整 `npm run verify` 因工作区未安装名为 `playwright` 的本地模块失败，改用 Codex 运行时 Playwright 重跑通过。

能力矩阵口径：所有行均做过**静态代码核对**；“200”指 2026-10-16 当前列表首选 URL 的官方或镜像 **HTTP HEAD**，不是文件下载/哈希校验/能运行的证据。单项隔离夹具见对应说明；**16 类均未执行本轮实机安装与卸载**。全部受管普通目录使用同一目录删除流程；外部纳管不论类别都只移除链接，不动原安装。

| 工具 | 下载与当前首选 URL | 安装后可证实的程度 | 现有移除与已知边界 |
| --- | --- | --- | --- |
| JDK | Adoptium Windows x64 ZIP（官方/清华 HEAD 200） | 文件结构；版本仅在后续扫描时探测 | 删除受管目录；系统 JAVA_HOME/PATH 不自动核验或清理 |
| Python | 官方发布 API 严格匹配稳定 Windows x64 EXE（官方/华为 HEAD 200） | 安装器退出后核对目标目录与解释器版本；未做真实安装 | 只删受管目录；原厂卸载记录、`--user` 包及缓存可能保留 |
| Node.js | Windows x64 ZIP（官方/npmmirror HEAD 200） | 文件结构，解压前不执行未校验二进制 | 删除受管目录；全局包若在目录内随目录删除，外置目录不动 |
| Go | Windows amd64 ZIP（官方/镜像 HEAD 200） | 文件结构 | 删除受管目录；外置 GOPATH/缓存不动 |
| Rust | rustup-init EXE（官方/rsproxy HEAD 200），渠道组件尚未核对 | 完成后检查隔离工具链可被探测，不等于全程包校验 | 删除受管 `cargo-home`/`rustup-home` 目录；外置工具链仅移除链接 |
| Maven | 3.x Windows 可用 ZIP（官方/华为 HEAD 200） | 文件结构 | 删除受管目录；用户仓库不动 |
| Gradle | Windows 可用 ZIP（官方/腾讯 HEAD 200） | 文件结构 | 删除受管目录；用户 Gradle 缓存不动 |
| PHP | 官方 Thread Safe x64 ZIP（HEAD 200） | 文件结构，不证明 VC++ 运行库齐备 | 删除受管目录 |
| LLVM | 最近可列 EXE 22.1.8（官方/ghfast HEAD 200）；23.1.x 仅 MSI 暂不支持 | EXE 安装器退出后核对 clang 版本；MSI 不运行 | 只删受管目录；原厂卸载记录可能保留 |
| Zig | 官方清单 Windows x64 ZIP 直链（HEAD 200） | 文件结构 | 删除受管目录 |
| Deno | GitHub Windows x64 ZIP（官方/ghfast HEAD 200） | 文件结构 | 删除受管目录；外置用户数据不动 |
| Bun | GitHub Windows x64 ZIP（官方/ghfast HEAD 200） | 文件结构 | 删除受管目录；外置用户数据不动 |
| Git | PortableGit 自解压 EXE（官方/ghfast HEAD 200） | 解压程序退出后检查 git 版本；并非注册式 Git 安装器 | 删除受管目录，不代替其它独立 Git 安装的卸载 |
| GitHub CLI | GitHub Windows amd64 ZIP（官方/ghfast HEAD 200） | 文件结构 | 删除受管目录；用户登录数据不动 |
| MinGW-w64 | WinLibs x64 UCRT ZIP（官方/ghfast HEAD 200） | 文件结构；复合发行标签不等于 gcc 单独版本 | 删除受管目录；非完整 MSVC/Windows SDK |
| ADB | Google 稳定版 Windows Platform Tools ZIP（HEAD 200） | ADB/Fastboot/运行库与 `source.properties` 版本夹具通过 | 删除受管 platform-tools；已有 SDK 纳管仅删链接；ADB server 占用需另行处理 |

包管理器版本均从当前选中的准确可执行路径读取 `--version`；下列是**更新管理器自身**而非升级全局包。本轮没有在真实用户环境执行更新。任何命令成功、版本未变化都不能证明已是官方最新。

| 管理器 | 现有自更新路径 | 已知限制 |
| --- | --- | --- |
| npm | 核实原安装目录后运行该目录 npm 的全局更新 | Corepack/其他来源无法确认时拒绝 |
| pnpm | `pnpm self-update` | 原安装方式可能不支持，需看命令错误与前后版本 |
| Yarn Classic | 核实原 npm 目录后更新 Yarn 包 | Berry/PnP 在项目中更新，应用内不走此路径 |
| pip | 查找所属 Python，以 `python -m pip` 更新 | 所属解释器验证失败时拒绝；系统权限/安装来源仍可能阻止更新 |
| Bun | `bun upgrade` | 依赖 Bun 自身与安装来源支持 |
| uv Tools | `uv self update` | 外部包管理器安装的 uv 可能需原方式更新 |
| Composer | `composer self-update` | 安装方式或权限不支持时以命令错误为准 |
| pipx | 无通用自更新 | 通过所属 Python 或原安装方式更新 |
| Cargo | 无通用自更新 | 随 Rust 工具链更新 |
| .NET CLI | 无通用自更新 | 随 .NET SDK 更新，`.NET Tools` 能力不等于 SDK 管理 |

### 2026-10-15 评估关联与产品建议

本轮评估负责人：Codex；评估状态：已完成；功能修复目标版本：待排期，实施负责人沿用总览或待认领。证据与范围见 [产品与可靠性评估](PRODUCT_ASSESSMENT.md)。本轮仅新增发现和建议，没有实施这些修复，也未打包或发布；长期完成数仍为 1 / 10。

| 发现 ID | 关联方向 | 实际影响 / 下一步 |
| --- | --- | --- |
| ASMT-01 | IMP-09、05 | Python 顶层版本目录不保证 Windows 安装器存在；按真实资产、平台和安装方法筛选并增加边界测试 |
| ASMT-02 | IMP-08 | DownloadSpec 与下载提交链路没有可信哈希/签名校验；在执行或解压前核对可信依据 |
| ASMT-03 | IMP-09、05、06 | 解压后结构/版本校验仅覆盖 ADB；定义通用验证能力，校正“已就绪”状态 |
| ASMT-04 | IMP-04、07、06 | 追加用户 PATH 不保证实际命令优先解析到受管版本；扩展现有诊断并提供可预览修复（2026-10-19：只读切换诊断、`reorder`/`manual` 修复预览与写入后重新诊断闭环均已实现并通过隔离/UI 回归；真实新终端验证与真实用户 PATH 写入仍未完成） |
| ASMT-05 | IMP-07、06 | 一键路径配置无统一差异与包影响预览；明确改路径、迁移、重装的区别并准备恢复 |
| ASMT-06 | IMP-05、10 | 必要文档、历史包 UI 与 Rust 集成测试有未跟踪项，verify 覆盖不全；补版本控制、CI 和实机发布矩阵 |

产品范围更新：用户已确认下载与扫描定位，撤下此前项目版本声明解析、隔离终端和项目启动试点建议。现有项目扫描和依赖查询保留。下一步按下载/扫描核心流程安排可靠性修复，不以实现完整项目环境为验收条件。现有切换、PATH 和包管理能力保留；其复杂增强按实际反馈排期，不作为当前核心建设任务。历史建议及依据见 PRODUCT_ASSESSMENT.md 的范围外章节。

### 0.5.2 小版本修复（2026-09）

| ID | 需求 / 关联方向 | 状态 | 验收条件 | 下一步 |
| --- | --- | --- | --- | --- |
| FIX-052-01 | 环境卸载 / IMP-04 | 已完成 | 明确卸载入口和确认；外部链接仅移除管理；活动安装保护和删除回归通过 | `tests/release-052-ui.mjs`、Rust 删除夹具 |
| FIX-052-02 | 下载版本完整性 / IMP-09 | 已完成 | Node 索引排序、平台筛选、缓存提示和全部列表访问；审查其他版本源限制 | Node 双源、八类官方源真实在线查询通过；分页 UI 通过 |
| FIX-052-03 | 本机 Rust 纳入管理 / IMP-04 | 已完成 | 识别 rustup 实际工具链；纳入、切换、探测与移除不损坏原环境 | Rust 目录夹具、UI 纳入测试；本机 rustup 只读确认 |
| FIX-052-04 | 本机扫描重复 / IMP-01 | 已完成 | 同安装 Git 多入口合并；不同安装、不同工具和失败诊断保留 | cmd/bin/mingw64 去重夹具与本机扫描均通过，两套独立 Git 保留 |

源码验证：最终 Rust 串行测试 71 passed / 7 ignored；三套 UI 回归通过；正常权限下 Node 双源及八类官方源在线测试通过。本机 Rust 临时链接、概览、切换、解除链接验证通过，原目录保留；Git 同安装入口合并且独立安装保留。0.5.2 最终打包：已完成，EXE/NSIS 文件及产品版本均为 0.5.2，路径、大小与 SHA-256 见 `docs/archive-legacy.md`。远端发布：未执行；安装包实际安装/覆盖升级未验证。


## 七、变更日志

2026-11-09 / REL-01 LLVM 回退路径修复：Windows CI `42557097874` 在 `8c8adc5` 仍有 1 项失败，原因是 LLVM 登记缺少 InstallLocation 时，父目录回退仍按字面路径比较。现已统一改用仅扩展普通路径短名称、跳过 reparse component 的辅助比较，并扩展 Windows 短路径回归覆盖 InstallLocation 与 fallback。本机完整 Rust 测试 374 passed / 14 ignored、`npm run verify`（前端构建、两套读状态与 10 套 UI）、`cargo check --offline` 和 `git diff --check` 均通过。当前补丁未提交/推送，CI 与 fresh clone 待重跑；旧 EXE 来自 `8c8adc5`，不含当前修复，未创建 tag 或 Release。

2026-11-08 / 文档一致性核对：核对当前版本元数据、18 类环境类型、默认下载源、验证脚本 UI 套件数、release record 输出路径、产物版本和 GitHub Release 状态；修正 README、测试/发布指南、审计快照标题及路线图中的过期状态和计数。复核 PWSH-01 证据为 373 passed / 14 ignored。本次未改业务源码、未运行测试、未打包或发布，长期改进状态不变。
2026-11-03 / UIR-01 前端重构提案：按用户意图编写 OpenDesign 交接说明，覆盖产品边界、六页信息架构与状态、关键流程、视觉系统、Tauri/API 约束和验收；`docs/README.md` 增加提案索引。当前仅供审核，仍属暂缓项；未改业务代码、未运行应用测试、未打包或发布。下一步由用户确认是否纳入路线及导航/快捷键决策。

2026-10-24 / INSTALL-01-a.1 重新打开真实 Rust 安装门槛：用户删除旧 Rust 后通过上次 EXE 重装，出现 unknown proxy name。已定位到 install.rs 通用临时命名使 rustup-init 进入工具代理分派；前次夹具只覆盖构造步骤，未覆盖此真实启动边界。用户授权修复并真实安装 Rust，当前 cargo/rustc/rustup 不可用；先可信引导受管工具链，再回归/构建修复版并激活，不删除四个失败目录、不改系统 PATH。

2026-10-24 / INSTALL-01：用户要求开始完成安装完整性审计待办；专项进入进行中，先核对 Rust、系统 PATH 提权与 journal，Python/Gradle 收口决策、LLVM 实测单独记录，不回退既有工作区改动。

2026-10-24 / INSTALL-01 验证阻塞：Rust 新增 2 项、PATH 新增 3 项隔离夹具通过；本机 rustup 不在 PATH，组件只读命令未能执行。联网官方文档读取超时。UI 夹具启动与静态构建随后均遇自动审批服务 403，命令未执行；停止继续请求同类执行，不使用其他工具绕过审批。a/b/f 保持待验证，LLVM 实测保持未完成，等待用户恢复审批/提供验证条件；未打包、未发布。

2026-10-24 / INSTALL-01 打包请求阻塞：用户要求进行打包；首次版本/配置/工作区核对及打包帮助命令即再次被自动审批服务 403 阻止，命令未执行，构建尚未启动。负责人 Codex；目标版本待恢复执行后核对，不擅自变更版本。下一步恢复审批后完成验证、独立 EXE/NSIS 构建与版本/大小/SHA-256 核验；保留旧产物，不自动发布。本次未生成新安装包。

2026-10-24 / INSTALL-01 EXE 构建接续：权限已恢复，用户明确不要安装包，只产出 EXE；三处版本一致为 0.5.5。完整 verify（360 Rust passed / 14 ignored、10 套 UI、构建与读状态）通过，Clippy 通过 / 1 条排序风格建议，历史 5 个产物哈希已留档。开始独立 target release --no-bundle，未发布；LLVM 等真实验收保持未完成。

2026-10-24 / INSTALL-01 EXE 已构建：Tauri release --no-bundle -- --offline 成功（9m57s）；src-tauri/target-install01-exe-20261024/release/envcon.exe 为 Windows x64 GUI，文件/产品版本 0.5.5，9,593,856 bytes，SHA-256 dd6ad765e58543754ab401854ce4c4a62b80a3875bc15387eced221b414da678。旧 5 个产物 SHA-256 不变，120 个源码快照文件未变，未生成安装包、未运行产物、未发布。INSTALL-01-c/d/f 完成，a/b 保持真实验收待验证，e 未完成；不改变长期完成数。证据 docs/evidence/install01/exe-artifact-2026-10-24.json。

2026-11-03 / PWSH-01 PowerShell 7 接入：用户要求先列计划并开始开发，后指定最终版本为 0.6.0。以独立受管类型接入，不替换 Windows PowerShell 5.1、不写系统级 PATH、不纳入 MSI/MSIX/WinGet。实现 `pwsh.exe` 扫描、版本探测和受管目录，接入官方稳定 ZIP、校验与 UI；源码和自动化回归完成，真实安装/切换、打包/发布分别待核验。

| 日期 | 条目 | 状态变化 | 内容 / 证据 |
| --- | --- | --- | --- |
| 2026-11-03 | PWSH-01 / PowerShell 7 接入 | 源码实现与完整回归通过；隔离 ZIP/PATH 运行验证通过；EnvCon 实际安装待验收 | 增加独立类型、`pwsh.exe` 扫描/版本识别、Program Files 常见目录、稳定 Windows x64 官方 ZIP、GitHub API SHA-256 与安装门禁；严格绑定版本/资产 URL，验证解压布局和实际版本，前端对无可信 SHA-256 的 PowerShell 版本禁用安装。完整 Rust 373 passed / 14 ignored，`npm run verify`（含构建、读状态和 10 套 UI）通过；实际下载官方 v7.6.9 ZIP，验证官方 API SHA-256、关键文件、实际版本及隔离 `current/powershell` PATH 子进程调用，证据 `docs/evidence/powershell/pwsh-runtime-verification-2026-11-03.json`，回归证据 `docs/evidence/powershell/powershell-development-verification-2026-11-03.json`。EnvCon app IPC 实际安装/切换待验收；0.6.0 未打包/发布。 |
| 2026-11-03 | PWSH-01 / PowerShell 7 接入 | 进行中，已完成计划登记；开发待验证 | 新增独立 PowerShell 7 管理范围：`pwsh.exe` 扫描/版本识别、官方稳定 Windows x64 ZIP 受管安装、校验和前端回归。初步审计确认现有代码已有 GitHub Releases `digest` 解析和下载校验管线；资产摘要和 ZIP 布局仍须真实官方数据/夹具核实。本次未真实安装、未打包/发布。 |
| 2026-11-03 | INSTALL-01 / Rust 重装、组件核验与修复版 EXE | Rust 真实安装与组件验证通过；完整回归及 EXE 核验通过，系统 PATH/LLVM 仍待验收 | stable 1.99.1 已安装至受管目录并由 current Junction 激活；Rust 364 passed / 14 ignored、构建、两套读状态、10 套 UI 和 Clippy 通过。独立 0.5.5 EXE 仅本地产出，无安装包、未启动/发布。证据 `docs/evidence/install01/rust-and-exe-verification-2026-11-03.json`。 |
| 2026-10-22 | 调研 / 工具安装完整性核查（17 类） | 仅调研与记录，未改代码 | 起因是用户发现 EnvCon 装的 Rust 里 `cargo clippy` / `cargo fmt` 直接报「未安装该组件」。逐类核查 `download_spec_with_verification` 的安装参数后确认：**Rust 是真缺陷**——`run_rustup_init` 传 `--profile minimal`，实测只装 rustc + cargo + rust-std 三个组件，clippy / rustfmt / rust-docs 全缺。**一个易误判的细节**：`cargo-home\bin` 里确实有 `cargo-clippy.exe` 等文件，但全是 12.13 MB 的 rustup shim（与 `rustup.exe` 同哈希），不是真组件，只看文件存在会得出错误结论。**另两类是取舍而非缺陷**：Python 传 `Include_launcher=0` 导致缺 `py.exe` 启动器（VS Code 等工具依赖它）；Gradle 用 `-bin.zip`（144 MB）而非 `-all.zip`（246 MB），缺 Gradle 自身源码与离线文档，影响 IDE 跳转。**LLVM 待实测**：NSIS `/S` 未传组件参数，装哪些由安装器默认值决定，本次未验证故不下结论。**其余 13 类为官方完整发行包**，无裁剪。**同时确认一处功能遗漏**：系统 PATH 的底层写入能力（`save_path(EnvScope::System, ...)`）已具备，但 `Paths.tsx` 第 298–307 行仍是「系统 PATH(只读)…仅作展示」，缺 UI 接线与提权路径——与上一轮「环境变量编辑」只做具名变量、漏掉 PATH 是同一处遗漏。**推荐方案**（已隔离实测有效）：保持 `minimal` 后追加 `rustup component add clippy rustfmt`，而非改用 `--profile default`——后者会连带装数百 MB 的 rust-docs，而 clippy/rustfmt 是刚需、rust-docs 不是。**另一个反直觉发现**：EnvCon 装出的 rustup 布局比用户当前纳管的扁平工具链能力更强（可 `component add`、可切版本、可 `update`），只是补装需用户自己敲命令且 UI 无任何提示。本次**未改任何代码**，`download.rs` 的 `--profile` 仍为 `minimal` 原样（已用 `git diff` 确认无残留）。完整调研见 `docs/audits/tool-install-completeness-audit-2026-10-22.md`（249 行，含 5 项按影响面排序的待办）。 |
| 2026-10-22 | DL-VERIFY / Rust 渠道显示具体版本号 | 源码与隔离夹具完成 | 此前 Rust 只显示 stable / beta / nightly 三个渠道名，用户看不出实际会装到哪个版本。现联网读取 `static.rust-lang.org/dist/channel-rust-{channel}.toml` 的 `[pkg.rust] version`，标注形如「1.99.1 · 稳定版工具链」。**关键取舍**：渠道名（`version` 字段）保持不变，因为它是下载与 rustup 安装的依据；具体版本号只写进 `note` 用于展示，两者不可混用。版本串形如 `1.99.1 (58cf8f9c3 2026-10-13)`，只取第一个空白前的语义版本，beta / nightly 的 `-beta.7` `-nightly` 后缀原文保留。**单个渠道查询失败不吞掉整张表**：渠道本身仍可安装，失败时如实标注「版本号查询失败：<原因>」而非猜一个版本号；清单无法解析时标注「清单格式无法解析，具体版本号未知」。为不联网也能验证，把校验依据抽成纯函数 `rust_channel_verification`，并让既有的 `rust_uses_the_publisher_checksum…` 夹具改用该纯函数（它原先直接调 `rust_channels()`，改动后会联网）。新增 3 项夹具：真实清单样本解析出 1.99.1 / 1.100.0-beta.7 / 1.101.0-nightly、缺段或损坏时返回 None 而不编造、三渠道齐全且各带校验依据。Rust 354 passed / 0 failed / 14 ignored；`tsc --noEmit` 通过；`download-ui` PASS。证据 `docs/evidence/dl-verify/rust-channel-version-2026-10-22.json`。**未验证**：真实联网下的三渠道版本号（夹具用静态样本）；网络超时或清单格式变更后的实际展示文案。 |
| 2026-10-22 | 本机 Rust 运行时清理（用户环境操作） | 已执行并验证；`~/.rustup` 待用户手动删除 | 背景：本会话早前为修复构建环境误执行 `rustup toolchain install stable --force`，在用户真实目录装出第二套 Rust（1.99.1）。用户要求改用受管版本（`D:\DevEnvManager\current\rust` → junction → `envs\rusts\system-1.98.1`）。**执行前发现真实风险**：用户级 PATH 里没有受管 Rust 的路径，只有 `C:\Users\island\.cargo\bin`，若直接删除 shim 会导致 `rustc` 完全消失。故按「先切 PATH、再删 shim、最后删 rustup 目录」的顺序执行，每步验证。已备份用户 PATH 到 `D:\DevEnvManager\logs\user-path-backup-before-rust-cleanup.txt`，删除清单与回滚命令写入 `D:\DevEnvManager\logs\rust-cleanup-manifest.txt`。PATH 中插入 `D:\DevEnvManager\current\rust\bin` 到第 2 位（`.cargo\bin` 之前），实测 `Get-Command rustc` 解析到受管目录并报 `rustc 1.98.1`。删除 `.cargo\bin` 下 14 个 rustup shim（实测哈希同一，均为 shim）。**保留**：`~/.cargo\bin` 中三个独立工具（`cargo-binstall.exe` 25.5 MB、`cargo-install-update.exe` 4.5 MB、`cargo-install-update-config.exe` 3.5 MB）、`~/.cargo\registry` 884 MB crate 缓存、`~/.cargo\config.toml`、受管 Rust 本体。**未完成**：`~/.rustup`（1.47 GB / 66106 文件）的递归删除被本会话安全策略拒绝（home 目录受保护），需用户手动执行 `Remove-Item "$env:USERPROFILE\.rustup" -Recurse -Force`。**未验证**：在全新终端中 `rustc` 是否解析到受管版本（当前 shell 进程仍持有旧 PATH 快照）；`cargo build` 在只有受管 toolchain 时是否正常。 |
| 2026-10-22 | IMP-09 / 环境变量编辑（用户级 + 系统级） | 源码与隔离夹具完成；提权链路未实机验证 | 此前用户级变量只能改 10 个固定名，系统级完全只读（UI 写着「需要管理员权限，仅作展示」）。现支持：任意变量名的增删改、用户级与系统级两套独立值、按作用域列出常见清单之外的变量。**产品仍是单 exe**：同一可执行文件带 `--env-write <file>` 启动时不初始化 GUI，只执行写入并退出；主程序用 `ShellExecuteExW` + `runas` 启动自身，等待退出后读结果。主程序保持普通权限，日常扫描/切换/改用户级变量不弹 UAC，只有真正写 HKLM 时提权一次。请求文件只含变量名与值，不含可执行路径。备份文件名带作用域前缀，避免同名变量恢复时写错 hive。**过程中修掉一个真实缺陷**：初版误用 `ShellExecuteW` 却读 `SHELLEXECUTEINFOW.hProcess`——该函数不填充此结构体，句柄永远为 null，无法确认写入结果，已改用 `ShellExecuteExW`。新增 3 项夹具（作用域映射到不同 hive、请求只含名值且可往返、非法名在进注册表前被拒）。Rust 351 passed / 14 ignored；`tsc --noEmit` 与 `npm run build`（1710 modules）通过；九套 UI PASS。**未验证**：从未真正提权写入（本机非管理员，且按规则不在真实环境写 HKLM）；UAC 取消/拒绝文案、提权后临时文件可访问性、新终端实际读到系统级变量均未验证。**UI 夹具已补**：新增 `tests/paths-ui.mjs`，断言切到系统级后输入框显示 `systemValue`（`C:/system/jdk`）而非用户级的 `D:/user/jdk`、`aria-pressed` 正确切换、保存携带 `scope=system`、自定义变量新增与删除跟随作用域、提权提示仅在系统级出现、390px 无横向溢出。**同轮修掉两个真实缺陷**：① 切到系统级后输入框仍显示用户级的值——用户会误以为系统级等于用户级，进而覆盖真实的系统级配置；② 环境变量行与新增变量表单在 390px 下横向溢出（实测 `right=487` 超出 `edge=390`），这是既有布局问题，因之前没有 Paths 页面的 UI 夹具而从未暴露。`paths-ui` 与九套全部 PASS。证据 `docs/evidence/imp09/env-var-editing-verification-2026-10-22.json`。 |
| 2026-10-22 | 打包 / 重新生成 0.5.5 产物 | 已打包；未发布 | 上一版产物生成于 2026-10-20 02:22，不含本会话全部改动（.NET SDK 支持、搬入受管与删除旧目录、搬移来源记录、中断残留清理、.NET 归档版本校验等）。现执行 `npx tauri build` 重新打包：`beforeBuildCommand` 的 `tsc && vite build` 输出 1710 modules / 946ms，`release` profile 编译 3m47s。三个产物均已生成并核对 SHA-256：`envcon.exe` 9.01 MB `c724cd6cc36bc549fbf0e0094a8abcb1efa219e7d0cd570faf8cd68e2838d2a0`；`EnvCon_0.5.5_x64-setup.exe` 3.06 MB `96630db30c1b7b1b5f5558c4ddde6cd422c13a3b5d29c13917793a1f2f2b8c59`；`EnvCon_0.5.5_x64_en-US.msi` 4.36 MB `d45fdfab709ffb9c9a6663e646d0d03ff4a39a4db1e37557ca2f93189e7fc544`。路径：`src-tauri/target/release/envcon.exe`、`bundle/nsis/`、`bundle/msi/`。**这不等于已发布**：未做代码签名与时间戳、未推送 tag、未创建 Release；版本号仍沿用 0.5.5，未因本会话新增功能而提升。**未验证**：新产物未做安装冒烟（R0 实机门槛仍未解除）、未验证覆盖升级与卸载、未在真实环境运行。**2026-10-22 04:01 再次重新打包**（加入环境变量编辑功能与 `paths-ui` 夹具，`release` 编译约 7 分钟）：`envcon.exe` 9.07 MB `6643804adf6aa855675841a6f0a35837e4cdceebf0896e5613ac4121d75feb09`；`EnvCon_0.5.5_x64-setup.exe` 3.08 MB `b618b3865fb0c46d74fbd490d82e268576f24a79bea57e7d111ff7f8b04e9ce3`；`EnvCon_0.5.5_x64_en-US.msi` 4.38 MB `b52bdb43c6e1fe6c23955912435f777f35e599ab91e706b871fba0c0ad2e568f`。上一轮的哈希 `c724cd6c…` / `96630db3…` / `d45fdfab…` 对应不含环境变量编辑的产物，已被本轮取代。**用户已自报在早前 0.5.5 构建上完成安装**（见 `docs/evidence/r0/r0-user-reported-install-2026-10-22.json`，按规则记为「已报告」而非「已验证」），当前新产物本身仍未经安装验证。 |
| 2026-10-22 | IMP-09 / 新增 .NET SDK 环境类型 | 源码与隔离夹具完成；实机待验证 | `EnvType` 由 16 类扩为 17 类，新增 `DotNet`。**关键取舍**：只装了 .NET Runtime 的目录同样有 `dotnet.exe`，但它无法编译项目，因此结构判定要求 `dotnet.exe` **且** `sdk/` 目录存在；缺 `sdk/` 时诊断信息明确写出「这可能是只装了 .NET 运行库，而不是 SDK」，而不是笼统的「结构不完整」。版本源接官方 `releases-index.json`，每通道取最新 SDK，LTS/STS/已停止支持三类分别标注；微软不发布逐文件 SHA-256 清单（只有 releases.json 内的 SHA-512），因此如实标为未验证而非假装已校验。Windows 只提供 zip，`strip_top: false`（zip 内直接是 `dotnet.exe` 与 `sdk/`，剥一层会散到目标目录外）。PATH 入口直接是 `current\dotnet`，无 `bin` 子层。前端同步类型、元数据、下载页与环境页说明、徽标首字母（`D`——三字符会撑破固定宽方块）。新增 3 项夹具；Rust 339 passed / 14 ignored；九套 UI PASS；`tsc --noEmit` 与 `npm run build` 通过。证据 `docs/evidence/r3/r3-move-external-env-verification-2026-10-21.json` 同轮归档。**未验证**：未在真实 Windows 上安装 .NET SDK；未验证 `strip_top: false` 在真实 zip 上的结果（内部布局依据官方说明推断）；未覆盖多 SDK 并存语义。**2026-10-22 在线验证并修复 3 个真实缺陷**：此前 `releases-index.json` 的解析只经合成夹具，从未在真实响应上跑过。实测端点返回 HTTP 200 / 6988 字节 / 14 个通道，暴露三处：① `support-phase` 实际有四种取值（active / maintenance / go-live / eol），原实现只处理 `eol`，其余一律按 LTS/STS 输出，用户看不出某通道已进入维护期——改为分档标注「已停止支持（原 X）」/「X · 维护期」/「X」；② 11.0 通道的 `latest-sdk` 是 `11.0.100-rc.2.26504.105`（RC 预发布版）、phase 为 `go-live`，原实现标成「STS」，用户装完才发现不是正式版——新增 `is_prerelease_version` 识别 -rc./-preview./-alpha/-beta，命中或 phase 为 go-live 时标注「预览版（X），非正式发布」；③ 2.0 与 1.0 通道的 `latest-sdk` 分别为 2.1.202 与 1.1.14，与 2.1/1.1 通道重复导致同一版本出现两行——排序后按版本号去重。另对 10.0.402（正式）与 11.0.100-rc.2.26504.105（RC）各做一次 HEAD，均返回 HTTP 200，确认推导出的下载地址格式正确。新增 2 项夹具覆盖预览版识别与跨通道去重。Rust 347 passed / 14 ignored；九套 UI PASS。证据 `docs/evidence/imp09/dotnet-sdk-support-verification-2026-10-22.json`。 |
| 2026-10-21 | 范围裁剪 / 五项功能决定不做 | 需求方明确判定不做 | 用户判定以下功能「没有必要也没有用」，决定不再实现：私有 registry/镜像/认证引用与凭据存储（第 200 条）、导出清理计划（第 202 条）、注册表与第三方管理器配置的备份恢复（第 256 条后半）、发布方签名验证（第 336/340 条的签名部分）。**哈希校验按用户选择保留现状、只是不再扩展**：`VerificationInfo` 体系（`TrustedSha256`/`ChecksumFile`/`Invalid`/`Unverified`）与 `validate_digest` 继续生效，下载边写边算 SHA-256、不符即拒绝落盘并按「最终失败、不重试」处理，无可信资料如实标为未验证；不追加签名或新的校验来源。按项目规则保留原条目与决定理由（不覆盖历史记录），五条均未勾选。本次为**纯文档范围裁剪，零代码改动**：未删除任何既有实现，`cargo test` 基线仍为 325 passed / 14 ignored。 |
| 2026-10-21 | IMP-06 / uv 恢复条目状态契约 | 缺陷已修复 | `UvRecoveryItem.status` 的 Rust 文档注释与前端 TS 类型都声明了四种取值：`recovered`、`recovered-with-residue`、`failed`、`retained`，但 **`retained` 从未被后端产生**——`retain` 实际是 `UvRecoveryReport` 顶层的独立 `Vec<String>` 字段。任何按注释写 `status === "retained"` 的调用方都会写一个永不触发的分支。更实质的问题是语义：`retained` 记录的是**预览阶段就判定为不可自动恢复、从未被尝试**的工具，而 `failed` 是**尝试过并失败**；混进同一状态集合会让「压根没试」与「试了但失败」在 UI 上看起来一样，用户无法判断是否值得人工重试。修复：Rust 注释改为只列三种取值并说明 `retained` 的位置与原因；TS 类型从四值联合收紧为三值联合。**没有**选择「补上 retained 状态」这个方向——保留项在预览阶段就被排除在 `recoverable` 之外、从未进入恢复循环，强行造条目等于把「未尝试」伪装成「已处理」。扩展夹具 `partial_recovery_keeps_blocked_tool_and_shared_cache`：断言未被尝试的工具（beta）不得出现在 `items` 里、`items` 中所有状态必须是封闭集合三者之一。Rust 325 passed / 14 ignored；`cargo check` 0 errors / 0 warnings；`npx tsc --noEmit` 通过；`git diff --check` 干净。**未验证**：未在真实 uv 布局上触发 partial 恢复（夹具用临时目录构造的 receipt 与假启动器）；未验证前端对 `recovered-with-residue` 的展示（UI 直接显示原始状态字符串，未做本地化）；**已升级为枚举**：`UvRecoveryItem.status` 现为 `UvRecoveryStatus`（`Recovered` / `RecoveredWithResidue` / `Failed`），编译器从此阻止第四种取值；序列化字符串经 `#[serde(rename_all = "kebab-case")]` 保持不变（`recovered` / `recovered-with-residue` / `failed`），前端无需改动。夹具改为锁定**序列化字符串**而非枚举取值——那才是前端真正依赖的契约。证据 `docs/evidence/imp06/uv-recovery-item-status-contract-verification-2026-10-21.json`。 |
| 2026-10-21 | IMP-06 / 卡住的大小统计可放弃 | 缺陷已修复 | 大小统计一旦卡住，用户点「取消」也没有用：任务永久停在「统计中」，所有统计按钮被禁用，只能重启应用。根因：`SizeTasks::cancel` 是**协作式**的——只设置 `AtomicBool`，由 `measure` 在循环边界检查；但 `measure` 内的 `read_dir`/`symlink_metadata` 都是**同步阻塞调用**，在网络盘、云占位符或响应极慢的卷上单次调用可能长时间不返回，`measure` 走不到检查点。任务停在 `running` 后，`start` 的互斥检查一直返回「已有大小统计正在进行」，前端 `anyRunning` 也禁用全部按钮。这是唯一一个用户**无法自行恢复**的失效模式。修复：新增 `SizeTasks::abandon(id)`，把记录从表中移除让新统计可以开始；与 `cancel`（请求尽快停止并保留部分结果）语义不同，故独立方法而非加参数。`abandon` 同时设置 `cancel` 标志，对只是「慢」而非卡死的任务会在下一检查点停止。守卫：只允许放弃 `running` 任务（已结束的是已有结论，不是卡住），不存在的 id 报错。被放弃任务的线程完成时发现记录已不存在（`if let Some(task)`），不会写回陈旧结果。命令 `abandon_size_scan` 注册进 `lib.rs`；前端在「统计中」状态同时显示取消与放弃按钮，后者 title 注明「取消无效时使用」。新增夹具覆盖：先验证卡住任务确实阻塞新统计、abandon 后记录移除且发出停止信号、统计能力恢复、已结束任务不可放弃且错误信息说明「仍在运行」、已结束记录必须保留、不存在的 id 报错。Rust 325 passed / 14 ignored；九套 UI 全 PASS；`tsc --noEmit` 与 `npm run build`（1710 modules）通过；`cargo check` 0 errors / 0 warnings；`git diff --check` 干净。**未验证**：未在真实卡住场景（网络盘、云占位符）下验证，夹具直接构造 `running` 记录；未验证被放弃线程确实在下一检查点退出。**UI 夹具已补**：`sizes-ui.mjs` 新增断言——「取消大小统计」与「放弃大小统计」两个按钮并存（语义不同不可互相替代）、放弃按钮 title 为「放弃这个卡住的任务（取消无效时使用）」、点击后 `abandon_size_scan` 恰好调用一次、记录移除后统计按钮恢复可用、放弃按钮随之消失；mock 的 `abandon_size_scan` 实现为**移除记录**而非改状态，与后端语义一致。九套 UI 全 PASS（输出含 `abandon-a-stuck-task`）。证据 `docs/evidence/imp06/abandon-stuck-size-task-verification-2026-10-21.json`。 |
| 2026-10-21 | IMP-06 / 重装中断的条目状态收尾 | 缺陷已修复 | 重装任务中途失败时，界面标题显示「重装失败」，但同一条目列表里仍有包显示「安装中」或「等待」。根因：`pip_reinstall::start` 与 `global_reinstall::start` 的失败分支只设置任务级 `t.status = "error"`，未处理条目级状态；`install()` 用 `?` 早退，失败点之后的条目停在 `pending`，正在装的那条停在 `installing`。前端**无条件渲染全部条目**（`task.items.map(...)`），因此任务级与条目级给出互相矛盾的说法，且这份矛盾会随 `update()` 写入 `envcon-reinstall.json` 报告。修复：在 `package_managers::types` 新增 `settle_unfinished_items(items, reason)`，只把 `pending`/`installing` 落成 `failed` 并写入原因；`installed`/`failed`/`canceled` 一律不动（覆盖它们等于篡改已记录的证据，已有失败原因也不被通用原因覆盖）。抽成共享函数而非各写一份循环，是因为两处逻辑必须一致。新增 2 项夹具：五态混合场景（含「已有失败原因不被覆盖」）与空列表/全已完成的 no-op。完整 Rust 324 passed / 14 ignored；`cargo check` 0 errors / 0 warnings；`git diff --check` 干净。**未验证**：未在真实重装流程中触发中途失败（夹具直接调用共享函数，不经过 `install()` 早退路径）；未新增 UI 侧夹具；未覆盖「失败发生在 `update()` 写报告之前」的情形。证据 `docs/evidence/imp06/reinstall-failure-item-state-verification-2026-10-21.json`。 |
| 2026-10-21 | IMP-05 / pip 重装的注入防线与 PEP 503 归一 | 部分完成；该条仍不勾选 | `pip_reinstall::index_pin` 决定一个「包名 + 版本」能否被拼成 `name==version` 交给 pip，它同时承担两个职责：拒绝**选项注入**（`--index-url`）与拒绝**换行注入**（把额外参数塞进版本字符串）。既有夹具只有 3 条断言，边界完全没有锁定。新增 `index_pin_accepts_only_an_alphanumeric_name_and_a_numeric_version`：接受常规包名与 PEP 440 后缀形式、长度恰好 200（边界包含）；拒绝首字符非字母数字（`--index-url`、`-pkg`、`.pkg`）、尾字符是分隔符、换行与回车注入、空格与路径分隔符、版本非数字开头（`abc`、`v1.0`）、版本含 `=`（`1==2` 会改写整个要求）、空值与 201 字符。另新增 `normalize_collapses_pep503_separators_and_flags_duplicates`：`A_B`/`a-b`/`a.b` 归一相同（PEP 503）、连续分隔符折叠、三份同名元数据只保留一份并标记冲突、`pip`/`setuptools`/`wheel` 标为「构建工具由新环境维护」、可复制包 reason 为空、结果按归一化名排序。pip_reinstall 模块 4 passed / 1 ignored；完整 Rust 322 passed / 14 ignored；`git diff --check` 干净。**未完成**：未执行真实 pip（`offline_python_reads_old_metadata_and_installs_into_new_venv` 仍 ignored，需 ENVCON_TEST_PYTHON 与离线 wheel）；夹具锁定的是本项目的允许集而非 pip 完整语法；卸载侧与失败恢复仍无 per-manager 夹具。证据 `docs/evidence/imp05/pip-index-pin-verification-2026-10-21.json`。 |
| 2026-10-21 | IMP-06 / 清理目录分离校验 | 部分完成；该条仍不勾选 | `old_packages::verify_separate` 是「清理旧包目录」与「新安装目录」之间的唯一防线，一旦两者重叠或被重定向，清理会删掉刚装好的东西。既有夹具只覆盖 `source == destination` 与 **source 侧**重定向，`destination` 侧的三种情形完全没有夹具——而这三种都会让「删除旧包」变成「删除新装」，尤其 destination 位于 source 之内时路径看起来完全不同，用户无法从界面察觉。新增 `verify_separate_rejects_overlap_and_redirects_on_both_sides`：平行目录放行（不能过度拒绝，否则正常清理无法进行）、destination 是 Junction 拒绝（清理会写到链接目标而非用户看到的路径）、destination 位于 source 之内拒绝（清理 source 会连带删掉新安装）、source 位于 destination 之内拒绝（清理后新目录残留空旧树）、通过 Junction 别名形成的父子关系同样拒绝（路径字符串不同但实际是同一棵树）。old_packages 模块 6 passed；完整 Rust 320 passed / 14 ignored；`git diff --check` 干净。**未完成**：未在真实清理流程中触发这些拒绝（夹具直接调用 `verify_separate`，不经过 `execute` 完整链路）；未覆盖符号链接（非 Junction，需额外权限）与大小写差异、8.3 短名、UNC 等 Windows 路径等价形式。证据 `docs/evidence/imp06/cleanup-root-separation-verification-2026-10-21.json`。 |
| 2026-10-21 | IMP-05 / 包查询状态分类 | 部分完成；该条仍不勾选 | `node_rows` 把每个依赖分成四种可观察状态（正常 / 缺少安装文件 / 版本与声明不一致 / 本地链接），每种都对应一条给用户看的 `detail`；既有夹具只覆盖 `missing` 一种，其余三种一旦回归不会有测试失败。新增 `node_rows_classifies_every_dependency_state_and_never_invents_one`：四态逐一锁定、`invalid: false`（npm 表示校验通过）不得被误判为不一致、路径优先取依赖自身 `path` 否则回退「项目根/node_modules/包名」、同名多字段按 (name, path) 去重。另新增 `pip_rows_flags_direct_sources_and_keeps_plain_packages_clean`：`direct_url` → 「本地或直接来源」、非 inspect 模式读顶层数组、editable 回退 `editable_project_location`、缺包名报错而非产出无名记录。packages 模块 9 passed / 1 ignored；完整 Rust 319 passed / 14 ignored；`git diff --check` 干净。**未完成**：未执行任何真实包管理器（`actual_managers_return_readable_lists` 仍 ignored，需 ENVCON_TEST_ROOT）；重装、卸载两个维度仍无 per-manager 夹具；夹具锁的是当前 JSON 字段约定，管理器改变输出格式时不会被发现。证据 `docs/evidence/imp05/package-query-state-classification-verification-2026-10-21.json`。 |
| 2026-10-21 | IMP-05 / 十种管理器自更新命令契约 | 部分完成；该条仍不勾选 | `self_update_command` 为每个管理器生成确切参数，这些参数会**先展示在预览对话框、随后原样执行**，因此命令内容本身就是对外契约。此前只有 npm（`--prefix` 重定向）与 pip（解释器切换）有间接覆盖，其余八个管理器的确切参数**没有任何夹具**——改动分支不会让任何测试失败。新增 `every_manager_self_update_command_is_pinned_or_refused_with_an_action`，按三类锁定：**能自更新的逐字锁定参数**（pnpm 为 `self-update`、bun 为 `upgrade`、uv 为 `self update`、composer 为 `self-update`，并断言可执行文件不被重定向、不额外加 prefix）；**不能自更新的拒绝信息必须点名替代动作**（pipx 指向 `python -m pip install --upgrade pipx`、cargo 指向 `rustup update stable`、dotnet 指向「更新 SDK」）——只说「不支持」会让用户无处可去；**未知工具被拒绝且不静默退化成默认命令**。npm/yarn 与 pip 的路径依赖分支已由既有 `self_update_targets_original_installation` 覆盖，Corepack 与 pip 安装的 uv 由 `corepack_and_pip_installed_managers_refuse_self_update_with_a_specific_reason` 与 `list_preview_and_executor_share_one_self_update_blocker` 覆盖。Rust 317 passed / 14 ignored、`git diff --check` 干净。**未完成**：全部夹具只验证命令**构造**，未执行任何真实管理器——「在线与离线测试分开」的在线部分仍未做；查询、重装、卸载三个维度没有像更新这样逐管理器锁定；失败恢复（更新中断、部分写入）无 per-manager 夹具。证据 `docs/evidence/imp05/manager-self-update-commands-verification-2026-10-21.json`。 |
| 2026-10-21 | IMP-08 / URL 凭据脱敏 | 部分完成；该条仍不勾选 | 全仓检索 `token`/`password`/`credential`/`secret`/`auth` 逐条判定：`old_packages`、`global_reinstall`、`commands` 里的 `token` 全是**内部预览令牌**（`scan-<pid>-<n>`），与凭据无关；`sources.rs` 的 16 类源全是编译期常量 URL，`reqwest::Client` 未配置任何认证头，**没有私有 registry、没有代理认证**，因此「凭据存储」在当前功能范围内**无实现对象**。但「统一脱敏」有真实缺口：`download.rs` 的失败信息把**完整 URL 原样**写进错误，经 `AppError::Serialize` 直达 UI 与日志——一旦支持带凭据的地址，`https://user:token@host/...` 会完整泄漏。新增 `error::redact_url`：userinfo 用 `rsplit_once('@')` 整体替换（密码含 `@` 时最后一个是分隔符）、敏感查询参数只替换值保留键名（`token`/`key`/`secret`/`password`/`passwd`/`auth`/`sig`/`signature`/`credential`，不区分大小写且允许包含）、fragment 保留、无 scheme 与空串不 panic、公开 URL 逐字符原样通过。已接入下载失败信息这一唯一出口。Rust 316 passed / 14 ignored、`git diff --check` 干净。**未完成**：凭据存储（keyring/DPAPI、凭据输入界面）完全未实现；**子进程输出未脱敏**——`pkgtools::run`/`mutate` 的 stdout/stderr 原样进错误信息，若管理器打印 token（如 `npm config get` 的认证键）不会被处理；日志文件与导出报告（`update_journal`、清理报告、环境清单）也不经过 `redact_url`；查询参数白名单是启发式的；`Authorization: Bearer ...` 这类非 URL 形态的凭据当前不处理。证据 `docs/evidence/imp08/url-credential-redaction-verification-2026-10-21.json`。 |
| 2026-10-21 | IMP-08 / 外部命令边界审计 | 部分完成；该条仍不勾选 | 逐文件检索 `Command::new` / `cmd.exe` / `powershell` 调用点，确认**生产路径中不存在「拼接字符串后交给 shell」的调用**：`pkgtools`、`download::run_installer`、`detect::probe`、`installer_uninstall::execute`、`reinstall_process`、`global_reinstall`、`pip_reinstall`、`uv_recovery` 全部使用 `Command::new(exe).args([...])` 结构化参数；`detect::system` 的 PowerShell 仅用于读取版本且脚本为常量。唯一的「字符串 → 进程」转换点是 `installer_uninstall::parse_command`——它把注册表里的 `UninstallString`（**不可信输入**，任意软件都能写）转成 `(PathBuf, Vec<String>)`，因此这个函数本身就是 Windows 启动器边界。此前只断言 4 个反例且**没有正例**，无法证明「合法登记仍能通过」。新增 `parse_command_accepts_only_an_exact_exe_with_literal_arguments`：接受引号路径加字面参数、引号路径无参数、首尾空白、无引号绝对路径；拒绝 shell 串联与管道、输入输出重定向、脱字符转义、环境变量展开、嵌套引号、单引号、引号未闭合、路径与参数间无分隔、非 `.exe`（`.bat`）、相对路径（`cmd.exe /c`、`msiexec.exe /x`）、父目录穿越。Rust 315 passed / 14 ignored、`git diff --check` 干净。**未完成**：未做动态注入测试（夹具只验证解析层）；未逐命令复核 Tauri IPC 参数传递；`rundll32`/`wscript` 等启动器因实现直接拒绝非 EXE 而未单独覆盖；凭据脱敏属第 339 条。证据 `docs/evidence/imp08/command-launcher-boundary-verification-2026-10-21.json`。 |
| 2026-10-21 | IMP-07 / 配置写入失败路径 | 部分完成；该条仍不勾选 | 实现早已具备（`SCHEMA_VERSION`、拒绝更高版本而非重置、`serde(flatten)` 保留未知字段、同目录临时文件 + `MoveFileExW`（REPLACE_EXISTING 与 WRITE_THROUGH 两个标志）、失败清理临时文件），缺的是验收条款「权限拒绝、磁盘满之后旧配置或可恢复备份仍有效」的**夹具**——此前只测并发冲突与陈旧实例，没测写入本身失败，而那正是最危险的场景：备份写失败后若主配置仍被覆盖，用户会同时失去新旧两份。新增 2 项：`a_failed_backup_write_leaves_the_original_config_untouched`（备份路径变目录制造写失败，断言主配置逐字节不变、不置位 `write_blocked`、不留 `.tmp-` 残骸、障碍移除后恢复可用）与 `a_failed_main_write_still_leaves_a_usable_backup`（用**只读属性**制造替换失败，这是 Windows 上「权限拒绝」的真实形态；断言备份已写好且 `has_valid_backup()` 为真、主配置仍是旧内容）。config 模块 12 passed；完整 Rust 314 passed / 14 ignored。**过程中夹具写错两次**：先断言「失败后内存设置回滚」（但回滚由调用方命令负责，不是 `save()` 的职责）；后用「路径换成目录」制造主配置写失败（`ensure_unchanged()` 读取阶段就先失败，走不到写备份）——已分别删除该断言、改为只读属性。**未完成**：未覆盖真正磁盘满（需填充卷）；跨版本迁移只有「拒绝更高版本」与「保留未知字段」，无旧→新字段重命名迁移；注册表与第三方配置不适用文件原子写入。证据 `docs/evidence/imp07/config-write-failure-verification-2026-10-21.json`。 |
| 2026-10-21 | IMP-05 / 解析器异常输出夹具 | 部分完成；该条仍不勾选 | `detect::probe::snippet` 是探测失败时**唯一被用户读到的文本**（拼进「输出没有可识别版本：…」），却完全没有夹具。它承担三个产品决策：多行压成一行、长度上限 120 字符、空输出不能返回空串。新增 `snippet_flattens_bounds_and_never_returns_an_empty_reason`，覆盖多行/CRLF/制表符归一、空与纯空白返回「命令没有输出」、120 字符边界**包含**、121 字符截断带省略号且总长 121、以及按**字符**而非字节截断（130 个多字节字符不在 UTF-8 边界 panic）。下载侧复核确认已有覆盖：长度不符与摘要不符不提交半截文件、响应头与数据两阶段均可取消、校验状态彼此区分且未验证不报成通过。**下载重试已实现**：`DOWNLOAD_ATTEMPTS = 3` 与 `Failure::{Transient, Final}`，只重试传输层失败（连接错误、请求发送失败、响应体中途断开、5xx、429），取消/本地写入失败/4xx（除 429）/长度不符/摘要不符一律不重试——同一 URL 每次返回同样字节，重试内容层失败只会把真实损坏掩盖在一串静默尝试背后；重试前删 `.part` 并把进度回报为 0，退避期间仍可取消。新增 4 项夹具（503 后重试成功且服务器恰好收到 2 次请求、持续 503 在 3 次后停止且不留残片、404 只请求 1 次、摘要不符只请求 1 次）。Rust 312 passed / 14 ignored；`git diff --check` 干净。**未完成**：未在真实网络故障下验证重试（夹具用本地 TCP 服务器模拟状态码，不覆盖连接中途断开）；`version_token` 未覆盖「同一行两个都合格的候选」这类歧义场景。证据 `docs/evidence/imp05/parser-anomalous-output-verification-2026-10-21.json`。 |
| 2026-10-21 | IMP-08 / 更新执行前复核 | 部分完成；该条仍不勾选 | 卸载与迁移早已用 `expected_preview` 绑定执行，但**更新没有**——`update_package_manager` 只收工具名，用户确认的预览与实际执行的命令之间没有任何约束。现补上：`PackageManagerUpdatePreview` 与 `UpdateCapabilities` 增加 `Debug/Clone/PartialEq/Eq/Deserialize`；新增 `validate_update_execution_preview`，先区分「当前已阻断」（报出具体原因，如 Yarn Berry）与「预览已变化」（提示重新预览）；执行器 `update_package_manager_inner` 接受 `Option<&PackageManagerUpdatePreview>`，用**本次真正要用的值**重新构建同一结构再比对——比对重算值而非调用方副本，否则检查恒真。批量更新传 `None` 并注明「批量没有逐工具确认可绑定」。前端 `applyUpdate` 先捕获已确认预览，缺失时报「没有已确认的预览，未执行任何命令」而非静默执行。新增夹具覆盖相同预览放行、可执行文件不同、参数不同、版本变化、变为阻断五种情形。Rust 305 passed / 14 ignored；九套 UI 全 PASS；`npm run build` 1710 modules；`tsc --noEmit`、`cargo check --offline`、`git diff --check` 均干净。**未完成**：删除/迁移/更新三者仍是三份独立实现，未统一为同一套抽象；未覆盖「预览后目录被整体替换但路径与版本都不变」；未在真实管理器上执行。证据 `docs/evidence/imp08/update-execution-precheck-verification-2026-10-21.json`。 |
| 2026-10-21 | IMP-07 / 配置历史与回滚 | 部分完成；该条仍不勾选 | 此前只有一级 `config.previous.json`，它在下一次保存时被覆盖，再往前保存两次就无从回溯。新增 `config-history/` 目录：每次 `save()` 在原子替换主配置前把上一份已校验字节归档，按纳秒时间戳命名并裁剪到 20 份；归档失败不阻断保存（历史是附加价值，不是保存前提）。新增 `list_config_history` / `restore_config_history` 命令，后者只接受历史目录内的 `.json`，回滚前另存 `config.before-history-restore-*` 使回滚本身可撤回，损坏历史经 `parse_settings` 拒绝而不覆盖可用配置。设置页新增「配置历史（最近 N 份）」区与确认对话框。**过程中发现并修掉一处真实缺陷**：首次接入时历史读取失败会让整个 `load()` 落入 catch，设置页只显示「加载设置失败」；现降级为空列表——历史不可读不应让用户看不到主设置。新增 4 项夹具（累积与排序、裁剪上限、回滚保留被替换文件、拒绝损坏历史）。Rust 304 passed / 14 ignored；九套 UI 全 PASS；`npm run build` 1710 modules；`tsc --noEmit`、`cargo check --offline`、`git diff --check` 均干净。**未完成**：注册表与第三方管理器写入的备份/恢复完全未实现，故该条保持未勾选；未在真实 `%APPDATA%\envcon` 上执行回滚，全部夹具使用临时目录。证据 `docs/evidence/imp07/config-history-verification-2026-10-21.json`。 |
| 2026-10-20 | IMP-03 / IMP-09 范围决定 | IMP-03 决定不做；IMP-09 收窄至扫描质量 | **IMP-03 砍掉**：回收站式「可撤销全局包清理」需对第三方包管理器保证恢复一致性，通用层面无法可靠验证，且超出「Windows 开发环境工具下载与扫描工具」定位；现有 `old_packages` 的「直接删除 + 报告保留 20 份 + 执行前重核清单 + 逐包失败记录」保留不变。**IMP-09 收窄**：原 6 项中 4 项（catalog 审计、适配器契约、新增品类评估、新管理器验证）为纯架构重构、用户不可感知，决定不做；ASMT-03 因 ADB 为特例而降级；**保留 SCAN-CORE-01**（扫描是核心功能）。**PATH-01 适配器部分标记完成**：`bin_path` 对 uv/pipx/bun 改用 `sibling_role`，入口目录不再落在工具环境根内，修掉 uv 把 `bin`/`cache` 当成损坏工具的原始缺陷；uv/pnpm/Yarn 专项夹具 9 项通过（`uv_legacy_layout_is_read_only_and_classifies_recovery`、`uv_cleanup_accepts_declared_sibling_or_legacy_entry_only`、`refuses_to_migrate_a_legacy_uv_layout_as_is`、`scan_warns_about_legacy_uv_layouts_seen_in_the_effective_environment`、`pnpm_and_yarn_classic_keep_package_store_and_command_prefixes_separate` 等）。**未验证**：真实 uv 恢复执行、真实注册表写入、新终端命令生效——均属 IMP-05 实机验收。ROADMAP 同步新增「1.4 已决定不做」分类表，被砍条目原文保留作为历史记录。 |
| 2026-10-20 | R0 / 隔离 Windows 安装冒烟 | 阻塞解除；R0 仍不整体关闭 | 主机启用 Windows Sandbox（`Containers-DisposableClientVM` + `HypervisorPlatform`，启用前状态存档于 `C:\ProgramData\EnvConVerify\sandbox-state.json`）。在一次性 Windows 11 Enterprise 10.0.26100（账户 `WDAGUtilityAccount`、计算机名随机、**Networking 禁用**、vGPU 禁用、4 GB 内存）中，对 EnvCon 0.5.5 NSIS 安装包执行 **10 项检查全部通过 / 0 失败 / 0 跳过**：WebView2 运行时 155.0.4283.62、静默安装 exit 0、`AppData\Local\EnvCon\EnvCon.exe` 落位、进程存活且主窗口创建、`current\node` Junction 创建并可读回目标、删除链接后目标 `payload.bin` 存活、受管根目录清理干净、卸载登记 `EnvCon 0.5.5`、静默卸载 exit 0、注册表登记项移除。被测安装包 SHA-256 `4f2ad2bb…77be2` 与当前构建产物逐字节一致。证据 `docs/evidence/r0/r0-sandbox-install-smoke-verification-2026-10-20.json`，原始报告 `scripts/hyperv/sandbox-payload/report.json`。**未覆盖**：16 类工具真实下载与安装、真实用户 PATH/注册表写入、十种管理器真实版本与自更新（仍 0/10）、覆盖升级、代码签名、管理员上下文。**局限**：Sandbox 每次全新，无法覆盖「安装后重启再启动」；报告由被测环境内脚本自产，不构成防篡改认证。**旁证**：Gen 2 Hyper-V 虚拟机因 UEFI 引导失败（`SCSI DVD (0,1) The boot loader failed`）不可用，Gen 1 可引导至安装界面但未完成安装；Sandbox 路线已在同一用途上替代，两者不冲突。 |
| 2026-10-19 | IMP-02 / 更新限制说明与留痕展示 | 第 4 条部分完成；第 3 条的留痕已可见 | 版本面板新增三条限制说明（不支持指定版本降级、依赖变化由管理器解析且不给出影响范围、旧版本可能已下架），**在用户操作前**可见。新增只读命令 `package_manager_update_record`，展示最近一次更新记录（状态、`更新前 → 更新后` 或“未确认新版本”、程序路径），并注明“仅用于审计，应用不会自动恢复到此版本”；清单缺失或不可读时隐藏该行，不谎称从未更新。`package-manager-update-ui` 增加三条断言：限制文案在操作前可见、无记录时不显示记录行、更新后显示“命令完成”与仅审计声明。过程中测试暴露了真实 UI 歧义（记录行的裸版本号与“当前版本”重复匹配），已改为合并成一段文本。`npm run build` 1709 modules / 670ms；Rust 273 passed / 14 ignored；九套 UI 全部 PASS；`git diff --check` 干净。**未完成**：无真正的分阶段进度显示，降级限制只是说明未在能力层禁用。证据 `docs/evidence/imp02/imp02-update-limits-and-record-ui-verification-2026-10-19.json`；未更新真实管理器，未打包/发布。 |
| 2026-10-19 | IMP-02 / 更新前清单（update_journal） | 第 3 条部分完成；明确不是回滚 | 新增 `update_journal` 模块，在更新器启动前把工具名、可执行路径、更新前版本、将运行的程序与参数写入 `backups/updates/journal.json`；写入失败即取消更新，不做无审计执行。完成后按 attempt 令牌落为 `completed`/`unverified`/`failed`——命令退出 0 但读不到版本记 `unverified` 而非成功；未知或已结算令牌报错，不静默忽略。沿用既有 journal 约定（schema 版本、`deny_unknown_fields`、写前重定向检查、临时文件再改名），保留最近 50 条。新增 5 项夹具；Rust 273 passed / 14 ignored；`npm run build` 1709 modules / 684ms；九套 UI 全部 PASS；`git diff --check` 干净。**明确限制**：这些管理器没有受支持的“装回指定旧版本”命令，因此这是审计留痕而非恢复方案，无法校验恢复结果，也未在 UI 展示。证据 `docs/evidence/imp02/imp02-update-journal-verification-2026-10-19.json`；未更新真实管理器，未打包/发布。 |
| 2026-10-19 | IMP-02 / 包管理器更新执行前预览 | 待开始 → 进行中（第 2、4 条部分完成） | 此前点「更新当前工具」立即执行，用户看不到将运行的程序、参数与安装位置。新增只读 `preview_package_manager_update`：展示当前版本、安装位置、精确命令行，并说明更新是替换自身还是通过所属解释器执行；阻断规则 `package_manager_update_blocker` 与真实执行共用，预览不会承诺执行器会拒绝的更新（Yarn Berry 与无法解析版本一律阻断）。目标版本如实显示“无法在更新前得知”，不猜数字。UI 改为先预览再确认，取消不执行。新增 2 项 Rust 夹具；`package-manager-update-ui` 增加“预览零执行、取消零执行、4 次执行对应 4 次确认、pipx 不请求预览”断言；Rust 268 passed / 14 ignored；`npm run build` 1709 modules / 647ms；九套 UI 全部 PASS；`git diff --check` 干净。**未完成**：批量更新、跳过与失败项重试、更新前清单与回滚、降级/依赖变化限制说明。证据 `docs/evidence/imp02/imp02-update-preview-verification-2026-10-19.json`；未更新任何真实管理器，未打包/发布。 |
| 2026-10-19 | R2 / 扫描 IO 失败结构化分类 | R2-g 新增，阶段仍不关闭 | `package_scan` 此前把每个 IO 失败都写成中文自由文本，无法区分权限不足/路径消失/包清单失败，也没有不可访问位置计数。新增 `ScanProblem{kind,path,detail}`（8 类 kind）与 `permissionDenied`，按 `ErrorKind` 分类而非解析文案；结构化项与自由文本同源写入，权限计数不由告警数推断，二者同受 80 条上限约束。旧包扫描页在权限不足时明确提示“这些位置未被扫描”。新增 2 项夹具；Rust 266 passed / 14 ignored；`npm run build` 1709 modules / 705ms；九套 UI 全部 PASS；`git diff --check` 干净。证据 `docs/evidence/r2/r2-scan-io-classification-verification-2026-10-19.json`；未扫描真实 ACL 拒绝目录，`tests/package-scan-ui.mjs` 因被忽略且有既有失败断言未作为门禁，未打包/发布。 |
| 2026-10-19 | 文档一致性 / 证据哈希语义核对 | 核对完成，结论记录 | 逐份核对 `docs/*-verification-2026-10-19.json` 中 `sourceHashes` 与当前文件实际 SHA-256：**42 处仍匹配、112 处已失效**。这是**语义上预期的结果**，不是错误——`sourceHashes` 记录的是「该轮改动完成那一刻」的文件快照，后续轮次继续修改同一文件必然使其失效。核对方式本身验证了这一点：最近一轮（`r3-structure-diagnosis`）记录的 `src-tauri/src/detect/mod.rs` 哈希**仍匹配**，而更早的证据因后续改动而不再匹配。**实际影响**：这些哈希只能用于「证明该轮验证针对的是哪份源码」，**不能**用于判断当前源码是否被篡改或是否仍需重测；后续引用旧证据时必须结合 ROADMAP 中该轮之后的变更记录阅读，不能仅按哈希比对下结论。另有部分证据（如 `r0-asset-audit-2026-10-19.json`）本就不记录 `sourceHashes`，只记录资产哈希，属不同用途。 |
| 2026-10-19 | 代码健康 / 消除唯一的编译警告 | 完成 | `cargo check` 长期报告 `path_ownership::preview_cleanup` 从未使用。核查后确认它**只被测试调用**——生产路径一律走 `preview_cleanup_with_confirmations` 以便显式记录历史条目同意。给它加 `#[cfg(test)]` 并注明理由，`cargo check --offline` 现在**零警告**（此前每轮都要在证据里记录“1 个既有警告”）。完整回归 292 passed / 14 ignored 保持不变，说明测试路径未受影响。 |
| 2026-10-19 | R3 / 卸载隔离夹具覆盖复核 | 复核完成；R3 仍不关闭 | 按第 357 条需求原文逐项比对现有夹具，**8 类场景均已有覆盖**：环境内 pip 包（2 项）、`--user` 包（3 项只读枚举）、外部 venv（夹具建 `project/.venv` 并断言存活）、共享缓存（夹具建 `pip-cache` 并断言存活）、安装器登记（9 项：精确匹配、重复/冲突、执行前后复核）、PATH 混合条目（12 项：有来源免确认、来源未知逐项授权、用户/共享/外部保留）、链接纳管（9 项：不遍历目标、未认领阻止移除）、失败恢复（8 项：占用/写失败/陈旧预览均报 Partial 并保留资源）。每项「移除」断言同时断言「必须存活什么」，因此「只移除所选且可证明归属的资源」有对应证据。基线 Rust 292 passed / 14 ignored。证据 `docs/evidence/r3/r3-uninstall-fixture-coverage-map-2026-10-19.json`。**未勾选**：全部为临时目录夹具，未运行真实卸载器、未写真实注册表或 PATH；实机验收仍受 Windows VM 缺失阻塞；覆盖来自逐项阅读夹具而非变异测试。 |
| 2026-10-19 | R3 / 卸载预览检测目录占用 | 子项部分完成；R3 仍不关闭 | 此前只探测文件占用，但**另一个进程打开的目录同样阻止删除**：用户要么只看到「目录树未能完整读取」而不知原因，要么毫无预警地等到删除失败。新增 `directory_blocks_deletion`，与文件探测同规则（`DELETE` 访问 + `share_mode(7)` + 立即关闭），**只认 `ERROR_SHARING_VIOLATION`**；必须带 `FILE_FLAG_BACKUP_SEMANTICS`，否则打开目录一律返回拒绝访问，那是权限问题、绝不能当作被占用。资源名用独立的「目录占用」而非复用「文件占用」，因为处置方式不同：文件通常关掉占用它的程序，目录往往是被设为工作目录的终端。目录在既有遍历中探测，与文件共用 200 条抽样上限；`LOCKED_DIR_RESOURCE` 与 `LOCK_RESOURCE` 一同在预览相等比较中剥离，避免切换间的打开动作让合法预览被判失效。**过程中发现并修正的真实缺陷**：完全独占的目录句柄（`share_mode(0)`）会让 `read_dir` 本身失败，遍历返回 `Err`，原本只在成功分支的上报会被静默丢弃——临时诊断输出 `DIAG blocks=true inv_ok=false dirs=2 locked_dirs=["cache"]` 证实探测有效而遍历失败；现改为两个分支都点名目录，夹具改用 `share_mode(3)`（允许读写、拒绝删除）模拟真实的工作目录而非极端独占。新增 2 项夹具（成功分支用 `share_mode(3)`、错误分支用 `share_mode(0)` 触发 `read_dir` 失败并断言 `scope_complete` 为 false）；UI 补 4 项断言（无占用不渲染、点名 cache 与「工作目录」处置、计数为「（1 项）」、必须落在「未验证/不自动处理」而不得出现在「将移除」），`release-052-ui` PASS；Rust 299 passed / 14 ignored；`cargo check` 0 warnings。证据 `docs/evidence/r3/r3-uninstall-directory-lock-detection-verification-2026-10-19.json`；只认共享冲突（仍允许 `DELETE` 的打开方式不报告，刻意避免误报），无进程识别，未在真实程序占用真实目录上验证，未打包/发布。 |
| 2026-10-19 | R3 / 按环境重新检测入口 | 子项部分完成；R3 仍不关闭 | 第 350 条要求「重新识别」，此前只有整页刷新（重扫 16 类及其中每个条目），修好一个目录后无法只问「这一个现在好了吗」。把扫描里每条环境的判定抽成 `env_health(env_type, path, accessible) -> EnvHealth{version, status, status_detail}`，`scan_category` 改为调用它；新命令 `redetect_environment` 走 `rescan_category` 取出指定条目，**两个入口共用同一判定**，单条复检因此不可能报出与全量扫描矛盾的状态（夹具用镜像断言锁定该等式，而非只靠重构）。环境名沿用 `switcher::validate_name` 不能逃出受管目录；未配置根目录与「此分类下无该名称」都给明确错误而非空结果；全程只读。UI 每行新增「重新检测」按钮，成功提示「通过」并带版本，失败提示「仍未恢复」并给出状态原因，绝不报成通过。新增 1 项 Rust 夹具（缺 node.exe 报结构原因→补上后转为探测失败，证明不复用旧结论）与 4 项 UI 断言（精确指向被点击的行、成功带版本、失败带原因、失败时不得出现「通过」文案——断言绑定同名行以免上一条未消失的 toast 误判）。Rust 297 passed / 14 ignored；`cargo check` 0 warnings；`npx tsc --noEmit` 通过；九套 UI 全 PASS；`git diff --check` 干净。证据 `docs/evidence/r3/r3-environment-redetect-verification-2026-10-19.json`；重新检测只读指定目录而非整个分类，结果经 toast 呈现无行内就地更新，未在真实修复后的安装上验证，未打包/发布。 |
| 2026-10-19 | R3 / 版本探测失败说明原因 | 子项部分完成；R3 仍不关闭 | 此前 `status_detail` 一律写「版本探测失败，无法确认环境可用性」，而 `run_capture_with_env` 已区分超时/无法启动/退出码+stderr，只是 `probe_version` 返回 `Option` 把它们全部丢弃，用户无法判断是缺文件、进程卡住还是 CLI 报错。新增 `VersionProbe { version, reason }`（两者互斥）与 `probe_version_with_reason`；`probe_version` 保留为 `Option` 薄包装，`install.rs`/`commands.rs` 既有调用方行为不变。Python/Node/Zig/Deno/Bun/Php/Llvm/Git/Gh/Mingw 走 `probe_simple`：缺文件报「缺少可执行文件：<路径>」、启动失败或超时报「<命令> 失败：<原因>」、输出不可解析报「<命令> 的输出没有可识别版本：<片段>」，片段压成一行并截断 120 字符。Jdk/Go/Rust/Maven/Gradle/Adb 保留原探测并如实说明「此类型目前不提供结构化原因」——编一个未观测到的原因比不提供更糟。新增 2 项夹具（缺文件点名路径、不可执行文件来自真实启动失败、版本与原因互斥、Maven 如实说明、`probe_version` 契约不变）；Rust 296 passed / 14 ignored；`cargo check` 0 warnings。证据 `docs/evidence/r3/r3-version-probe-reason-verification-2026-10-19.json`；六类仍无结构化原因，原因仍是自由文本而不可过滤，未在真实故障安装上探测，未打包/发布。 |
| 2026-10-19 | R3 / 损坏目录点名缺失入口 | 子项部分完成；R3 仍不关闭 | `looks_like` 只返回布尔值，扫描把它折成「目录结构不完整或缺少必要可执行文件」，用户无法判断是**可修的一个文件缺失**还是**选错了目录**。现抽出 `structure_problem(env_type, root) -> Option<String>` 点名缺失入口，`looks_like` 定义为 `structure_problem(..).is_none()`，两者不可能分歧。有备选语义的类型保持原样（Maven/Gradle 接受 `.cmd` 或 `.bat`；Rust 接受 `cargo-home/bin/rustc.exe` 或工具链目录；Adb 沿用既有探测）。扫描保留原因，broken 的 `status_detail` 变为「目录结构不完整：<原因>」；结构正常但版本探测失败仍走单独文案，两种失败模式保持可区分。新增 2 项夹具（点名 node.exe / bin/javac.exe，补齐后 `looks_like` 同步转真；对全部 16 类断言 `looks_like` 与 `structure_problem(..).is_none()` 恒等）；Rust 292 passed / 14 ignored；`cargo check` 0 errors。证据 `docs/evidence/r3/r3-structure-diagnosis-verification-2026-10-19.json`；无「重新检测」按钮（总览每次扫描本就重读），版本探测失败仍不附带 stderr，未在真实损坏目录上验证，未打包/发布。 |
| 2026-10-19 | R3 / 环境清单导入（只读比对） | 子项部分完成；R3 仍不关闭 | 此前清单**只能写不能读**：`EnvironmentManifest` 只有 `Serialize`，`notes` 还写着「导入不会自动执行」却没有读回入口。补上导入的诚实形态——**只读差异比对**而非隐式还原（清单记录单机绝对路径且不含文件内容，还原必须凭空决定文件来源）。`compare_manifests` 为纯函数，配对键（类型, 小写名称），每条只落入 `unchanged`/`added`/`missing`/`changed`；新增行不留清单一侧、缺失行不留本机一侧。三处刻意细节：路径经 `path_key` 比较，大小写/分隔符/尾分隔符差异不产生幻影变更；**记录版本为 null 表示当时探测失败**，即便现在已知版本也不算变更；根目录不同才出现「跨机器比对」说明。`validate_manifest_source` 先拒绝相对路径、非 `.json`、不存在与目录，`schemaVersion` 单独校验。新增 2 项夹具；Rust 294 passed / 14 ignored；`cargo check` 0 warnings；`tsc --noEmit` 通过；`release-052-ui` 新增 8 项断言 PASS。证据 `docs/evidence/r3/r3-environment-manifest-import-verification-2026-10-19.json`；备份与恢复完全未实现，配对按名称（同名不同目录视为同一条目），跨机器比对会对每条报「路径」差异且无路径映射 UI，未导入真实他机清单，未打包/发布。 |
| 2026-10-19 | R3 / 环境清单只读导出 | 子项部分完成；R3 仍不关闭 | 此前环境层面完全无法导出（只有设置与 PATH 有备份机制）。新增 `ManifestEntry`/`EnvironmentManifest`：名称、类型、本机绝对路径、解析后身份路径、版本、状态、链接/current 标记，**不含任何可据此重建环境的内容**。`build_manifest` 不碰文件系统与时钟，映射/排序/措辞全由夹具覆盖；条目按（类型，小写名称）确定性排序，同状态两次导出字节一致。跨机器限制写成可核对事实：固定四条 + 便携模式与未配置根目录两条条件项，其中「空清单不代表没有安装」只在确实没有根目录时出现。`validate_manifest_target` 在写入前拒绝相对路径、非 `.json` 与无父目录；落盘走 `config::atomic_write`，目标路径不会出现半截清单。UI 新增「导出清单」按钮。新增 3 项夹具；Rust 290 passed / 14 ignored；`npm run build` 1709 modules / 524ms；九套 UI PASS；`git diff --check` 干净。证据 `docs/evidence/r3/r3-environment-manifest-export-verification-2026-10-19.json`；导入/还原/跨机器迁移均未实现，未从 UI 真实写盘验证，未打包/发布。 |
| 2026-10-19 | R3 / 悬空受管链接可被识别与清理 | 子项部分完成；R3 仍不关闭 | `preview_uninstall_scope` 原先把「链接目标不可解析」与「链接对象本身读不了」混为一谈：前者也让 `scope_complete=false` 并压入「管理链接目标」阻止项，结果是**最需要修复的悬空链接反而被应用拒绝处理**。现拆成两条路径——`link_target_path` 失败仍阻断（对象身份无法确认），已读到目标却 `canonicalize` 失败时记 `unresolved_target`，生成可移除的「失效链接入口」，明确「只移除链接对象、绝不跟随或删除任何目标」，且不再谎称保留已不存在的「受管目标目录」。执行端无需新路径（`link_ownership` 本就用 `FILE_FLAG_OPEN_REPARSE_POINT` 按句柄删除，从不解析目标），由夹具端到端证明。新增 2 项夹具（预览可清理且不阻断；执行后入口消失、目标未被重建）；Rust 287 passed / 14 ignored；`release-052-ui` PASS；`git diff --check` 干净。证据 `docs/evidence/r3/r3-dangling-link-repair-verification-2026-10-19.json`；只覆盖悬空链接，损坏的普通目录仍无修复动作，未在真实运行时上复现，未打包/发布。 |
| 2026-10-19 | R3 / 卸载新增「停用（保留文件）」模式 | 子项部分完成；R3 仍不关闭 | `uninstall_env` 原先没有 mode 参数，三种要求模式中只有“卸载”存在。新增 `UninstallMode { Uninstall, Detach }` 随 `UninstallPreview` 传递（`serde(default)` 保证旧客户端按 `Uninstall` 解析）；`apply_detach_mode` 把目录/链接项从 `removable` 移到 `retained` 并改写说明，**当前版本 Junction 与有来源证据的 PATH 仍移除**——否则环境仍在生效，不叫停用。执行端在 Detach 下整段跳过删除分支。**原厂卸载器与停用互斥**：原厂卸载删除的正是停用承诺保留的内容，预览写明原因并在执行前拒绝。**两处必须修的接线**：解除 Junction 后的复核会重新生成预览，不带同一 mode 会让每次 Detach 被判“卸载预览已变化”；`uninstall_env` 原先用默认模式重建基线，Detach 永远匹配不上。UI 确认框新增“选择处理方式”，切换模式重新预览。新增 2 项夹具（保留字节且解除 active；模式与范围不符的提交在改动前被拒）；Rust 285 passed / 14 ignored；`npm run build` 1709 modules / 517ms；九套 UI PASS；`git diff --check` 干净。证据 `docs/evidence/r3/r3-uninstall-detach-mode-verification-2026-10-19.json`；「仅移除管理记录」未单独实现，未在真实受管目录上执行，未打包/发布。**同日再补**：`release-052-ui` 增加停用模式断言（切换后重新预览一次且 `mode=detach`、`aria-pressed` 切换、目录移到「明确保留」、主按钮变「确认停用」、取消不执行），并修掉两处自相矛盾文案——停用模式下对话框仍写「将永久删除安装目录」、主按钮仍是「确认删除目录」；`npm run build` 1709 modules / 484ms；九套 UI PASS。 |
| 2026-10-19 | R3 / 卸载预览前置文件占用检测 | 子项部分完成；R3 仍不关闭 | 此前文件占用只会在 `execute_uninstall_scope` 失败后暴露，且此时 current Junction 已解除，用户面对半完成状态。新增 `file_blocks_deletion`：以 `DELETE` 访问权打开后立即关闭（不修改内容），**只把 `ERROR_SHARING_VIOLATION` 判为占用，访问拒绝是权限问题不冒充占用**。预览最多探测 200 个文件并如实区分“已检查全部”“已抽样检查前 N 个（共 M 个）”“目录内没有普通文件未做检查”。占用进入 `unverified`（资源名「文件占用」），列出前 10 个相对路径并提示先关闭占用程序；**占用不阻断预览**。**顺带修掉本改动暴露的真实回归**：`validate_uninstall_execution_preview` 用整份预览做相等比较，而占用是易变观察，预览与执行之间程序打开文件会让合法预览被判“已变化”；现两侧比对前剔除 `LOCK_RESOURCE` 条目。新增预览占用夹具 1 项，既有占用失败夹具恢复通过；Rust 283 passed / 14 ignored；卸载套件 31 passed；九套 UI PASS；`git diff --check` 干净。证据 `docs/evidence/r3/r3-uninstall-preview-lock-detection-verification-2026-10-19.json`；未用真实运行程序占住真实环境文件，未做进程识别，未打包/发布。 |
| 2026-10-19 | IMP-02 / 结构化版本范围能力 | 子项缺口收窄；IMP-02 仍不勾选 | 原先把“不支持指定版本降级”作为笼统说明，会被误读为管理器缺陷；实际上本模块构造的每条命令都只装 latest、根本不接受版本参数。新增 `UpdateCapabilities{latestOnly, manualPinnedCommand}` 随每个预览返回（被阻止时也返回，因为这是入口限制）：`latestOnly` 说明入口范围，`manualPinnedCommand` 仅在该管理器确有文档化命令时给出模板（npm/pnpm/pip/composer/uv），yarn/bun/pipx/cargo/dotnet 返回 `None`——表示“无可背书单行命令”而非“不可能”。UI 预览显示“版本范围”与“指定版本需手动执行”取代笼统说法。新增 `update_capabilities_state_the_entry_point_limit_and_real_pin_commands` 夹具；Rust 282 passed / 14 ignored；`npm run build` 1709 modules / 537ms；九套 UI PASS；`git diff --check` 干净。证据 `docs/evidence/imp02/imp02-update-capabilities-verification-2026-10-19.json`；模板取自各管理器公开 CLI 文档、未在本机执行，未打包/发布。 |
| 2026-10-19 | IMP-02 / composer 与 bun 安装形态评估 | 评估完成；IMP-02 仍不勾选 | 复核这两个管理器是否存在可靠的安装形态证据，结论是**都没有**：composer 启动器只能证明它包装了 `composer.phar`，无法反证不是其他安装器所生成；Windows 上各 bun 发行版写出的都是同一个静态 `bun.exe`，scoop/winget 的 shim 位于安装目录之外、二进制旁不留标记。按“只在有可正面验证的信号时才拒绝”的原则，两者保持 `Standalone`，避免误禁本可正常执行的更新。在 catch-all 分支写明原因，并用 `composer_and_bun_stay_standalone_because_no_reliable_evidence_exists` 锁定该决定（含“uv 判定不得泄漏到 bun”的反例）。Rust 281 passed / 14 ignored；`git diff --check` 干净。证据 `docs/evidence/imp02/imp02-composer-bun-channel-evaluation-2026-10-19.json`；未检视真实 composer/bun 安装，未打包/发布。 |
| 2026-10-19 | IMP-02 / 运行时能力判定提升到列表层 | 子项缺口收窄；IMP-02 仍不勾选 | 上一轮只在 `self_update_command` 内部做安装形态判定，因此工具列表仍以静态 catalog 判断可更新性——由 Corepack 管理的 pnpm 或 pip 安装的 uv 会显示为可更新，只有点开预览才被拒绝。本轮抽出 `self_update_channel_blocker` 作为唯一转换点，执行器与列表共用，文案不会漂移；`ToolConfig` 新增 `selfUpdateBlocker`（`#[serde(default)]`），由同一个已解析 `Invocation` 填入。UI 在列表层即禁用“更新当前工具”并在版本面板显示不可用原因。新增 2 项夹具锁定“列表原因 === 执行器原因”与 IPC 字段名及旧快照兼容；Rust 280 passed / 14 ignored；pkgtools 26 passed；`npm run build` 1709 modules / 503ms；九套 UI PASS；`git diff --check` 干净。证据 `docs/evidence/imp02/imp02-runtime-capability-verification-2026-10-19.json`；未在真实 Corepack/pip 安装上验证，UI 夹具尚未建模该字段渲染，未打包/发布。 |
| 2026-10-19 | IMP-02 / 按实际安装方式诊断 | 子项缺口收窄；IMP-02 仍不勾选 | 新增 `UpdateChannel`（Standalone/Corepack/PythonPackage），在生成任何自更新命令前依据命令文件旁的证据判定安装形态：正文指向 Corepack 的 npm/pnpm/yarn 拒绝自更新（会与 Corepack 版本固定冲突），与 `pip.exe` 同级且上层有解释器的 uv 拒绝 `uv self update`（会覆盖 pip 管理的文件）。**无法读取的命令文件不作为证据**，回退 `Standalone` 而非猜测。判定置于 `self_update_command` 顶部，预览与执行器给出同一原因。**夹具暴露并修正一处漏判**：pip 把 uv 装进 `<Python>/Scripts`，解释器在上一层而非同级，原判定因此漏掉真实布局。新增 1 项夹具覆盖四种形态；Rust 278 passed / 14 ignored；九套 UI PASS。证据 `docs/evidence/imp02/imp02-update-channel-diagnosis-verification-2026-10-19.json`；未在真实 Corepack/pip 安装上验证，composer 与 bun 仍无判定，未打包/发布。 |
| 2026-10-19 | IMP-02 / 批量更新、跳过与失败项重试 | 子项缺口补齐；IMP-02 仍不勾选 | 新增只读 `preview_package_manager_updates`（汇总十个管理器的预览并给出 ready/blocked 划分，被阻止项保留原因）与 `update_package_managers`（全程只取一次独占锁，逐项复用单工具执行器）。**跳过的语义是执行前的决定**，带原因、不计失败、不进入重试；只有 `commandFailed` 记为 `failed`，`retryable` 由 items 派生。UI 新增“批量更新”按钮、预览对话框、结果对话框与“重试失败项”（合并回首次结果）。**修复重构缺陷**：守卫最初未绑定即被丢弃，会静默解除所有锁，已在验证前修正。新增 2 项 Rust 夹具 + 批量 UI 断言（预览零执行、只发 ready 项、跳过项不进执行器、重试仅重发失败项）；Rust 277 passed / 14 ignored；九套 UI PASS；`npm run build` 1709 modules / 537ms；`git diff --check` 干净。证据 `docs/evidence/imp02/imp02-batch-update-verification-2026-10-19.json`；未执行真实管理器更新，未打包/发布。 |
| 2026-10-19 | R0 / 镜像差异比对与发布方校验资料复查 | R0 证据补齐；阶段仍不关闭 | 6 类双源资产的镜像与官方文件**逐字节一致**（Node/Go/Gradle/Maven/Python/JDK 各 6/6）。原记“无校验资料”的 4 类经复查**均存在发布方校验文件**：Rust `.sha256`、PHP `sha256sum.txt` 为 SHA-256 且与本地一致，已接入代码；Maven 仅 SHA-512、ADB 仅 SHA-1，因校验器只接受 SHA-256 而明确标注原因。**修复真实缺陷**：`download_spec_with_verification` 原先无条件覆盖分支校验资料，导致镜像 URL 会继承官方摘要、Rust 丢弃已解析的校验文件；现改为“分支已决策则保留”。新增 2 项夹具；Rust 264 passed / 14 ignored；`cargo check` 0 errors / 1 个既有警告。证据 `docs/evidence/r0/r0-mirror-and-publisher-checksums-verification-2026-10-19.json`；Windows VM 安装冒烟仍未做，未打包/发布。 |
| 2026-10-19 | R0 / 官方 16 类资产在线复测与十管理器矩阵 | R0 资产证据更新；阶段仍不关闭 | 在线审计成功执行（381.83 秒）：16/16 完整下载、长度一致、ZIP/PE 结构检查通过；**12/16 取得官方 SHA-256 且全部与本地一致**（JDK、Python、Node、Go、Gradle、LLVM、Zig、Deno、Bun、Git、GitHub CLI、MinGW）；Rust rustup-init、Maven、PHP、ADB 无官方校验资料，保持“未验证”。十管理器离线矩阵完成：10/10 隔离夹具通过（`package_managers::` 16 passed），真实版本与自更新 0/10 并记录跳过原因。证据 `docs/evidence/r0/r0-asset-audit-2026-10-19.json`、`docs/evidence/r0/r0-package-manager-matrix-2026-10-19.json`；镜像差异与 Windows VM 安装冒烟仍未做，未打包/发布。 |
| 2026-10-19 | IMP-06 / 安装任务跨页恢复 | 待开始 → 进行中（跨页缺陷已修，统一契约未做） | 安装成功后任务原先 2 秒即从后端移除，离开页面后结果丢失。现保留 30 分钟；前端新增 `dismiss`（仅本地隐藏，拒绝隐藏运行中任务），`init` 改为合并而非覆盖，取消任务也会补报。新增下载 UI 场景覆盖 done/error/canceled 三类跨页任务且不重复提示；`npm run build` 1709 modules；九套 UI PASS；Rust 262 passed / 14 ignored。证据 `docs/evidence/imp06/imp06-install-task-recovery-verification-2026-10-19.json`；未跑真实长时安装，任务状态仍在内存中（重启后丢失）。 |
| 2026-10-19 | R2 / 扫描入口版本冲突 UI | UI 回归通过；实机待验证 | `ScanResults` 在受影响条目内展示“同一安装的其他入口版本不同（N 个）”，逐项列出路径与“报告 <版本>”或“无法读取版本（<错误>）”；折叠时分组标题显示“N 个入口版本不一致”。路径 `break-all` 换行，390px 无横向溢出。`scan-ui` 新增断言并保存 `tests/scan-conflicts-390.png`；`npm run build` 1709 modules；九套 UI PASS；Rust 262 passed / 14 ignored。证据 `docs/evidence/r2/r2-scan-conflict-ui-verification-2026-10-19.json`；未在真实主机扫描到真实分歧、未打包/发布。 |
| 2026-10-19 | R2 / 同安装入口版本不一致 | 隔离实现与回归通过；实机待验证 | 每个重复入口现在都会探测，版本差异写入 `ExternalEnv.versionConflicts`，扫描为该工具生成警告并降级为 `Partial`，不再静默丢弃差异。另两类情形复核为已有覆盖：不可执行启动器→`StartFailed`、权限拒绝→`PermissionDenied`。新增 2 项夹具；`cargo test --offline --lib` 262 passed / 14 ignored；九套 UI PASS；`cargo check` 0 errors / 1 个既有警告。证据 `docs/evidence/r2/r2-scan-entry-version-conflicts-verification-2026-10-19.json`；未在真实主机扫描到真实版本分歧、未打包/发布。 |
| 2026-10-19 | R1 / 校验文件语义与配置路径注入边界 | 隔离实现与回归通过；在线校验与实机待验证 | `parse_sha256_checksum_file` 区分“列出了其他资产”与“无可用条目”，前者报告条目数并提示版本/资产名可能变化。`validate_tool_config_layout` 拒绝含分号/引号/空字符/换行的配置目录，理由是这些值会写入管理器配置文件。新增 3 项夹具（含离线缓存必须降级可信哈希）；复核确认 `resolve_download_spec` 每次重新拉取并解析校验文件，无旧哈希复用路径；另记录 14 处外部命令均为结构化参数、不经 shell。`cargo test --offline --lib` 260 passed / 14 ignored；九套 UI PASS；`cargo check` 0 errors / 1 个既有警告。证据 `docs/evidence/r1/r1-checksum-and-config-injection-verification-2026-10-19.json`；未做在线校验文件拉取、未写真实管理器配置、未打包/发布。 |
| 2026-10-19 | ASMT-04 / 修复后重新诊断闭环 | 实现并回归通过；实机待验证 | `apply_path_repair` 改为返回 `PathRepairResult`：写入后重读注册表并重新解析，`verified` 仅在实时用户 PATH 与计划一致且无命令仍被覆盖时为真；返回实时 PATH、逐命令状态、具体问题与“需新终端”。UI 区分“已调整并核验”与“已写入但核验未通过”。新增 1 项隔离夹具与两条 UI 断言；`cargo test --offline --lib` 257 passed / 14 ignored；九套 UI PASS；`npm run build` 1709 modules。证据 `docs/evidence/asmt04/asmt04-repair-rediagnosis-verification-2026-10-19.json`；未写真实用户 PATH、未开新终端。 |
| 2026-10-19 | ASMT-04 / PATH 顺序修复预览 | 只读预览与受绑执行实现，隔离/UI 回归通过；实机待验证 | 新增 `ReorderPlan` 与 `Change::Reorder`：绑定精确 PATH 文本、受管入口与冲突目录；执行前从实时 PATH 重算并拒绝陈旧计划；写入前后条目集合必须一致，只调顺序不增删；写入后复查并记账。`preview_path_repair` 返回 `reorder`/`manual`/`none`，manual 时列出需手动处理的目录。环境页对当前环境提供“预览 PATH 修复”，只读对话框展示入口、冲突与结果 PATH，无绑定计划时禁用执行。新增 6 项隔离夹具与 UI 场景；`cargo test --offline --lib` 256 passed / 14 ignored；九套 UI PASS；`npm run build` 1709 modules。证据 `docs/evidence/asmt04/asmt04-path-repair-verification-2026-10-19.json`；未写真实用户 PATH、未开新终端对照。 |
| 2026-10-19 | ASMT-04 / 切换后命令诊断 | 待开始 → 只读诊断实现并回归通过；实机待验证 | `switch_env` 改为返回 `SwitchDiagnosis`：读取新 current 链接目标、探测环境版本、按 16 类环境列出应解析命令并复用只读合并 PATH 解析，区分 `managed`/`shadowed`/`missing` 与 `not-in-path`；被覆盖或缺失时 UI 降级为“命令生效待确认”。新增 2 项隔离夹具与 `release-052-ui` 切换场景；`cargo test --offline --lib` 250 passed / 14 ignored；九套 UI PASS；`npm run build` 1709 modules。证据 `docs/evidence/asmt04/asmt04-switch-diagnosis-verification-2026-10-19.json`；未在真实环境切换、未开新终端对照，可预览修复入口未实现。 |
| 2026-10-19 | PATH-01 / 批量受影响包与 R3-b Python 用户级包 | 隔离与 UI 回归通过，实机待验证（提前实现） | 批量预览对话框按工具分组展示受影响包（名称、版本、失效标记与计数）；无包时显示原因。新增只读 `PythonUserScope`：解析 `PYTHONUSERBASE`/`%APPDATA%\Python`，按版本目录统计 `.dist-info`/`.egg-info`，标注多解释器共用，相对/无法解析/读取失败分别报告；卸载预览保留项携带路径与数量。新增 3 项隔离夹具与批量 UI 断言；`cargo test --offline --lib` 248 passed / 14 ignored；九套 UI PASS；`npm run build` 1709 modules。证据 `docs/evidence/path-01/path-batch-packages-and-python-user-scope-verification-2026-10-19.json`；未读取真实用户级安装、未打包/发布。 |
| 2026-10-19 | PATH-01 / 受影响全局包清单 | 进行中；隔离与 UI 回归通过，实机待验证（提前实现） | 新增只读 `packages::global_package_rows` 与 `pkgtools::read_affected_packages`，从当前全局目录读出现有包名/版本/诊断，不执行管理器命令；命令入口变化时标记 `unresolved` 并计数，入口不变标 `kept`；空读取与缺失/不可读目录分别给出原因，不伪造空清单。`ToolConfigPreview` 新增 `affectedPackages`，单工具预览列出具体包与“入口可能失效”标记。新增 3 项隔离夹具与 UI 断言；`cargo test --offline --lib` 245 passed / 14 ignored；九套 UI PASS；`npm run build` 1709 modules。证据 `docs/evidence/path-01/path-affected-packages-verification-2026-10-19.json`；未读取真实全局安装、未打包/发布。 |
| 2026-10-19 | PATH-01 / 一键配置批量预览 | 进行中；隔离与 UI 回归通过，实机待验证（提前实现） | 新增只读 `preview_tool_configs`/`ToolConfigBatchPreview`，逐个复用单工具检查并返回 `ready`/`blocked`；被阻止项清空变更行且 `changedCount=0`，单项预览失败记为阻止而非中断整批。`ToolConfigPreview` 新增 `changedCount`。UI“一键配置全部”改为先显示只读汇总，确认后仅写入 `ready` 项并分报已配置/已跳过/失败。新增 3 项隔离夹具与批量 UI 场景；`cargo test --offline --lib` 242 passed / 14 ignored；九套 UI PASS；`npm run build` 1709 modules。证据 `docs/evidence/path-01/path-batch-config-preview-verification-2026-10-19.json`；未写真实管理器配置、未打包/发布。 |
| 2026-10-19 | PATH-01 / 配置差异预览 | 进行中；隔离与 UI 回归通过，实机待验证（提前实现） | 新增只读 `preview_tool_config`/`ToolConfigPreview`，复用与写入相同的 catalog 检查、布局校验和目标计算，但不创建目录、不执行 `config set`、不写环境变量；返回逐键旧值/新值、角色分类、阻止原因、入口变化、合并 PATH 后的解析状态和“已装包不搬移”说明。UI“仅设置路径”改为先预览，`blocked` 非空时禁用“写入配置”。新增 4 项隔离夹具与 `packages-ui` 预览/阻止场景；`cargo test --offline --lib` 239 passed / 14 ignored；九套 UI PASS；`npm run build` 1709 modules。证据 `docs/evidence/path-01/path-config-preview-verification-2026-10-19.json`；未写真实管理器配置、未打包/发布。 |
| 2026-10-19 | PATH-01 / Yarn 2+ 迁移门禁 | 进行中；隔离与 UI 回归通过，实机待验证（提前实现） | 新增 `migration::Capabilities` 由实际管理器版本推导 `yarn_berry`；`preview`/`execute` 在任何目录操作前拒绝 Berry 的 `global` 迁移，命令层读取精确可执行文件版本而非静态 catalog。`ToolMigration` 对 Yarn 2+ 隐藏“全局包目录”并说明 `globalFolder` 语义，只保留缓存迁移。新增 2 项隔离夹具与 `packages-ui` Berry 场景；`cargo test --offline --lib` 235 passed / 14 ignored；九套 UI PASS；`npm run build` 1709 modules。证据 `docs/evidence/path-01/path-yarn-capability-verification-2026-10-19.json`；未安装真实 Yarn Classic/Berry、未打包/发布。 |
| 2026-10-19 | PATH-01 / 命令解析优先级检查 | 进行中；隔离回归通过，实机待验证（提前实现） | 新增只读 `command_resolution`、`launcher_statuses` 与 `CommandResolution`：按系统→用户的合并顺序解析新终端将命中的启动器，区分 `managed`、`shadowed`、`missing`，不执行命令也不写 PATH；`%VAR%` 展开、未知变量保留原文、相对条目不计命中。`apply_tool_config` 返回新增“命令解析”与“受管入口被 PATH 靠前位置覆盖”条目。新增 3 项隔离夹具；`cargo test --offline --lib` 233 passed / 14 ignored；九套 UI PASS；`npm run build` 1709 modules；`cargo check` 0 errors / 1 个既有警告。仅临时目录/模拟 UI，未对照真实注册表与新终端，未打包/发布。 |
| 2026-10-19 | REL-01 / R4 可执行部分 | 待开始 → 进行中（实现与隔离测试通过，CI/真实发布待验证） | 新增 Windows CI（`npm ci`、`npm run build`、`cargo test --lib`、固定 Playwright + `npm run verify`）、`scripts/verify-fresh-clone.ps1`、`scripts/record-release.ps1`、`docs/guides/release-process.md` 和版本检查/升级提示。更新检查后端区分五种状态，只读取 GitHub Releases 元数据，不自动下载/安装；设置页提供“检查更新”和“打开发布页”。`app_update::tests` 4 passed / 1 ignored；`tests/settings-ui.mjs` 更新检查可用/失败场景 PASS；完整 `npm run verify`：Rust 230 passed / 14 ignored、1709 modules、两套读状态及九套 UI PASS；`cargo check --offline` 0 errors / 1 个既有警告；`git diff --check` 0；fmt 缺组件未执行。证据 `docs/evidence/r4/r4-release-verification-2026-10-19.json`。CI 未运行、真实 Release 未检查、tag 未创建/推送；版本仍 0.5.5，未打包、未发布。 |
| 2026-10-19 | ASMT-06 / 干净克隆 | 脚本已实现；当前 HEAD 未通过 | `scripts/verify-fresh-clone.ps1 -SkipInstall -SkipTests` 实际克隆当前 HEAD 后报告 11 个缺失文件：`PRODUCT_ASSESSMENT.md`、三份 `src-tauri/tests/*.rs` 集成测试、三份 UI 测试、`.github/workflows/windows.yml`、`docs/guides/release-process.md` 和两份发布脚本。源工作区 `.gitignore` 检查未新增误伤；`src-tauri/tests` 已从整目录忽略改为只允许三份明确集成测试，未列入文件仍被忽略。离线 `sources_test` 的 3 项 download spec 测试通过。上述文件尚未提交，故干净克隆无法复现当前验证；提交前不关闭 ASMT-06。 |
| 2026-10-19 | AUD-016 / DL-VERIFY-01 资产审计解析路径 | 隔离实现与测试通过；在线资产审计未执行 | 修复 `resolve_download_spec` 的校验文件解析返回值编译错误，并让官方资产审计使用与生产相同的 `resolve_verification` 路径：HTTPS 校验文件解析为可信 SHA-256，失败/无效资料单独记录；审计输出新增 `officialSha256`、`officialSha256Matches` 与校验元数据，哈希不符归为 `downloaded-but-check-failed`。新增 `verification_resolution_keeps_trusted_metadata_and_rejects_insecure_checksum_files`；`cargo test --offline --lib sources::tests::verification_resolution_keeps_trusted_metadata_and_rejects_insecure_checksum_files -- --exact --quiet` → 1 passed；`cargo test --offline --lib sources::tests -- --test-threads=1 --quiet` → 16 passed / 3 ignored；`cargo check --offline` → 0 errors / 1 个既有 warning。在线 16 类资产审计因本轮网络受限未执行，R1 仍为提前实现、待验收，R0 不关闭。 |
| 2026-10-19 | PATH-01 / uv 旧布局恢复、实际入口查询与扫描警告 | 进行中；恢复执行核心、实际入口查询及隔离/UI 回归通过，真实 uv 执行待验证（提前实现） | `recover_uv_layout` 接入精确预览绑定：执行前复核布局、receipt 哈希和旧入口内容哈希；按 receipt 精确 `name==version` 执行 `uv tool install --force`，使用独立 `UV_TOOL_BIN_DIR`，复查新 receipt、入口文件和版本后才删除旧入口；失败、残留和预览后变化保留旧数据并报告部分结果；共享缓存仅在全部可恢复且无人工项时迁移。扫描结果新增 uv 旧布局警告（有效 `UV_TOOL_DIR`、受管 `uv-global`、默认工具目录），UI 增加按预览确认的恢复入口。`pkgtools::values` 对 pnpm/Yarn/uv/pipx/Bun/Composer 查询实际命令入口，查询失败时回退声明布局；`commandPathSource` 与 UI 明确显示“实际查询/布局推导”。新增 5 项 `uv_recovery` 隔离夹具、1 项扫描警告夹具、1 项实际入口夹具和 1 项入口来源夹具；`cargo test --offline --lib` 225 passed / 13 ignored；完整 `npm run verify` 生产构建、两套读状态及九套 UI PASS；`cargo check` 0 errors / 1 个既有警告；`git diff --check` 0。证据 `docs/evidence/path-01/path-layout-verification-2026-10-19.json`；仅临时目录/模拟 UI，未执行真实 uv 恢复、未写真实 PATH/注册表、未打包/发布。 |
| 2026-10-19 | PATH-01 / 布局策略与 uv 旧布局 | 进行中；隔离/UI 回归通过，恢复执行待验证（提前实现） | 显式命令入口策略覆盖十种管理器：npm/.NET 根入口、Cargo/Composer/Yarn 固定子目录、uv/pipx/Bun/pnpm 兄弟入口；`validate_layout` 拒绝命令入口与声明布局不一致及缓存/配置嵌套。新增 uv 旧布局只读检查，区分可规划恢复与需人工处理，损坏/自定义来源/外部入口不标记可恢复；旧布局阻止迁移和清理，兄弟/旧入口清理只接受 receipt 声明路径。PATH 写入后区分当前进程、新终端、重复和缺失。`cargo test --offline --lib` 217 passed / 13 ignored；`npm run verify` 生产构建 1708 modules、两套读状态及九套 UI PASS（`packages-ui` 纳入回归）；`cargo check` 0 errors / 1 个既有警告；`git diff --check` 0；fmt 缺组件未执行。证据 `docs/evidence/path-01/path-layout-verification-2026-10-19.json`；未写真实 PATH/注册表、未执行真实恢复/安装/卸载、未打包/发布。 |
| 2026-10-19 | PATH-01 / 布局语义与 uv 旧布局 | 进行中；只读模型与夹具开始 | 开始把 npm/.NET 根目录入口、Cargo/Composer 固定子目录和 uv/pipx/Bun/pnpm 兄弟入口写成显式策略；为 uv 旧 `tool/bin`、`tool/cache` 布局建立只读检查，拒绝迁移让旧布局继续存在，并把清理校验从“必须位于工具根目录”改为“必须位于该管理器声明的入口目录”。负责人 Codex、目标 0.5.7～0.5.8；提前实现、验收待 R0 关闭；不写真实配置、不执行真实卸载。 |
| 2026-10-19 | UNINST-01 / R3-h 生产执行链 | 实现及隔离/UI 回归通过；实机待验证（提前实现） | 精确登记计划 `ready` 时直接运行固定 EXE 和参数，不经过 shell；生产执行器带超时及 Job Object 进程树终止。执行前重新读取登记并核对文件哈希；先移除 EnvCon 当前 Junction，执行后同时复查登记删除和运行时目录消失。命令失败、3010、登记/目录残留、复查失败、Junction 失败和 PATH 写失败均返回部分结果；`ready` 计划拒绝递归删除回退，PATH 仅在原厂范围核验成功后清理。新增 7 项命令层夹具及同步执行器超时夹具；`cargo test --offline --lib installer -- --test-threads=1` 21 passed，完整 `npm run verify` Rust 212 passed / 13 ignored、构建及八套 UI PASS。证据 `docs/evidence/uninstall/installer-uninstall-verification-2026-10-19.json`；未运行真实卸载器、未写真实 PATH/注册表、未打包/发布。 |
| 2026-10-19 | UNINST-01 / R3-h | 进行中；生产执行链开始 | 开始把已通过隔离回归的登记审计计划接入生产卸载：仅 `ready` 计划执行精确 EXE，执行前复查登记指纹和文件哈希；先解除 EnvCon 当前 Junction，原厂范围核验成功后才清理 PATH，失败/残留不回退递归删除。负责人 Codex、目标 0.5.9；真实卸载器仍未运行，提前实现、验收待 R0 关闭。 |
| 2026-10-19 | R3-g / 历史 PATH 执行复核 | 进行中；代码及隔离/UI 回归通过，实机待验证 | 来源日志语义快照、精确确认契约、重复/候选外/改写/共享依赖/预览后变化拒绝、未选择保留、部分选择、写失败/复查失败/晚到依赖部分结果均已接通。新增手工保存恢复语义夹具：显式保存只写 `manual/verified` 基线并清空所有权，pending 可被用户确认实际 PATH 后恢复，但旧条目不会被认领。完整 `npm run verify`：Rust 205 passed / 13 ignored、1708 modules 构建、两套读状态及八套 UI PASS；`release-052-ui` 单独 PASS；`cargo check --offline` 0 errors / 5 个既有警告，`git diff --check` 0，fmt 缺组件未执行。负责人 Codex、目标 0.5.9；仅临时目录/模拟 PATH，未写真实用户环境、未打包/发布。证据 `docs/evidence/uninstall/legacy-path-verification-2026-10-19.json`。 |
| 2026-10-17 | R3-g / 历史 PATH 首轮定向 | 进行中；8 passed / 2 failed | 新历史候选取代部分原 retained 项，两项旧断言失败；未把此轮计为通过。补充协议决定：未勾选项保留，不强制用户为了删除环境授权清理未知来源 PATH。仅临时目录及模拟存储，未写真实用户环境。 |
| 2026-10-17 | R3-g / 历史 PATH 逐项确认 | 待开始 → 进行中（实现与隔离夹具待验证） | 复核确认自动清理只接受连续、已复查的 EnvCon 写入证据；当前候选均因缺乏来源证明而保留。开始为固定 current PATH 项增加单独、默认未勾选的历史候选确认，严格限制到预览列出的精确 PATH 字符串；确认不建立 EnvCon 创建来源。负责人 Codex，目标 0.5.9；提前实现、验收待 R0 关闭。禁止真实 PATH/注册表写入。 |
| 2026-10-17 | PATH-01 / 配置写入角色校验 | 隔离实现与回归通过，整体仍进行中 | `pkgtools::apply_tool_config` 在查询、目录创建和 npm/pnpm/Yarn 配置写入前统一校验工具环境、命令入口、缓存/存储和配置目录，补充缓存/配置重叠及仅缓存路径夹具。`pkgtools` 定向 6 passed、`package_managers` 定向 4 passed；完整 `npm run verify` 193 passed / 13 ignored，构建及八套 UI 通过。证据 `docs/evidence/path-01/path-role-verification-2026-10-17.json`；未操作真实用户配置。 |
| 2026-10-17 | PATH-01 / 迁移根目录边界 | 隔离实现与回归通过，整体仍进行中 | 迁移预览现在拒绝源目录位于或通过 Junction 别名指向 EnvCon 管理根目录，避免把受管目录树作为包管理器数据迁移；新增 `migration::tests::refuses_sources_inside_or_aliasing_the_managed_root`。完整 `npm run verify` 191 passed / 13 ignored，构建、`cargo check --offline`、`git diff --check` 通过。证据 `docs/evidence/path-01/path-role-verification-2026-10-17.json`；未操作真实用户目录。 |
| 2026-10-17 | PATH-01 / 四角色目录模型 | 隔离实现与回归通过，整体仍进行中 | `directory_roles`/`validate_directory_roles` 统一工具环境、命令入口、缓存/存储和配置目录；公共配置入口及 dotnet 重装布局接入。定向 `package_managers::tests` 4 passed；完整 `npm run verify` 190 passed / 13 ignored，`npm run build`、`cargo check --offline`、`git diff --check` 通过。证据 `docs/evidence/path-01/path-role-verification-2026-10-17.json`。旧布局恢复、迁移清理联动、真实命令生效和 Windows 实机仍待验证，未操作用户环境。 |
| 2026-10-17 | UNINST-01 / R3-h 审计增量 | 部分实现，隔离回归通过 | 原厂登记/路径/命令审计及模拟前后核验：新增9项核心+1项命令预览夹具；完整 verify 188 passed / 13 ignored、构建及八套 UI PASS。实际执行仍未接入（check 4条未使用警告如实保留），匹配/冲突/读取受限时保留卸载入口。证据 docs/evidence/uninstall/installer-audit-verification-2026-10-17.json；提前实现、验收待 R0 关闭。 |
| 2026-10-17 | UNINST-01 / R3-h | 进行中 | 开始原厂登记只读采集、精确路径匹配、审计计划与隔离执行核心。负责人 Codex，目标 0.5.9；提前实现、验收待 R0 关闭；真实卸载器执行与 VM 验收仍未执行。 |
| 2026-10-17 | UNINST-01 / R3-g 回归 | 代码及隔离回归通过，实机待验证 | 历史无收据链接可逐项确认，仅移除预览绑定的对象，漏选/重复/夹带路径/对象替换拒绝；损坏或 pending 收据不能绕过。完整 verify 178 passed / 13 ignored、两套读状态及八套 UI PASS，最终窄屏/文案增量 build/UI PASS。证据 docs/evidence/uninstall/legacy-link-verification-2026-10-17.json。提前实现、验收待 R0 关闭；历史 PATH、原厂卸载、PATH-01 全链路仍未完成。 |
| 2026-10-17 | UNINST-01 / R3-g | 进行中 | 开始无历史来源链接的独立候选预览和逐项确认；新增来源链路保持 R3-f 已有证据。确认不改变链接创建归属结论，损坏/替换仍阻止。负责人 Codex，目标 0.5.9，提前实现、验收待 R0 关闭。 |
| 2026-10-17 | UNINST-01 / R3-f 回归 | 新增链接来源链路隔离通过；旧链接确认仍待实现 | 完整 verify 174 passed / 13 ignored、构建、读状态及八套 UI PASS；单独 build/check/diff-check 通过。首次占用夹具失败经实际删除状态定位，拆分禁止删除/延迟删除用例后通过；保留两次输出于 docs/evidence/uninstall/junction-ownership-verification-2026-10-17.json。提前实现、验收待 R0 关闭；未做真实工具安装卸载。 |
| 2026-10-17 | UNINST-01 / R3-f 接入 | 部分实现，定向回归通过 | 链接来源核心 5 项、卸载定向 20 项通过；切换/纳管/预览/执行已接入文件身份核验。同目标替换及陈旧 current 预览会阻止删除；缺失/损坏来源保留。旧链接单独确认仍未实现；完整 verify 待执行。提前实现、验收待 R0 关闭。 |
| 2026-10-17 | UNINST-01 / R3-f | 进行中 | 开始 Junction 创建来源、文件身份和卸载复查链路；现有同目标判断无法证明链接创建者。负责人 Codex，目标 0.5.9，提前实现、验收待 R0 关闭；仅隔离目录验证。 |
| 2026-10-17 | UNINST-01 / R3-e 卸载接入回归 | 隔离回归通过，实机待验证 | 完整 npm run verify：164 passed / 13 ignored，构建及八套 UI 通过；单独 npm run build 通过。源码哈希、六个夹具名和完整实际输出见 docs/evidence/uninstall/uninstall-path-verification-2026-10-17.json。真实 PATH/注册表未操作，原厂卸载与 Junction 来源未完成；提前实现、验收待 R0 关闭。 |
| 2026-10-17 | UNINST-01 / R3-e 卸载接入 | 代码已接通，定向隔离测试通过；完整回归待验证 | 预览逐项显示 PATH 移除/保留并绑定快照；受管目录成功移除后才清理，写前重查共享依赖，写后核验失败报告部分结果。新增 6 项夹具覆盖全部 16 类入口与故障边界。未操作真实 PATH；提前实现、验收待 R0 关闭。 |
| 2026-10-17 | UNINST-01 / R3-e | 来源日志隔离回归通过；卸载接入进行中 | 完整 verify：158 passed / 13 ignored、构建及八套 UI 通过，证据 docs/evidence/path-01/path-ownership-verification-2026-10-17.json。继续实现 PATH 卸载预览/执行注入及共享依赖夹具；提前实现、验收待 R0 关闭。真实 PATH/注册表未操作。 |
| 2026-10-17 | UNINST-01 / PATH-01 本轮回归 | 当前增量隔离验证通过；整体进行中 | npm run verify：Rust 149 passed / 13 ignored、生产构建、两套读状态及八套 UI 通过。修正配置角色类型错误、npm 缓存嵌套、Composer 初始化、Python Lib 链接归属及同目标别名误解除 current；补 scan-ui 忽略例外。完整输出/源码哈希见 docs/evidence/r3/r3-path-verification-2026-10-17.json。rustfmt 缺失，真实安装卸载/VM/签名/发布未执行。提前实现、验收待 R0 关闭。 |
| 2026-10-17 | UNINST-01 / R3-e | 进行中（提前实现、验收待 R0 关闭） | 开始 PATH 来源日志与可审计清理计划；先接入现有写入链路和文件模拟后端，未操作真实 PATH/注册表，完整卸载接入仍待实现。 |
| 2026-10-17 | UNINST-01 / PATH-01 最新接续 | 进行中；最新修改待验证 | 复核确认目录枚举已有排序；发现独立配置目录的旧夹具断言、npm 重装缓存仍嵌套、Python site-packages 预览未检查 Lib 祖先链接。先补齐这些实现与夹具；迁移全角色联动仍未完成。提前实现、验收待 R0 关闭，不操作真实环境。 |
| 2026-10-17 | UNINST-01 / PATH-01 接续 | 进行中；新增风险修复与回归 | 恢复前轮未同步改动：必填卸载预览、复核/结果、内部链接已接通。全量发现规范化比较导致分类目录/旧包清理重定向保护失效；定向修复通过，继续复核祖先重定向。发现临时兄弟目录可被复用并误清理，正在隔离容器并补所有权夹具。无真实环境写入，未打包/发布，提前实现不改变阶段门槛。 |
| 2026-10-17 | UNINST-01 / R3 | 进行中；只读预览已写入，验证待执行 | 恢复工作区后发现只读预览与三项隔离夹具已存在；审查发现环境页传递第三个预览参数而 API 只接受两个参数。下一步修正接口、跑编译和隔离回归。预览标出目录、Python 环境内包、外部链接、明确保留项和未验证的 PATH/安装器登记；原删除路径没有扩展，预览后不重新核验快照，不能宣称完整卸载。 |
| 2026-10-17 | PATH-01 / IMP-07 | 待开始 → 进行中（提前实现） | R3 接口恢复并通过定向隔离回归后，按计划启动 PATH-01；先不写真实 PATH/环境变量，建立 uv 嵌套布局、pnpm/Yarn 入口角色模型和规范化路径校验。 |
| 2026-10-17 | SCAN-CORE-01 / R2 | 进行中 → 实现与隔离回归通过，待验收（提前实现） | 核心扫描状态/身份去重/UI 与隔离夹具通过。`detect::system::tests`：16 passed / 1 ignored；完整 `npm run verify`：TypeScript/Vite、Rust 120 passed / 13 ignored、读状态及八套 UI 通过。空结果分类筛选回归通过。依赖 R0/R1 与 Windows 实机一致性门槛未关闭；未打包/发布。 |
| 2026-10-17 | SCAN-CORE-01 / R2 | 待开始 → 进行中（提前实现） | 复核现有系统扫描只返回发现入口，缺少零结果与逐类结构化状态，UI 将不同失败统一显示为“待检查”。按要求先登记 R2 负责人 Codex、目标版本 0.5.8、下一步；不把“扫描范围内未发现”误报为绝对未安装，验收依赖 R0/R1。 |
| 2026-10-17 | DL-VERIFY-01 | 实现/隔离测试通过；待 R0 关闭后验收 | 完整 `npm run verify` 使用 Codex 运行时 Playwright 通过：Rust 116 passed / 13 ignored、前端生产构建、读状态和七套 UI 回归通过。下载校验状态、来源依据、失败阻止提交/安装与镜像未验证提示均由隔离夹具覆盖；官方校验资料逐项在线核实、镜像差异和 Windows VM 安装仍待验证。未打包/发布。 |
| 2026-10-17 | DL-VERIFY-01 接续修复 | 进行中；编译恢复 | 首次 `cargo check --offline` 复现 sources.rs/download.rs 语法错误；逐处移除错位字段并补齐 DownloadSpec 初始化后 check 通过。发现无效/未解析校验资料可静默通过、前端误称“已核对”、后端未用列表缓存等未完成部分，继续修复并补隔离夹具，未作 R1 完成声明。 |
| 2026-10-17 | DL-VERIFY-01 / ASMT-02 | 待开始 → 进行中（提前实现） | 按执行范围先更新主清单，再实现可信下载校验。R0 尚未关闭；R1 的源码和隔离夹具只能记录为提前实现，官方校验资料、镜像差异和 Windows VM 安装冒烟仍待验证。 |
| 2026-10-17 | AUD-016 / R0-a～d | 四项源码修复已验证；R0 整体继续进行中 | PortableGit 失败恢复、Python 镜像独立列出真实安装器、包管理器更新结果结构化、UI 测试跟踪/下载来源文案均已实现。`cargo test --offline --test sources_test download_spec_git_portable_sfx_args -- --exact`：1 passed；`cargo test --offline --lib -- --test-threads=1`：107 passed / 10 ignored；`npm run build` 通过；使用 Codex 运行时 Playwright 模块执行 `npm run verify`，七套 UI 和读状态测试通过。直接运行 verify 因本地缺少 Playwright 模块失败，已记录并以运行时模块复测通过。`cargo fmt --check` 因 rustfmt 未安装无法执行。16 类完整下载及 VM 实装仍未验证；未打包/发布。 |
| 2026-10-16 | AUD-016 源码复核 | 复核完成，新增待修复项；R0 仍部分完成 | 只读复核发现：Git 便携 EXE 失败后保留目录使同名重试失败；Python 镜像列表完全依赖 python.org API；包管理器自更新成功后二次版本查询失败被报告为更新失败；新 UI 测试已引用但未跟踪、Python 页面文案未同步。`npm run build` 与完整 `npm run verify`（指定本机 Playwright 模块）通过，Rust 101 passed / 10 ignored、七套 UI 回归通过，`git diff --check` 通过。Node 只读核对 Python API 和抽样官方/华为安装器 HEAD 200；Rust 在线全源清单因本机网络/TLS 请求错误失败，不能据此断言源不可用。`cargo fmt --check` 因 rustfmt 未安装无法验证。未执行 16 类完整下载/格式核验或 Windows VM 安装冒烟；未修改业务源码、未打包或发布。 |
| 2026-10-16 | 产品路线 R0～R4 / PATH-01 | 当前计划再次收敛；功能状态不变 | 将原六阶段计划收敛为五阶段：0.5.6 下载与 Python 验证、0.5.7 可信校验、0.5.8 核心扫描、0.5.9 安全卸载、0.6.0 发布验收；PATH-01 作为并行 P1 维护项安排在 0.5.7～0.5.8。明确 R0 首轮执行清单和发布门槛。仅修改文档，未改业务源码、重新运行测试、打包或发布；历史阶段记录保留。 |
| 2026-10-16 | AUD-016 / IMP-09、05、04、02 | 进行中；完成可复现问题修复与在线只读核查 | Python 版本改用官方 API 和 Windows x64 安装器资产，旧年份+时间日期显示为“日期未知”；下载加入响应长度校验，LLVM MSI-only 版本明确排除，普通压缩包仅做结构检查；包管理器显示准确命令版本并区分自更新支持、版本变化和未变化；普通环境移除改称删除受管目录并说明目录外残留边界。官方/镜像首选资产 HEAD：16 类静态 URL 核对通过；GitHub 六类单项官方/ghfast 200，Rust 全源串行清单曾因 GitHub 未认证限流失败；无真实安装、更新、卸载或打包。 |
| 2026-10-16 | PATH-01 / IMP-07、05、06、09 | 新增后续迭代项；实现待开始 | 用户授权将发现与建议写入最新 ROADMAP。记录 uv bin/cache 嵌套与潜在入口误删风险、重装/清理规则耦合、pnpm/Yarn Classic 入口漏配、Yarn 2+ 语义及 PATH 生效验证缺口；补兄弟目录方案、旧布局恢复、十管理器检查边界和隔离验收。负责人 Codex，P1，目标版本待排期；仅文档，未改业务源码或真实配置，未执行功能测试、打包或发布；长期完成数不变，保留 AUD-016 等已有记录。 |
| 2026-10-16 | UNINST-01 / IMP-04、05 | 新增需求与验收定义；实现未开始 | 用户要求完整卸载。将范围定义为精确移除 EnvCon 管理且可证明归属的资源；环境内 pip 包随解释器目录移除，`--user`、外部 venv、共享缓存默认保留，外部纳管仅移除链接。加入安装器登记、PATH/Junction 来源追踪、预览、验证及隔离夹具要求。pip/Python 官方文档核对 `--user` 独立安装位置和可自定义 user base；当前源码行为见 commands.rs 与 Environments.tsx；未改业务代码、未测试实现、未打包。 |
| 2026-10-16 | 产品路线 R0～R5 | 历史方案保留 | 当时根据当前工作区已实现的 Python、下载安全边界及包管理器版本/更新反馈，形成六阶段草案；当前计划已在上方收敛为 R0～R4 五阶段，历史版本号不再作为当前承诺。 |
| 2026-10-15 | 产品定位 / ASMT-01～06 | 收窄范围；长期完成数不变 | 用户确认定位为 Windows 环境工具下载与扫描；README 与产品评估同步，项目管理/隔离/自动配置建议标为范围外。现有功能与历史完成记录保留，未修改业务源码、未打包或发布。 |
| 2026-10-15 | ASMT-01～06 / IMP-04～10 | 新发现已记录；功能状态保持 | 完成产品/可靠性评估，记录 Python 可安装性、下载校验、安装后验证、PATH 生效、配置影响预览与测试可复现缺口。非 UI verify 通过；修复和产品试点尚未实施，长期完成数保持 1/10，历史完成及产物记录保留。 |
| 2026-09-19 | IMP-09-A | 源码已验证 → 已打包 | 独立 target-v0.5.5 release/NSIS 构建成功；两个文件 FileVersion/ProductVersion 均为 0.5.5，大小与 SHA-256 见 docs/releases/release-0.5.5.md；旧 EXE 哈希未变。UI 首次页面加载超时，构建后六套单独重跑全部通过。 |
| 2026-09-19 | IMP-09-A | 进行中 → 子项已完成 | ADB 扫描、SDK 标准目录发现、发行版本解析、Google Windows 稳定版索引、安装结构校验、纳管/切换/移除保护已实现；92 项 Rust 和前端构建通过，源码版本 0.5.5。长期 IMP-09 保持进行中。 |
| 2026-09-19 | IMP-09-A | 待开始 → 进行中 | 用户要求管理 ADB；以 Android Platform Tools 为安装单位，仅纳管其目录，保留其他 SDK 组件。 |
| 2026-09-18 | IMP-01 | 进行中 → 已完成 | A～D 与原始四项验收全部完成。热扫描均值下降 85.0%，冷扫描均值增加 4.8%（保留波动样本），预算通过；12 次原生样本、实际进程/内存/IO、自动版本变化/移除恢复/大小验证与产物哈希已归档。源码 GitHub 上传和正式 Release 分开记录。 |
| 2026-09-18 | IMP-01-D | 进行中 | ca65fa0 已推送；补齐强制启动读取、失败调度公平性和安装完成失效。86 项 Rust 通过，六套 UI 分两次通过；原生 EXE 试跑可读取实际进程树资源。0.5.4 独立构建中，正式三组基准待构建后执行。 |
| 2026-09-18 | IMP-01-C | 实现 → 已验证 | 环境/缓存按需大小、取消、部分诊断和跨页保留通过专项回归；窄窗口无横向溢出。B 推送恢复成功，远端 SHA ac25768 已核对。继续 D。 |
| 2026-09-18 | IMP-01-B | 实现 → 已验证 | 缓存跨启动复用、周期复核、失效与 UI 刷新状态通过；准备英文提交。GitHub 连续连接重置/超时，上传待网络恢复。继续 C/D 不停止本地开发。 |
| 2026-09-18 | IMP-01-B/C/D | 启动 | 用户要求持续至 IMP-01 完成、每轮 GitHub 上传和英文提交。已建立 codex/imp01，245f142 保存已验证基线；推送审批补充仓库/内容证据后获准，网络结果待核对。 |
| 2026-09-18 | IMP-01-A / IMP-07-A / IMP-05-A | 首轮已完成 → 已打包 | 独立 target-v0.5.3 EXE/NSIS 构建成功，文件及产品版本均为 0.5.3，大小/哈希见 `docs/archive-legacy.md`；0.5.2 产物哈希未变。长期方向继续进行，不宣称全计划完成。 |
| 2026-09-18 | IMP-01-A / IMP-07-A / IMP-05-A | 待验证 → 首轮已完成 | 统一 verify、四套 UI 与本机只读性能基准通过；旧下载测试桩补齐设置 API 后通过。完整 IMP-01/05/07 仍进行中，目标不缩减；0.5.3 独立打包中。 |
| 2026-09-18 | IMP-01-A / IMP-07-A / IMP-05-A | 实现 → 待验证 | 配置 schema/备份/显式恢复、并发查询合并和旧读响应保护已实现；配置 6 项、并发 3 项、读缓存和设置 UI 已通过。另发现 effective_environment 隐式遍历安装目录，已拆分为纯环境快照；全量回归与本机基准进行中。 |
| 2026-09-18 | IMP-01 / IMP-05 / IMP-07 | 待开始 → 进行中 | 启动首轮子项 A；配置加载失败静默默认影响根目录判断，同时查询重复增加子进程负载。修复并验证后再记录完成，不将首轮等同于整个长期条目。 |
| 2026-09-18 | FIX-052-01～04 | 已验证 → 已打包 | 最终 release/NSIS 构建成功；EXE 与安装包的文件及产品版本均为 0.5.2，大小和 SHA-256 已归档至发布记录。保留旧版产物，未远端发布。 |
| 2026-09-18 | FIX-052-01～04 | 进行中 → 待验证 | 版本源、卸载 UI、Rust 工具链和 Git 去重实现；保留长期改进计划未完成状态。Rust PATH 布局变化曾导致测试失败，改为验证两种稳定入口；临时目录用例通过。 |
| 2026-09-18 | FIX-052-01～04 | 待验证 → 已完成 | Rust 71 项单元测试、前端构建和三套 UI 回归通过；Git 去重由 Rust 临时夹具验证。补充在线 HTTP 验证受限权限与正常权限差异。 |
| 2026-09-18 | FIX-052-02～04 | 补充验证 → 通过 | 正常权限下官方源在线解析全部通过；本机发现 Git 内部入口后补齐规则并复核。Rust 实际工具链仅在临时目录创建/移除链接，版本可正确探测。窄窗口布局截图已复核。 |
| 2026-09-17 | IMP-01～IMP-10 | 建立 → 待开始 | 整理十个方向、子项、验收条件、依赖和维护规则；完成文档不计入功能完成数。 |
