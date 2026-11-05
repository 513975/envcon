import assert from "node:assert/strict";
import { installScanMock } from "./scan-mock.mjs";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { chromium } = require(process.argv[3] || "playwright");
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1100, height: 850 } });
const errors = [];
page.on("pageerror", e => errors.push(String(e)));
try {
  await installScanMock(page);
  await page.addInitScript(() => {
    window.calls = [];
    window.offline = false;
    window.failUninstall = false;
    window.previewBlocked = false;
    window.previewSourceBlocked = false;
    window.previewDirLocked = false;
    window.previewLegacyLinks = false;
    window.previewLegacyPath = false;
    window.previewInstallerBlocked = false;
    window.cancelManifestSave = false;
    window.cancelManifestOpen = false;
    window.redetectStillBroken = false;
    window.lastRedetect = null;
    window.lastManifestPath = null;
    window.lastImportPath = null;
    window.envs = [
      { name: "jdk-owned", envType: "jdk", isCurrent: true, isLinkEntry: false, isExternalLink: false },
      { name: "jdk-linked", envType: "jdk", isCurrent: false, isLinkEntry: true, isExternalLink: true },
      // 已搬入受管的环境：真实目录 + 来源记录，卡片上应出现「删除旧目录」。
      { name: "jdk-moved", envType: "jdk", isCurrent: false, isLinkEntry: false, isExternalLink: false, movedFrom: "E:/external/jdk-old" },
      { name: "llvm-registered", envType: "llvm", isCurrent: false, isLinkEntry: false, isExternalLink: false },
    ].map(e => ({ ...e, path: `D:/managed/envs/${e.envType === "llvm" ? "llvms" : "jdks"}/${e.name}`, identityPath: e.isExternalLink ? "E:/external/jdk" : `D:/managed/envs/${e.envType === "llvm" ? "llvms" : "jdks"}/${e.name}`,
      version: "25.0.1", sizeBytes: null, sizeComplete: false, status: "available", statusDetail: null }));
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        switch (command) {
          case "get_overview": return { rootExists: true, root: "D:/managed", categories: ["jdk", "rust", "llvm"].map(envType => ({ envType,
            envs: window.envs.filter(e => e.envType === envType), current: window.envs.find(e => e.envType === envType && e.isCurrent)?.name ?? null,
            junctionPath: "D:/managed/current/" + envType, scanWarning: null, currentError: null })) };
          case "uninstall_env":
            await new Promise(resolve => { window.finishUninstall = resolve; });
            if (window.failUninstall) {
              return {
                name: args.name,
                status: "partial",
                removed: [{ resource: "当前版本 Junction", path: "D:/managed/current/jdk", details: "fixture detached" }],
                retained: [{ resource: "EnvCon 受管目录", path: `D:/managed/envs/jdks/${args.name}`, details: "fixture still present", count: 1 }],
                unverified: [{ resource: "PATH 与环境变量", path: null, details: "fixture unverified", count: null }],
                failed: [
                  { resource: "EnvCon 受管目录", path: `D:/managed/envs/jdks/${args.name}`, details: "fixture: file in use" },
                  { resource: "EnvCon 用户 PATH 条目", path: "D:/managed/current/jdk/bin", details: "fixture: PATH recheck failed; may have executed" },
                ],
              };
            }
            window.envs = window.envs.filter(e => e.name !== args.name);
            return {
              name: args.name,
              status: "verifiedScopeRemoved",
              removed: [{ resource: window.previewLegacyLinks ? "用户确认移除的管理链接" : args.name === "jdk-linked" ? "EnvCon 外部纳管链接" : "EnvCon 受管目录", path: `D:/managed/envs/jdks/${args.name}`, details: "fixture removed" }],
              retained: [{ resource: args.name === "jdk-linked" ? "外部原安装目标" : "Python 用户级包、外部 venv 与共享缓存", path: args.name === "jdk-linked" ? "E:/external/jdk" : null, details: "fixture retained", count: null }],
              unverified: [{ resource: "PATH 与环境变量", path: null, details: "fixture unverified", count: null }],
              failed: [],
            };
          case "preview_uninstall_env":
            if (args.envType === "llvm") {
              const path = "D:/managed/envs/llvms/llvm-registered";
              const installerBlocked = window.previewInstallerBlocked;
              window.lastUninstallPreview = {
                name: args.name, envType: "llvm", isCurrent: false, isLinkEntry: false, isExternalLink: false,
                snapshotHash: "fixture-llvm-tree", pathSnapshotHash: null, currentLinkSnapshotHash: null,
                legacyLinks: [], confirmedLegacyLinks: [], legacyPathEntries: [], confirmedLegacyPathEntries: [],
                removable: installerBlocked ? [{ resource: "EnvCon 受管目录", path, details: "fixture directory", count: 2 }]
                  : [{ resource: "原厂卸载范围（执行后复查）", path: path + "/Uninstall.exe",
                    details: "登记：HKCU/64/LLVM；固定参数：/S。执行前重新读取登记并核对 EXE 哈希；不经过 shell，也不回退到递归删除目录。", count: null }],
                retained: [],
                unverified: [{ resource: "安装器目录外副作用", path: null,
                  details: "执行仅依据精确匹配的本地卸载入口；目录外注册项、共享组件、用户级包和缓存不在可证明删除范围内。", count: null }],
                blocked: installerBlocked ? [{ resource: "原厂卸载登记", path,
                  details: "登记读取不完整、路径/命令无法核验或存在冲突；保留目录和卸载入口。", count: null }] : [],
                scopeComplete: !installerBlocked,
                installerPlan: { envType: "llvm", state: installerBlocked ? "unverified" : "ready", installPath: path,
                  invocation: { registrationId: "HKCU/64/LLVM", executable: path + "/Uninstall.exe", arguments: ["/S"], fileSha256: "fixture-only" },
                  assessments: [], errors: [], registryFingerprint: "fixture-registry" },
              };
              return window.lastUninstallPreview;
            }
            const scopeBlocked = window.previewBlocked;
            window.lastUninstallPreview = {
              name: args.name, envType: args.envType,
              isCurrent: args.name === "jdk-owned" || window.previewLegacyLinks,
              isLinkEntry: args.name === "jdk-linked",
              isExternalLink: args.name === "jdk-linked",
              snapshotHash: "fixture-snapshot",
              pathSnapshotHash: "fixture-path-snapshot",
              currentLinkSnapshotHash: args.name === "jdk-owned" || window.previewLegacyLinks ? "fixture-link-identity" : null,
              confirmedLegacyLinks: [],
              legacyPathEntries: window.previewLegacyPath ? [
                { resource: "来源未知的用户 PATH 条目", path: "D:/managed/current/jdk/bin", details: "fixture legacy PATH; no EnvCon ownership receipt", count: null },
              ] : [],
              confirmedLegacyPathEntries: [],
              legacyLinks: window.previewLegacyLinks ? [
                { resource: "来源未知的 current 链接", path: "D:/managed/current/jdk", details: "fixture legacy current link; target is the selected management link", count: null },
                { resource: "来源未知的管理链接", path: "D:/managed/envs/jdks/jdk-linked", details: "fixture legacy management link; external target retained", count: null },
              ] : [],
              removable: window.previewLegacyLinks ? [] : [
                { resource: args.name === "jdk-linked" ? "EnvCon 外部纳管链接" : "EnvCon 受管目录", path: `D:/managed/envs/jdks/${args.name}`, details: "fixture preview", count: 1 },
                ...(args.name === "jdk-owned" && !window.previewLegacyPath ? [{ resource: "EnvCon 用户 PATH 条目", path: "D:/managed/current/jdk/bin", details: "fixture: owned PATH entry, no shared dependency", count: null }] : []),
              ],
              retained: [
                { resource: args.name === "jdk-linked" ? "外部原安装目标" : "Python 用户级包、外部 venv 与共享缓存", path: args.name === "jdk-linked" ? "E:/external/jdk" : null, details: "fixture retained", count: null },
                ...(args.name === "jdk-linked" ? [{ resource: "EnvCon 用户 PATH 条目", path: "D:/managed/current/jdk/bin", details: "fixture: shared PATH entry retained", count: null }] : []),
              ],
              unverified: [
                { resource: "PATH 与环境变量", path: null, details: "fixture unverified", count: null },
                ...(window.previewDirLocked ? [{
                  resource: "目录占用",
                  path: `D:/managed/envs/jdks/${args.name}`,
                  details: "1 个目录当前被其他进程打开，删除会失败：cache。请先退出占用这些目录的程序（或被设为工作目录的终端）再执行删除。",
                  count: 1,
                }] : []),
              ],
              blocked: window.previewSourceBlocked
                ? [{ resource: "管理链接来源", path: `D:/managed/envs/jdks/${args.name}`, details: "fixture: link identity changed or source damaged; entry retained", count: null }]
                : scopeBlocked ? [{ resource: "受管目录树", path: `D:/managed/envs/jdks/${args.name}`, details: "fixture unreadable", count: null }] : [],
              scopeComplete: !scopeBlocked && !window.previewSourceBlocked,
              mode: args.mode ?? "uninstall",
            };
            // Mirror the backend's Detach shaping: the directory moves from
            // removable to retained, and only the current junction/PATH stay.
            if (window.lastUninstallPreview.mode === "detach") {
              const keepsRunning = (item) =>
                item.resource === "当前版本 Junction" || item.resource === "EnvCon 用户 PATH 条目";
              const kept = window.lastUninstallPreview.removable.filter(item => !keepsRunning(item));
              window.lastUninstallPreview.removable =
                window.lastUninstallPreview.removable.filter(keepsRunning);
              window.lastUninstallPreview.retained = [
                ...window.lastUninstallPreview.retained,
                ...kept.map(item => ({
                  ...item,
                  details: `停用模式：保留磁盘内容，不删除。原说明：${item.details}`,
                })),
              ];
            }
            return window.lastUninstallPreview;
          case "scan_system": return { warnings: [], tools: [{ tool: "Rust", version: "1.90.0", path: "E:/rust/toolchains/stable/bin/rustc.exe",
            command: "rustc.exe", source: "PATH", envType: "rust", installRoot: "E:/rust/toolchains/stable", identityPath: "E:/rust/toolchains/stable/bin/rustc.exe", isPreferred: true, error: null }] };
          case "integrate_external_env": return "system-1.90.0";
          case "preview_move_environment":
            window.lastMovePreview = {
              envType: args.envType,
              name: args.name,
              source: "E:/external/jdk",
              target: `D:/managed/envs/jdks/${args.name}`,
              files: 42,
              bytes: 1048576,
              links: 0,
            };
            return window.lastMovePreview;
          case "execute_move_environment":
            // 不改写 window.envs：后续断言仍然依赖 jdk-linked 保持外部链接状态。
            return { target: args.plan.target, source: args.plan.source, files: 42, bytes: 1048576 };
          case "preview_discard_moved_source":
            window.lastDiscardPreview = {
              envType: args.envType,
              name: args.name,
              source: args.source,
              files: 42,
              bytes: 1048576,
            };
            return window.lastDiscardPreview;
          case "execute_discard_moved_source":
            window.discardedSource = args.plan.source;
            return null;
          case "get_sources": return [{ id: "mirror", label: "Mirror" }, { id: "official", label: "Official" }];
          case "list_versions": return Array.from({ length: 125 }, (_, i) => ({ version: `26.${124-i}.0`, date: "2026-09-16", sizeBytes: null, url: null, note: window.offline ? "离线缓存（在线查询失败）" : null }));
          case "get_tool_configs": case "get_caches": case "install_tasks": return [];
          case "switch_env": {
            const tool = args.envType === "llvm" ? "clang" : "java";
            const folder = args.envType === "llvm" ? "llvms" : "jdks";
            window.envs = window.envs.map(e =>
              e.envType === args.envType ? { ...e, isCurrent: e.name === args.name } : e,
            );
            return {
              envType: args.envType,
              name: args.name,
              currentTarget: `D:/managed/envs/${folder}/${args.name}`,
              version: "25.0.1",
              commands: [
                { command: tool, status: window.switchShadowed ? "shadowed" : "managed", expected: `D:/managed/current/${args.envType}/bin`, resolved: window.switchShadowed ? `C:/other/${tool}.exe` : `D:/managed/current/${args.envType}/bin/${tool}.exe`, detail: window.switchShadowed ? "PATH 靠前位置命中了其他安装" : "PATH 解析命中受管入口" },
              ],
              pathStatus: window.switchShadowed ? "shadowed" : "requires-new-terminal",
              pathDetail: "入口目录已写入用户或系统 PATH，但当前应用进程尚未刷新；请在新终端核对命令",
              retained: window.switchShadowed ? ["已有终端与 PATH 靠前位置仍可能命中旧版本；请在新终端核对，必要时自行清理旧入口。"] : [],
              notes: ["切换只改变 current 链接；已打开终端继承的是旧环境，需要新终端才会读取新 PATH"],
            };
          }
          case "preview_path_repair":
            return {
              envType: args.envType,
              entry: `D:/managed/current/${args.envType}/bin`,
              shadowing: ["C:/Program Files/Other/bin"],
              mode: window.repairManual ? "manual" : "reorder",
              expectedPath: "C:/Program Files/Other/bin;D:/managed/current/llvm/bin",
              resultingPath: "D:/managed/current/llvm/bin;C:/Program Files/Other/bin",
              summary: "把受管入口移到用户 PATH 最前；1 项冲突目录随后保留",
              notes: ["只调整顺序：不新增、不删除、不改写任何条目", "需要新终端才能看到效果；本预览本身不写入 PATH"],
              blocked: window.repairManual ? ["受管入口在用户 PATH 中缺失或重复出现，无法只调整顺序"] : [],
              plan: window.repairManual ? null : {
                expectedPath: "C:/Program Files/Other/bin;D:/managed/current/llvm/bin",
                entry: `D:/managed/current/${args.envType}/bin`,
                shadowing: ["C:/Program Files/Other/bin"],
                resultingPath: "D:/managed/current/llvm/bin;C:/Program Files/Other/bin",
              },
              manualEntries: window.repairManual ? ["C:/Program Files/Other/bin"] : [],
            };
          case "apply_path_repair":
            return {
              applied: args.plan,
              verified: !window.repairUnverified,
              userPath: args.plan.resultingPath,
              commands: [{ command: "clang", status: window.repairUnverified ? "shadowed" : "managed", expected: args.plan.entry, resolved: `${args.plan.entry}/clang.exe`, detail: "fixture" }],
              problems: window.repairUnverified ? ["以下命令仍被覆盖，可能需要新终端或处理系统 PATH：clang"] : [],
              requestedPath: args.plan.expectedPath,
              requiresNewTerminal: true,
            };
          case "redetect_environment":
            window.lastRedetect = { envType: args.envType, name: args.name };
            return {
              envType: args.envType,
              name: args.name,
              path: `D:/managed/envs/jdks/${args.name}`,
              version: window.redetectStillBroken ? null : "25.0.1",
              status: window.redetectStillBroken ? "broken" : "available",
              statusDetail: window.redetectStillBroken
                ? "版本探测失败：node --version 失败：无法启动: fixture"
                : null,
              isCurrent: args.name === "jdk-owned",
            };
          case "plugin:dialog|save":
            // A cancelled dialog returns null and must not trigger an export.
            return window.cancelManifestSave ? null : "D:/exports/envcon-manifest.json";
          case "plugin:dialog|open":
            // Cancelling the picker must not trigger a comparison.
            return window.cancelManifestOpen ? null : "D:/exports/recorded-manifest.json";
          case "export_environment_manifest":
            window.lastManifestPath = args.path;
            return args.path;
          case "import_environment_manifest":
            window.lastImportPath = args.path;
            return {
              schemaVersion: 1,
              recordedGeneratedAt: "fixture-token",
              recordedPortable: false,
              recordedRoot: "D:/managed",
              currentRoot: "D:/managed",
              sameRoot: true,
              rows: [
                {
                  envType: "jdk", name: "jdk-owned", kind: "unchanged",
                  recordedVersion: "25.0.1", recordedPath: "D:/managed/envs/jdks/jdk-owned", recordedStatus: "available",
                  currentVersion: "25.0.1", currentPath: "D:/managed/envs/jdks/jdk-owned", currentStatus: "available",
                  differences: [],
                },
                {
                  envType: "jdk", name: "jdk-retired", kind: "missing",
                  recordedVersion: "21.0.1", recordedPath: "D:/managed/envs/jdks/jdk-retired", recordedStatus: "available",
                  currentVersion: null, currentPath: null, currentStatus: null,
                  differences: [],
                },
                {
                  envType: "llvm", name: "llvm-registered", kind: "changed",
                  recordedVersion: "19.0.0", recordedPath: "D:/managed/envs/llvms/llvm-registered", recordedStatus: "available",
                  currentVersion: "20.0.0", currentPath: "d:\\managed\\envs\\llvms\\llvm-registered", currentStatus: "broken",
                  differences: ["版本", "状态"],
                },
                {
                  envType: "node", name: "node-new", kind: "added",
                  recordedVersion: null, recordedPath: null, recordedStatus: null,
                  currentVersion: "22.0.0", currentPath: "D:/managed/envs/nodes/node-new", currentStatus: "available",
                  differences: [],
                },
              ],
              summary: { unchanged: 1, added: 1, missing: 1, changed: 1 },
              limitations: [
                "比对按（环境类型 + 名称）配对：同名但实际不同的两个目录会被当作同一条目",
                "本功能只读：不会创建、移动、切换或删除任何环境",
              ],
            };
          default: return null;
        }
      },
    };
  });
  await page.goto(process.argv[2]);
  await page.getByRole("button", { name: "环境管理", exact: true }).click();

  // 导出清单是只读操作：必须恰好调用一次导出命令，并如实说明这不是可还原的备份。
  const manifestButton = page.getByRole("button", { name: "导出清单", exact: true });
  await manifestButton.click();
  await page.waitForFunction(() =>
    window.calls.some(c => c.command === "export_environment_manifest"));
  assert.equal(
    await page.evaluate(() => window.lastManifestPath),
    "D:/exports/envcon-manifest.json",
  );
  await page.getByText("环境清单已导出", { exact: true }).waitFor();
  await page.getByText(/不是可还原的备份/).first().waitFor();
  const exportsAfterSave = await page.evaluate(() =>
    window.calls.filter(c => c.command === "export_environment_manifest").length);
  assert.equal(exportsAfterSave, 1);

  // 取消保存对话框时不得发起导出。
  await page.evaluate(() => { window.cancelManifestSave = true; });
  await manifestButton.click();
  await page.waitForFunction(() =>
    window.calls.filter(c => c.command === "plugin:dialog|save").length === 2);
  assert.equal(
    await page.evaluate(() =>
      window.calls.filter(c => c.command === "export_environment_manifest").length),
    1,
    "cancelling the save dialog must not export",
  );
  await page.evaluate(() => { window.cancelManifestSave = false; });

  // 环境列表的搜索与健康过滤只在本地生效：不重新扫描，也不把筛选空结果
  // 说成「没有安装」。
  //
  // 定位用「重新检测 <名称>」的 aria-label，而不是按文本匹配名称：
  // 页面上其他地方（修复说明、清单限制）也会出现同样的名字，按文本匹配
  // 会命中那些隐藏节点。
  const overviewCallsBefore = await page.evaluate(() =>
    window.calls.filter(c => c.command === "get_overview").length);
  const envSearch = page.getByLabel("搜索环境");
  const rowFor = (name) =>
    page.getByRole("button", { name: `重新检测 ${name}`, exact: true });

  await envSearch.fill("jdk-owned");
  await rowFor("jdk-owned").waitFor();
  assert.equal(
    await rowFor("jdk-linked").count(),
    0,
    "a non-matching environment must be filtered out",
  );
  // 筛选后无结果时，不能沿用「暂无环境 / 去下载中心」的文案。
  await envSearch.fill("does-not-exist");
  await page.getByText("没有符合筛选条件的环境").waitFor();
  assert.equal(
    await page.getByText("去下载中心安装一个吧").count(),
    0,
    "a filtered-empty list must not claim the environment is missing",
  );
  // 健康过滤：当前环境都可用，选「有问题」应得到空结果。
  await envSearch.fill("");
  await rowFor("jdk-owned").waitFor();
  const healthGroup = page.getByRole("group", { name: "按健康状态筛选" });
  await healthGroup.getByRole("button", { name: "有问题", exact: true }).click();
  await page.getByText("没有符合筛选条件的环境").waitFor();
  await healthGroup.getByRole("button", { name: "全部", exact: true }).click();
  await rowFor("jdk-owned").waitFor();
  // 排序默认按名称，且筛选与排序都不触发重新扫描。
  assert.equal(await page.getByLabel("环境排序方式").inputValue(), "name");
  assert.equal(
    await page.evaluate(() => window.calls.filter(c => c.command === "get_overview").length),
    overviewCallsBefore,
    "filtering and sorting must not trigger a rescan",
  );

  // 导入比对是只读操作：选文件后恰好比对一次，并如实呈现四类结论。
  const importButton = page.getByRole("button", { name: "导入比对", exact: true });
  await importButton.click();
  await page.waitForFunction(() =>
    window.calls.some(c => c.command === "import_environment_manifest"));
  assert.equal(
    await page.evaluate(() => window.lastImportPath),
    "D:/exports/recorded-manifest.json",
  );
  let compareDialog = page.getByRole("dialog");
  await compareDialog.getByText("环境清单比对", { exact: true }).waitFor();
  await compareDialog.getByText(/只读比对，未创建、还原或搬移任何环境/).waitFor();
  // 四类结论各自计数，不能互相吞掉。
  for (const [label, value] of [["未变", "1"], ["新增", "1"], ["缺失", "1"], ["已变更", "1"]]) {
    const cell = compareDialog.locator("div", { hasText: new RegExp(`^${label}$`) }).first();
    assert.equal(await cell.locator("xpath=following-sibling::div[1]").innerText(), value);
  }
  // 缺失：只在清单侧有值，本机侧不得被填成空版本误导读者。
  await compareDialog.getByText(/清单：available · 21\.0\.1 · D:\/managed\/envs\/jdks\/jdk-retired/).waitFor();
  const missingRow = compareDialog.locator("li", { hasText: "jdk-retired" });
  assert.equal(
    await missingRow.getByText(/本机：/).count(),
    0,
    "a missing entry must not render a current side",
  );
  // 已变更：列出具体差异字段，路径大小写/分隔符差异不算变更。
  await compareDialog.getByText(/已变更：版本、状态/).waitFor();
  // 新增：只在本文机有值。
  await compareDialog.getByText(/本机：available · 22\.0\.0 · D:\/managed\/envs\/nodes\/node-new/).waitFor();
  const addedRow = compareDialog.locator("li", { hasText: "node-new" });
  assert.equal(
    await addedRow.getByText(/清单：/).count(),
    0,
    "an added entry must not render a recorded side",
  );
  // 只读：不带任何确认按钮，关闭即退出。
  assert.equal(
    await compareDialog.getByRole("button", { name: /确认|恢复|导入环境/ }).count(),
    0,
  );
  await compareDialog.getByRole("button", { name: "关闭", exact: true }).click();
  assert.equal(await page.evaluate(() => window.calls.some(c => c.command === "apply_path_repair")), false);
  assert.equal(
    await page.evaluate(() =>
      window.calls.filter(c => c.command === "import_environment_manifest").length),
    1,
  );

  // 取消选择文件时不得发起比对。
  await page.evaluate(() => { window.cancelManifestOpen = true; });
  await importButton.click();
  await page.waitForFunction(() =>
    window.calls.filter(c => c.command === "plugin:dialog|open").length === 2);
  assert.equal(
    await page.evaluate(() =>
      window.calls.filter(c => c.command === "import_environment_manifest").length),
    1,
    "cancelling the file picker must not compare",
  );
  await page.evaluate(() => { window.cancelManifestOpen = false; });

  // 重新检测是只读的单环境读取：修复目录后不必整页刷新。
  await page.getByRole("button", { name: /^JDK/ }).click();
  const redetectButton = page.getByRole("button", { name: "重新检测 jdk-owned", exact: true });
  await redetectButton.click();
  await page.waitForFunction(() =>
    window.calls.some(c => c.command === "redetect_environment"));
  assert.deepEqual(
    await page.evaluate(() => window.lastRedetect),
    { envType: "jdk", name: "jdk-owned" },
    "re-detect must target exactly the clicked environment",
  );
  // 恢复成功时给出通过提示，并带上版本。
  await page.getByText("jdk-owned 重新检测通过", { exact: true }).waitFor();
  await page.getByText("版本 25.0.1", { exact: true }).first().waitFor();
  assert.equal(
    await page.evaluate(() =>
      window.calls.filter(c => c.command === "redetect_environment").length),
    1,
  );

  // 仍未恢复时必须如实说明原因，不能报成通过。这里换一个环境，
  // 避免上一条尚未消失的「通过」toast 干扰断言。
  await page.evaluate(() => { window.redetectStillBroken = true; });
  const brokenRedetect = page.getByRole("button", { name: "重新检测 jdk-linked", exact: true });
  await brokenRedetect.click();
  await page.waitForFunction(() =>
    window.calls.filter(c => c.command === "redetect_environment").length === 2);
  await page.getByText("jdk-linked 仍未恢复", { exact: true }).waitFor();
  await page.getByText(/版本探测失败：node --version 失败/).first().waitFor();
  assert.equal(
    await page.getByText("jdk-linked 重新检测通过", { exact: true }).count(),
    0,
    "a still-broken re-detect must not claim success",
  );
  assert.deepEqual(
    await page.evaluate(() => window.lastRedetect),
    { envType: "jdk", name: "jdk-linked" },
  );
  await page.evaluate(() => { window.redetectStillBroken = false; });

  // 搬入受管：只有外部链接条目才有这个入口；受管普通目录本来就在受管位置。
  assert.equal(await page.getByRole("button", { name: "搬入受管目录 jdk-owned", exact: true }).count(), 0);
  await page.getByRole("button", { name: "搬入受管目录 jdk-linked", exact: true }).click();
  const moveDialog = page.getByRole("dialog");
  await moveDialog.getByText("将把外部目录复制进受管目录，完成后受管条目是真实目录，不再依赖原位置。", { exact: true }).waitFor();
  // 预览必须如实列出源与范围，并明确说明原外部目录会保留。
  assert.equal(await moveDialog.getByText(/E:\/external\/jdk/, { exact: false }).count() > 0, true);
  assert.equal(await moveDialog.getByText(/42 个文件/, { exact: false }).count() > 0, true);
  assert.equal(await moveDialog.getByText(/原外部目录仍然保留/, { exact: false }).count() > 0, true);
  assert.deepEqual(await page.evaluate(() => window.calls.find(c => c.command === "preview_move_environment").args), {
    envType: "jdk", name: "jdk-linked",
  });
  await moveDialog.getByRole("button", { name: "确认搬入受管目录", exact: true }).click();
  // 搬移成功后自动进入「删除外部旧目录」确认：不可恢复，必须独立确认。
  const discardDialog = page.getByRole("dialog");
  await discardDialog.getByText("受管副本已核对完整覆盖该目录，删除后不可恢复。", { exact: true }).waitFor();
  assert.deepEqual(
    await page.evaluate(() => window.calls.find(c => c.command === "execute_move_environment").args.plan),
    await page.evaluate(() => window.lastMovePreview),
  );
  assert.deepEqual(await page.evaluate(() => window.calls.find(c => c.command === "preview_discard_moved_source").args), {
    envType: "jdk", name: "jdk-linked", source: "E:/external/jdk",
  });
  // 取消删除：不得删除任何东西。
  await discardDialog.getByRole("button", { name: "取消", exact: true }).click();
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "execute_discard_moved_source").length), 0);
  assert.equal(await page.getByRole("dialog").count(), 0);
  // 取消后该环境仍是外部链接，后续卸载断言依赖这一状态。
  assert.equal(await page.getByRole("button", { name: "移除管理 jdk-linked", exact: true }).count(), 1);

  // 搬入后取消删除的环境必须仍能重新打开删除入口：受管位置已是真实目录，
  // 不再有链接指向旧目录，入口只能来自搬移记录（`movedFrom`）。
  assert.equal(
    await page.getByRole("button", { name: "删除外部旧目录 jdk-owned", exact: true }).count(),
    0,
    "没有搬移记录的环境不能出现删除旧目录入口",
  );
  const discardAgain = page.getByRole("button", { name: "删除外部旧目录 jdk-moved", exact: true });
  assert.equal(await discardAgain.count(), 1);
  assert.equal(
    await discardAgain.getAttribute("title"),
    "删除搬移前的外部旧目录：E:/external/jdk-old",
  );
  await discardAgain.click();
  const againDialog = page.getByRole("dialog");
  await againDialog.getByText("受管副本已核对完整覆盖该目录，删除后不可恢复。", { exact: true }).waitFor();
  assert.deepEqual(
    await page.evaluate(() => window.calls.filter(c => c.command === "preview_discard_moved_source").at(-1).args),
    { envType: "jdk", name: "jdk-moved", source: "E:/external/jdk-old" },
  );
  await againDialog.getByRole("button", { name: "取消", exact: true }).click();
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "execute_discard_moved_source").length), 0);

  // Switching must report the post-switch command diagnosis, not just "done".
  // llvm-registered is a plain, non-current environment at this point.
  await page.getByRole("button", { name: /^LLVM/ }).click();
  // A shadowed command must degrade the switch result instead of reporting success.
  await page.evaluate(() => { window.switchShadowed = true; });
  await page.getByRole("button", { name: "设为当前", exact: true }).first().click();
  await page.getByText(/命令生效待确认/).first().waitFor();
  await page.getByText(/clang 被靠前安装覆盖/).first().waitFor();
  assert.equal(await page.evaluate(() => window.calls.some(c => c.command === "switch_env")), true);
  await page.evaluate(() => { window.switchShadowed = false; });
  // The repair entry must preview first and only write after explicit confirmation.
  await page.getByRole("button", { name: "预览 PATH 修复", exact: true }).click();
  let repairDialog = page.getByRole("dialog");
  await repairDialog.getByText("PATH 顺序修复预览", { exact: true }).waitFor();
  await repairDialog.getByText(/只读预览，尚未修改用户 PATH/).waitFor();
  await repairDialog.getByText("D:/managed/current/llvm/bin", { exact: true }).first().waitFor();
  await repairDialog.getByText("C:/Program Files/Other/bin", { exact: true }).first().waitFor();
  assert.equal(await page.evaluate(() => window.calls.some(c => c.command === "apply_path_repair")), false);
  await repairDialog.getByRole("button", { name: "调整顺序", exact: true }).click();
  await page.waitForFunction(() => window.calls.some(c => c.command === "apply_path_repair"));
  const repairCall = await page.evaluate(() => window.calls.find(c => c.command === "apply_path_repair"));
  assert.equal(repairCall.args.plan.entry, "D:/managed/current/llvm/bin");
  // The result must be the recomputed live state, not just the requested plan.
  await page.getByText("已调整 PATH 顺序并核验", { exact: true }).waitFor();
  await page.getByText(/受管入口已在用户 PATH 首位/).waitFor();
  // A write whose re-diagnosis still finds a shadow must say so, not claim success.
  await page.evaluate(() => { window.repairUnverified = true; });
  await page.getByRole("button", { name: "预览 PATH 修复", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "调整顺序", exact: true }).click();
  await page.getByText("PATH 顺序已写入，但核验未通过", { exact: true }).waitFor();
  await page.getByText(/仍被覆盖/).first().waitFor();
  await page.evaluate(() => { window.repairUnverified = false; });
  // A blocked preview must keep the write action disabled.
  await page.evaluate(() => { window.repairManual = true; });
  await page.getByRole("button", { name: "预览 PATH 修复", exact: true }).click();
  repairDialog = page.getByRole("dialog");
  await repairDialog.getByText(/无法只调整顺序/).waitFor();
  assert.equal(await repairDialog.getByRole("button", { name: "调整顺序", exact: true }).isDisabled(), true);
  await page.keyboard.press("Escape");
  await page.evaluate(() => { window.repairManual = false; });
  await page.getByRole("button", { name: /^JDK/ }).click();
  await page.getByRole("button", { name: "删除受管目录 jdk-owned", exact: true }).click();
  let dialog = page.getByRole("dialog");
  await dialog.getByText(/永久删除安装目录/).waitFor();
  await dialog.getByText(/不调用原厂卸载器/).first().waitFor();
  await dialog.getByText(/当前链接也会一并移除/).waitFor();
  await dialog.getByText("fixture: owned PATH entry, no shared dependency", { exact: true }).waitFor();

  // 目录占用与文件占用是两个不同的条目：处置方式不同（目录多是被设为工作目录的终端），
  // 因此必须各自呈现，不能被合并或被忽略。
  const unverifiedSection = dialog.locator("section", {
    has: page.getByRole("heading", { name: "未验证/不自动处理" }),
  });
  assert.equal(
    await unverifiedSection.getByText("目录占用", { exact: true }).count(),
    0,
    "a preview without an observed directory lock must not render one",
  );
  await page.keyboard.press("Escape");
  await page.evaluate(() => { window.previewDirLocked = true; });
  await page.getByRole("button", { name: "删除受管目录 jdk-owned", exact: true }).click();
  dialog = page.getByRole("dialog");
  await dialog.getByText("目录占用", { exact: true }).waitFor();
  // 具体目录与处置建议都要出现，否则用户不知道该关什么。
  await dialog.getByText(/删除会失败：cache/).first().waitFor();
  await dialog.getByText(/工作目录/).first().waitFor();
  // 计数按条目原样显示，不被吞掉。
  await dialog.getByText(/（1 项）/).first().waitFor();
  // 它必须落在「未验证/不自动处理」，而不是被误报成「将移除」。
  const unverifiedLocked = dialog.locator("section", {
    has: page.getByRole("heading", { name: "未验证/不自动处理" }),
  });
  assert.equal(
    await unverifiedLocked.getByText("目录占用", { exact: true }).count(),
    1,
    "the directory lock belongs to the unverified section",
  );
  const removableLocked = dialog.locator("section", {
    has: page.getByRole("heading", { name: "将移除" }),
  });
  assert.equal(
    await removableLocked.getByText("目录占用", { exact: true }).count(),
    0,
    "a directory lock must not be listed as something to remove",
  );
  await page.keyboard.press("Escape");
  await page.evaluate(() => { window.previewDirLocked = false; });
  await page.getByRole("button", { name: "删除受管目录 jdk-owned", exact: true }).click();
  dialog = page.getByRole("dialog");
  await dialog.getByText(/永久删除安装目录/).waitFor();

  // 停用模式：只解除管理、保留文件。切换必须重新预览，因为旧预览绑定的是
  // 另一种范围；否则执行端会以“预览已变化”拒绝。
  const previewsBefore = await page.evaluate(() =>
    window.calls.filter(c => c.command === "preview_uninstall_env").length);
  const detachTab = dialog.getByRole("button", { name: "停用（保留文件）", exact: true });
  await detachTab.click();
  await dialog.getByText(/停用后目录与文件全部原地保留/).waitFor();
  assert.equal(await detachTab.getAttribute("aria-pressed"), "true");
  const previewsAfter = await page.evaluate(() =>
    window.calls.filter(c => c.command === "preview_uninstall_env").length);
  assert.equal(previewsAfter, previewsBefore + 1, "切换模式必须重新预览");
  assert.equal(
    await page.evaluate(() => window.calls.filter(c => c.command === "preview_uninstall_env").at(-1).args.mode),
    "detach",
  );
  // 目录从“将移除”移到“明确保留”，并写明保留原因。
  const retainedSection = dialog.locator("section", {
    has: page.getByRole("heading", { name: "明确保留" }),
  });
  await retainedSection.getByText(/停用模式：保留磁盘内容/).waitFor();
  const removableSection = dialog.locator("section", {
    has: page.getByRole("heading", { name: "将移除" }),
  });
  assert.equal(await removableSection.getByText(/EnvCon 受管目录/).count(), 0);
  // 按钮文案随之改变，不再写着“删除”。
  assert.equal(await dialog.getByRole("button", { name: "确认停用", exact: true }).isVisible(), true);
  assert.equal(await dialog.getByRole("button", { name: "确认删除目录", exact: true }).count(), 0);
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "uninstall_env").length), 0);
  await page.evaluate(() => { window.previewSourceBlocked = true; });
  await page.getByRole("button", { name: "移除管理 jdk-linked", exact: true }).click();
  await dialog.getByText("fixture: link identity changed or source damaged; entry retained", { exact: true }).waitFor();
  assert.equal(await dialog.getByRole("button", { name: "移除管理", exact: true }).isDisabled(), true);
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "uninstall_env").length), 0);
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await page.evaluate(() => { window.previewSourceBlocked = false; window.previewLegacyLinks = true; });
  await page.getByRole("button", { name: "移除管理 jdk-linked", exact: true }).click();
  await dialog.getByText("来源未知的链接：需要逐项确认", { exact: true }).waitFor();
  const legacyCurrent = dialog.getByRole("checkbox", { name: /确认移除 来源未知的 current 链接/ });
  const legacyManagement = dialog.getByRole("checkbox", { name: /确认移除 来源未知的管理链接/ });
  assert.equal(await legacyCurrent.isChecked(), false);
  assert.equal(await legacyManagement.isChecked(), false);
  assert.equal(await dialog.getByRole("button", { name: "移除管理", exact: true }).isDisabled(), true);
  await legacyCurrent.check();
  assert.equal(await dialog.getByRole("button", { name: "移除管理", exact: true }).isDisabled(), true);
  await legacyManagement.check();
  assert.equal(await dialog.getByRole("button", { name: "移除管理", exact: true }).isEnabled(), true);
  await legacyCurrent.uncheck();
  assert.equal(await dialog.getByRole("button", { name: "移除管理", exact: true }).isDisabled(), true);
  await page.setViewportSize({ width: 390, height: 780 });
  await legacyManagement.scrollIntoViewIfNeeded();
  assert.equal(await dialog.evaluate(element => element.scrollWidth > element.clientWidth), false);
  const legacyBox = await dialog.boundingBox();
  assert.ok(legacyBox && legacyBox.x >= 0 && legacyBox.x + legacyBox.width <= 390);
  await dialog.screenshot({ path: "tests/legacy-link-confirmation-mobile.png" });
  await page.setViewportSize({ width: 1100, height: 850 });
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "uninstall_env").length), 0);
  await page.getByRole("button", { name: "移除管理 jdk-linked", exact: true }).click();
  await dialog.getByText(/原安装目录保留：E:\/external\/jdk/).waitFor();
  await dialog.getByText("fixture: shared PATH entry retained", { exact: true }).waitFor();
  assert.equal(await legacyCurrent.isChecked(), false);
  assert.equal(await legacyManagement.isChecked(), false);
  await legacyCurrent.check();
  await legacyManagement.check();
  await dialog.getByRole("button", { name: "移除管理", exact: true }).click();
  await page.waitForFunction(() => typeof window.finishUninstall === "function");
  assert.deepEqual(
    await page.evaluate(() => window.calls.find(c => c.command === "uninstall_env").args.expectedPreview),
    await page.evaluate(() => ({
      ...window.lastUninstallPreview,
      confirmedLegacyLinks: window.lastUninstallPreview.legacyLinks.map(item => item.path),
    })),
  );
  await page.keyboard.press("Escape");
  assert.equal(await dialog.count(), 1);
  await page.evaluate(() => window.finishUninstall());
  await page.getByRole("button", { name: "移除管理 jdk-linked", exact: true }).waitFor({ state: "detached" });
  await page.evaluate(() => { window.failUninstall = true; window.finishUninstall = null; window.previewLegacyLinks = false; });
  await page.evaluate(() => { window.previewBlocked = true; });
  await page.getByRole("button", { name: "删除受管目录 jdk-owned", exact: true }).click();
  await dialog.getByText("卸载范围不完整，已禁用执行。", { exact: true }).waitFor();
  assert.equal(await dialog.getByRole("button", { name: "确认删除目录", exact: true }).isDisabled(), true);
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "uninstall_env").length), 1);
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await page.evaluate(() => { window.previewBlocked = false; });
  await page.getByRole("button", { name: "删除受管目录 jdk-owned", exact: true }).click();
  await dialog.getByRole("button", { name: "确认删除目录", exact: true }).click();
  await page.waitForFunction(() => typeof window.finishUninstall === "function");
  assert.deepEqual(
    await page.evaluate(() => window.calls.findLast(c => c.command === "uninstall_env").args.expectedPreview),
    await page.evaluate(() => window.lastUninstallPreview),
  );
  await page.evaluate(() => window.finishUninstall());
  await page.getByText("移除部分失败", { exact: true }).waitFor();
  await page.getByText(/fixture: file in use/).waitFor();
  await page.getByText(/fixture: PATH recheck failed; may have executed/).waitFor();
  assert.equal(await dialog.count(), 0);
  await page.evaluate(() => { window.failUninstall = false; window.previewLegacyPath = true; });
  await page.getByRole("button", { name: "删除受管目录 jdk-owned", exact: true }).click();
  await dialog.getByText("来源未知的历史 PATH：默认保留", { exact: true }).waitFor();
  let legacyPath = dialog.getByRole("checkbox", { name: /确认移除此历史 PATH 条目/ });
  assert.equal(await legacyPath.isChecked(), false);
  assert.equal(await dialog.getByRole("button", { name: "确认删除目录", exact: true }).isEnabled(), true);
  await legacyPath.check();
  assert.equal(await dialog.getByRole("button", { name: "确认删除目录", exact: true }).isEnabled(), true);
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await page.getByRole("button", { name: "删除受管目录 jdk-owned", exact: true }).click();
  legacyPath = dialog.getByRole("checkbox", { name: /确认移除此历史 PATH 条目/ });
  assert.equal(await legacyPath.isChecked(), false);
  await legacyPath.check();
  await dialog.getByRole("button", { name: "确认删除目录", exact: true }).click();
  await page.waitForFunction(() => typeof window.finishUninstall === "function");
  assert.deepEqual(
    await page.evaluate(() => window.calls.findLast(c => c.command === "uninstall_env").args.expectedPreview),
    await page.evaluate(() => ({
      ...window.lastUninstallPreview,
      confirmedLegacyLinks: [],
      confirmedLegacyPathEntries: ["D:/managed/current/jdk/bin"],
    })),
  );
  await page.evaluate(() => window.finishUninstall());
  await page.getByRole("button", { name: "删除受管目录 jdk-owned", exact: true }).waitFor({ state: "detached" });
  await page.evaluate(() => { window.previewLegacyPath = false; });
  await page.getByRole("button", { name: "展开 Rust 扫描结果", exact: true }).click();
  await page.getByRole("button", { name: "纳入管理", exact: true }).click();
  assert.deepEqual(await page.evaluate(() => window.calls.find(c => c.command === "integrate_external_env").args), {
    envType: "rust", path: "E:/rust/toolchains/stable/bin/rustc.exe", version: "1.90.0",
  });
  await page.screenshot({ path: "tests/release-052-environments-desktop.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 780 });
  assert.equal(await page.evaluate(() => document.querySelector("main").scrollWidth > document.querySelector("main").clientWidth), false);
  await page.screenshot({ path: "tests/release-052-environments-mobile.png", fullPage: true });
  await page.getByRole("button", { name: /^LLVM/ }).click();
  const beforeInstaller = await page.evaluate(() => window.calls.filter(c => c.command === "uninstall_env").length);
  await page.evaluate(() => { window.previewInstallerBlocked = true; });
  await page.getByRole("button", { name: "删除受管目录 llvm-registered", exact: true }).click();
  await dialog.getByText("登记读取不完整、路径/命令无法核验或存在冲突；保留目录和卸载入口。", { exact: true }).waitFor();
  assert.equal(await dialog.getByRole("button", { name: "确认删除目录", exact: true }).isDisabled(), true);
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await page.evaluate(() => { window.previewInstallerBlocked = false; });
  await page.getByRole("button", { name: "删除受管目录 llvm-registered", exact: true }).click();
  await dialog.getByText("原厂卸载范围（执行后复查）", { exact: true }).waitFor();
  await dialog.getByText(/登记：HKCU\/64\/LLVM；固定参数：\/S。/).waitFor();
  await dialog.getByText(/将运行原厂卸载器并核验所选安装路径/).waitFor();
  assert.equal(await dialog.getByRole("button", { name: "执行原厂卸载", exact: true }).isEnabled(), true);
  assert.equal(await dialog.evaluate(element => element.scrollWidth > element.clientWidth), false);
  await dialog.getByRole("button", { name: "执行原厂卸载", exact: true }).click();
  await page.waitForFunction(() => typeof window.finishUninstall === "function");
  assert.deepEqual(
    await page.evaluate(() => window.calls.findLast(c => c.command === "uninstall_env").args.expectedPreview),
    await page.evaluate(() => window.lastUninstallPreview),
  );
  await page.evaluate(() => window.finishUninstall());
  await page.getByRole("button", { name: "删除受管目录 llvm-registered", exact: true }).waitFor({ state: "detached" });
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "uninstall_env").length), beforeInstaller + 1);
  assert.equal(await dialog.count(), 0);
  await page.getByRole("button", { name: "下载中心", exact: true }).click();
  await page.getByText("125 个版本", { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.calls.find(c => c.command === "list_versions").args.source), "official");
  assert.equal(await page.getByRole("button", { name: "下载", exact: true }).count(), 60);
  await page.getByRole("button", { name: /显示更多/ }).click();
  assert.equal(await page.getByRole("button", { name: "下载", exact: true }).count(), 120);
  await page.getByRole("button", { name: /显示更多/ }).click();
  assert.equal(await page.getByRole("button", { name: "下载", exact: true }).count(), 125);
  await page.getByPlaceholder("搜索 Node.js 版本…").fill("26.0.0");
  assert.equal(await page.getByRole("button", { name: "下载", exact: true }).count(), 1);
  await page.getByPlaceholder("搜索 Node.js 版本…").fill("");
  await page.evaluate(() => { window.offline = true; });
  await page.getByRole("button", { name: "刷新", exact: true }).last().click();
  await page.getByRole("alert").filter({ hasText: "版本可能过期" }).waitFor();
  await page.screenshot({ path: "tests/release-052-download-mobile.png", fullPage: true });
  assert.equal(await page.evaluate(() => document.querySelector("main").scrollWidth > document.querySelector("main").clientWidth), false);
  await page.setViewportSize({ width: 1100, height: 850 });
  await page.screenshot({ path: "tests/release-052-download-desktop.png", fullPage: true });
  assert.deepEqual(errors, []);
  console.log("PASS: environment removal preview/blocked-range/cancel/failure/busy, exact installer preview executes original-uninstall action and submits expectedPreview, changed link ownership blocks execution, per-link legacy confirmation/default retention/cancel reset, owned/shared PATH preview and partial recheck result, move external env into managed dir with preview/execute/discard-cancel, Rust integration, official default, pagination/history search, cache alert and responsive layouts");
} finally { await browser.close(); }
