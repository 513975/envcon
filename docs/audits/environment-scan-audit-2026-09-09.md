# EnvCon 全部环境扫描审查

## 修复状态（0.2.1）

本轮已关闭 SCAN-01 至 SCAN-17：版本解析、Rust/JDK 健康校验、项目与安装身份去重、HOME 变量发现、注册表变量类型与删除语义、损坏链接和 current 诊断、包管理器实际命令探测、缓存重定向与完整性统计、PATH 缺失处理及首页状态文案均已落实。对应实现位于 `src-tauri/src/detect`、`pkgtools.rs`、`caches.rs`、`pathman.rs` 以及前端环境/路径/仪表盘页面。

验证命令：`cargo check --offline`、`cargo test --offline --lib`、`npm run build`、`git diff --check`。剩余“兼容性风险”和“能力边界”是需要真实发行版或项目上下文才能继续扩展的范围，不再作为已确认缺陷。

审查日期：2026-09-09。

审查对象：当前工作区源码，包含尚未提交的扫描增强代码。本文不是此前生成的 0.1.2 EXE 的功能验收报告；此前打包后发生的扫描修改不自动包含在该 EXE 中。

本次只记录问题，不修复功能、不重新打包。下文源码位置以本次审查时的行号为准。

## 结论与范围

目前扫描功能尚不健全，但不能据此认为每一种工具都无法扫描。基础路径发现和错误展示已有实现，主要缺口集中在版本解析、受管环境验证、真实安装路径与代理识别，以及系统扫描、包管理器配置、缓存页之间的数据不一致。

本次覆盖：

- 系统扫描定义的全部 28 种工具，以及额外的 `python -m pip` 探测。
- 全部 15 类受管环境的目录发现、版本探测、当前版本判断和大小统计。
- npm、pnpm、Yarn、pip 配置读取。
- npm、pip、Maven、Gradle、Cargo、Go 和 EnvCon 下载目录的缓存发现与统计。
- PATH 和环境变量读取、前端扫描结果与当前环境展示。

已实现且不应再列为“完全缺失”的能力：重新读取注册表 PATH、按 PATHEXT 查找 Windows 启动器、发现多个安装位置、Python 注册表目录和 Conda 环境记录、选定项目目录扫描、逐解释器探测 pip 模块、系统扫描启动失败/超时/退出错误展示、目录扫描深度与数量提示。

证据分为三类：

- **确认缺陷**：已从当前代码确认错误路径及其触发条件；不代表已在所有真实发行版上运行复现。
- **兼容性风险**：代码存在限制，但需要对应安装包或隔离环境进一步验证实际影响。
- **能力边界**：当前没有实现的扫描范围，不能直接等同于已有功能出错。

优先级：P1 为优先修复的核心识别错误；P2 为特定条件下的错误、漏扫或不完整诊断；P3 为信息完整性和体验改进。

## 确认缺陷

### SCAN-01 · P1 · 系统版本探测把输出首行当成版本

- 位置：`src-tauri/src/detect/system.rs:219`，尤其成功输出解析段。
- 触发：`gradle --version` 先输出欢迎信息或分隔线；Java 在 stderr 的版本行之前输出 `Picked up JAVA_TOOL_OPTIONS`；工具先输出警告或提示。
- 结果：版本栏保存欢迎信息、警告或分隔线。stdout 仅有空白而 stderr 有版本时，也不会正确回退到 stderr。
- 建议：按工具解析完整 stdout/stderr，识别版本行并校验格式，保留原始诊断但不把它作为版本。
- 验收：Gradle 首次/再次运行、Java 带选项提示、PHP 前置警告、stdout 空白且 stderr 有版本均得到正确版本或明确解析错误。

### SCAN-02 · P1 · 受管 Rust 探测的是 Cargo，且未隔离工具链目录

