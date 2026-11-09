# EnvCon 发布与回退流程

本文只定义可复核的版本记录、标记和回退步骤，不代表已有正式发布。代码签名、Release 上传和真实安装矩阵仍是外部阻塞项。

## 版本一致性

发布前三个文件必须一致：

- `package.json` 的 `version`
- `src-tauri/tauri.conf.json` 的 `version`
- `src-tauri/Cargo.toml` 中 `[package] version`

运行：

```powershell
pwsh -File scripts/record-release.ps1
```

脚本默认在 `docs/release-record-<version>-<UTC时间>.json` 生成记录，记录 commit、分支、工作区是否脏、Node/Rust 构建环境、两个锁文件哈希以及发现的产物 SHA-256。已存在的产物只标为“存在但来源未核实”；工作区脏或没有产物时也会如实记录，不能把记录当作“已发布”。

## Tag 命名

- 正式版本：`v<major>.<minor>.<patch>`，例如 `v0.6.0`
- 预发布：`v<semver>-rc.<n>`，例如 `v0.6.0-rc.1`
- 不使用 `latest`、`release` 等浮动标签
- 不移动、不覆盖、不删除已推送的 tag；需要修正时发布新的补丁版本

## 发布门槛

1. R0～R3 的验收项已关闭，或未关闭项被明确记录为外部阻塞。
2. `npm run verify`、`cargo check --offline` 和 `git diff --check` 通过。
3. `scripts/record-release.ps1` 生成的记录中版本一致。
4. EXE/NSIS 由同一 commit 构建，产物路径、大小和 SHA-256 已写入 release record。
5. 发布说明写明已验证范围、待验证项和已知限制。

## 打 Tag（需要用户明确授权推送）

```powershell
git tag -a v0.6.0 -m "EnvCon v0.6.0"
git push origin v0.6.0
```

在用户授权前只创建本地 tag 也不能算作已发布；推送后仍需上传产物并核对记录。

## 回退

- 回退目标优先选择上一个已记录且产物哈希可核对的 tag。
- 不删除或强推覆盖历史 tag；从回退目标重新构建并记录新产物。
- 配置数据由应用自己的配置备份机制恢复；不把回退安装等同于恢复用户 PATH、全局包或第三方环境。
- 回退后用同一份 release record 模板重新记录实际部署的 commit、产物和哈希。

## 当前状态

截至 2026-11-09：源码元数据版本为 0.6.0。`5ddcb49` 的 Windows workflow `42564957845`、Pages workflow `42564957638` 均成功；Rust 374 passed / 14 ignored、完整 verify/10 套 UI、31 个必需文件 fresh clone、`npm ci`、前端构建和 Rust 测试通过。因 `.gitignore` 漏收 `tests/paths-ui.mjs` 的问题已修复并验证。之后 `npm audit` 发现 `vite`/`source-map-js` 开发依赖的 2 个 high 项；`npm audit fix` 更新 7 个锁定依赖，完整与生产审计均为 0，本机更新锁后的完整 `npm run verify` 和 `cargo check --offline` 通过。更新后的 `package-lock.json` 尚未提交，需重新跑 fresh clone 和 Windows CI。此前 EXE 是锁文件更新前基于 `5ddcb49` 构建，0.6.0、9,590,784 bytes、SHA-256 `1d70ad27da9fddf98a9940a83915777f570547a9ddb063e1a343c247942c3045`，不得作为候选资产；未生成安装包。待锁文件最终提交通过 CI 后再重建/核验并考虑 v0.6.0-rc.1。应用内 PowerShell 安装/切换仍未验收；最新 Release 仍为 v0.5.0 预发布；代码签名/时间戳仍需证书或服务。
