import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { mkdir } from "node:fs/promises";
const require = createRequire(import.meta.url);
const { chromium } = require(process.argv[3] || "playwright");
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1100, height: 760 } });
const errors = [];
page.on("pageerror", error => errors.push(String(error)));
try {
  await page.addInitScript(() => {
    window.sizeTasks = [];
    window.sizeStarts = [];
    window.sizeAbandons = [];
    window.__TAURI_INTERNALS__ = { transformCallback: () => 1, invoke: async (name, args) => {
      if (name === "query_scan") {
        if (args.cachedOnly) return null;
        const env = name => ({ name, envType: "node", path: `D:/fixture/envs/nodes/${name}`, version: "24.0.0", sizeBytes: null, sizeComplete: false, status: "available", isCurrent: false, isLinkEntry: false, isExternalLink: false });
        const value = args.kind === "overview" ? { root: "D:/fixture", rootExists: true, dataDir: "D:/fixture/data", categories: [{ envType: "node", current: null, envs: [env("node-24"), env("node-22")] }] } : args.kind === "system" ? { tools: [], warnings: [] } : args.kind === "caches" ? [{ tool: "npm", name: "npm", path: "D:/fixture/cache/npm", exists: true, sizeBytes: null, sizeComplete: false }] : [];
        return { value, updatedAt: Date.now(), durationMs: 1, warning: null };
      }
      if (name === "size_scan_status") return window.sizeTasks;
      if (name === "start_size_scan") {
        window.sizeStarts.push(args);
        const path = args.cacheTool ? "D:/fixture/cache/npm" : `D:/fixture/envs/nodes/${args.name}`;
        const task = { id: window.sizeTasks.length + 1, key: args.cacheTool ? `cache:${args.cacheTool}` : path, path, status: "running", bytes: 1024, files: 1, directories: 1, skippedLinks: 0, issues: 0, detail: null, currentPath: `${path}/packages`, complete: false, elapsedMs: 100 };
        window.sizeTasks.push(task);
        return task.id;
      }
      if (name === "cancel_size_scan") {
        const task = window.sizeTasks.find(t => t.id === args.id);
        task.status = "canceled"; task.detail = "已取消，显示已读取部分的大小";
        return null;
      }
      if (name === "abandon_size_scan") {
        // 放弃是移除记录，而不是把状态改成 canceled：卡住的任务永远不会自己
        // 走到终态，只有移除才能让统计能力恢复。
        window.sizeAbandons.push(args.id);
        const index = window.sizeTasks.findIndex(t => t.id === args.id);
        if (index >= 0) window.sizeTasks.splice(index, 1);
        return null;
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
  await page.getByRole("button", { name: "统计 node-24 大小", exact: true }).click();
  await page.getByText("1 文件 / 1 目录", { exact: true }).waitFor();
  assert.equal(await page.getByRole("button", { name: "统计 node-22 大小", exact: true }).isDisabled(), true);
  await page.locator("aside").getByRole("button", { name: "包管理器", exact: true }).click();
  await page.getByRole("tab", { name: "路径与迁移", exact: true }).click();
  assert.equal(await page.getByRole("button", { name: "统计 npm 大小", exact: true }).isDisabled(), true);
  await page.getByRole("button", { name: "环境管理", exact: true }).click();
  await page.getByRole("button", { name: "取消大小统计", exact: true }).filter({ visible: true }).click();
  await page.getByText("已取消，显示已读取部分的大小", { exact: true }).waitFor();
  await page.getByRole("button", { name: "统计 node-24 大小", exact: true }).click();
  await page.evaluate(() => { const t = window.sizeTasks[1]; t.status = "partial"; t.skippedLinks = 1; t.detail = "未跟随子目录链接或其他重解析点"; });
  await page.getByText("未跟随子目录链接或其他重解析点", { exact: true }).waitFor();
  await mkdir("tests/artifacts", { recursive: true });
  await page.screenshot({ path: "tests/artifacts/sizes-desktop.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: "tests/artifacts/sizes-mobile.png", fullPage: true });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await page.locator("aside").getByRole("button", { name: "包管理器", exact: true }).click();
  await page.getByRole("button", { name: "统计 npm 大小", exact: true }).click();
  assert.equal(await page.evaluate(() => window.sizeStarts.length), 3);
  await page.getByRole("button", { name: "取消大小统计", exact: true }).filter({ visible: true }).click();
  // 放弃：卡住的任务取消无效，必须能移除记录让统计恢复。
  await page.getByRole("button", { name: "统计 npm 大小", exact: true }).click();
  // 两个按钮必须并存——取消是「尽快停并保留部分结果」，放弃是「移除卡住记录」，
  // 语义不同，不能互相替代。
  assert.equal(await page.getByRole("button", { name: "取消大小统计", exact: true }).filter({ visible: true }).count() > 0, true);
  const abandonButton = page.getByRole("button", { name: "放弃大小统计", exact: true }).filter({ visible: true });
  assert.equal(await abandonButton.count() > 0, true);
  assert.equal(await abandonButton.getAttribute("title"), "放弃这个卡住的任务（取消无效时使用）");
  await abandonButton.click();
  assert.equal(await page.evaluate(() => window.sizeAbandons.length), 1);
  // 记录被移除后统计按钮恢复可用，否则用户会永久失去统计能力。
  assert.equal(await page.getByRole("button", { name: "统计 npm 大小", exact: true }).isDisabled(), false);
  assert.equal(await page.getByRole("button", { name: "放弃大小统计", exact: true }).count(), 0);
  assert.deepEqual(errors, []);
  console.log("PASS: on-demand sizes, bounded work, navigation retention, cancellation, abandon-a-stuck-task, partial diagnostics, cache sizing and narrow layout");
} finally { await browser.close(); }
