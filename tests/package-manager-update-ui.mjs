import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { installScanMock } from "./scan-mock.mjs";

const require = createRequire(import.meta.url);
const { chromium } = require(process.argv[3] || "playwright");
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1100, height: 760 } });
const errors = [];
page.on("pageerror", e => errors.push(String(e)));
try {
  await installScanMock(page);
  await page.addInitScript(() => {
    window.managerVersion = "10.0.0";
    window.updateResult = "changed";
    window.updateCalls = [];
    window.previewCalls = [];
    window.updateRecord = null;
    window.batchPreviewCalls = 0;
    window.batchCalls = [];
    window.batchFailFirst = false;
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (command, args) => {
        switch (command) {
          case "get_overview": return { rootExists: true, root: "D:/managed", categories: [] };
          case "get_tool_configs": return [
            { tool: "npm", name: "npm", available: true, version: window.managerVersion, executablePath: "D:/node/npm.cmd", globalPath: null, cachePath: null, error: null, defaultGlobal: null, defaultCache: null },
            { tool: "pipx", name: "pipx", available: true, version: "1.7.1", executablePath: "D:/Python/Scripts/pipx.exe", globalPath: null, cachePath: null, error: null, defaultGlobal: null, defaultCache: null },
            // Installed through Corepack: the catalog says pnpm supports
            // self-update, but this installation must not use it.
            { tool: "pnpm", name: "pnpm", available: true, version: "9.1.0", executablePath: "D:/node/pnpm.cmd", globalPath: null, cachePath: null, error: null, defaultGlobal: null, defaultCache: null, selfUpdateBlocker: "pnpm 由 Corepack 管理（命令文件指向 Corepack），自更新会与 Corepack 的版本固定冲突；请在项目中使用 corepack prepare pnpm@latest --activate，或先关闭 Corepack 管理" },
          ];
          case "preview_package_manager_update": {
            // The preview must be read-only: record it separately from the
            // calls that actually run the update.
            window.previewCalls.push(args.tool);
            if (args.tool === "pipx") return {
              tool: "pipx", blocked: "pipx 没有通用自更新命令，请通过安装它的 Python 执行 python -m pip install --upgrade pipx",
              executablePath: "D:/Python/Scripts/pipx.exe", currentVersion: "1.7.1",
              updateProgram: null, updateArguments: [], runsSameFile: null,
              capabilities: { latestOnly: true, manualPinnedCommand: null },
              notes: ["没有生成更新命令，也没有执行任何写入"],
            };
            return {
              tool: args.tool, blocked: null, executablePath: "D:/node/npm.cmd",
              currentVersion: window.managerVersion, updateProgram: "D:/node/npm.cmd",
              updateArguments: ["install", "--global", "npm@latest", "--prefix", "D:/node"],
              runsSameFile: true,
              capabilities: { latestOnly: true, manualPinnedCommand: "npm install --global npm@<version>" },
              notes: ["命令执行成功不代表版本一定变化；执行后仍会复查版本"],
            };
          }
          case "update_package_manager": {
            window.updateCalls.push(args.tool);
            const beforeVersion = window.managerVersion;
            if (window.updateResult === "changed") window.managerVersion = "10.0.1";
            if (window.updateResult === "verification-failed") return {
              status: "verificationFailed", tool: args.tool, executablePath: "D:/node/npm.cmd",
              beforeVersion, afterVersion: null, versionChanged: null, output: "completed",
              error: "版本命令启动失败",
            };
            if (window.updateResult === "command-failed") return {
              status: "commandFailed", tool: args.tool, executablePath: "D:/node/npm.cmd",
              beforeVersion, afterVersion: null, versionChanged: null, output: "",
              error: "退出码 1",
            };
            // Mirror the backend: a successful attempt leaves a journal record.
            window.updateRecord = {
              status: window.updateResult === "changed" ? "completed" : "unverified",
              tool: args.tool, executablePath: "D:/node/npm.cmd",
              beforeVersion, afterVersion: window.managerVersion,
              program: "D:/node/npm.cmd",
              arguments: ["install", "--global", "npm@latest"],
              attempt: "fixture",
            };
            return {
              status: window.updateResult === "changed" ? "versionChanged" : "versionUnchanged",
              tool: args.tool, executablePath: "D:/node/npm.cmd", beforeVersion,
              afterVersion: window.managerVersion,
              versionChanged: window.updateResult === "changed", output: "completed", error: null,
            };
          }
          case "list_installed_packages": return { tool: args.tool, executable: "D:/node/npm.cmd", scope: "global", source: "D:/node", warnings: [], packages: [] };
          case "preview_package_manager_updates": {
            // A batch preview is read-only: count it, never run an update.
            window.batchPreviewCalls += 1;
            const items = [
              {
                tool: "npm", blocked: null, executablePath: "D:/node/npm.cmd",
                currentVersion: window.managerVersion, updateProgram: "D:/node/npm.cmd",
                updateArguments: ["install", "--global", "npm@latest"],
                runsSameFile: true,
                capabilities: { latestOnly: true, manualPinnedCommand: "npm install --global npm@<version>" },
                notes: ["命令执行成功不代表版本一定变化；执行后仍会复查版本"],
              },
              {
                tool: "pipx",
                blocked: "pipx 没有通用自更新命令，请通过安装它的 Python 执行 python -m pip install --upgrade pipx",
                executablePath: "D:/Python/Scripts/pipx.exe", currentVersion: "1.7.1",
                updateProgram: null, updateArguments: [], runsSameFile: null,
                capabilities: { latestOnly: true, manualPinnedCommand: null },
                notes: ["没有生成更新命令，也没有执行任何写入"],
              },
            ];
            return { items, ready: ["npm"], blocked: ["pipx"] };
          }
          case "update_package_managers": {
            // The batch must only run what the preview marked ready; a blocked
            // tool appearing here would be a real defect.
            window.batchCalls.push(args.tools);
            const failedNow = window.batchFailFirst;
            window.batchFailFirst = false;
            return {
              items: args.tools.map((tool) => ({
                tool,
                outcome: failedNow ? "failed" : "updated",
                result: {
                  status: failedNow ? "commandFailed" : "versionChanged",
                  tool, executablePath: "D:/node/npm.cmd",
                  beforeVersion: window.managerVersion,
                  afterVersion: failedNow ? null : "10.0.1",
                  versionChanged: failedNow ? null : true,
                  output: failedNow ? "" : "completed",
                  error: failedNow ? "退出码 1" : null,
                },
                skippedReason: null,
              })),
              retryable: failedNow ? args.tools.slice() : [],
            };
          }
          case "package_manager_update_record": return window.updateRecord ? { ...window.updateRecord, tool: args.tool } : null;
          case "get_path_state": return { user: [], system: [] };
          case "get_caches": case "get_env_vars": case "install_tasks": return [];
          default: return null;
        }
      },
    };
  });

  await page.goto(process.argv[2]);
  await page.getByRole("button", { name: "包管理器", exact: true }).click();
  const version = page.getByRole("region", { name: "当前包管理器版本" });
  await version.getByText("10.0.0", { exact: true }).waitFor();
  await version.getByText(/有条件支持/).waitFor();
  // The limitations of a self-update must be stated before the user acts,
  // not discovered afterwards.
  await version.getByText(/不支持指定版本降级/).waitFor();
  await version.getByText(/旧版本可能已从来源下架/).waitFor();
  // Nothing has been recorded yet, so no record line is claimed.
  assert.equal(await version.getByText(/最近一次更新记录/).count(), 0);
  const update = page.getByRole("button", { name: "更新当前工具", exact: true });
  const dialog = page.getByRole("dialog");

  // Opening the preview must not run the update: the dialog shows the exact
  // command that would run, and only confirming executes it.
  await update.click();
  await dialog.getByText("npm 更新预览").waitFor();
  assert.deepEqual(await page.evaluate(() => window.updateCalls), []);
  await dialog.getByText(/install --global npm@latest/).waitFor();
  await dialog.getByText("D:/node/npm.cmd").first().waitFor();
  await dialog.getByText("10.0.0").waitFor();
  // The version-range limit is a structured capability, not prose: the dialog
  // must say the entry point is latest-only and hand over the manager's own
  // pinned-version command instead of a blanket "downgrade unsupported".
  await dialog.getByText(/此入口只安装 latest/).waitFor();
  await dialog.getByText(/指定版本需手动执行/).waitFor();
  await dialog.getByText(/npm install --global npm@<version>/).waitFor();
  // Cancelling leaves the manager untouched.
  await dialog.getByRole("button", { name: "取消" }).click();
  await dialog.waitFor({ state: "hidden" });
  assert.deepEqual(await page.evaluate(() => window.updateCalls), []);

  const runUpdate = async () => {
    await update.click();
    await dialog.getByText("npm 更新预览").waitFor();
    await dialog.getByRole("button", { name: "执行更新" }).click();
  };

  await runUpdate();
  await page.getByText("npm 版本已变化", { exact: true }).waitFor();
  await version.getByText("10.0.1", { exact: true }).waitFor();
  await dialog.waitFor({ state: "hidden" });
  // The journal record becomes visible after the update, and it says plainly
  // that it is audit-only rather than a rollback.
  await version.getByText(/最近一次更新记录/).waitFor();
  await version.getByText(/应用不会自动恢复到此版本/).waitFor();
  await version.getByText("命令完成").waitFor();

  await page.evaluate(() => { window.updateResult = "unchanged"; });
  await runUpdate();
  await page.getByText("npm 更新结果待确认", { exact: true }).waitFor();
  await page.evaluate(() => { window.updateResult = "verification-failed"; });
  await runUpdate();
  await page.getByText(/二次版本复查失败；更新可能已执行/).waitFor();
  await page.getByText("版本命令启动失败").waitFor();
  await page.evaluate(() => { window.updateResult = "command-failed"; });
  await runUpdate();
  await page.getByText("npm 更新命令失败", { exact: true }).waitFor();
  await page.getByText("退出码 1").waitFor();
  assert.deepEqual(await page.evaluate(() => window.updateCalls), ["npm", "npm", "npm", "npm"]);
  // Four executions plus the cancelled preview above: opening a preview never
  // executes, so the call counts must differ by exactly the cancelled one.
  assert.deepEqual(await page.evaluate(() => window.previewCalls), ["npm", "npm", "npm", "npm", "npm"]);

  await page.getByRole("button", { name: "选择 pipx", exact: true }).click();
  await version.getByText("1.7.1", { exact: true }).waitFor();
  await version.getByText(/不支持通用命令/).waitFor();
  assert.equal(await update.isDisabled(), true);
  // A manager without a general self-update never even asks for a preview.
  assert.deepEqual(await page.evaluate(() => window.previewCalls), ["npm", "npm", "npm", "npm", "npm"]);

  // pnpm *does* advertise a self-update in the catalog, but this installation
  // is managed by Corepack. The list-level verdict must disable the button and
  // state the reason, rather than looking updatable until the preview refuses.
  await page.getByRole("button", { name: "选择 pnpm", exact: true }).click();
  await version.getByText("9.1.0", { exact: true }).waitFor();
  await version.getByText(/此安装的自更新不可用/).waitFor();
  await version.getByText(/由 Corepack 管理/).waitFor();
  assert.equal(await update.isDisabled(), true);
  // The reason must also be the button's own explanation, not a generic label.
  assert.match(await update.getAttribute("title") ?? "", /Corepack/);
  // A disabled button must not open a preview that would only fail.
  assert.deepEqual(await page.evaluate(() => window.previewCalls), ["npm", "npm", "npm", "npm", "npm"]);

  // ---- Batch update: preview first, then one confirmation ----
  const batch = page.getByRole("button", { name: "批量更新", exact: true });
  await batch.click();
  await dialog.getByText("批量更新预览").waitFor();
  // Opening the batch preview runs nothing and never executes a per-tool update.
  assert.deepEqual(await page.evaluate(() => window.batchCalls), []);
  assert.deepEqual(await page.evaluate(() => window.updateCalls), ["npm", "npm", "npm", "npm"]);
  // Every manager is listed with its own state: the updatable one shows the
  // exact command, the unsupported one shows why it will be skipped.
  await dialog.getByText(/install --global npm@latest/).waitFor();
  await dialog.getByText("将更新", { exact: true }).waitFor();
  await dialog.getByText("跳过", { exact: true }).waitFor();
  await dialog.getByText(/pipx 没有通用自更新命令/).waitFor();
  await dialog.getByText(/尚未执行任何更新命令/).waitFor();
  // Cancelling the batch leaves every manager untouched.
  await dialog.getByRole("button", { name: "取消" }).click();
  await dialog.waitFor({ state: "hidden" });
  assert.deepEqual(await page.evaluate(() => window.batchCalls), []);

  // Confirming runs only the managers the preview marked ready: pipx is a
  // skip, so it must never reach the executor.
  await batch.click();
  await dialog.getByText("批量更新预览").waitFor();
  await dialog.getByRole("button", { name: "执行可用项" }).click();
  await dialog.getByText("批量更新结果").waitFor();
  assert.deepEqual(await page.evaluate(() => window.batchCalls), [["npm"]]);
  assert.equal(await dialog.getByText("版本已变化", { exact: true }).count(), 1);
  await dialog.getByText(/共 1 项/).waitFor();
  await dialog.getByText(/“跳过”是执行前的决定，不计为失败/).waitFor();

  // A failed batch offers a retry, and the retry resends only that tool.
  await dialog.getByRole("button", { name: "取消" }).click();
  await dialog.waitFor({ state: "hidden" });
  await page.evaluate(() => { window.batchFailFirst = true; });
  await batch.click();
  await dialog.getByText("批量更新预览").waitFor();
  await dialog.getByRole("button", { name: "执行可用项" }).click();
  await dialog.getByText("批量更新结果").waitFor();
  await page.getByText(/1 个包管理器更新失败/).waitFor();
  await dialog.getByText("失败", { exact: true }).waitFor();
  const retry = dialog.getByRole("button", { name: "重试失败项" });
  await retry.click();
  // Exactly one extra call, and it targets the same failed tool.
  assert.deepEqual(await page.evaluate(() => window.batchCalls), [["npm"], ["npm"], ["npm"]]);
  await page.getByText("失败项已重试", { exact: true }).waitFor();
  assert.equal(await retry.isDisabled(), true);
  await dialog.getByRole("button", { name: "取消" }).click();
  await dialog.waitFor({ state: "hidden" });

  assert.deepEqual(errors, []);
  console.log("PASS: manager versions, read-only update preview with cancel, changed/unchanged update results, unsupported self-update, batch preview/skip/retry");
} finally {
  await browser.close();
}