- 位置：`src-tauri/src/detect/probe.rs:110`；对照系统探测 `src-tauri/src/detect/system.rs:219`。
- 触发：扫描 EnvCon 内 `cargo-home/bin/cargo.exe` 的 rustup 代理，而进程继承的 `CARGO_HOME`、`RUSTUP_HOME` 指向其他安装或没有设置。
- 结果：可能显示全局默认工具链的 Cargo 版本、探测失败，或把 Cargo 版本当作该目录的 Rust 编译器版本。系统扫描中的隔离设置没有覆盖受管扫描。
- 建议：受管 Rust 用对应目录的 `rustc` 获取版本，并显式设置该安装的 Cargo/Rustup home；Cargo 版本单独记录。
- 验收：两个不同受管工具链加一个不同全局工具链，逐项显示各自 rustc 版本，结果不随全局默认工具链改变。

### SCAN-03 · P1 · 受管版本探测未检查退出状态，超时后不保证停止进程

- 位置：`src-tauri/src/detect/probe.rs:30`、`src-tauri/src/detect/probe.rs:44`。
- 触发：工具退出非零并输出错误，或版本命令超过超时限制。
- 结果：错误文本可能被通用解析器截成“版本”；超时丢弃 future 时没有启用 `kill_on_drop`，子进程可能继续运行。失败原因最终只剩 `None`。
- 建议：复用具备退出状态检查、超时清理和结构化错误的探测实现，区分无法启动、超时、退出失败和无法解析。
- 验收：非零退出不得产生成功版本；超时后没有遗留探测进程；受管列表能解释失败原因。

### SCAN-04 · P2 · Go 的多行 VERSION 文件被整体保存为版本

- 位置：`src-tauri/src/detect/probe.rs:90`。
- 触发：VERSION 内容为 `go1.24.0\ntime 2025-02-11T00:00:00Z` 这类包含构建时间的格式。
- 结果：代码只对整个字符串去掉 `go` 前缀，返回值包含换行和时间，污染版本展示。
- 建议：只解析文件中的版本行并验证格式，失败再执行对应 `go.exe version`。
- 验收：单行、多行、空文件、畸形文件均得到正确版本或明确失败。

### SCAN-05 · P1 · JRE 或不完整安装仍可能被识别为可管理 JDK

- 位置：`src-tauri/src/detect/system.rs:67`、`src-tauri/src/detect/system.rs:172`；导入入口 `src-tauri/src/commands.rs:104`。
- 触发：目录仅存在 `bin/java.exe`，没有 `javac`；或候选可执行文件存在但版本探测启动失败。
- 结果：JRE 被归入 JDK；结构看似符合但不能运行的环境仍可能出现“纳入管理”。`looks_like` 还使用 `exists()`，没有坚持要求目标为文件。
- 建议：区分 JDK/JRE；检查该类别所需的核心组件；把探测健康状态纳入导入判断，或明确要求用户处理异常后再导入。
- 验收：完整 JDK、纯 JRE、缺失 javac、损坏 java、同名目录冒充 exe 等样本有不同且准确的状态。

### SCAN-06 · P2 · 项目环境是否可导入取决于发现顺序

- 位置：`src-tauri/src/detect/system.rs:132`、`src-tauri/src/detect/system.rs:172`。
- 触发：同一个项目内便携 Node/JDK 等先通过 PATH 或常见目录被发现，随后又从所选项目目录发现。
- 结果：去重只保留最先发现的来源；`source != "项目目录"` 的排除失效，项目环境仍可能被提供导入。Python 的 `pyvenv.cfg` 和 Conda 标记检查只能覆盖部分情况。
- 建议：按规范化路径判断项目归属，保留多个发现来源，导入策略不依赖展示用来源字符串。
- 验收：相同目录同时出现在 PATH、常见根和项目根时，调整发现顺序不改变导入资格。

### SCAN-07 · P2 · 安装身份判断混用原始路径和规范路径

