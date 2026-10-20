# EnvCon 文档索引

本目录按**用途**分类。进度主清单是根目录 [`ROADMAP.md`](../ROADMAP.md)，本文档只是它的索引与说明补充，不替代它。

## 目录结构

```
docs/
├── guides/          功能说明 —— 面向使用者，长期维护
├── evidence/        验收证据 —— 一次性快照，按阶段归档
├── audits/          阶段性审计 —— 某一版本的代码/功能审查
├── releases/        发布与回退记录
├── iterations/      迭代过程记录
└── index.html       产品落地页
```

## guides/ —— 功能说明

描述**功能怎么用、边界在哪**。这些文件长期有效，功能变更时同步更新。

| 文件 | 内容 |
| --- | --- |
| [adb.md](guides/adb.md) | ADB / Android Platform Tools 管理与 SDK 共存边界 |
| [package-managers.md](guides/package-managers.md) | 包管理器入口与十种管理器的角色定义 |
| [package-manager-architecture.md](guides/package-manager-architecture.md) | 分层架构与适配器边界 |
| [package-manager-migration.md](guides/package-manager-migration.md) | 目录迁移流程与十管理器复用 |
| [global-package-reinstall.md](guides/global-package-reinstall.md) | 从旧目录逐包重装到新目录 |
| [python-package-reinstall.md](guides/python-package-reinstall.md) | pip 包重装的入口与限制 |
| [old-package-cleanup.md](guides/old-package-cleanup.md) | 旧包清理的可删边界与会话限制 |
| [scan-results.md](guides/scan-results.md) | 本机扫描结果的汇总与分类 |
| [testing.md](guides/testing.md) | 验证入口与 Windows 测试矩阵 |
| [release-process.md](guides/release-process.md) | 版本一致性、标记与回退步骤 |

## evidence/ —— 验收证据

每份文件记录**一轮具体验证**：做了什么、怎么验证、哪些没验证。文件名后缀即生成日期。

**这些是快照，不是当前状态。** 引用某份证据时，必须同时看 `ROADMAP` 里该条目之后的变更——源码在证据生成后仍可能继续改动。

按阶段分子目录：

| 目录 | 内容 |
| --- | --- |
| `evidence/r0/` | 资产审计、镜像比对、包管理器矩阵、安装冒烟 |
| `evidence/r1/` | 校验与配置注入边界 |
| `evidence/r2/` | 扫描状态、入口版本冲突、UI 渲染 |
| `evidence/r3/` | 环境诊断、清单导入导出、卸载各模式、链接修复 |
| `evidence/r4/` | 发布相关可执行部分 |
| `evidence/path-01/` | 目录角色、配置预览、命令解析、Yarn 能力 |
| `evidence/asmt04/` | PATH 修复预览、修复后复诊、切换诊断 |
| `evidence/imp02/` | 包管理器更新预览、批量、留痕、能力边界 |
| `evidence/imp06/` | 安装任务跨页状态 |
| `evidence/uninstall/` | 安装器卸载、Junction 归属、历史 PATH 清理 |

### 单份证据的结构

统一包含这些字段（少数早期文件缺 `status`，见下方「已知不一致」）：

- `schemaVersion`、`date`、`scope`
- `status` —— 一句话说明**验证到什么程度**，例如 `implemented-and-isolated-verified; no real user environment touched`
- `fixtures` / `uiAssertions` —— 具体夹具与断言
- `verification` —— 实际执行的命令与结果
- `notVerified` —— **明确列出没验证的部分**
- `limitations` —— 方法本身的局限

读取时先看 `status` 和 `notVerified`：这两项决定这份证据能支撑什么结论。

## audits/ —— 阶段性审计

某一版本的代码与功能审查，**审计范围写在文件开头**。审计结论不构成兼容性保证。

| 文件 | 范围 |
| --- | --- |
| [environment-scan-audit-2026-09-09.md](audits/environment-scan-audit-2026-09-09.md) | 0.2.1 全部环境扫描审查 |
| [key-function-audit-2026-09-12.md](audits/key-function-audit-2026-09-12.md) | 0.4.0 关键调用链 |
| [key-function-audit-2026-09-15.md](audits/key-function-audit-2026-09-15.md) | 0.5.1 需求验收与修订 |
| [reinstall-audit-2026-09-12.md](audits/reinstall-audit-2026-09-12.md) | 0.3.1 迁移、重装、Python 包 |

## releases/ —— 发布与回退

| 文件 | 内容 |
| --- | --- |
| [release-0.5.1.md](releases/release-0.5.1.md) | 0.5.1 变更 |
| [release-0.5.2.md](releases/release-0.5.2.md) | 0.5.2 变更 |
| [release-0.5.5.md](releases/release-0.5.5.md) | 0.5.5 构建记录 |
| [release-record-0.5.5-20261018-230054.json](releases/release-record-0.5.5-20261018-230054.json) | 0.5.5 可复核记录 |
| [rollback-to-0.5.0.md](releases/rollback-to-0.5.0.md) | 0.5.0 源码与产物核验 |

**注意**：0.5.5 已打包但**未正式发布**（无 tag、无 Release 上传）。

## iterations/ —— 迭代记录

| 文件 | 内容 |
| --- | --- |
| [imp01.md](iterations/imp01.md) | 增量扫描与性能验收过程 |
| [iteration-0.5.3.md](iterations/iteration-0.5.3.md) | 0.5.3 首轮持续改进 |

## 已知不一致

以下问题已确认，暂未修复：

| 问题 | 影响 |
| --- | --- |
| 5 个 JSON 缺 `status` 字段：`imp01-benchmark.json`、`path-ownership-verification-2026-10-17.json`、`r0-asset-audit-2026-10-17.json`、`r0-asset-audit-2026-10-19.json`、`r4-release-verification-2026-10-19.json` | 无法用统一脚本判断验证程度，需人工读 `scope`/`notVerified` |
| `imp01-benchmark.json` 是性能基准数据，非验收证据 | 已归入 `evidence/imp01/`，但结构与其余证据不同 |

## 如何判断某个结论是否可信

1. 先看 `ROADMAP.md` 对应条目的**当前**描述
2. 找到它引用的证据文件
3. 读该证据的 `status` 与 `notVerified`
4. 若 `ROADMAP` 在该证据日期之后又有改动，以 `ROADMAP` 为准

**未验证不得标记已完成**——这是本项目的硬规则。看到「隔离回归通过、实机待验证」时，意思是代码写完了、夹具过了，但**没在真实 Windows 环境跑过**。
