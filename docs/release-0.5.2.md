# EnvCon 0.5.2

## 变更

- 环境管理页提供明确的“卸载”与“移除管理”入口；外部链接只删除管理链接，普通受管环境删除自身目录，当前环境会先移除 current 链接。
- 下载中心默认使用官方源核对最新列表，Node 版本按 Windows x64 ZIP 过滤、完整排序并分页显示；镜像/离线缓存会明确提示可能滞后。
- 支持发现标准 rustup 工具链并纳入 EnvCon 管理；保留 EnvCon 原有 `cargo-home` / `rustup-home` 布局。
- 系统扫描按实际安装根目录合并 Git 的 `cmd`、`bin` 等重复入口；不同安装和不同探测结果仍分别保留。
- 版本统一为 0.5.2。

## 验证

- `cargo test --offline --lib -- --test-threads=1`: 71 passed, 7 ignored
- `npm run build`: passed
- 三套 UI 回归（0.5.2 专项、下载中心、启动预热）：passed；Git 去重由 Rust 临时夹具验证
- `git diff --check`: passed

最终串行库测试 71 passed / 7 ignored。另显式执行 Node 双源联网测试、本机扫描测试（含 Rust 临时链接/切换/探测/解除链接）通过。八类官方源在线测试通过：Node 585、Go 364、Python 205、JDK 13、Maven 2、Gradle 529、Zig 23、PHP 12 条；这些数字是本次查询结果，后续随上游变化。

应用同款 Rust HTTP 客户端在正常 Windows 权限下查询官方和镜像均通过：585 个 Windows x64 ZIP 版本，最新为 Node 26.9.0（2026-09-16）。受限测试环境曾遇到 TLS 凭据错误，不能据此认定用户机器正常运行时网络不可用。

本机只读扫描识别 Rust 1.98.1 的真实工具链，并将系统 Git 的 cmd/bin/mingw64 内部入口合并；另一套独立 Git 安装仍保留。未修改真实 PATH、环境目录或全局包。

## 原因与边界

- 环境卸载后端原已存在；本版补齐文字入口、外部链接说明、忙碌保护和失败后的刷新。外部安装仅解除纳管，不代替系统卸载器。
- Node 不存在固定的 2025 年截止条件。已确认原列表后端最多 100 条、前端只显示 60 条，且在线失败会回退旧缓存。无法从现在的查询还原用户之前看到旧数据的唯一原因；本版显式提示缓存状态，默认官方源并支持访问完整返回列表。
- JDK 官方主版本改由 Adoptium 可用版本接口发现，最多 4 个请求并发；修复镜像资产重复 `.zip` 后缀。镜像仍仅常用 LTS，官方每个主版本取最新构建，不冒称包含所有历史补丁。
- Go 仅保留 Windows amd64 archive。GitHub 系列仅最近 100 个发布记录，Maven 当前 3.x 分发目录、Rust 渠道及 Python 目录/Windows 安装器差异均在界面说明。
- rustup 的 `.cargo/bin/rustc.exe` 是代理，不是完整工具链，旧规则直接禁用了 Rust 纳管。现在解析实际 `toolchains/<channel>`，验证 rustc/cargo/lib/rustlib 后链接原目录；原工具链仍由 rustup 维护，原位置更新会反映到链接。不会把工具链目录误设为 CARGO_HOME 或 RUSTUP_HOME。
- 两种 Rust 布局的稳定 PATH 入口均可集成；之前仅配置单一路径的用户切换布局后需在路径管理中重新执行集成，并重开终端。
- Git 合并基于安装根和探测结果；不同安装、不同版本或失败结果不会被合并。Python 解释器与 pip 模块等不同调用语义继续独立展示。

## 验证中发现的情况

并行构建负载下，一项已有进程树超时测试未及时写入夹具 PID 文件；单独重跑通过，最终串行套件结果以发布记录为准。截图发现窄窗口环境详情挤成竖排，已改为响应式网格并重新检查。

## 本地构建产物（2026-09-18）

最终 `tauri build --bundles nsis` 成功，两个产物的 FileVersion / ProductVersion 均为 0.5.2。使用独立 `src-tauri/target-v0.5.2`，保留旧版产物。本次为本地打包，未上传远端或正式发布；未执行安装包的实际安装/覆盖升级验收。

| 产物（相对项目根目录） | 字节数 | SHA-256 |
| --- | ---: | --- |
| `src-tauri/target-v0.5.2/release/envcon.exe` | 7,965,184 | `16cc486669a8a2998f82fed2c7287ab687470d929a52a89b873aefe38516a504` |
| `src-tauri/target-v0.5.2/release/bundle/nsis/EnvCon_0.5.2_x64-setup.exe` | 2,723,484 | `2c6bd5cebc6db4cd1d95fef3837515bd1fa222b7c64b7eb84cd6c2a4de5af079` |