- 位置：`src-tauri/src/detect/system.rs:90`、`src-tauri/src/detect/system.rs:110`、`src-tauri/src/detect/system.rs:172`。
- 触发：同一环境通过目录链接、大小写不同的路径或 Windows 扩展路径前缀访问。
- 结果：候选去重做了 canonicalize，但管理根排除使用原始 `starts_with`，已导入目标比较也只处理大小写和分隔符。因此同一安装的不同路径形式可能再次被当作外部可导入环境。
- 建议：统一安装根和可执行文件的身份比较，处理 junction、扩展路径前缀和规范化失败；显示路径与身份路径分别保存。
- 验收：真实目录、junction、大小写差异、带/不带扩展前缀都识别为同一安装，不重复提供导入。

### SCAN-08 · P2 · 已支持的部分 HOME 环境变量未用于发现工具

- 位置：`src-tauri/src/detect/discovery.rs:49` 的根目录枚举；对照 `src-tauri/src/commands.rs:470`。
- 触发：Maven、Gradle、PHP、Zig、LLVM 只设置 `MAVEN_HOME`、`GRADLE_HOME`、`PHP_HOME`、`ZIG_HOME`、`LLVM_HOME`，没有进入 PATH、受管目录或已枚举的常见目录。
- 结果：变量管理页面认识这些变量，系统扫描却不从这些位置发现工具，导致漏扫。
- 建议：对齐可支持的 HOME 变量清单，补充别名并验证值是否为有效安装根。
- 验收：每个 HOME 变量独立指向临时安装、PATH 不包含工具时，仍能扫描到对应工具。

### SCAN-09 · P2 · 环境刷新保留已删除的旧变量，且丢失注册表值类型

- 位置：`src-tauri/src/detect/discovery.rs:36`、`src-tauri/src/detect/discovery.rs:49`。
- 触发：应用启动后删除注册表中的 JAVA_HOME 等变量；或者 `REG_SZ` 中包含应按字面保留的 `%NAME%`。
- 结果：扫描从启动环境构造 map 再覆盖注册表值，删除的旧变量仍保留；注册表值类型被丢弃后全部做 `%...%` 展开，普通字符串也可能被改写。扫描得到的环境不完全等于刷新后的持久化环境。
- 建议：明确进程变量和持久化变量的来源与合并规则，处理删除语义；保留 REG_SZ/REG_EXPAND_SZ 类型，按 Windows 语义展开。
- 验收：设置、修改、删除变量后重扫均符合预期；普通字符串中的百分号不被误展开；进程专有变量是否保留有明确规则。

### SCAN-10 · P2 · 受管目录未区分有效安装、空目录和失效链接

- 位置：`src-tauri/src/detect/mod.rs:17`。
- 触发：分类目录中有普通空目录、不完整解压目录、损坏安装或指向已删除安装的 junction。
- 结果：非隐藏普通目录均被列为环境；失效链接因 `is_dir()` 为 false 直接消失；读取目录失败也缺少诊断。用户无法区分“没有环境”和“环境损坏/无法访问”。
- 建议：引入可用、损坏、不可访问、链接失效等状态，保留异常条目及原因，并按类型验证基本结构。
- 验收：上述四类异常目录均有准确状态，不能被显示为健康安装或静默消失。

### SCAN-11 · P2 · “当前版本”按目标目录名比较，未校验完整目标

- 位置：`src-tauri/src/config.rs:125`、`src-tauri/src/detect/mod.rs:20`。
- 触发：`current/node` 指向分类目录之外的另一个同名目录；或大小写与枚举名称不同。
- 结果：仅提取目标 basename 并与安装名称做字符串比较，可能把错误的同名安装标记为当前，或漏标实际当前安装。
- 建议：比较规范化后的完整目标身份，并单独展示当前链接指向了未登记环境的情况。
- 验收：不同父目录下同名安装不会混淆；等价路径和大小写差异不影响识别。

### SCAN-12 · P2 · 配置页工具定位与系统扫描不一致

