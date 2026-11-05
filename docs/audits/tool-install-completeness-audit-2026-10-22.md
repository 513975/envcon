# 工具安装完整性核查（2026-10-22）

## 最新状态（2026-11-03）

- Rust 引导文件名问题已修复；EnvCon 管理的 stable 1.99.1 已安装，`rustc`、`cargo`、`cargo-clippy`、`cargo-fmt` 实际命令及五个已安装组件均核验通过。
- 修复版完整回归通过（Rust 364 passed / 14 ignored、生产构建、两套读状态、10 套 UI）；Clippy 通过，有 1 条既有排序风格建议。
- 0.5.5 Windows x64 EXE 仅本地构建，大小 9,565,184 字节，SHA-256 `49bdb3b7d3a2dd8bfb1419a3611709e966c31b210f25830dee026f4ca40d5d15`；未生成安装包、未正式发布。
- 系统 PATH 真实写入和 LLVM 隔离安装仍未验证。详细机器/构建证据：`docs/evidence/install01/rust-and-exe-verification-2026-11-03.json`。

## 2026-10-24 迭代状态（INSTALL-01）

负责人 Codex；目标版本 0.5.6；主状态见根目录 `ROADMAP.md`。下文安装参数与“未修改代码”是初次审计时的历史状态，不代表当前工作区。

| 待办 | 本轮处理 | 状态 / 证据 |
|---|---|---|
| Rust clippy / rustfmt | 保留 minimal，随后用本次安装的绝对 rustup 路径补装两个组件，并分别执行 cargo-clippy / cargo-fmt 验证；固定同一 homes、channel、cwd、镜像，失败中止，不以 shim 存在判成功 | 源码、完整隔离回归通过；2026-11-03 已完成受管 Rust 实装和真实组件命令核验，见最新状态与证据 |
| 系统 PATH 编辑 | 新增逐项增删/排序、原始值与 journal 快照、完整前后预览、来源说明、显式确认和按需 UAC；提权后复算，写前备份及写后复查，手工保存不认领共享项 | 源码、隔离及 UI 回归通过；新增 3 项 Rust 夹具；不视为真实系统 PATH 验收 |
| Python launcher | 决策：维持 Include_launcher=0，不扩大共享启动器与文件关联写入面；下载页说明使用 python -m pip / venv，不保证 py -3.x 跟随切换 | 决策已记录；策略夹具及完整回归通过，保持既有参数；未执行 Python 安装 |
| Gradle bin / all | 决策：维持 bin 默认，不因低优先级体验取舍增加默认下载体积；下载页说明缺自身源码与离线文档、IDE 可能需额外下载 | 决策已记录；策略夹具及完整回归通过；未新增 all 选择或安装 Gradle |
| LLVM 组件实测 | 下载页明确 NSIS 支持边界及组件范围未验证；不把 /D 指向临时目录当作系统隔离 | **未完成**：未运行真实隔离安装，不能得出 clang/lld/运行库组件完整性结论；需可用一次性 Windows 环境与明确安装授权 |

验证记录：

- `cargo test --offline --lib rustup_install -- --test-threads=1`：2 passed。
- `cargo test --offline --lib manual_edit -- --test-threads=1`：4 passed（其中 1 项既有用例，新增 3 项）。
- 后续加入 PATH UI、Python/Gradle 策略和提权序列化夹具，尚未完成最终验证。自动审批服务对 UI 启动和仅静态构建均返回 403 审核故障，两条命令均未执行；不使用其他工具绕过审批。
- 本机 PATH 无 rustup，两个组件命令只读探测未能运行；官方文档网络读取超时，不作为新证据。
- 本轮未执行真实工具安装、PATH/注册表写入、打包或正式发布。恢复审批后先运行完整回归和 Clippy，再核实 LLVM 隔离条件。未验证项不能标记已完成。

权限恢复后的验证 / 构建接续（2026-10-24）：

