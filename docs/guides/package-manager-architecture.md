# 包管理器架构与验证（0.5.1）

## 本次方向

采用共享能力清单、领域数据类型、管理器适配器和通用业务流程的分层结构。保留 React + TypeScript + Tauri/Rust 技术栈，重构包管理器相关模块。已有下载、环境安装和 PATH 功能继续使用原边界，避免整项目重写引入额外回归。

## 模块职责

| 模块 | 职责 |
| --- | --- |
| src/lib/managerCatalog.json | 前后端共用的十管理器清单、家族、适配器类型、恢复方式、查询/配置/迁移能力 |
| src/lib/managers.ts | 前端能力类型、查找入口；页面不另存一份管理器列表 |
| package_managers/mod.rs | Rust 能力类型、ManagerId 校验、统一清单读取与配置映射 |
| package_managers/types.rs | 与具体 Python/Node 流程无关的 Package、ItemResult |
| package_managers/node.rs | Node 系管理器清单、pnpm 版本布局、包名/版本/来源校验 |
| package_managers/native.rs | uv/pipx/Cargo/Composer/.NET 元数据、路径映射及清理边界 |
| package_managers/commands.rs | 安装/卸载 Action、Invocation、隔离环境变量和目标初始化 |
| pkgtools.rs | 运行实例定位、版本可用性探测、实际路径查询及配置写入 |
| packages.rs | 当前、项目和历史目录查询的应用入口 |
| package_scan.rs | 扫描根目录、遍历限制、取消、分组、诊断和任务快照 |
| global_reinstall.rs / pip_reinstall.rs | 全局目录 / Python venv 两类恢复流程，预览、复核、安装、最终验证、报告 |
| migration.rs / old_packages.rs | 数据迁移事务，以及成功任务的可选旧包卸载 |
| reinstall_process.rs / fsutil.rs | 进程树生命周期、超时和通用路径/目录工具 |
| src/lib/useTaskStatus.ts | 通用任务状态轮询、过期响应隔离和操作期间暂停轮询 |
| PackageManagers / InstalledPackages / OldPackageScan | 工作区组织、安装清单和旧来源扫描 |

新增管理器应先定义实际语义，再声明能力。adapter 区分 Node CLI、pip 和使用隔离环境变量的适配器；family 描述包布局家族。Bun 使用 Node 包清单，但使用自己的配置与命令适配器，这两个维度不能合并为“是不是 Node”的单个开关。

管理器专有命令和元数据匹配仍使用局部 match 分支。它们表达实际差异，不引入运行时插件加载、动态反射或每工具复制一套页面和任务状态机。

## 业务约束

1. 查询旧包只读取磁盘元数据；缺失、损坏、来源不明确分别保留诊断。
2. 路径设置、迁移、重装和旧包清理含义独立。配置不会自动搬包，恢复不会自动删除旧包。
3. 非空目标、源目标重叠、重定向入口和运行时根有后端校验。开始执行前重新生成计划，拒绝过期清单。
4. 所选顶层包固定版本；不承诺重建原依赖树或原私有源配置。自定义来源、附加参数和注入包不能悄悄丢失。
5. 迁移/重装/配置/扫描通过同一互斥边界防止并发操作；外部进程并不受此锁保护。
6. 子进程使用超时和 Windows Job Object，停止/超时不遗留安装进程树。扫描读取配置阶段也可取消。
7. 旧包清理仅接受当前会话的成功恢复结果，执行前复核新副本、原版本和文件范围，默认保留旧包。
8. 所有未实现命令返回明确错误，不以 Yarn 或其他管理器作为默认兜底。

## 本轮发现与修复

- 清单读取原依附全局重装模块，其他只读功能反向依赖它；已迁入管理器模块。
- 共用包类型原定义在 pip 重装模块；已迁入通用领域类型。
- 新管理器最初使用固定“额外六个”判断；已替换为能力清单中的 AdapterKind。
- Node 安装构建器曾把未知类型落入 Yarn；现显式拒绝。
- Bun 空全局目录会向父目录查找项目；目标初始化必须建立自己的 package.json。真实回归验证父项目清单和 lock 未被修改。
- 扫描对 junction 的诊断顺序错误；现先识别链接并记录跳过，再检查普通目录。
- 扫描等待路径配置时无法及时停止；现支持该阶段取消。
- pipx 缓存、Cargo home 读取未充分使用最新用户环境；已统一读取路径覆盖值。
- .NET 实际全局路径被“不能永久配置全局目录”的 UI 条件隐藏；已拆开显示与配置能力。
- Cargo 恢复预览缓存位置与 CARGO_HOME 布局不一致；已显示实际 .cargo/registry。
- pip 历史来源输入仍称虚拟环境目录；现显示 site-packages，避免混淆输入。
- UI 测试依赖外部常驻 Vite 易连接失败；新增 tests/run-ui.mjs 自动启动、就绪等待、执行和关闭。

## 扩展步骤

1. 在 managerCatalog.json 声明能力；Rust 对 id 唯一性、支持的配置适配器等执行校验测试。
2. 实现实例定位、实际路径读取；错误必须保留，不把查询失败解释为空清单。
3. 在适配器实现安装清单、来源约束、安装/卸载命令和清理文件范围。
4. 添加结构化元数据夹具及路径边界测试；能获得实际工具时，用临时安装验证生成的路径和删除范围。
5. 检查项目查询、迁移、恢复入口是否符合实际语义，更新能力表和 UI 用例。

使用 rustfmt 标准格式维护 Rust 模块，TypeScript/TSX 使用 Prettier 默认格式。避免将整段业务挤成单行；类型承载共同契约，专有差异留在适配器中。

## 验证记录

- Rust 单元测试：64 passed，6 个需要指定真实环境的用例默认忽略。
- 显式执行真实离线临时安装/卸载：npm、pnpm、Yarn Classic、Bun、uv、pipx、Cargo 通过。保留新副本、未选包及无关文件。
- 本机只读旧目录扫描：D:/DevEnvManager/envs/nodes/node-24 返回 13 个安装记录。该目录来源元数据不足，可自动恢复数为 0，界面保留原因。
- Composer/.NET：安装记录、官方/自定义源、固定版本参数、重复记录、路径约束夹具通过。本机缺少 PHP 和 .NET SDK，真实安装未验证。
- UI：十管理器能力选择、状态灯、实际路径、扫描取消、分组、诊断、来源到恢复窗口、版本号，以及 1100/390 像素布局。
- 既有全局重装、pip 重装、旧包清理界面用例通过；迁移和包列表继续回归。
- 生产前端构建和 Windows EXE/NSIS 构建通过。最终产物与摘要见 progress.md。

运行方式（Windows，工具已安装时）：

```powershell
rtk cargo test --offline --lib --manifest-path src-tauri/Cargo.toml
rtk npm run build
rtk proxy node tests/run-ui.mjs <playwright模块目录> tests/package-scan-ui.mjs tests/packages-ui.mjs tests/migration-ui.mjs
```

真实管理器测试通过 ENVCON_TEST_* 显式指定工具，测试写入工作区随机临时目录，不改动用户真实包或注册表。界面测试使用 IPC 夹具，不能替代真实管理器测试。

## 已知范围

支持十种开发包管理器；WinGet、Scoop、Chocolatey、Conda、Yarn Berry/PnP 尚未接入。历史扫描是有上限的目录发现，不承诺覆盖所有位置。Python 的普通 site-packages 恢复使用新 venv；孤立旧目录不能自动卸载。跨主机迁移、私有仓库凭据搬迁和依赖树原样复现不在本版范围。