- 位置：`src-tauri/src/pkgtools.rs:32`、`src-tauri/src/pkgtools.rs:126`。
- 触发：应用启动后改变 PATH；工具存在多个版本；pip 只有 `python -m pip` 可用；`where` 首项不是可直接执行的 Windows 启动器。
- 结果：配置页继续用旧进程 PATH 和 `where` 首项，且只有 npm 优先受管安装；可能与扫描列表指向不同工具。`available` 只代表找到路径，即使配置命令不能运行也显示可用。读取失败原因被丢弃。
- 建议：共享工具解析与解释器关联，允许选择具体实例，报告启动/读取错误；对 pip 支持模块调用。
- 验收：PATH 刷新、多实例、仅模块 pip、无扩展名脚本与 cmd 并存等场景，扫描结果和配置读取实例一致。

### SCAN-13 · P2 · 配置读取没有区分显式配置与实际生效目录

- 位置：`src-tauri/src/pkgtools.rs:103`。
- 触发：pip 没有显式设置 `global.cache-dir`，使用内建默认目录或 `PIP_CACHE_DIR`；配置命令返回 `undefined` 一类占位值。
- 结果：缓存明明可用却显示为空，或占位文本被当作路径；无法解释配置来源。pnpm 的包存储目录与 `cache-dir` 也没有分别呈现。
- 建议：使用各工具的有效目录查询能力，例如所选解释器的 `pip cache dir`、pnpm 的 store 查询；区分有效值、显式值、默认建议和不可用状态。
- 验收：默认值、环境变量覆盖、用户配置覆盖三种场景准确显示有效目录；占位值不能作为有效路径。

### SCAN-14 · P2 · 缓存发现忽略重定向配置和自定义下载目录

- 位置：`src-tauri/src/caches.rs:23`、`src-tauri/src/commands.rs:367`；对照 `src-tauri/src/config.rs:111`。
- 触发：修改 npm/pip 缓存路径、Maven 本地仓库、`GRADLE_USER_HOME`、`CARGO_HOME`、`GOMODCACHE`，或应用设置中的下载目录。
- 结果：缓存页仍扫描大部分固定默认位置；EnvCon 下载缓存也只拼接 `root/downloads`。实际缓存可能被漏计，旧目录反而继续显示。GOPATH 多路径值也没有按列表处理。
- 建议：按对应工具实际配置解析缓存，读取应用的 `downloads_dir`；允许默认残留目录和当前使用目录同时显示并标记来源。
- 验收：每类缓存重定向后均显示真实路径；自定义下载目录一致；多 GOPATH 不生成包含分号的伪目录。

### SCAN-15 · P2 · 大小统计缺少完整性状态，汇总可能误导

- 位置：`src-tauri/src/detect/mod.rs:62`、`src-tauri/src/caches.rs:84`、`src/pages/Dashboard.tsx:92`。
- 触发：部分子目录或文件无权限读取；外部环境通过链接纳入管理；同一内容存在多个登记入口。
- 结果：受管统计可能整体变成 `None`，缓存统计则静默跳过错误并返回偏小数值；仪表盘汇总把未知大小视作 0。外部目标大小计入总和，不能直接解释为管理根实际占用，多个入口还可能重复计数。
- 建议：返回已统计字节、是否完整、跳过项和错误；区分受管内容大小与本地实际占用，并按目录身份去重。
- 验收：权限受限显示“统计不完整”，未知不伪装成零；外部链接占用归属明确。

### SCAN-16 · P2 · 缺少用户 PATH 值时，PATH 页面读取整体失败

- 位置：`src-tauri/src/pathman.rs:19`、`src-tauri/src/pathman.rs:25`。
- 触发：用户注册表环境键存在，但没有用户级 Path 值，系统级 PATH 正常。
- 结果：`get_raw_value("Path")?` 将合法的“未设置”作为错误向上传播，无法返回系统 PATH；与系统扫描允许空用户 PATH 的行为不一致。
- 建议：缺少值按空列表处理，其他读取错误继续明确报告；后续写入也要支持首次创建值。
- 验收：无用户 Path 时页面仍显示系统 PATH 和空用户列表，首次添加用户路径可完成。

### SCAN-17 · P2 · 首页把 current 链接状态等同于终端实际使用版本