- 完整 verify 已通过：Rust 360 passed / 14 ignored、生产构建、两套读状态及 10 套 UI；先前 403 阻塞已解除。日志 docs/evidence/install01/exe-verify-2026-10-24.log。
- Clippy 通过，报告 1 条排序风格建议；日志 docs/evidence/install01/exe-clippy-2026-10-24.log。
- 用户要求只产出 EXE，不生成安装包；版本沿用源码 0.5.5，独立 target-install01-exe-20261024 release 构建已通过（9m57s），旧产物未变，不发布。真实工具安装与系统写入未执行，LLVM 实测仍未完成。
- 产物 src-tauri/target-install01-exe-20261024/release/envcon.exe：Windows x64 GUI，FileVersion/ProductVersion 0.5.5，9,593,856 bytes（9.15 MiB），SHA-256 dd6ad765e58543754ab401854ce4c4a62b80a3875bc15387eced221b414da678。
- 证据 docs/evidence/install01/exe-artifact-2026-10-24.json：包括产物元数据、测试摘要、构建参数、120 个源码快照文件未变化及 5 个旧产物哈希未变。无安装包输出，未启动 EXE、未正式发布。

## 为什么做这次核查

起因是一个具体现象：用户在 EnvCon 里安装 Rust 后，`cargo clippy` 和 `cargo fmt` 直接报错
「未安装该组件」。顺着这条线追下去，发现根因是安装参数把工具链裁掉了，而不是工具本身有问题。

既然一类工具存在这种「装完少东西」的情况，就需要逐类核查其余 16 类，避免同样的坑藏在别处。

本次只做调研与记录，**未修改任何代码**。

## 核查方法

对每一类工具，读取 `src-tauri/src/sources.rs` 中 `download_spec_with_verification`
的实际安装参数，判断它安装的是完整发行包还是被裁剪的子集。

证据等级分三档，文中逐项标注：

| 标记 | 含义 |
|---|---|
| **实测** | 在本机隔离目录真实执行过安装/下载并观察结果 |
| **代码确认** | 从源码参数直接读出，参数语义明确 |
| **待实测** | 参数会传递到上游安装器，但上游的默认行为未在本机验证 |

## 结论速览

| 类型 | 安装方式 | 是否完整 | 风险 |
|---|---|---|---|
| **Rust** | rustup-init `--profile minimal` | ❌ **缺 clippy / rustfmt / rust-docs** | **高：已实际发生** |
| **Python** | 官方安装器 + `Include_launcher=0` | ⚠️ 缺 `py.exe` 启动器 | 中 |
| **Gradle** | `-bin.zip` | ⚠️ 缺源码与离线文档 | 中 |
| LLVM | NSIS `/S` | ⚠️ 组件由安装器默认决定 | 待实测 |
| Node | 官方 zip，完整 | ✅ | — |
| Go | 官方 zip，完整 | ✅ | — |
| JDK | Adoptium 完整 zip | ✅ | — |
| Maven | `-bin.zip`（Maven 无更全的二进制包） | ✅ | — |
| PHP | 官方 zip，完整 | ✅ | — |
| Zig | 官方 zip，完整 | ✅ | — |
| Deno | 官方 zip，完整 | ✅ | — |
| Bun | 官方 zip，完整 | ✅ | — |
| Git | PortableGit 自解压包，完整 | ✅ | — |
| GitHub CLI | 官方 zip，完整 | ✅ | — |
| Mingw | winlibs 完整 zip | ✅ | — |
| ADB | platform-tools 完整 zip | ✅ | — |
| .NET SDK | 官方 zip，完整 | ✅ | — |

## Rust：确认的问题

**代码位置**：`src-tauri/src/download.rs`，`run_rustup_init`

```rust
cmd.args(["-y", "--no-modify-path", "--default-toolchain", channel, "--profile", "minimal"])
```

**实测过程**（在 `%TEMP%` 隔离目录，用相同参数跑真实 rustup-init）：

```
info: profile set to minimal
info: downloading 3 components
...
stable-x86_64-pc-windows-msvc installed - rustc 1.99.1
```

装完的组件列表：

```
cargo-x86_64-pc-windows-msvc
rust-std-x86_64-pc-windows-msvc
rustc-x86_64-pc-windows-msvc
```

**实测后果**：

