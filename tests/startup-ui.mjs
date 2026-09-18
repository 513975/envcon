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
    localStorage.setItem("envcon.scan.projectDirs", JSON.stringify(["D:/saved-project"]));
    window.calls = [];
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        switch (command) {
          case "get_overview": return { rootExists: true, root: "D:/managed", categories: [] };
          case "scan_system":
            await new Promise(resolve => { window.finishScan = resolve; });
            return { tools: [], warnings: [] };
          case "get_path_state": return { user: [], system: [] };
          case "get_tool_configs": case "get_caches": case "get_env_vars": case "install_tasks": return [];
          default: return null;
        }
      },
    };
  });
  await page.goto(process.argv[2]);
  await page.waitForFunction(() => typeof window.finishScan === "function");
  await page.getByRole("button", { name: "环境管理", exact: true }).click();
  await page.getByRole("button", { name: "路径管理", exact: true }).click();
  await page.getByRole("button", { name: "环境管理", exact: true }).click();
  assert.deepEqual(await page.evaluate(() => window.calls.filter(c => c.command === "scan_system")), [
    { command: "scan_system", args: { projectDirs: ["D:/saved-project"] } },
  ]);
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "get_overview").length), 1);
  await page.evaluate(() => window.finishScan());
  await page.getByRole("button", { name: "重新扫描", exact: true }).waitFor();
  await page.getByRole("button", { name: "重新扫描", exact: true }).click();
  await page.waitForFunction(() => window.calls.filter(c => c.command === "scan_system").length === 2);
  await page.evaluate(() => window.finishScan());
  await page.waitForFunction(() => window.calls.some(c => c.command === "get_caches"));
  assert.deepEqual(errors, []);
  console.log("PASS: navigation during pending scan, saved project prewarm, shared in-flight overview/system reads, forced rescan, delayed cache prewarm");
} finally {
  await browser.close();
}