- 位置：`src/pages/Dashboard.tsx:149`、`src/pages/Dashboard.tsx:161`。
- 触发：current 路径未加入 PATH，或更早的 PATH 条目、终端别名、已激活虚拟环境覆盖了它。
- 结果：页面声称终端使用当前链接版本、激活后立即可用，但后台实际只证明了链接指向。
- 建议：分别展示 EnvCon 当前链接、持久化 PATH 首选和终端上下文未知状态；沿用系统结果页已有的 PATH 首选限定说明。
- 验收：current 与 PATH 首选不同或未加入 PATH 时，首页不会宣称终端已经切换成功。

## 兼容性与稳健性风险

以下需要补充真实安装包或受控夹具，不能表述为已在用户机器全部复现。

| 编号 | 优先级 | 风险与代码证据 | 验证/改进方向 |
| --- | --- | --- | --- |
| RISK-01 | P2 | JDK 受管探测仅读 release/JAVA_VERSION，没有命令回退；Maven/Gradle 只匹配固定 jar 名，且首个匹配即返回。`detect/probe.rs:76`、`:125`、`:141` | 覆盖缺失元数据、不同发行布局、残留多版本 jar；元数据与实际可执行版本交叉校验。 |
| RISK-02 | P2 | PHP/LLVM 依赖第一行固定前缀，GCC 直接取末词。`detect/probe.rs:157`、`:168`、`:202` | 覆盖带发行商前缀、启动警告、版本后带额外修订信息的输出，不能把修订描述误当版本。 |
| RISK-03 | P2 | shim 只显示发现路径，未查询代理实际目标；安装根只尝试父/祖父目录。`detect/system.rs:90` | 验证 Scoop、Chocolatey、Volta、rustup、Corepack；分别记录入口和实际目标，无法解析时明确标为代理。 |
| RISK-04 | P2 | 系统版本探测设置 TEMP 为工作目录，无法代表项目选择的工具链；移除 PYTHONHOME/PYTHONPATH 也会改变某些定制解释器行为。`detect/system.rs:219` | 对 global.json、rust-toolchain.toml、packageManager、yarnPath 等做项目专用识别；不要把全局探测结果声称为项目有效版本。 |
| RISK-05 | P2 | 固定使用 Yarn Classic 的 global-folder/cache-folder 查询，未先判断 Yarn 主版本。`pkgtools.rs:103` | 增加 Classic/Berry 实测与版本适配，错误不能静默变成空路径。 |
| RISK-06 | P2 | 系统探测只 kill 直接子进程；受管和配置读取缺少 kill_on_drop；output() 无输出字节上限。`detect/system.rs:219`、`detect/probe.rs:30`、`pkgtools.rs:59` | 用生成子进程及大量输出的夹具验证清理和内存限制，Windows 进程树需单独处理。 |
| RISK-07 | P2 | 系统目录遍历每根上限 2000 个目录，没有全局数量/时间预算；同一工具多个候选顺序探测。`detect/discovery.rs:156`、`detect/system.rs:132` | 大量环境、慢磁盘或网络路径下评估总耗时；补取消、总体预算和渐进结果。不要把单命令超时误当整次扫描超时。 |
| RISK-08 | P2 | 两处大小遍历没有显式 reparse-point 策略、visited 集合和总预算。`detect/mod.rs:62`、`caches.rs:84` | 需在 Windows 用 junction、符号链接及其他 reparse point 验证是否跳过、漏计或重复遍历；本次未复现循环，不能断言一定死循环。 |
| RISK-09 | P2 | Python 注册表只取 InstallPath 默认值，未使用 ExecutablePath；部分子键/元数据读取错误被跳过。`detect/discovery.rs:135` | 验证非标准解释器文件名、不同发行商注册项及权限错误；按注册的可执行入口补充发现并展示诊断。 |

## 逐工具覆盖表

“已实现”仅表示存在扫描路径，不等于已完成真实发行版验收。所有工具都受 SCAN-01、发现范围及通用探测限制影响；表中列出额外的重点。

