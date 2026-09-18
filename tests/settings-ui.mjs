import assert from "node:assert/strict";
import { installScanMock } from "./scan-mock.mjs";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { chromium } = require(process.argv[3] || "playwright");
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1100, height: 760 } });
const errors = [];
page.on("pageerror", error => errors.push(String(error)));
try {
  await installScanMock(page);
  await page.addInitScript(() => {
    window.calls = [];
    window.settings = { root: null, downloadsDir: null, portable: true, dataDir: "D:/fixture/data", configWarning: "配置文件格式无效。原文件未修改，请恢复备份。", writeBlocked: true, hasConfigBackup: true };
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        switch (command) {
          case "get_settings": return { ...window.settings };
          case "get_overview": return { root: window.settings.root, rootExists: !!window.settings.root, categories: [] };
          case "scan_system": return { tools: [], warnings: [] };
          case "get_path_state": return { user: [], system: [] };
          case "list_path_backups": case "get_tool_configs": case "get_caches": case "get_env_vars": case "install_tasks": return [];
          case "restore_settings_backup":
            await new Promise(resolve => { window.finishRecovery = resolve; });
            if (window.failRecovery) throw new Error("配置文件已被其他程序修改");
            window.settings = { ...window.settings, root: "D:/restored", configWarning: null, writeBlocked: false };
            return null;
          default: return null;
        }
      },
    };
  });
  await page.goto(process.argv[2]);
  await page.getByRole("button", { name: "前往设置修复", exact: true }).click();
  await page.getByText("配置保护与恢复", { exact: true }).waitFor();
  assert.equal(await page.getByRole("button", { name: "应用", exact: true }).isDisabled(), true);
  assert.equal(await page.getByRole("button", { name: "更改", exact: true }).isDisabled(), true);
  await page.getByRole("button", { name: "恢复上次有效配置", exact: true }).click();
  await page.getByRole("button", { name: "取消", exact: true }).click();
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "restore_settings_backup").length), 0);
  await page.getByRole("button", { name: "恢复上次有效配置", exact: true }).click();
  await page.evaluate(() => { window.failRecovery = true; });
  await page.getByRole("button", { name: "确认恢复", exact: true }).click();
  await page.waitForFunction(() => typeof window.finishRecovery === "function");
  await page.keyboard.press("Escape");
  assert.equal(await page.getByRole("dialog").count(), 1);
  await page.evaluate(() => { window.finishRecovery(); });
  await page.getByText("配置恢复失败", { exact: true }).waitFor();
  assert.equal(await page.getByRole("button", { name: "更改", exact: true }).isDisabled(), true);
  await page.evaluate(() => { window.failRecovery = false; window.finishRecovery = null; });
  await page.getByRole("button", { name: "确认恢复", exact: true }).click();
  await page.waitForFunction(() => typeof window.finishRecovery === "function");
  await page.evaluate(() => window.finishRecovery());
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await page.getByText("D:/restored", { exact: true }).waitFor();
  await page.getByRole("button", { name: "前往设置修复", exact: true }).waitFor({ state: "hidden" });
  assert.equal(await page.getByRole("button", { name: "更改", exact: true }).isDisabled(), false);
  await page.evaluate(() => { window.settings.hasConfigBackup = false; });
  await page.getByRole("button", { name: "重新加载配置", exact: true }).click();
  await page.waitForFunction(() => window.calls.some(c => c.command === "reload_settings"));
  await page.getByText("暂无可恢复的有效备份。", { exact: true }).waitFor();
  assert.equal(await page.getByRole("button", { name: "恢复上次有效配置", exact: true }).isDisabled(), true);
  while (await page.getByRole("button", { name: "关闭通知", exact: true }).count()) {
    await page.getByRole("button", { name: "关闭通知", exact: true }).first().click();
  }
  await page.screenshot({ path: "tests/settings-desktop.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.equal(await page.locator("main").evaluate(el => el.scrollWidth <= el.clientWidth), true);
  await page.screenshot({ path: "tests/settings-mobile.png", fullPage: true });
  assert.deepEqual(errors, []);
  console.log("PASS: damaged config banner, blocked writes, recovery confirmation/cancel/failure/busy guard, restored root and reload");
} finally {
  await browser.close();
}
