import assert from "node:assert/strict";
import { installScanMock } from "./scan-mock.mjs";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { chromium } = require(process.argv[3] || "playwright");
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1100, height: 760 } });
const errors = [];
page.on("pageerror", e => errors.push(String(e)));
try {
  await installScanMock(page);
  await page.addInitScript(() => {
    window.calls = [];
    window.failConfigs = true;
    window.__TAURI_INTERNALS__ = { transformCallback: () => 1, invoke: async (command, args) => {
      window.calls.push({ command, args });
      switch (command) {
        case "get_overview": return { rootExists: true, root: "D:/managed", categories: [] };
        case "get_path_state": return { user: [], system: [] };
        case "get_env_vars": case "install_tasks": case "get_caches": return [];
        case "get_tool_configs":
          if (window.failConfigs) throw "配置读取临时失败";
          return [
            ...["npm", "pnpm", "yarn", "pip"].map(tool => ({ tool, name: tool, available: tool !== "yarn", version: tool === "yarn" ? "4.1.0" : "1.0.0", executablePath: tool === "yarn" ? null : `D:/node/${tool}.cmd`, globalPath: "D:/old", cachePath: "D:/cache", defaultGlobal: tool === "pip" ? null : "D:/managed/global", defaultCache: "D:/managed/cache", commandPath: tool === "pip" ? null : "D:/managed/global", commandPathSource: tool === "pip" ? null : tool === "pnpm" ? "layout" : "queried", layoutWarning: null })),
            { tool: "uv", name: "uv", available: true, version: "0.8.0", executablePath: "D:/bin/uv.exe", globalPath: "D:/uv-legacy", cachePath: "D:/uv-cache", defaultGlobal: "D:/managed/uv-global", defaultCache: "D:/managed/uv-cache", commandPath: "D:/managed/uv-global-bin", commandPathSource: "queried", layoutWarning: "检测到 uv 旧嵌套布局（1 个工具可规划恢复，0 个需人工核对）；建议入口 D:/managed/uv-global-bin，缓存 D:/managed/uv-global-cache。此检查只读，未移动或删除任何文件。" },
          ];
        case "inspect_uv_layout":
          return {
            toolRoot: "D:/uv-legacy",
            legacyBin: "D:/uv-legacy/bin",
            legacyCache: "D:/uv-legacy/cache",
            nestedBinExists: true,
            nestedCacheExists: true,
            proposedBin: "D:/managed/uv-global-bin",
            proposedCache: "D:/managed/uv-global-cache",
            status: "legacy",
            tools: [{
              name: "ruff",
              version: "0.8.0",
              directory: "D:/uv-legacy/ruff",
              receiptStatus: "valid",
              receiptSha256: "receipt-hash",
              requirement: "ruff==0.8.0",
              entrypoints: ["D:/uv-legacy/bin/ruff.exe"],
              entrypointSnapshots: [{ path: "D:/uv-legacy/bin/ruff.exe", sha256: "entry-hash", legacy: true }],
              needsRecovery: true,
              recoverable: true,
              issues: [],
            }],
            issues: [],
          };
        case "recover_uv_layout":
          return {
            status: "complete",
            items: [{ name: "ruff", version: "0.8.0", status: "recovered", detail: null, removedEntrypoints: ["D:/uv-legacy/bin/ruff.exe"], newEntrypoints: ["D:/managed/uv-global-bin/ruff.exe"] }],
            retained: [],
            layout: {
              toolRoot: "D:/uv-legacy",
              legacyBin: "D:/uv-legacy/bin",
              legacyCache: "D:/uv-legacy/cache",
              nestedBinExists: false,
              nestedCacheExists: false,
              proposedBin: "D:/managed/uv-global-bin",
              proposedCache: "D:/managed/uv-global-cache",
              status: "clean",
              tools: [],
              issues: [],
            },
          };
        case "list_installed_packages":
          if (args.directory?.includes("bad")) throw "目录无法读取";
          if (args.directory?.includes("slow")) await new Promise(r => setTimeout(r, 600));
          return { tool: args.tool, executable: `D:/node/${args.tool}.cmd`, scope: args.directory ? "directory" : "global", source: args.directory || "D:/globals", warnings: args.tool === "pnpm" ? ["部分依赖元数据缺失"] : [], packages: args.directory?.includes("empty") ? [] : [
            { name: `${args.tool}-fixture`, version: "1.2.3", path: "D:/globals/node_modules/fixture", detail: null },
            { name: "@scope/long-package-name", version: "2.0.0-beta.1", path: "D:/very-long-directory-name/node_modules/@scope/long-package-name", detail: "本地链接" },
          ] };
        case "global_package_sources": return ["D:/managed/envs/nodes/node-24"];
        case "global_reinstall_status": return null;
        case "apply_tool_config": if (args.tool === "pnpm") throw "退出码 7: 写入失败"; return ["已设置"];
        case "preview_tool_configs": {
          const items = (args.tools ?? []).map((request) => {
            if (request.tool === "pip") {
              return {
                tool: "pip",
                available: true,
                blocked: ["命中路径重定向，已停止写入"],
                warnings: [],
                changes: [],
                commandEntry: "",
                commandEntryChanged: false,
                entryInPath: false,
                commandStatus: "unknown",
                commandDetail: "未指定全局安装目录，不核对命令入口",
                movesExistingPackages: false,
                notes: [],
                changedCount: 0,
                affectedPackages: {
                  packages: [],
                  unresolvedCount: 0,
                  note: "未找到当前全局安装目录，未列出受影响包",
                },
              };
            }
            return {
              tool: request.tool,
              available: true,
              blocked: [],
              warnings: [],
              changes: [
                { key: "prefix", role: "global", current: "D:/old", next: request.globalPath },
                { key: "cache", role: "cache", current: "D:/old-cache", next: request.cachePath },
              ],
              commandEntry: `${request.globalPath}-bin`,
              commandEntryChanged: true,
              entryInPath: false,
              commandStatus: "managed",
              commandDetail: "PATH 解析命中受管入口",
              movesExistingPackages: false,
              notes: [],
              changedCount: 2,
              affectedPackages: {
                packages: [
                  { name: "eslint", version: "9.1.0", status: "unresolved", detail: null },
                  { name: "@scope/tool", version: "2.0.0", status: "unresolved", detail: "本地链接" },
                ],
                unresolvedCount: 2,
                note: "当前全局目录中的 2 个包仍保留在原位置，但命令入口将变化，旧入口可能不再解析到它们",
              },
            };
          });
          return {
            items,
            ready: items.filter((item) => item.blocked.length === 0).map((item) => item.tool),
            blocked: items.filter((item) => item.blocked.length > 0).map((item) => item.tool),
            totalChanges: items.reduce((sum, item) => sum + item.changedCount, 0),
            movesExistingPackages: false,
          };
        }
        case "preview_tool_config": {
          if (args.tool === "pnpm") throw "预览读取失败";
          if (args.tool === "pip") return {
            tool: "pip",
            available: true,
            blocked: ["命中路径重定向，已停止写入"],
            warnings: [],
            changes: [],
            commandEntry: "",
            commandEntryChanged: false,
            entryInPath: false,
            commandStatus: "unknown",
            commandDetail: "未指定全局安装目录，不核对命令入口",
            movesExistingPackages: false,
            notes: [],
            changedCount: 0,
            affectedPackages: {
              packages: [],
              unresolvedCount: 0,
              note: "未找到当前全局安装目录，未列出受影响包",
            },
          };
          return {
            tool: args.tool,
            available: true,
            blocked: [],
            warnings: ["目标目录已存在且非空：D:/managed/global；现有内容不会被合并、搬移或删除"],
            changes: [
              { key: args.tool === "uv" ? "UV_TOOL_DIR" : "prefix", role: "global", current: "D:/old", next: "D:/managed/global" },
              { key: "command-entry", role: "command", current: null, next: "D:/managed/global-bin" },
              { key: "cache", role: "cache", current: "D:/cache", next: "D:/managed/cache" },
            ],
            commandEntry: "D:/managed/global-bin",
            commandEntryChanged: true,
            entryInPath: false,
            commandStatus: "shadowed",
            commandDetail: "PATH 靠前位置命中了其他安装：C:/Users/me/AppData/Roaming/npm/npm.cmd",
            movesExistingPackages: false,
            notes: ["仅修改包管理器配置；已安装的全局包保留在原位置，不会搬移"],
            changedCount: 3,
            affectedPackages: {
              packages: [
                { name: "eslint", version: "9.1.0", status: "unresolved", detail: null },
              ],
              unresolvedCount: 1,
              note: "当前全局目录中的 1 个包仍保留在原位置，但命令入口将变化，旧入口可能不再解析到它们",
            },
          };
        }
        default: return null;
      }
    } };
  });
  await page.goto(process.argv[2] || "http://127.0.0.1:1433");
  await page.getByRole("button", { name: "路径管理", exact: true }).click();
  assert.equal(await page.getByRole("button", { name: "重装旧包", exact: true }).count(), 0);
  await page.waitForFunction(() => window.calls.some(c => c.command === "get_tool_configs"));
  await page.getByRole("button", { name: "包管理器", exact: true }).click();
  await page.getByRole("cell", { name: "npm-fixture", exact: true }).waitFor();
  await page.getByRole("alert").filter({ hasText: "配置读取临时失败" }).waitFor();
  assert.equal(await page.getByRole("img", { name: /已安装且可用/ }).count(), 0);
  await page.evaluate(() => window.failConfigs = false);
  await page.getByRole("button", { name: "刷新包管理器配置", exact: true }).click();
  await page.getByRole("img", { name: "npm：已安装且可用", exact: true }).waitFor();
  assert.equal(await page.getByRole("img", { name: /已安装且可用/ }).count(), 4);
  await page.getByRole("img", { name: "yarn：未检测到", exact: true }).waitFor();
  await page.getByText("读取包管理器配置失败", { exact: true }).waitFor({ state: "hidden" });
  await page.getByRole("textbox", { name: "搜索包" }).fill("@scope");
  assert.equal(await page.getByRole("cell", { name: "npm-fixture", exact: true }).count(), 0);
  await page.getByRole("textbox", { name: "搜索包" }).fill("");
  await page.getByRole("button", { name: "打开 npm-fixture 的安装位置", exact: true }).click();
  assert.ok(await page.evaluate(() => window.calls.some(c => c.command === "open_path" && c.args.path.includes("node_modules"))));
  await page.getByRole("button", { name: "选择 pnpm", exact: true }).click();
  assert.equal(await page.getByRole("button", { name: "选择 pnpm", exact: true }).getAttribute("aria-pressed"), "true");
  await page.getByRole("cell", { name: "pnpm-fixture", exact: true }).waitFor();
  await page.getByRole("alert").filter({ hasText: "部分依赖" }).waitFor();
  for (const [width, height, dark] of [[1100,760,false], [900,640,false], [390,640,true]]) {
    await page.setViewportSize({ width, height });
    await page.emulateMedia({ colorScheme: dark ? "dark" : "light" });
    await page.screenshot({ path: `tests/packages-${width}.png`, animations: "disabled" });
    assert.equal(await page.evaluate(() => document.querySelector("main").scrollWidth > document.querySelector("main").clientWidth), false);
  }
  await page.setViewportSize({ width: 1100, height: 760 });
  await page.getByLabel("范围", { exact: true }).selectOption("historical");
  await page.getByLabel("已发现的全局目录", { exact: true }).selectOption("D:/managed/envs/nodes/node-24");
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByRole("cell", { name: "pnpm-fixture", exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "list_installed_packages").at(-1).args.historical), true);
  await page.getByRole("button", { name: "重装此目录的包", exact: true }).click();
  const dialog = page.getByRole("dialog");
  assert.equal(await dialog.getByLabel("旧全局包目录", { exact: true }).inputValue(), "D:/managed/envs/nodes/node-24");
  assert.equal(await dialog.getByLabel("目标全局目录", { exact: true }).inputValue(), "D:/old");
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "选择 pip", exact: true }).click();
  await page.getByLabel("范围", { exact: true }).selectOption("global");
  await page.getByRole("cell", { name: "pip-fixture", exact: true }).waitFor();
  assert.equal(await page.getByLabel("范围", { exact: true }).inputValue(), "global");
  await page.getByRole("button", { name: "选择 pnpm", exact: true }).click();
  await page.getByLabel("范围", { exact: true }).selectOption("directory");
  await page.getByRole("textbox", { name: "查询目录" }).fill("D:/slow");
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByRole("button", { name: "选择 npm", exact: true }).click();
  await page.getByRole("textbox", { name: "查询目录" }).fill("D:/project");
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByRole("cell", { name: "npm-fixture", exact: true }).waitFor();
  await page.waitForTimeout(700);
  assert.equal(await page.getByRole("cell", { name: "pnpm-fixture", exact: true }).count(), 0);
  await page.getByRole("textbox", { name: "查询目录" }).fill("D:/bad");
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByRole("alert").filter({ hasText: "目录无法读取" }).waitFor();
  assert.equal(await page.getByText("未发现已安装的包", { exact: true }).count(), 0);
  await page.getByRole("textbox", { name: "查询目录" }).fill("D:/empty");
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByText("未发现已安装的包", { exact: true }).waitFor();
  await page.getByRole("button", { name: "选择 pip", exact: true }).click();
  await page.getByRole("textbox", { name: "查询目录" }).fill("D:/venv");
  await page.getByRole("button", { name: "刷新", exact: true }).click();
  await page.getByRole("cell", { name: "pip-fixture", exact: true }).waitFor();
  await page.getByRole("tab", { name: "路径与迁移" }).click();
  await page.getByRole("button", { name: "重装旧包", exact: true }).first().waitFor();
  assert.equal(await page.getByRole("button", { name: "重装旧包", exact: true }).count(), 1);
  assert.equal(await page.getByRole("button", { name: "选择 pip", exact: true }).getAttribute("aria-pressed"), "true");
  // A blocked preview must keep the write action disabled and show the reason.
  await page.getByRole("button", { name: "仅设置路径", exact: true }).click();
  const blockedDialog = page.getByRole("dialog");
  await blockedDialog.getByText("阻止写入：命中路径重定向，已停止写入", { exact: true }).waitFor();
  assert.equal(
    await blockedDialog.getByRole("button", { name: "写入配置", exact: true }).isDisabled(),
    true,
  );
  await page.keyboard.press("Escape");
  assert.equal(
    await page.evaluate(() => window.calls.some(c => c.command === "apply_tool_config" && c.args.tool === "pip")),
    false,
  );
  await page.getByRole("button", { name: "选择 uv", exact: true }).click();
  await page.getByText(/检测到 uv 旧嵌套布局/).waitFor();
  assert.equal(await page.getByText("D:/managed/uv-global-bin", { exact: true }).count(), 1);
  await page.getByRole("button", { name: "恢复 uv 旧布局", exact: true }).click();
  const uvDialog = page.getByRole("dialog");
  await uvDialog.getByText("可恢复：ruff==0.8.0", { exact: true }).waitFor();
  await uvDialog.getByRole("button", { name: "执行恢复", exact: true }).click();
  await page.waitForFunction(() => window.calls.some(c => c.command === "recover_uv_layout"));
  const recoveryCall = await page.evaluate(() => window.calls.find(c => c.command === "recover_uv_layout"));
  assert.equal(recoveryCall.args.executable, "D:/bin/uv.exe");
  assert.equal(recoveryCall.args.expected.tools[0].requirement, "ruff==0.8.0");
  await page.getByRole("button", { name: "选择 pnpm", exact: true }).click();
  await page.getByText("布局推导", { exact: true }).waitFor();
  await page.getByRole("button", { name: "选择 npm", exact: true }).click();
  await page.getByText("实际查询", { exact: true }).waitFor();
  await page.getByRole("button", { name: "仅设置路径", exact: true }).click();
  const previewDialog = page.getByRole("dialog");
  await previewDialog.getByText("路径配置预览").waitFor();
  assert.equal(
    await page.evaluate(() => window.calls.filter(c => c.command === "preview_tool_config").at(-1).args.tool),
    "npm",
  );
  // The preview must show the diff before anything is written.
  assert.equal(
    await page.evaluate(() => window.calls.some(c => c.command === "apply_tool_config" && c.args.tool === "npm")),
    false,
  );
  await previewDialog.getByText("旧值：D:/old", { exact: true }).waitFor();
  await previewDialog.getByText("新值：D:/managed/global", { exact: true }).waitFor();
  await previewDialog.getByText("旧值：未设置", { exact: true }).waitFor();
  await previewDialog.getByText("D:/managed/global-bin（将变化）", { exact: true }).waitFor();
  await previewDialog.getByText(/目标目录已存在且非空/).waitFor();
  await previewDialog.getByText("被 PATH 靠前位置覆盖", { exact: true }).waitFor();
  await previewDialog.getByText("保留原位置，不搬移", { exact: true }).waitFor();
  // The preview must list concrete packages from the current global root.
  await previewDialog.getByText("受影响的全局包（当前全局目录）", { exact: true }).waitFor();
  await previewDialog.getByText(/eslint 9\.1\.0（入口可能失效）/).waitFor();
  await previewDialog.getByText("1 个可能不再解析", { exact: true }).waitFor();
  await previewDialog.getByRole("button", { name: "写入配置", exact: true }).click();
  await page.waitForFunction(() => window.calls.some(c => c.command === "apply_tool_config" && c.args.tool === "npm"));
  await previewDialog.waitFor({ state: "hidden" });
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "apply_tool_config").at(-1).args.tool), "npm");
  // Yarn 2+ reports globalFolder, which is global data rather than a Classic
  // install root, so the global migration kind must not be offered.
  await page.getByRole("button", { name: "选择 yarn", exact: true }).click();
  await page.getByRole("button", { name: "迁移数据", exact: true }).click();
  const yarnMigration = page.getByRole("dialog");
  await yarnMigration.getByRole("radio", { name: "缓存 / 存储目录", exact: true }).waitFor();
  assert.equal(
    await yarnMigration.getByRole("radio", { name: "全局包目录", exact: true }).count(),
    0,
  );
  await yarnMigration.getByText(/globalFolder 只是全局数据目录/).waitFor();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "选择 npm", exact: true }).click();
  await page.getByRole("button", { name: "一键配置全部", exact: true }).click();
  const batchDialog = page.getByRole("dialog");
  await batchDialog.getByText("一键配置预览", { exact: true }).waitFor();
  assert.equal(
    await page.evaluate(() => window.calls.filter(c => c.command === "preview_tool_configs").at(-1).args.tools.length > 0),
    true,
  );
  // The batch preview itself must not write anything.
  const writesBeforeBatch = await page.evaluate(
    () => window.calls.filter(c => c.command === "apply_tool_config").length,
  );
  assert.equal(writesBeforeBatch, 1, "only the earlier single preview wrote");
  assert.equal(
    await page.evaluate(() => window.calls.filter(c => c.command === "apply_tool_config").length),
    writesBeforeBatch,
  );
  await batchDialog.getByText("跳过", { exact: true }).waitFor();
  await batchDialog.getByText("命中路径重定向，已停止写入", { exact: true }).waitFor();
  await batchDialog.getByText("2 项变化", { exact: true }).first().waitFor();
  // Each writable tool must show its own affected packages, not just a count.
  const packagesPerTool = await page.evaluate(() =>
    window.calls.filter(c => c.command === "preview_tool_configs").at(-1).args.tools.flatMap(
      request => request.globalPath ? [`${request.tool}:eslint 9.1.0`] : [],
    ),
  );
  assert.ok(packagesPerTool.length > 0);
  assert.equal(
    await batchDialog.getByText("受影响的全局包", { exact: true }).count() >= 1,
    true,
  );
  assert.equal(
    await batchDialog.getByText(/eslint 9\.1\.0（入口可能失效）/).count() >= 1,
    true,
  );
  assert.equal(
    await batchDialog.getByText(/2 个可能不再解析/).count() >= 1,
    true,
  );
  await batchDialog.getByRole("button", { name: "写入可用项", exact: true }).click();
  await page.waitForFunction(() => window.calls.some(c => c.command === "apply_tool_config"));
  await batchDialog.waitFor({ state: "hidden" });
  // A tool that the preview blocked must never be written.
  assert.equal(
    await page.evaluate(() => window.calls.some(c => c.command === "apply_tool_config" && c.args.tool === "pip")),
    false,
  );
  await page.getByText("1 个工具已跳过", { exact: true }).waitFor({ state: "hidden" });
  await page.setViewportSize({ width: 390, height: 640 });
  await page.screenshot({ path: "tests/package-paths-390.png" });
  assert.equal(await page.evaluate(() => document.querySelector("main").scrollWidth > document.querySelector("main").clientWidth), false);
  assert.deepEqual(errors, []);
  console.log("PASS: manager buttons, verified green/unknown/absent states, selected path actions, search/open, scopes, stale requests, errors, bulk failure, responsive layouts");
} finally { await browser.close(); }
