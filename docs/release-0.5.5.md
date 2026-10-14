# EnvCon 0.5.5 构建记录

本版增加 ADB / Android SDK Platform Tools 环境管理，使用说明见 [ADB](adb.md)。版本声明在 package.json、锁文件、Cargo 和 Tauri 配置中统一为 0.5.5。

## 验证

- 源码阶段：Rust 92 passed / 9 ignored；其中 ADB 专项 6 passed / 1 ignored。
- TypeScript 和前端生产构建通过；两套前端读缓存/扫描状态测试通过。
- 六套 UI（设置、启动、卸载/纳管、下载、扫描、大小）在构建结束后单独重跑全部通过。首次并行编译期间设置页加载超时，保留该记录，最终结果采用独立重跑。
- Google XML 已通过外部只读获取并检查当前 Platform Tools 37.0.1 Windows 条目；应用 HTTP 在线测试仍属未执行的 ignored 项，不等同于在线端到端验证。

## 构建与交付

2026-09-19 使用独立 `src-tauri/target-v0.5.5` 目录完成 Windows x64 release/NSIS 构建（Rust release 耗时 10m16s）。两个产物的 FileVersion / ProductVersion 均为 **0.5.5**。

| 产物（相对项目根目录） | 字节数 | SHA-256 |
| --- | ---: | --- |
| `src-tauri/target-v0.5.5/release/envcon.exe` | 8,318,464 | `e31f14bce50cf3ef9e13863ff362a6aadcc73e4265087c634cae56c8260efe23` |
| `src-tauri/target-v0.5.5/release/bundle/nsis/EnvCon_0.5.5_x64-setup.exe` | 2,842,439 | `7071493d047085c9356f56b77cd6b18d0338e0b7e7cb6f041860f8b2bff71a1e` |

本地构建包含当前工作区 ADB 改动，不将此前的 Git 提交冒充为 0.5.5 源码提交。未执行安装包实际安装、覆盖升级和卸载；未正式发布 GitHub Release。

旧版本 EXE 已核对保持原哈希：

| 版本 | SHA-256 |
| --- | --- |
| 0.5.2 | `16cc486669a8a2998f82fed2c7287ab687470d929a52a89b873aefe38516a504` |
| 0.5.3 | `64dba5931a8eb976abd3736e4c772b043288ad5916d8314664ba393d8cd6e950` |
| 0.5.4 | `ff5a5a42845d8cbf62d0bd1d6764cd56a268d88d7f56d8479a2bfc3b9ac0c0f5` |