| 工具 | 当前系统扫描入口 | 受管扫描 | 本次判断/主要缺口 |
| --- | --- | --- | --- |
| Java / JDK | java -version | release 文件 | SCAN-01、05；缺 JDK/JRE、javac、供应商与架构区分，RISK-01。 |
| Python | python --version | python.exe --version | 多解释器及虚拟环境已有发现；别名、注册入口和代理仍有缺口，RISK-09。 |
| Node.js | node --version | node.exe --version | 标准布局已实现；版本管理器真实目标与项目上下文未完整解析，RISK-03/04。 |
| Go | go version | VERSION 或 go.exe | SCAN-04；模块缓存路径另见 SCAN-14。 |
| Rust | rustc --version | cargo-home/bin/cargo.exe | SCAN-02；未完整枚举 rustup 工具链及项目 override。 |
| PHP | php -v | php.exe -v | HOME 漏扫 SCAN-08；警告/前缀解析 RISK-02。 |
| Zig | zig version | zig.exe version | 标准输出已有处理；ZIG_HOME 漏扫 SCAN-08。 |
| LLVM | clang --version | bin/clang.exe --version | LLVM_HOME 漏扫；厂商前缀风险；没有验证 clang-cl/lld 等组件。 |
| Deno | deno --version | deno.exe --version | 标准输出已有处理；未结构化记录输出中的 V8/TypeScript 版本，属扩展信息。 |
| Bun | bun --version | bun.exe --version | 标准输出已有处理；未发现独立于通用问题的确定缺陷。 |
| GCC / MinGW | gcc --version | bin/gcc.exe --version | 末词解析 RISK-02；仅 gcc 存在不能证明 g++、链接器及工具链完整。 |
| CMake | cmake --version | 不支持 | 系统入口已实现；无受管类别是当前能力范围。 |
| GitHub CLI | gh --version | bin/gh.exe --version | 固定受管布局；根目录直接放 gh.exe 等便携布局需要兼容验证。 |
| npm | npm --version | 随 Node 发现，无独立类别 | 系统列表已有入口/版本；配置页定位和缓存数据不一致，SCAN-12/14。 |
| pnpm | pnpm --version | 无独立类别 | Corepack/项目上下文、配置读取和 store/cache 区分不足。 |
| Yarn | yarn --version | 无独立类别 | Classic/Berry 和项目 yarnPath 需适配，RISK-04/05。 |
| pip | pip --version；每个 Python 的 -m pip --version | 随 Python 发现，无独立类别 | 模块模式已实现；配置页未复用，SCAN-12/13；启动器和模块结果缺少关联展示。 |
| pipx | pipx --version | 不支持 | 只识别启动器，不枚举其管理环境；不是 pipx 完整环境清单。 |
| uv | uv --version | 不支持 | 不枚举 uv 管理的 Python 与 tool 环境；未在可发现位置的环境可能漏扫。 |
| Poetry | poetry --version | 不支持 | 不查询 poetry env list；自定义缓存下环境不保证被扫描。 |
| Cargo | cargo --version | Rust 附属 | 代理/工具链上下文和缓存目录问题；Cargo 与 rustc 版本没有明确关联。 |
| Composer | composer --version | 不支持 | 标准入口可发现；PHP 依赖失败保留为错误；仅 composer.phar 无启动器的安装不支持。 |
| Conda | conda --version | 不支持独立纳管 | 环境记录和常见目录已有发现；不读取 conda info/envs_dirs 的完整清单，非记录环境可能漏扫。 |
| Maven | mvn --version | lib/maven-core-*.jar | MAVEN_HOME 漏扫；命令依赖 JAVA_HOME；固定元数据布局 RISK-01。 |
| Gradle | gradle --version | lib/gradle-base-services-*.jar | 首行版本误判 SCAN-01，GRADLE_HOME 漏扫；固定元数据布局 RISK-01。 |
| Git | git --version | cmd/git.exe --version | 标准 PortableGit 布局已有处理；非 cmd 布局的纳管兼容性有限。 |
| .NET | dotnet --version | 不支持 | 只能查询所选 SDK；没有 --list-sdks/--list-runtimes，纯 runtime 安装可能只显示版本命令失败。 |
| Docker | docker --version | 不支持 | 只证明 CLI 版本；没有 daemon 可用性、服务端版本、context/容器内环境探测。 |