```
$ cargo clippy --version
error: 'cargo-clippy.exe' is not installed for the toolchain 'stable-x86_64-pc-windows-msvc'.
help: run `rustup component add clippy` to install it

$ cargo fmt --version
error: 'cargo-fmt.exe' is not installed for the toolchain 'stable-x86_64-pc-windows-msvc'.
help: run `rustup component add rustfmt` to install it
```

**一个容易误判的细节**：`cargo-home\bin` 里确实存在 `cargo-clippy.exe`、`cargo-fmt.exe`、
`rust-analyzer.exe` 等文件，但它们全都是 12.13 MB 的 **rustup shim**（与 `rustup.exe`
大小、哈希一致），只是转发器，真正的组件并未安装。仅看文件是否存在会得出错误结论。

**rustup 的 profile 定义**（官方语义）：

| profile | 组件 |
|---|---|
| `minimal` | rustc, cargo, rust-std |
| `default` | minimal + rust-docs, rustfmt, clippy |
| `complete` | default + miri, rls, rust-analysis, llvm-tools, 等 |

**建议方案**：先按 `minimal` 装，装完再显式补 clippy 与 rustfmt。

```rust
rustup component add clippy rustfmt
```

理由：`--profile default` 会连带装 rust-docs（数百 MB 的 HTML 文档），而 clippy/rustfmt
是日常开发刚需、rust-docs 不是。显式补装能精确控制这两者的取舍。

**该方案已在隔离目录实测有效**：执行后组件列表出现 `clippy-x86_64-pc-windows-msvc`。

**一个反直觉的发现**：EnvCon 装出来的 Rust（rustup 布局）比用户当前纳管进来的那套
（扁平工具链、无 `rustup.exe`）**能力更强**——前者可以 `rustup component add`、可以
切换版本、可以 `rustup update`；后者什么都做不了。所以「重装后少东西」的同时也
「多了补装能力」，只是补装需要用户自己敲命令，而 EnvCon 目前没有任何 UI 提示这一点。

## Python：代码确认的问题

**代码位置**：`src-tauri/src/sources.rs`，`EnvType::Python` 分支

```rust
args: vec![
    "/quiet",
    "InstallAllUsers=0",
    format!("TargetDir={}", target_dir.replace('/', "\\")),
    "PrependPath=0",
    "Shortcuts=0",
    "Include_launcher=0",
]
```

**逐项判断**（**代码确认**）：

| 参数 | 影响 | 评价 |
|---|---|---|
| `/quiet` | 静默安装 | 合理 |
| `InstallAllUsers=0` | 仅当前用户，免管理员 | 合理 |
| `TargetDir=...` | 装进受管目录 | 合理 |
| `PrependPath=0` | 不改 PATH，由 EnvCon 自己管 | 合理 |
| `Shortcuts=0` | 不建快捷方式 | 合理 |
| `Include_launcher=0` | **不装 `py.exe` / `pyw.exe` 启动器** | 见下节分析 |

`py.exe` 是 Windows 上的 Python 启动器（PEP 397），做三件事：按版本选解释器
（`py -3.11`）、识别脚本 shebang、接管 `.py` 双击。它靠注册表
`HKCU/HKLM\Software\Python\PythonCore\<ver>\InstallPath` 发现解释器，**不依赖 PATH**。

**这一项的实际影响比初判更小**，理由：

- EnvCon 的模型是「同一时刻只有一个 `current`」，多版本靠 junction 切换。`py.exe`
  最核心的价值（并存多版本、按版本号挑）在本项目里用不上——切版本是在环境页点一下，
  不是敲 `py -3.11`
