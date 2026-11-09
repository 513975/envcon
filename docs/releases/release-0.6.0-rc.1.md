# EnvCon v0.6.0-rc.1

这是 0.6.0 的 Windows x64 预发布候选，仅提供独立 EXE，不包含安装包。源码版本元数据为 0.6.0。

## 本次交付

- 依赖安全修复已包含在候选源码中：完整与生产依赖审计均为 0 vulnerabilities。
- Windows workflow `42575447096` 在源码提交 `4892dfec887018c2cc8f536e331e2c46c5a0d436` 上通过；GitHub Pages workflow `42575446887` 部署成功。
- 干净克隆验证覆盖 31 个必需文件、`npm ci`、前端构建与 Rust 库测试；完整 UI/读状态验证在 Windows CI 与本机通过。
- 本机 Windows x64 release 构建使用 Tauri `--no-bundle`，没有生成 NSIS/MSI 安装包。

## 下载文件

| 文件 | 版本 | 大小 | SHA-256 |
| --- | --- | ---: | --- |
| `envcon.exe` | FileVersion/ProductVersion 0.6.0；Windows x64 GUI | 9,590,784 bytes | `4082894c045dd101ac02b77bbb06b0f89bfc1898709aa6f28ca663a64d375e41` |

该 EXE 未签名。建议仅在你接受预发布软件的隔离环境中试用，并先核对下载文件的 SHA-256。

## 尚未完成

- EnvCon 应用内 PowerShell 7 安装/切换验收。
- 完整真实环境安装、覆盖升级、卸载与用户数据保留矩阵。
- Windows 代码签名与时间戳。
- 本次只提供 EXE；不提供 NSIS/MSI 安装包。

因此这是预发布候选，不代表 0.6.0 稳定版门槛已满足。
