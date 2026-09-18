import assert from "node:assert/strict";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { chromium } = require(process.argv[3] || "playwright");
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1100, height: 850 } });
const errors = [];
page.on("pageerror", e => errors.push(String(e)));
try {
  await page.addInitScript(() => {
    window.calls = [];
    window.offline = false;
    window.failUninstall = false;
    window.envs = [
      { name: "jdk-owned", envType: "jdk", isCurrent: true, isExternalLink: false },
      { name: "jdk-linked", envType: "jdk", isCurrent: false, isExternalLink: true },
    ].map(e => ({ ...e, path: "D:/managed/envs/jdks/" + e.name, identityPath: e.isExternalLink ? "E:/external/jdk" : "D:/managed/envs/jdks/" + e.name,
      version: "25.0.1", sizeBytes: null, sizeComplete: false, status: "available", statusDetail: null }));
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        switch (command) {
          case "get_overview": return { rootExists: true, root: "D:/managed", categories: ["jdk", "rust"].map(envType => ({ envType,
            envs: window.envs.filter(e => e.envType === envType), current: window.envs.find(e => e.envType === envType && e.isCurrent)?.name ?? null,
            junctionPath: "D:/managed/current/" + envType, scanWarning: null, currentError: null })) };
          case "uninstall_env":
            await new Promise(resolve => { window.finishUninstall = resolve; });
            if (window.failUninstall) throw new Error("fixture: file in use");
            window.envs = window.envs.filter(e => e.name !== args.name); return null;
          case "scan_system": return { warnings: [], tools: [{ tool: "Rust", version: "1.90.0", path: "E:/rust/toolchains/stable/bin/rustc.exe",
            command: "rustc.exe", source: "PATH", envType: "rust", installRoot: "E:/rust/toolchains/stable", identityPath: "E:/rust/toolchains/stable/bin/rustc.exe", isPreferred: true, error: null }] };
          case "integrate_external_env": return "system-1.90.0";
          case "get_sources": return [{ id: "mirror", label: "Mirror" }, { id: "official", label: "Official" }];
          case "list_versions": return Array.from({ length: 125 }, (_, i) => ({ version: `26.${124-i}.0`, date: "2026-09-16", sizeBytes: null, url: null, note: window.offline ? "离线缓存（在线查询失败）" : null }));
          case "get_tool_configs": case "get_caches": case "install_tasks": return [];
          default: return null;
        }
      },
    };
  });
  await page.goto(process.argv[2]);
  await page.getByRole("button", { name: "环境管理", exact: true }).click();
  await page.getByRole("button", { name: "卸载环境 jdk-owned", exact: true }).click();
  let dialog = page.getByRole("dialog");
  await dialog.getByText(/永久删除安装目录/).waitFor();
  await dialog.getByText(/当前链接也会一并移除/).waitFor();
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "uninstall_env").length), 0);
  await page.getByRole("button", { name: "移除管理 jdk-linked", exact: true }).click();
  await dialog.getByText(/原安装目录保留：E:\/external\/jdk/).waitFor();
  await dialog.getByRole("button", { name: "移除管理", exact: true }).click();
  await page.waitForFunction(() => typeof window.finishUninstall === "function");
  await page.keyboard.press("Escape");
  assert.equal(await dialog.count(), 1);
  await page.evaluate(() => window.finishUninstall());
  await page.getByRole("button", { name: "移除管理 jdk-linked", exact: true }).waitFor({ state: "detached" });
  await page.evaluate(() => { window.failUninstall = true; window.finishUninstall = null; });
  await page.getByRole("button", { name: "卸载环境 jdk-owned", exact: true }).click();
  await dialog.getByRole("button", { name: "确认卸载", exact: true }).click();
  await page.waitForFunction(() => typeof window.finishUninstall === "function");
  await page.evaluate(() => window.finishUninstall());
  await page.getByText("卸载失败", { exact: true }).waitFor();
  assert.equal(await dialog.count(), 1);
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await page.getByRole("button", { name: "展开 Rust 扫描结果", exact: true }).click();
  await page.getByRole("button", { name: "纳入管理", exact: true }).click();
  assert.deepEqual(await page.evaluate(() => window.calls.find(c => c.command === "integrate_external_env").args), {
    envType: "rust", path: "E:/rust/toolchains/stable/bin/rustc.exe", version: "1.90.0",
  });
  await page.screenshot({ path: "tests/release-052-environments-desktop.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 780 });
  assert.equal(await page.evaluate(() => document.querySelector("main").scrollWidth > document.querySelector("main").clientWidth), false);
  await page.screenshot({ path: "tests/release-052-environments-mobile.png", fullPage: true });
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
  console.log("PASS: environment removal confirmation/cancel/failure/busy, Rust integration, official default, pagination/history search, cache alert and responsive layouts");
} finally { await browser.close(); }
