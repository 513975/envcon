# EnvCon — Windows 开发环境一站式管理工具

<p align="center">
  <img src="src-tauri/icons/icon.png" width="96" height="96" alt="EnvCon Logo">
</p>

<p align="center">
  <strong>下载 · 安装 · 切换 · 清理,一个应用全搞定</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Tauri-2.x-orange?logo=tauri" alt="Tauri">
  <img src="https://img.shields.io/badge/React-19-61dafb?logo=react" alt="React">
  <img src="https://img.shields.io/badge/TypeScript-6-3178c6?logo=typescript" alt="TypeScript">
  <img src="https://img.shields.io/badge/Rust-2021-dea584?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/platform-Windows-0078d4?logo=windows" alt="Windows">
  <img src="https://img.shields.io/badge/license-MIT-green" alt="License">
</p>

---

## 简介

EnvCon 是一款面向 Windows 的**便携式开发环境管理器**。它把 JDK、Python、Node.js、Go 等各类开发工具统一收纳到独立目录中,通过 **NTFS Junction** 实现版本秒级切换,配合国内镜像加速下载,从此告别"配环境配一天"的日子。

优先将环境安装到指定根目录。部分运行时需要调用安装器；PATH 和环境变量设置会写入 Windows 用户配置。跨机器复制后需重新配置路径并检查 Junction 目标。

## 功能特性

### 📊 仪表盘
- 一览所有已管理环境的分类统计
- 检测系统中散装安装的环境(含版本与路径)
- 根目录占用空间统计

### 📦 环境管理
- **多版本并存**:同一类型环境可安装任意多个版本,互不干扰
- **一键切换**:通过 Junction 指针切换当前版本,无需改动 PATH
- **散装环境纳入**:扫描 PATH 中已安装的散装环境(JDK/Python/Node/Git/GitHub CLI 等),一键以链接方式纳入统一管理——不移动、不复制文件,原安装位置不变
- **安全卸载**:删除旧版本不残留垃圾文件(链接环境仅移除链接,不动原目录)

### ⬇️ 下载中心
- 支持 **15 种**环境类型(见下表)
- **镜像加速**:默认走国内镜像源,大幅提升下载速度
- **并行下载**:多个环境同时下载,实时显示进度与速度
- **断点反馈**:下载 / 解压 / 安装全流程状态提示

### 🛣️ 路径管理
- 用户 / 系统 PATH 可视化编辑(增删、排序、去重)
- 常用环境变量配置(`JAVA_HOME`、`GOPATH`、`CARGO_HOME` 等),自动给出建议值
- **每次修改 PATH 前自动备份**,可随时恢复

### 包管理器
- **0.5.0 通用包管理器工作区**：集中能力定义，支持 npm、pnpm、Yarn Classic、pip、Bun、uv Tools、pipx、Cargo、Composer、.NET Tools。新增常见位置、本地磁盘、指定目录的旧包扫描，按来源查看包并进入重装。[能力与限制](docs/package-managers.md) · [开发架构](docs/package-manager-architecture.md)。
- **历史全局包**：可查询旧目录并进入重装，跳过 npm 隐藏更新残留；支持已配置的空目标，预览实际命令及缓存路径，拒绝非空冲突。[使用说明](docs/global-package-reinstall.md)。
- **0.4.2 旧包清理**：全部重装成功后，可预览并确认卸载指定旧包，保留运行时及新环境；支持逐包诊断和清理报告。[范围与限制](docs/old-package-cleanup.md)。
- **0.4.1 界面整理**：与环境管理共用选择按钮，已验证可用管理器显示绿灯；路径操作跟随所选管理器。本机扫描按工具分类汇总，多路径与诊断展开查看。[扫描结果说明](docs/scan-results.md)。
- **包管理器路径配置**:一键设置 npm / pnpm / yarn / pip 的全局安装路径与缓存路径
- **包管理器数据迁移与全局包重装**:预览旧目录并迁移数据，或在新目录逐包恢复 npm、pnpm、Yarn Classic、pip 全局包。详见 [迁移说明](docs/package-manager-migration.md) 和 [重装说明](docs/global-package-reinstall.md)。
- **0.3.1 审查修订**:修复 npm 配置冲突、pnpm 11 安装组识别及安装进程树超时清理，完善任务恢复、逐包结果和弹窗布局。[审查与验证记录](docs/reinstall-audit-2026-09-12.md)。
- **Python 包重装**:从旧解释器或 site-packages 读取清单，按选择的旧版本安装到新虚拟环境，展示失败与依赖检查结果。详见 [重装说明](docs/python-package-reinstall.md)。
- **0.4.0 包管理器工作区**:左侧独立入口，查看 npm/pnpm/Yarn Classic 全局包、项目直接依赖及 pip 当前/虚拟环境包，支持搜索版本与安装位置。路径设置、缓存、迁移与重装集中放置。[使用说明](docs/package-managers.md) · [关键代码审计](docs/key-function-audit-2026-09-12.md)。
- 缓存检测与一键清理(npm / pip / cargo 等缓存目录)

