# EnvCon 0.5.1

## 本版变更

- 包管理器页面新增“更新当前工具”，按各工具官方命令更新自身；不具备安全一键语义的 .NET CLI 和 Cargo 给出升级指引。
- 环境删除使用不跟随 junction/symlink 的递归实现，纳入管理的外部环境只移除链接。
- 启动 Overview 扫描先完成目录结构和版本探测，跳过递归大小统计，首屏不再等待大目录遍历。
- 统一应用、前端、Tauri 和锁文件版本为 0.5.1。

## 产物

- `src-tauri/target-v0.5.1/release/envcon.exe`，7,808,512 字节，SHA-256 `781c3bbd6d988aa8fe0e5b1312ce9496feaa5a7d15f6f731bc80132a64bc639a`
- `src-tauri/target-v0.5.1/release/bundle/nsis/EnvCon_0.5.1_x64-setup.exe`，2,681,457 字节，SHA-256 `71ebec4c73de9eb6e080f5fccafb2cce907039b9ad10089eed013a24c0c56d80`

## 验证

- `cargo test --offline --lib`: 64 passed, 6 ignored
- `npm run build`: passed
- `git diff --check`: passed

Composer、.NET SDK 和真实用户全局包未在本机执行破坏性更新；旧包缺少仓库来源元数据时仍需用户显式选择来源后恢复。