- `py.exe` 默认装到 `C:\Windows\`，需要管理员权限，与本项目「不写系统目录」冲突
- 即使启用，还得把受管路径写进 `HKCU\Software\Python\PythonCore` 才能被发现，
  又是一处额外的注册表写入面

受影响的具体场景：`py -3.11` 选版本、双击 `.py`、依赖 shebang 的脚本、硬编码 `py` 的
构建脚本。不受影响：`python script.py`、`python -m pip`、`python -m venv`。

**结论**：`Include_launcher=0` 在本项目架构下是合理取舍，不改。

其余 `Include_*` 项未显式设置，走安装器默认值（**待实测**）：`Include_pip` 默认装（好），
`Include_test`、`Include_doc`、`Include_tcltk` 默认也装（占体积但不是功能缺失）。

### 本机已执行的两处修复（2026-10-24）

在用户机器上发现并修复了两个与 Python 相关的独立问题：

**修复一：WindowsApps 下的 0 字节别名存根**

`C:\Users\island\AppData\Local\Microsoft\WindowsApps\` 下有 `python.exe` 与 `python3.exe`
两个 0 字节的 App Execution Alias 存根。敲 `python3` 不会报「找不到命令」，而是把用户
引向微软商店——这类静默误导比明确报错更难排查。

处理方式：改名为 `python.exe.disabled-envcon` / `python3.exe.disabled-envcon`，**不删除**，
随时可改回。

修复后实测：

| 命令 | 修复前 | 修复后 |
|---|---|---|
| `python` | `D:\DevEnvManager\current\python\python.exe` | 不变（原本就对） |
| `python3` | `WindowsApps\python3.exe`（0 字节存根） | **未解析，明确报错** |

需要说明：`python3` 现在不可用。受管 Python 目录里本来就没有 `python3.exe`（只有
`python.exe` / `pythonw.exe`），要让它可用需要额外创建一个副本或链接。

**修复二：注册表 InstallPath 指向已删除的旧目录**

`HKCU\Software\Python\PythonCore\3.14\InstallPath` 原本指向
`C:\Users\island\AppData\Local\Python\pythoncore-3.14-64`，该目录**已不存在**。

修复内容：

| 项 | 修复前 | 修复后 |
|---|---|---|
| `(default)` | 旧路径（不存在） | `D:\DevEnvManager\current\python` |
| `ExecutablePath` | 旧路径 | `D:\DevEnvManager\current\python\python.exe` |
| `WindowedExecutablePath` | 旧路径 | `D:\DevEnvManager\current\python\pythonw.exe` |
| `IdlePath` | 指向不存在的 `Lib\idlelib\idle.pyw` | **已移除**（受管包不含 idlelib） |
| `Help\Main Python Documentation` | 指向不存在的 `Doc\html\index.html` | **已移除**（受管包不含 Doc） |

保留 `Help\Online Python Documentation`（指向 docs.python.org，有效）。

修复后实测：`python --version` → 3.14.6；`python -m pip --version` → pip 26.1.2；
`python -m venv <tmp>` → exit 0 且 venv 内解释器可用。

备份：`D:\DevEnvManager\logs\python-registry-backup-20261024-120054.reg`（`reg export` 导出
整个 `HKCU\Software\Python` 分支）。


## Gradle：代码确认的取舍

**代码位置**：`src-tauri/src/sources.rs`，`EnvType::Gradle` 分支

```rust
url: format!("{base}/gradle-{v}-bin.zip", v = version),
```

**实测体积对比**（HEAD 请求，官方源）：

| 文件 | 体积 |
|---|---|
| `gradle-9.7.1-bin.zip` | 144 MB |
| `gradle-9.7.1-all.zip` | 246 MB |

`-bin` 只有二进制；`-all` 额外包含 Gradle 自身源码与离线文档。

**影响**：IDE 里查看 Gradle API 源码、跳转到定义时会失效。这不影响构建能跑，但影响开发体验。

**判断**：这属于「体积与开发体验」的取舍，不像 Rust 那样属于功能缺失。是否改由用户决定。

## LLVM：待实测

**代码位置**：`src-tauri/src/sources.rs`，`EnvType::Llvm` 分支

```rust
args: vec!["/S".into(), format!("/D={}", target_dir.replace('/', "\\"))],
```

LLVM 的 Windows NSIS 安装器支持组件选择（clang、lld、compiler-rt、sanitizers 等），
但 `/S` 静默模式下未传任何组件参数时，装哪些组件由安装器默认值决定。

**本次未实测**，因此不写入结论。需要一次隔离安装并检查 `bin\` 下的实际内容才能定论。

另外代码里有一条明确限制：只有 `.exe` 格式才提供安装，MSI 格式直接拒绝，理由写在注释里
（MSI 的属性契约未验证）。这是有意的保守设计，不是缺陷。

## 其余 11 类：完整发行包

以下类型的安装参数均为官方完整发行包，不存在组件裁剪：

| 类型 | 依据 |
|---|---|
| Node | `node-v{v}-win-x64.zip`，官方完整包 |
| Go | `go{v}.windows-amd64.zip`，官方完整包 |
| JDK | Adoptium 完整 JDK zip（非 JRE） |
| Maven | `apache-maven-{v}-bin.zip`，Maven 只有 `-bin` / `-src` 两种，`-bin` 即完整 |
| PHP | windows.php.net 官方 zip |
| Zig | 官方 zip，`zig.exe` + `lib/` 完整 |
| Deno | 官方单文件 zip |
| Bun | 官方 zip |
| Git | PortableGit 自解压包，含完整 Git for Windows |
| GitHub CLI | 官方 zip |
| Mingw | winlibs 完整 UCRT 版 zip |
| ADB | Google platform-tools 完整 zip，且额外校验 `fastboot.exe` 与 `source.properties` 版本 |
| .NET SDK | 微软官方 zip，含 `dotnet.exe` + `sdk/` + `shared/` |

其中 ADB 和 .NET SDK 有额外的安装后校验（ADB 核对 `Pkg.Revision`，.NET 核对 `sdk/`
下的版本目录名），属于比其它类型更严格的检查。

## 被遗漏的功能：系统 PATH 编辑

这是本次核查顺带确认的一处**功能缺口**，与上面的安装完整性无关，但同样需要记录。

**现状**：`src/pages/Paths.tsx` 第 298–307 行：

```tsx
{/* 系统 PATH(只读) */}
...
系统 PATH(只读)
desc="修改系统 PATH 需要管理员权限,EnvCon 出于安全仅作展示"
```

**实际已实现的部分**：具名环境变量（`JAVA_HOME`、`GOPATH` 这类）的用户级与系统级
读写都已支持，走 `--env-write` 提权子进程；`pathman.rs` 里的 `save_path` 也已支持
`EnvScope::System`。

**缺的部分**：系统 PATH 的**界面入口**仍停留在只读展示，没有接上已有的写入能力。

也就是说底层能力已经具备，缺的是 UI 接线。这与「环境变量编辑」那一轮只做了具名变量、
没做 PATH 是同一处遗漏。

**需要澄清的边界**：系统 PATH 被很多安装器共享使用，随意编辑会破坏别人的安装。
接上这个入口时，必须沿用已有的保护机制（`path_ownership` 的 journal、写入前后复核、
条目来源判定），不能提供「整段文本自由编辑」那种破坏性入口。

## 初次审计待办清单（进展见文首）

按影响面排序：

1. **Rust 补装 clippy + rustfmt**（高）—— 已在隔离目录验证方案有效，改动点在
   `download.rs::run_rustup_init`，装完后追加一次 `rustup component add`。
2. **系统 PATH 编辑入口**（高）—— 底层 `save_path(EnvScope::System, ...)` 已具备，
   缺 UI 接线与提权路径。
3. **Python 的 `py.exe` 启动器**（中）—— 是否改为 `Include_launcher=1` 需要决策：
   启动器会写到系统位置还是受管目录，需要实测确认。
4. **Gradle 用 `-all` 还是 `-bin`**（低）—— 纯体验取舍，多占 102 MB 换 IDE 源码跳转。
5. **LLVM 组件实测**（低）—— 需要一次隔离安装才能定论，目前不应下结论。

## 初次审计未做的事（历史记录）

- 未修改任何代码（`download.rs` 的 profile 参数保持 `minimal` 原样）。
- 未执行 Python / LLVM / Gradle 的真实安装，因此相关判断标注为「代码确认」或「待实测」。
- 未验证 Rust 补装方案在 EnvCon 的实际安装流程中是否可行（只在隔离目录手工验证了
  `rustup component add` 本身有效）。
- 未确认 `py.exe` 在受管布局下的安装位置。