### ⚙️ 设置
- 自定义环境根目录与下载目录
- 按环境类型分别指定镜像 / 官方源
- 便携模式,数据随目录走

### 🎨 体验细节
- 深色 / 浅色主题自动跟随系统
- `Ctrl+1..5` 保持原有页面快捷键，`Ctrl+6` 打开包管理器
- 页面 keep-alive,切换不重载

## 支持的环境类型

| 类型 | 说明 | 镜像源 |
|------|------|--------|
| JDK | Eclipse Temurin | 清华 TUNA |
| Python | CPython 官方发行版 | 华为云镜像 |
| Node.js | 官方发行版 | npmmirror |
| Go | 官方工具链 | golang.google.cn |
| Rust | 官方发行版 | rsproxy.cn |
| Maven | Apache Maven | 华为云镜像 |
| Gradle | 官方发行版 | 腾讯云镜像 |
| PHP | windows.php.net | 官方源 |
| LLVM | 官方预编译版 | ghfast.top 加速 |
| Zig | 官方发行版 | 官方源 |
| Deno | 官方发行版 | ghfast.top 加速 |
| Bun | 官方发行版 | ghfast.top 加速 |
| Git | Git for Windows | ghfast.top 加速 |
| GitHub CLI | 官方发行版 | ghfast.top 加速 |
| C/C++ | MinGW-w64 (WinLibs, UCRT) | ghfast.top 加速 |

## 工作原理

EnvCon 在你指定的根目录(默认 `D:\DevEnv`)下按类型分文件夹存放各版本:

```
D:\DevEnv\
├── jdks\          # 各版本 JDK
│   ├── temurin-8\
│   └── temurin-21\
├── nodes\
│   ├── node-20\
│   └── node-22\
├── current\       # Junction 指针目录
│   ├── jdk   -> jdks\temurin-21   # 切换版本 = 重指 Junction
│   └── node  -> nodes\node-22
└── globals\       # npm/pip 等全局包与缓存(可配置)
```

切换版本时只需将 `current\jdk` 重新指向目标版本。确保 `current\*` 位于用户 PATH 的优先位置后，新开的终端会读取该版本；已打开的终端需重新打开。

## 从源码构建

### 环境要求

- [Node.js](https://nodejs.org/) 20.19+ 或 22.12+（Vite 8 要求）
- [Rust](https://www.rust-lang.org/) (stable 工具链)
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) (含 C++ 桌面开发工作负载)
- [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (Windows 10/11 一般已内置)

### 构建步骤

```bash
# 1. 克隆仓库
git clone https://github.com/513975/envcon.git
cd envcon

# 2. 安装前端依赖
npm install

# 3. 开发模式运行(热重载)
npm run tauri dev

# 4. 构建 Release 安装包(NSIS / MSI)
npm run tauri build
```

构建产物位于 `src-tauri/target/release/bundle/` 下。

## 项目结构

```
envcon/
├── src/                        # 前端 (React + TypeScript)
│   ├── pages/                  #   六个页面:仪表盘/环境管理/下载中心/路径管理/包管理器/设置
│   ├── components/             #   通用组件与 UI 库
│   └── lib/                    #   API 封装、状态管理、类型定义
└── src-tauri/                  # 后端 (Rust + Tauri 2)
    └── src/
        ├── commands.rs         #   Tauri 命令入口
        ├── download.rs         #   多任务并行下载
        ├── install.rs          #   解压与安装流程
        ├── sources.rs          #   各环境版本源解析(镜像/官方)
        ├── switcher.rs         #   Junction 版本切换
        ├── pathman.rs          #   PATH / 环境变量 / 注册表操作
        ├── pkgtools.rs         #   包管理器全局路径配置
        ├── packages.rs         #   已安装包查询与结构化清单解析
        ├── caches.rs           #   缓存检测与清理
        ├── detect/             #   系统环境扫描
        └── config.rs           #   应用配置持久化
```

## 技术栈

| 层 | 技术 |
|----|------|
| 桌面框架 | Tauri 2 |
| 前端 | React 19 · TypeScript · Vite 8 · Tailwind CSS 4 · Zustand |
| 后端 | Rust · tokio · reqwest · winreg · junction |
| 打包 | NSIS / MSI |

## 使用提示

- 修改 PATH / 环境变量后,**新开的终端**才能读到新值
- PATH 修改前的备份存放在数据目录 `backups\path\` 下,可在"路径管理"页恢复
- `src-tauri/target/release/envcon.exe` 可直接启动，需已安装 WebView2；NSIS 安装包可协助安装运行时。
- 跨机器复制管理根后，检查绝对路径、Junction 和包管理器配置，再配置新机器的 PATH。

## 许可证

[MIT](LICENSE)

## 致谢

- 各镜像站:清华 TUNA · 华为云 · npmmirror · rsproxy.cn · 腾讯云 · ghfast.top
- [Tauri](https://tauri.app/) · [React](https://react.dev/) · [Rust](https://www.rust-lang.org/)
