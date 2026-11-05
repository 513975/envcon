# 历史文档归档摘要

本页合并早期审计、迭代和旧版本发布记录中的长期有用信息。当前状态和完整历史顺序以根目录 `ROADMAP.md` 为准；`docs/evidence/` 中仍保留逐项验证数据。已删除的旧文档不再作为当前功能或发布状态的依据。

## 早期审计

- 0.2.1 扫描审查：修复版本解析、安装身份去重、HOME / 注册表发现、链接诊断、包管理器探测、缓存统计、PATH 缺失处理及扫描状态展示；早期审计行号和待办已过时。
- 0.3.1 重装审查：修复 npm 配置隔离、pnpm v11 安装组识别、安装进程树超时清理、旧预览提交及逐包状态展示。
- 0.4.0 关键链路审计：强化路径约束、配置恢复、缓存清理确认、包管理器查询诊断、pnpm 布局及环境切换回退。
- 0.5.1 需求审计：加入包管理器自更新诊断、旧包删除预览、缺失来源时明确确认恢复、启动预热及环境删除保护。实现边界以现有使用指南为准。

## 历史版本产物

本地构建不代表远端发布。旧安装包的真实安装、升级或卸载验收情况见 `ROADMAP.md` 对应阶段记录。

| 版本 | 产物 | 字节数 | SHA-256 |
| --- | --- | ---: | --- |
| 0.5.1 | `src-tauri/target-v0.5.1/release/envcon.exe` | 7,808,512 | `781c3bbd6d988aa8fe0e5b1312ce9496feaa5a7d15f6f731bc80132a64bc639a` |
| 0.5.1 | `src-tauri/target-v0.5.1/release/bundle/nsis/EnvCon_0.5.1_x64-setup.exe` | 2,681,457 | `71ebec4c73de9eb6e080f5fccafb2cce907039b9ad10089eed013a24c0c56d80` |
| 0.5.2 | `src-tauri/target-v0.5.2/release/envcon.exe` | 7,965,184 | `16cc486669a8a2998f82fed2c7287ab687470d929a52a89b873aefe38516a504` |
| 0.5.2 | `src-tauri/target-v0.5.2/release/bundle/nsis/EnvCon_0.5.2_x64-setup.exe` | 2,723,484 | `2c6bd5cebc6db4cd1d95fef3837515bd1fa222b7c64b7eb84cd6c2a4de5af079` |
| 0.5.3 | `src-tauri/target-v0.5.3/release/envcon.exe` | 7,998,464 | `64dba5931a8eb976abd3736e4c772b043288ad5916d8314664ba393d8cd6e950` |
| 0.5.3 | `src-tauri/target-v0.5.3/release/bundle/nsis/EnvCon_0.5.3_x64-setup.exe` | 2,736,194 | `d6a66f870fddb59e7dc7ddf48f5ceaa2783e1cbceb239bdc7b3961e904b7164c` |
| 0.5.0 | `src-tauri/target-v0.5.0/release/envcon.exe` | 7,798,784 | `f16c0b0ead5501714ed23e38a5656c8ed26c36419a59d719516e33a1ddea3ba7` |
| 0.5.0 | `src-tauri/target-v0.5.0/release/bundle/nsis/EnvCon_0.5.0_x64-setup.exe` | 2,679,765 | `6e626fb7d22475c52257b7f0454d9a7be18058e0915a7e1da8f4651c7c81bdce` |

0.5.3 首轮设置配置兼容、缓存及旧响应保护的验收记录已合并至 `ROADMAP.md`；0.5.4 性能样本仍单独保存在 `docs/evidence/imp01/imp01-benchmark.json`。历史基准曾记录三组十管理器配置查询均值由 4,104 ms 降至 3,355 ms，命令尝试数由 68 降至 34；这只是该机器和该场景的本地测量，不是通用性能承诺。
