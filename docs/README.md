# EnvCon 文档索引

日常状态、当前待办和历史验收统一以根目录 [ROADMAP](../ROADMAP.md) 为准。本目录保留长期使用指南、提案、当前审计、验收证据和最新构建记录。

## 使用指南

- [ADB 管理](guides/adb.md)
- [包管理器](guides/package-managers.md)、[架构](guides/package-manager-architecture.md)与[迁移](guides/package-manager-migration.md)
- [全局包重装](guides/global-package-reinstall.md)、[Python 包重装](guides/python-package-reinstall.md)与[旧包清理](guides/old-package-cleanup.md)
- [扫描结果](guides/scan-results.md)
- [测试与验证](guides/testing.md)
- [发布流程](guides/release-process.md)

## 提案

- [前端重构项目说明（OpenDesign 交付版）](proposals/ui-redesign-brief-2026-11.md)：逐页需求、交互状态、安全边界、技术约束与验收标准；待产品审核和排期，不代表已批准实施。

## 审计与证据（各项注明快照日期；当前状态以路线图为准）

- [工具安装完整性审计](audits/tool-install-completeness-audit-2026-10-22.md)：截至 2026-11-03 的 Rust 组件安装、系统 PATH、Python / Gradle 取舍及 LLVM 待验证项快照。
- [早期审计与旧版本归档摘要](archive-legacy.md)。
- `evidence/`：按改进条目归档的测试、实机检查和构建证据。证据是生成时快照；当前状态请对照 `ROADMAP.md`。
- [0.5.5 构建记录](releases/release-0.5.5.md)与[可复核产物记录](releases/release-record-0.5.5-20261018-230054.json)。

## 文档维护

阶段记录和旧版本验收已合并到 `ROADMAP.md`，不再为每次迭代另建说明文件。新增文档前先确认内容不能放入现有指南、主清单或证据文件。
