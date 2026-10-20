# 0.5.0 源码及产物核验

核验日期：2026-09-14。

## 结论

本轮开始时，工作区已经恢复到本次会话可核对的 0.5.0 状态。0.5.0 发布后新增的更新功能、启动预热、目录统计变更和删除实现替换均不在当前源码中。因此本轮没有再次回退或修改业务源码，没有重置 Git，也没有修改真实环境或包管理器配置。

Git 目前的 HEAD 是早期提交 `b91b5b0`，没有 0.5.0 提交或标签。0.5.0 的功能包括大量未提交和未跟踪文件；直接重置到 HEAD 会丢失这些功能。由于没有原始发布源码的完整哈希清单，本次不能声称已经对历史源码做了逐字节验证。结论依据是会话中的实际改动记录、当前代码、版本声明及回归测试。

## 核对内容

| 项目 | 当前结果 |
| --- | --- |
| package.json / package-lock.json 根包 | 0.5.0 |
| Cargo.toml / Cargo.lock 的 envcon 包 | 0.5.0 |
| tauri.conf.json | 0.5.0 |
| 应用版本显示 | Vite 从 package.json 注入 APP_VERSION |
| 更新 IPC 和注册 | 无 update_package_manager |
| 更新前端 API 和按钮 | 无 updatePackageManager / 更新当前工具 |
| 启动预热 | App.tsx 无额外 getOverview 调用 |
| 环境目录大小 | 恢复原 dir_size_blocking 统计调用 |
| 环境删除 | 原 force_remove_dir_all 实现；未混入后续替换 |
| npm 来源识别 | 保留 0.5.0 的仓库来源校验；后续尝试的补丁未落入源码 |

回到旧版本也会保留旧版本的功能边界和已知问题，本次没有夹带新功能或修复。

## 验证

- `cargo test --offline --lib`：64 passed，6 ignored；没有执行需要额外运行条件的 ignored 测试。
- `npm run build`：TypeScript 与 Vite 生产构建通过。
- 六套 Playwright 回归通过：包列表、旧包扫描、迁移、全局重装、Python 重装、重装后清理。界面测试使用模拟 IPC，不更新或删除用户真实包。
- 与核验前快照比较，85 个源码及构建配置文件的 SHA-256 未变化。

## 原版 Windows 产物

原版单 EXE：`src-tauri/target-v0.5.0/release/envcon.exe`。

- 大小：7,798,784 字节。
- SHA-256：`f16c0b0ead5501714ed23e38a5656c8ed26c36419a59d719516e33a1ddea3ba7`。

原版安装包：`src-tauri/target-v0.5.0/release/bundle/nsis/EnvCon_0.5.0_x64-setup.exe`。

- 大小：2,679,765 字节。
- SHA-256：`6e626fb7d22475c52257b7f0454d9a7be18058e0915a7e1da8f4651c7c81bdce`。

两者校验值均与原 0.5.0 发布记录一致，未重新打包覆盖。后续问题构建曾沿用 0.5.0 的版本号，仅凭版本号无法区分；本轮核验时 `target-v0.5.1/release/envcon.exe` 已不存在。

## 核验前备份

`tests/tooling/recovery-0.5.0-2026-09-14T01-31-46-645Z/`：

- `workspace/`：709 个核验前工作区文件，包含未提交和未跟踪源码、文档及测试。
- `manifest.json`：文件大小及 SHA-256。
- `git-status.txt`：核验前 Git 状态。

此备份是当前工作区快照，不是原始发布时保存的源码快照；不包含 Git 数据库、依赖目录或编译产物。
