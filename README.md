# EnvCon — Windows 开发环境工具下载与扫描

<p align="center">
  <img src="src-tauri/icons/icon.png" width="96" height="96" alt="EnvCon Logo">
</p>

<p align="center">
  <strong>集中下载开发工具，查看本机版本与安装位置</strong>
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

EnvCon 是一款面向 Windows 的**开发环境工具下载与扫描工具**。它提供 JDK、Python、Node.js、Git 等工具的版本下载入口，并扫描本机已有工具，展示版本、安装位置和检测结果，减少逐个查找下载地址与安装目录的操作。

优先将环境安装到指定根目录。部分运行时需要调用安装器；PATH 和环境变量设置会写入 Windows 用户配置。跨机器复制后需重新配置路径并检查 Junction 目标。

### 产品边界

- 核心：工具下载与现有安装扫描；明确来源、版本、路径及操作结果。
- 现有版本切换、路径设置和包管理功能保留，作为辅助能力维护，不因定位收窄而直接删除。
- 当前不做项目自动配置、依赖求解、项目环境隔离或一键启动项目，不承诺安装某个工具就具备完整开发环境。
- 工具列表供用户按需选择，不是一份需要全部安装的清单；优先修好下载与扫描，不以增加工具数量作为目标。

## 功能特性

### 📊 仪表盘
- 一览所有已管理环境的分类统计
- 检测系统中散装安装的环境(含版本与路径)
- 根目录占用空间统计

### 📦 环境管理
- **0.5.5 / ADB**：扫描 PATH 和 Android SDK 标准目录，管理、下载、切换及卸载 Platform Tools；已有 SDK 只链接 `platform-tools`，不会删除其他 SDK 组件。[使用与边界](docs/guides/adb.md)。
- **扫描与验证**：支持增量扫描缓存、按需统计大小及取消；测试入口见[验证指南](docs/guides/testing.md)，历史验收以 [ROADMAP](ROADMAP.md) 和 `docs/evidence/` 为准。
- **多版本并存**:同一类型环境可安装任意多个版本,互不干扰
- **一键切换**:通过 Junction 指针切换当前版本,无需改动 PATH
- **链接来源核验**：新增切换/纳管链接记录入口、目标和文件身份；卸载预览及执行时核对。历史无来源记录但身份可读取的链接默认保留，可在预览中逐项确认仅移除入口；不因此认领创建来源。记录损坏、未完成或身份不匹配的链接仍阻止移除；外部目标保留。
- **散装环境纳入**:扫描 PATH 中已安装的散装环境(JDK/Python/Node/Git/GitHub CLI 等),一键以链接方式纳入统一管理——不移动、不复制文件,原安装位置不变
- **环境移除**:先预览影响范围，再删除受管目录及其中的包；外部纳管只移除链接，保留原目录。用户 PATH 的固定 current 入口仅在有连续来源记录、无其他受管版本依赖且执行时复查一致时清理，失败逐项报告；用户自建/来源不明/共享条目保留。原厂卸载器、用户级 pip 包、缓存和目录外登记不自动处理；完整卸载仍按 `UNINST-01` 验收，当前代码的隔离测试不代表真实安装卸载已验证。
- **安装器登记预览（进行中）**：Python/LLVM 普通受管目录预览只读核对卸载登记、精确安装路径及可识别的卸载入口。匹配到安装器或登记存在冲突/读取限制时，暂时阻止直接删除目录以保留卸载入口；原厂命令执行和残留目录清理仍待接入。外部纳管继续只处理链接。

### ⬇️ 下载中心
- 默认查询官方源，Node 按 Windows x64 ZIP 筛选并提供完整返回列表的分页访问；离线缓存与各源覆盖范围会明确提示。
- 下载会核对响应长度；当前部分官方源提供的 SHA-256 / 校验文件也会在提交和安装前核验，其他资产明确显示“未验证”，不把本地计算哈希当成来源证明。压缩包另检查路径/必要结构；LLVM 23.1.x 只有 MSI 时暂不列为可安装版本。
- 支持 **16 种**环境类型(见下表)，包含 Android Platform Tools / ADB
- **镜像加速**:默认走国内镜像源,大幅提升下载速度
- **并行下载**:多个环境同时下载,实时显示进度与速度
- **断点反馈**:下载 / 解压 / 安装全流程状态提示

### 🛣️ 路径管理
- 用户 / 系统 PATH 可视化编辑(增删、排序、去重)
- 常用环境变量配置(`JAVA_HOME`、`GOPATH`、`CARGO_HOME` 等),自动给出建议值
- **每次修改 PATH 前自动备份**,可随时恢复

### 包管理器
- 支持多个包管理器的版本探测、自更新诊断、全局包扫描与路径配置；具体能力和限制见[使用指南](docs/guides/package-managers.md)。
- **历史全局包**：可查询旧目录并进入重装，跳过 npm 隐藏更新残留；支持已配置的空目标，预览实际命令及缓存路径，拒绝非空冲突。[使用说明](docs/guides/global-package-reinstall.md)。
- **包管理器路径配置**:一键设置 npm / pnpm / yarn / pip 的全局安装路径与缓存路径
- **包管理器数据迁移与全局包重装**:预览旧目录并迁移数据，或在新目录逐包恢复 npm、pnpm、Yarn Classic、pip 全局包。详见 [迁移说明](docs/guides/package-manager-migration.md) 和 [重装说明](docs/guides/global-package-reinstall.md)。
- **Python 包重装**:从旧解释器或 site-packages 读取清单，按选择的旧版本安装到新虚拟环境，展示失败与依赖检查结果。详见 [重装说明](docs/guides/python-package-reinstall.md)。
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
| ADB | Android SDK Platform Tools | Google 官方 |

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

持续改进计划、验收条件和最新开发进度见 [ROADMAP.md](ROADMAP.md)。相关开发完成子项、验证或出现阻塞时同步更新，源码完成与产物发布分别记录。

发布、版本记录与回退流程见 [docs/guides/release-process.md](docs/guides/release-process.md)。提交前可运行 `scripts/verify-fresh-clone.ps1` 检查干净克隆；发布记录由 `scripts/record-release.ps1` 生成，不把本地构建或产物存在等同于正式发布。

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
