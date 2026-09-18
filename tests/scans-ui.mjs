import assert from "node:assert/strict";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { chromium } = require(process.argv[3] || "playwright");
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1100, height: 760 } });
const errors = [];
page.on("pageerror", error => errors.push(String(error)));
try {
  await page.addInitScript(() => {
    window.scans = [];
    window.waiting = {};
    const overview = name => ({ root: "D:/fixture", rootExists: true, dataDir: "D:/fixture/data", categories: [{ envType: "node", current: null, envs: [{ name, envType: "node", path: "D:/fixture/node", version: "24.0.0", sizeBytes: null, sizeComplete: false, status: "available", isCurrent: false, isExternalLink: false }] }] });
    const result = value => ({ value, updatedAt: Date.now() - 40_000, durationMs: 2, warning: null });
    window.__TAURI_INTERNALS__ = { transformCallback: () => 1, invoke: async (name, args) => {
      if (name === "query_scan") {
        window.scans.push(args);
        if (args.cachedOnly) return args.kind === "overview" ? result(overview("cached-node")) : null;
        if (args.kind === "overview") return await new Promise(resolve => { window.waiting.overview = () => resolve(result(overview("fresh-node"))); });
        if (args.kind === "system") return result({ tools: [], warnings: [] });
        return result([]);
      }
      if (name === "get_settings") return { configWarning: null };
      if (name === "get_path_state") return { user: [], system: [] };
      if (name === "install_tasks" || name === "get_env_vars") return [];
      return null;
    } };
  });
  await page.goto(process.argv[2]);
  await page.getByRole("button", { name: "环境管理", exact: true }).click();
  await page.getByRole("button", { name: /^Node.js/ }).click();
  await page.getByText("cached-node", { exact: true }).waitFor();
  assert.equal(await page.getByText("正在核对最新结果...", { exact: true }).count() > 0, true);
  assert.equal(await page.evaluate(() => window.scans.filter(s => s.kind === "overview" && !s.cachedOnly).length), 1);
  await page.getByRole("button", { name: "包管理器", exact: true }).click();
  await page.getByRole("button", { name: "环境管理", exact: true }).click();
  await page.evaluate(() => window.waiting.overview());
  await page.getByText("fresh-node", { exact: true }).waitFor();
  assert.equal(await page.getByText("cached-node", { exact: true }).count(), 0);
  await page.getByRole("button", { name: "刷新", exact: true }).filter({ visible: true }).click();
  await page.waitForFunction(() => window.scans.some(s => s.kind === "overview" && s.force));
  await page.evaluate(() => window.waiting.overview());
  assert.deepEqual(errors, []);
  console.log("PASS: cached startup, visible revalidation, shared requests, responsive navigation, fresh replacement and forced scan");
} finally { await browser.close(); }
