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

截至 2026-11-09：源码元数据三处一致为 0.6.0。提交 `4892dfec887018c2cc8f536e331e2c46c5a0d436` 的 Windows workflow `42575447096` 和 Pages workflow `42575446887` 均成功；Rust 374 passed / 14 ignored、完整 verify（两套读状态与 10 套 UI）、31 个必需文件 fresh clone、`npm ci`、前端构建和 Rust 测试通过。完整及生产 `npm audit` 均为 0；本机完整 `npm run verify` 和 `cargo check --offline` 通过。该提交构建的 Windows x64 独立 EXE 采用 `--no-bundle`：9,590,784 bytes，SHA-256 `4082894c045dd101ac02b77bbb06b0f89bfc1898709aa6f28ca663a64d375e41`，FileVersion/ProductVersion 0.6.0。GitHub `v0.6.0-rc.1` 已作为预发布发布，且只上传单独 EXE；没有生成安装包。发布说明与证据见 `docs/releases/release-0.6.0-rc.1.md` 和 `docs/releases/release-record-0.6.0-rc.1.json`。EnvCon 应用内 PowerShell 安装/切换、完整真实安装/覆盖升级/卸载与用户数据保留矩阵仍未验收；代码签名/时间戳仍需证书或服务。因此 RC 不代表 0.6.0 稳定版门槛已满足，ASMT-06 / R4 保持进行中。
