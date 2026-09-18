# 验证入口与 Windows 测试矩阵

本文件追踪 IMP-05 的首轮最小矩阵，不代表完整 Windows 兼容认证。进度主清单为 [ROADMAP](../ROADMAP.md)。

## 一次执行本轮必要回归

先安装项目锁文件中的前端依赖，并准备 Rust Windows MSVC 工具链、Tauri 构建依赖和本地 Cargo 依赖缓存。UI 测试需要 Playwright 模块和 Microsoft Edge。

```powershell
# Playwright 已可直接解析时
rtk npm run verify

# 使用本机已有的 Playwright 模块（替换为实际路径）
$env:ENVCON_PLAYWRIGHT_MODULE = 'C:\path\to\node_modules\playwright'
rtk npm run verify

# 无浏览器时仅执行源码和非 UI 回归；结果明确标记跳过 UI
rtk npm run verify -- --skip-ui
```

`tests/verify.mjs` 按顺序执行 TypeScript/Vite 生产构建、Rust 离线串行库测试、read-cache/scan-client 两套读状态专项与六套 UI 测试（设置、启动、0.5.2、下载、扫描、大小）。遇到错误立即失败退出。串行运行 Rust 进程夹具，避免重型并行构建影响超时测试。测试脚本纳入版本控制，截图、临时数据和 tooling 不纳入。

`--offline --lib` 不包括全部集成测试；被忽略的真实安装测试、在线源测试和本机扫描/基准必须分别显式执行，不能计入默认通过数。

## 只读基准

在 `src-tauri` 目录执行：

```powershell
rtk cargo test --offline --lib host_query_benchmark -- --ignored --nocapture --test-threads=1
```

它对十种管理器执行只读版本/路径查询，分别比较两份独立查询与共享并发查询，连续三组，输出 JSON。root=None，不选项目目录；命令尝试次数包含 where.exe、版本及路径探测，并非操作系统全进程统计。输出不包含 registry 凭据或完整配置值。不要与其他 Cargo 测试或性能任务并行运行。该基准不代表冷启动、磁盘 I/O、内存或全盘扫描性能。

## 本轮矩阵（2026-09-18）

0.5.4 原生基准：从项目根运行 `rtk proxy node tests/native-benchmark.mjs <exe> <label> <playwright-module> 3`。先后测量两个版本，不要与构建/测试并行。脚本仅启动私有 EXE 副本、隔离 WebView 配置和固定测试目录；不会启动安装器或修改用户 PATH/管理器配置。Windows Job 对进程树累计计数，包括短命令、启动器和 WebView2；资源是逻辑 I/O 与提交内存，不声称物理磁盘吞吐或整机内存。缓存冷/热是应用快照状态，未清空 OS 文件缓存。结果写入 tests/artifacts/native-benchmark，正式摘要见 [IMP-01](imp01.md)。

| 场景 | 验证方式 | 结果 / 边界 |
| --- | --- | --- |
| Windows 本机 MSVC x64 | 实机 Rust 与 Edge UI | 本轮通过；具体最终计数见迭代记录 |
| 旧 config.json 升级 | 临时 JSON 夹具 | schema 迁移、未知字段保留、上次有效文件保留 |
| 损坏 / null / 错误字段 / 未来 schema | 临时 JSON 夹具 | 拒绝写入与默认根目录回退；原文件不变 |
| 配置恢复 / 损坏备份 / 主文件丢失 | 临时文件夹具 + 模拟 IPC UI | 显式确认；保留恢复前文件；失败不改变状态 |
| 配置文件占用、其他实例与外部修改 | Windows 文件共享锁 + 两份 AppConfig | 保存失败、内存 root 回滚、备份可读、阻止覆盖过期快照 |
| 空格与 Unicode 目录 | Windows 临时目录夹具 | 保存失败/回滚覆盖该路径；完整长路径矩阵未做 |
| 并发查询、强制刷新、取消初始化者 | Tokio 夹具 | 共享工作、失效后不复用旧任务、取消后其他调用继续 |
| 前端过期成功/错误响应 | read-cache.mjs | 转接最新查询；异常后可重试 |
| 持久缓存、定时失效、子文件/配置变化 | scan_cache.rs + scan-client.mjs + scans-ui.mjs | 重启读取、损坏忽略、并发合并、写期间拒绝旧响应、强制刷新、公平调度和后台提示 |
| 环境/缓存大小 | size_tasks.rs + sizes-ui.mjs | 逻辑字节、取消、并发上限、跨页保留、链接循环、不完整读取、390px 布局 |
| 恢复 UI、启动、卸载/Rust 纳管、下载 | 四套模拟 IPC Edge UI | 真实组件及交互；不代替原生 WebView2 端到端验收 |
| 设置页 1100px / 390px | 截图及布局断言 | 已检查；长路径换行另见最终截图记录 |
| Windows 10/11 各版本、管理员/标准账户 | 尚未建立多机矩阵 | 未覆盖，不把当前机器推及所有系统 |
| 磁盘满、突然断电、网络盘、跨卷、长路径 | 尚未故障注入 | 未验证；已用 flush + 同目录替换，不承诺硬件级事务 |
| 安装包全新安装/覆盖升级/卸载 | 本轮不操作用户系统 | 未验证；本地构建与正式发布分开记录 |

后续优先加入故障注入、多机矩阵和 CI；真实环境测试必须使用隔离夹具，不能修改用户包、PATH 或唯一配置副本。
