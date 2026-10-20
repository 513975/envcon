# 全局包重装

“包管理器 / 路径与迁移”支持从旧目录读取并在新目录逐包重装：

0.4.2 增加重装成功后的可选旧包卸载，先预览再确认，详情见 [旧包清理](old-package-cleanup.md)。默认仍保留旧包。

- npm、pnpm（传统数字版本目录及 11.x 的 v11 安装组）、Yarn Classic 1.x：读取顶层全局包清单，固定版本逐包安装，并验证最终目录。
- pip：从旧 Python 或 `site-packages` 读取清单，在新虚拟环境重装。
- Yarn Berry 不支持 `global add`，界面会明确提示。
- 0.5.0 接入 Bun、uv Tools、pipx、Cargo、Composer、.NET Tools；共用预览、选择、执行、验证、取消、报告和成功后清理流程，包布局与来源验证由适配器负责。

从 0.4.3 起，Node 管理器的目标可以是尚不存在的目录，也可以是“仅设置路径”已经创建的空普通目录。目标不能与旧包目录重叠，不能是目录链接；非空目标列出冲突内容并拒绝写入，不合并或覆盖已有包。开始执行前再次检查目录和清单。pip 仍必须使用尚不存在的新虚拟环境。

应用不自动修改旧目录、全局配置和 PATH。目标若已是当前配置目录，新包安装后自然会进入当前全局查询。安装完成后生成 `envcon-source.json` 和 `envcon-reinstall.json` 报告。URL、Git、本地、链接、别名和损坏元数据的包无法仅凭版本号恢复，需要原始安装材料。

## 目标目录含义（0.4.3）

目标输入和目录选择器均直接指定实际全局目录 `T`，不再自动附加 `-restored` 或额外的 `global`。默认目标取当前配置的全局目录，无配置时取受管默认目录。旧目录独立选择，避免把已配置的空目标误当作旧包来源。

| 管理器 | 全局配置对应值 | 包布局 | 命令目录 | 本次安装缓存/存储 |
| --- | --- | --- | --- | --- |
| npm | prefix = T | T/node_modules | T | T/cache |
| pnpm | global-dir = T | T/版本目录，如 v11 | T/bin | T/store |
| Yarn Classic | global-folder = T | T/node_modules | T/bin | T/cache |
| Bun | BUN_INSTALL_GLOBAL_DIR = T | T/node_modules | T/bin | T/cache |
| uv Tools | UV_TOOL_DIR = T | T/工具名 | T/bin | T/cache |
| pipx | PIPX_HOME = T | T/venvs/工具名 | T/bin | T/cache |
| Cargo | --root T | T/bin、.crates.toml | T/bin | T/.cargo/registry |
| Composer | COMPOSER_HOME = T | T/vendor | T/vendor/bin | T/cache |
| .NET Tools | --tool-path T | T/.store | T | T/cache |

预览显示上述实际路径。pnpm/Yarn 的命令目录与全局包目录不同；重装只创建相应启动脚本，不自动把 `T/bin` 加入 PATH。需要时在路径管理中加入该命令目录，后续常规安装的 bin/cache 配置也应另行核对。本次任务的缓存/存储是独立路径，不会自动替换原缓存配置。

历史目录可从“已安装包 / 历史全局目录”查询，或使用 0.5.0 的“旧包扫描”查找常见位置、全部本地磁盘及指定目录，再点击“重装此目录的包”。历史查询直接读取磁盘，未安装包管理器也能查看。npm scope 内的隐藏更新残留不再冒充正式包；损坏的正常包仍展示诊断。

例如本机旧来源为 `D:\DevEnvManager\envs\nodes\node-24`，目标可直接选择已配置且为空的 `D:\DevEnvManager\globals\npm-global`。Node 安装目录中的 npm、Yarn、Corepack 自身仍在清单中标为不可自动重装，不会连同运行时删除。

## 0.3.1 审查修订

- 启动器可手工输入或选择；未自动检测到管理器时仍能恢复历史目录。修改启动器或目录后必须重新读取清单。
- npm 读取隐藏 `.package-lock.json` 的来源；缺少可确认来源的包禁选。自动识别 npm 官方仓库和 npmmirror 的标准 tarball 路径，无法确认的私有仓库条目显示原因。
- 用户配置与全局配置分别使用独立的空文件，避免 npm 重复加载 `NUL` 报错。Yarn 安装不加载旧 rc；因此不会自动继承私有仓库凭证或自定义镜像。
- 窗口显示逐包状态、命令目录、失败原因和可打开的报告；可关闭窗口在本次应用会话中后台运行。关闭整个应用会中止任务；重启后仅保留磁盘报告，没有自动断点续装。
- 停止操作等待当前包结束，仍核对已装包；单个安装超过十分钟将结束其 Windows Job Object 中的进程树。未执行包标记为已停止。
- 同一应用会话保留各 Node 管理器的最近结果；新任务不隐藏另一个管理器的结果。

## 验证边界

固定的是所选顶层包版本，并非原依赖树的逐字节重建。新的 Node/Python、操作系统或仓库状态可能让某些旧版本无法安装。最终元数据核对不等同于验证每个 CLI 的全部功能；确认后再手动配置新路径。

新目录隔离是路径配置隔离，不是安全沙箱。所选包及依赖的安装脚本具有当前用户权限；应用自身不搬动旧目录，但不保证第三方脚本没有其他副作用。pnpm 自身对构建脚本的限制仍然生效。

Bun 执行前在目标创建独立 package.json，避免空目标向父目录查找项目。Cargo 固定安装版本并请求 locked 依赖；自定义仓库、Git 或本地安装记录不会冒充 crates.io 包。.NET 使用独立 NuGet 配置，仅恢复已确认的 nuget.org 来源。uv/pipx 附加依赖、注入包或自定义参数保留诊断，避免不完整复制原安装要求。

审查与回归记录见 [新功能审查](../audits/reinstall-audit-2026-09-12.md)。