## 需要明确的能力边界

1. **不是全盘扫描。** 当前搜索持久化 PATH、启动进程 PATH、硬编码常见目录、部分 HOME、Python/Conda 记录、EnvCon 根和最多 8 个用户选择目录。未被这些入口覆盖的便携安装可能漏扫。
2. **项目遍历有边界。** 常见根按配置深度遍历，项目深度为 6，每个根最多 2000 个目录；跳过 node_modules、lib、dist、target 等名称和子链接目录。该策略用于控制成本，但会漏掉位于这些目录中的工具或 wrapper。
3. **命令别名未完整支持。** 固定 basename 配合 exe/com/cmd/bat；不枚举 python3、pip3、带版本后缀的命令、py 启动器清单、mvnw/gradlew、仅 PowerShell 脚本入口等。
4. **版本管理器清单尚未实现。** 扫到一个代理或部分安装目录，不等于已完整扫描 nvm/fnm/Volta、rustup、uv、Poetry、Conda 管理的全部版本。
5. **结果字段不足以表达环境关系。** `src-tauri/src/types.rs:145` 没有结构化供应商、架构、安装根、代理目标、解释器关联、JDK/JRE 类型。pip 启动器和模块结果可能指向同一 pip，但没有合并关联。
6. **用户变量页面只展示用户层。** `src-tauri/src/commands.rs:504` 明确读取 HKCU；系统变量不显示并非这个页面的读取错误，但它不能充当“有效环境变量全景”。建议扩展展示系统值、用户覆盖和有效值。
7. **终端与项目上下文不等价。** PATH 首选标记已有提示说明这一点；Shell 别名、函数、已激活虚拟环境、项目配置不在当前全局扫描语义内。
8. **不扫描 WSL、容器内部或远程主机。** Docker CLI 的发现不能代表这些环境已被检查。

## 相邻功能问题

- 缓存清理在 `src-tauri/src/caches.rs:106` 返回清理前估算大小，删除失败会被忽略；被占用文件没有删除时，仍可能宣称释放了相应空间。这不是扫描本身的缺陷，但会放大缓存统计的误导，需要与 SCAN-14/15 一起修复。
- 配置命令查询按工具和字段串行执行，多次超时会拖慢整个页面；`where.exe` 自身也没有 timeout。应与 RISK-06/07 的超时和进程管理一并处理。

## 验证记录与后续顺序

本次执行：在 `src-tauri` 下运行 `rtk cargo test --offline --lib detect::`，结果 **9 passed、1 ignored、13 filtered out**。被忽略的是依赖本机安装状态的 `host_scan`，本次没有执行它。

这些测试验证了 Windows 启动器优先级、精确命令路径、失败/超时/空输出、Python 模块参数、PATH 顺序、受管目录发现及项目遍历等。它们没有证明 28 种真实工具的全部发行版兼容，也没有覆盖本文列出的受管探测和配置/缓存问题。本次没有新增行为测试、修改系统配置或执行缓存清理。

建议修复顺序：

1. SCAN-01/02/03/05：统一可靠探测，修正版本与健康状态；补 Gradle/Java 多行输出、Rust 隔离、失败退出/JRE 夹具。
2. SCAN-04/06/07/08/09/10/11：修正元数据、安装身份、发现范围与环境刷新；补路径别名、项目重叠、变量删除和坏链接测试。
3. SCAN-12/13/14/15/16/17：统一各页面的数据来源与状态语义；补配置重定向、空 PATH、部分大小统计和 PATH 冲突测试。
4. 按 RISK 清单补真实发行版回归，再扩展工具链清单、项目上下文、架构/厂商和解释器关系等能力。
5. 修复验证完成后重新打包，并以新 EXE 单独验收。源码测试通过不能代替已发布产物验证。
