import assert from "node:assert/strict";
import { createRequire } from "node:module";

// Run against Vite; provide a Playwright module path when using a bundled runtime.
const require = createRequire(import.meta.url);
const { chromium } = require(process.argv[3] || "playwright");
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));

try {
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async () => 1,
    };
  });
  await page.route(/\/src\/lib\/api\.ts(?:\?.*)?$/, (route) => route.fulfill({
    contentType: "application/javascript",
    body: `
      window.downloadTest = { requests: [], installs: [], canceled: [] };
      export const api = {
        getSettings: async () => ({ configWarning: null }),
        getToolConfigs: async () => [], getCaches: async () => [], scanSystem: async () => ({tools: [], warnings: []}),
        getSources: async () => [{id: 'mirror', label: 'Mirror'}, {id: 'official', label: 'Official'}],
        listVersions: (type, source) => new Promise((resolve, reject) => {
          window.downloadTest.requests.push({type, source, resolve, reject});
        }),
        startInstall: async (...args) => { window.downloadTest.installs.push(args); return 10; },
        cancelInstall: async (id) => { window.downloadTest.canceled.push(id); },
        installTasks: async () => [
          { id: 1, envType: 'node', version: '22', targetName: 'downloading-task',
            status: 'downloading', downloaded: 1, total: 10, speed: 1 },
          { id: 2, envType: 'git', version: '2', targetName: 'external-installer',
            status: 'installing', message: 'Installing', cancelable: false },
          { id: 3, envType: 'node', version: '24', targetName: 'extracting-task',
            status: 'installing', message: 'Extracting', cancelable: true }
        ],
        getOverview: async () => ({ rootExists: false, categories: [] })
      };
    `,
  }));
  const waitForRequest = (count) => page.waitForFunction(
    (n) => window.downloadTest?.requests.length === n, count,
  );
  const resolve = (index, version) => page.evaluate(({ index, version }) => {
    window.downloadTest.requests[index].resolve([
      { version, date: null, sizeBytes: null, note: null, url: `https://example.test/${version}.zip` },
    ]);
  }, { index, version });
  await page.goto(process.argv[2] || "http://127.0.0.1:1420");
  await page.getByRole("button", { name: "下载中心", exact: true }).click();
  await waitForRequest(1);
  await page.getByRole("button", { name: "Go", exact: true }).click();
  await waitForRequest(2);
  await resolve(1, "1.26.0");
  await page.getByText("1.26.0", { exact: true }).waitFor();
  await resolve(0, "22.0.0");
  await page.waitForTimeout(100);
  assert.equal(await page.getByText("22.0.0", { exact: true }).count(), 0);

  await page.getByRole("button", { name: "Mirror", exact: true }).click();
  await waitForRequest(3);
  await page.getByRole("button", { name: "Official", exact: true }).click();
  await waitForRequest(4);
  await resolve(3, "1.26.1");
  await page.getByText("1.26.1", { exact: true }).waitFor();
  await page.evaluate(() => window.downloadTest.requests[2].reject(new Error("stale failure")));
  await page.waitForTimeout(100);
  assert.equal(await page.getByText("stale failure", { exact: true }).count(), 0);
  await page.getByRole("button", { name: "下载", exact: true }).click();
  const installs = await page.evaluate(() => window.downloadTest.installs);
  assert.deepEqual(installs, [["go", "1.26.1", "go-1.26.1", "official", "https://example.test/1.26.1.zip"]]);

  const cancelButtons = page.getByRole("button", { name: "取消", exact: true });
  assert.equal(await cancelButtons.count(), 3);
  assert.equal(await cancelButtons.nth(1).isDisabled(), true);
  await cancelButtons.nth(0).click();
  await cancelButtons.nth(2).click();
  assert.deepEqual(await page.evaluate(() => window.downloadTest.canceled), [1, 3]);
  assert.deepEqual(errors, []);
  console.log("PASS: stale success/error responses, install arguments, cancellation controls");
} catch (error) {
  console.error("Browser errors:", errors);
  console.error(await page.locator("body").innerText());
  throw error;
} finally {
  await browser.close();
}
