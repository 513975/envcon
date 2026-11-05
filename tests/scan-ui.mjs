import assert from "node:assert/strict";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { chromium } = require(process.argv[3] || "playwright");
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1100, height: 720 } });
const errors = [];
page.on("pageerror", error => errors.push(String(error)));
try {
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = { transformCallback: () => 1,
      invoke: async (command) => command === "plugin:dialog|open" ? "D:/Projects" : 1 };
  });
  await page.route(/\/src\/lib\/api\.ts(?:\?.*)?$/, (route) => route.fulfill({
    contentType: "application/javascript",
    body: `
      window.scanOpened = [];
      window.scanDirectories = [];
      export const api = {
        getOverview: async () => ({rootExists: false, root: null, categories: []}),
        getSettings: async () => ({configWarning: null}),
        getToolConfigs: async () => null,
        getCaches: async () => null,
        installTasks: async () => [],
        scanSystem: async (dirs) => { window.scanDirectories = dirs; return {status: 'partial', coreTools: [
          {tool: 'Git', status: 'permissionDenied', entryCount: 1, detail: '权限不足'},
          {tool: 'GitHub CLI', status: 'notDiscovered', entryCount: 0, detail: '本次范围内未发现'},
          {tool: 'Node.js', status: 'available', entryCount: 1, detail: null},
          {tool: 'Python', status: 'available', entryCount: 1, detail: null},
          {tool: 'Java', status: 'available', entryCount: 3, detail: null},
          {tool: 'Go', status: 'notDiscovered', entryCount: 0, detail: '本次范围内未发现'},
          {tool: 'Rust', status: 'notDiscovered', entryCount: 0, detail: '本次范围内未发现'},
          {tool: 'PowerShell 7', status: 'available', entryCount: 1, detail: null}
        ], tools: [
          {tool: 'Java', version: '21.0.8', path: 'D:/Java/jdk-21/bin/java.exe', command: 'java.exe', source: 'PATH', envType: 'jdk', isPreferred: true, error: null, scanStatus: 'available', installRoot: 'D:/Java/jdk-21'},
          {tool: 'Java', version: '21.0.8', path: 'E:/Other/jdk-21/bin/java.exe', command: 'java.exe', source: '常见安装目录', envType: 'jdk', isPreferred: false, error: null, scanStatus: 'available'},
          {tool: 'Java', version: '17.0.10', path: 'D:/Java/jdk-17/bin/java.exe', command: 'java.exe', source: 'JAVA_HOME', envType: 'jdk', isPreferred: false, error: null, scanStatus: 'available'},
          {tool: 'Python', version: '3.14.0', path: 'D:/Python/python.exe', command: 'python.exe', source: 'PATH', envType: 'python', isPreferred: true, error: null, scanStatus: 'available'},
          {tool: 'Node.js', version: '22.18.0', path: 'D:/Node/node.exe', command: 'node.exe', source: 'PATH', envType: 'node', isPreferred: true, error: null, scanStatus: 'available', versionConflicts: [
            {path: 'D:/Node/bin/node.cmd', version: '20.11.0', error: null},
            {path: 'D:/Node/npm-cache/node.cmd', version: null, error: '无法启动: 拒绝访问'},
          ]},
          {tool: 'npm', version: '11.18.0', path: 'D:/Node/npm.cmd', command: 'npm.cmd', source: 'PATH', envType: null, isPreferred: true, error: null, scanStatus: 'available'},
          {tool: 'pip', version: '26.1.2', path: 'D:/Python/Scripts/pip.exe', command: 'pip.exe', source: 'EnvCon', envType: null, isPreferred: false, error: null, scanStatus: 'available'},
          {tool: 'Maven', version: '3.9.11', path: 'D:/Maven/bin/mvn.cmd', command: 'mvn.cmd', source: 'MAVEN_HOME', envType: 'maven', isPreferred: false, error: null, scanStatus: 'available'},
          {tool: 'Git', version: null, path: 'D:/Git/cmd/git.exe', command: 'git.exe', source: 'PATH', envType: null, isPreferred: true, error: '权限不足: 访问被拒绝', scanStatus: 'permissionDenied'},
          {tool: 'pip (Python 模块)', version: null, path: 'D:/Projects/.venv/Scripts/python.exe', command: 'python.exe -m pip', source: '项目目录', envType: null, isPreferred: false, error: '命令退出码 1: No module named pip', scanStatus: 'commandFailed'},
          {tool: 'PowerShell 7', version: '7.6.9', path: 'C:/Program Files/PowerShell/7/pwsh.exe', command: 'pwsh.exe', source: '常见安装目录', envType: 'powershell', isPreferred: false, error: null, scanStatus: 'available', installRoot: 'C:/Program Files/PowerShell/7'}
        ], warnings: ['已达到扫描深度 6: D:/Projects/nested']}; },
        openPath: async path => { window.scanOpened.push(path); }
      };`,
  }));
  await page.goto(process.argv[2] || "http://127.0.0.1:1422");
  await page.getByRole("button", { name: "环境管理", exact: true }).waitFor({ timeout: 10000 }).catch(async error => {
    console.error("Navigation unavailable:", await page.locator("body").innerText().catch(() => ""), errors);
    throw error;
  });
  await page.getByRole("button", { name: "环境管理", exact: true }).click();
  await page.getByRole("button", { name: "项目目录", exact: true }).click();
  await page.getByText("D:/Projects", { exact: true }).waitFor();
  await page.getByRole("button", { name: /^(开始扫描|重新扫描)$/ }).click();
  await page.getByText("11.18.0", { exact: true }).waitFor();
  await page.getByText("PowerShell 7", { exact: true }).first().waitFor();
  assert.equal(await page.locator("[data-scan-tool]").count(), 8);
  assert.equal(await page.getByRole("button", { name: /收起 .* 扫描结果/ }).count(), 0);
  await page.getByText("11 个入口", { exact: true }).waitFor();
  await page.getByText("2 个入口待检查", { exact: true }).waitFor();
  await page.getByLabel("核心工具扫描状态").getByText("扫描范围内未发现", { exact: true }).first().waitFor();
  await page.getByText("权限不足", { exact: true }).first().waitFor();
  for (const [width, height, dark] of [[1100,900,false], [900,700,false], [390,740,true]]) {
    await page.setViewportSize({ width, height });
    await page.emulateMedia({ colorScheme: dark ? "dark" : "light" });
    await page.locator('[aria-label="扫描结果"]').evaluate(el => el.scrollIntoView({ block: "start" }));
    await page.screenshot({ path: `tests/scan-grouped-${width}.png`, animations: "disabled" });
    assert.equal(await page.evaluate(() => document.querySelector("main").scrollWidth > document.querySelector("main").clientWidth), false);
    assert.ok((await page.getByLabel("筛选扫描结果").boundingBox()).width > 150);
  }
  await page.setViewportSize({ width: 1100, height: 900 });
  await page.getByRole("button", { name: "展开 Java 扫描结果", exact: true }).click();
  await page.getByText("E:/Other/jdk-21/bin/java.exe", { exact: true }).waitFor();
  assert.equal(await page.locator('[data-scan-tool="Java"]').getByText("21.0.8", { exact: true }).count(), 2);
  await page.getByRole("button", { name: "收起 Java 扫描结果", exact: true }).click();
  await page.getByRole("group", { name: "扫描分类", exact: true }).getByRole("button", { name: /包管理器/ }).click();
  assert.equal(await page.locator("[data-scan-tool]").count(), 2);
  await page.getByRole("button", { name: "展开 npm 扫描结果", exact: true }).click();  await page.getByRole("button", { name: "展开 pip 扫描结果", exact: true }).click();
  await page.getByText("D:/Python/Scripts/pip.exe", { exact: true }).waitFor();
  assert.deepEqual(await page.evaluate(() => window.scanDirectories), ["D:/Projects"]);
  assert.equal(await page.getByText("PATH 首选", { exact: true }).count(), 1);
  await page.getByText("命令退出码 1: No module named pip", { exact: true }).waitFor();
  await page.getByLabel("扫描状态", { exact: true }).selectOption("attention");
  assert.equal(await page.locator("[data-scan-tool]").count(), 1);
  assert.equal(await page.getByText("D:/Python/Scripts/pip.exe", { exact: true }).count(), 0);
  await page.locator('[data-scan-tool="pip"]').getByText("探测详情", { exact: true }).click();
  await page.getByText("python.exe -m pip", { exact: true }).waitFor();
  await page.getByLabel("扫描状态", { exact: true }).selectOption("all");
  await page.getByText("扫描提示（1）", { exact: true }).click();
  await page.getByText("已达到扫描深度 6: D:/Projects/nested", { exact: true }).waitFor();
  await page.getByLabel("筛选扫描结果").fill("26.1.2");
  assert.equal(await page.getByText("11.18.0", { exact: true }).count(), 0);
  await page.getByTitle("打开所在目录", { exact: true }).click();
  assert.deepEqual(await page.evaluate(() => window.scanOpened), ["D:/Python/Scripts"]);
  assert.equal(await page.getByRole("button", { name: "纳入管理", exact: true }).count(), 0);
  await page.getByTitle("移除扫描目录", { exact: true }).click();
  assert.equal(await page.getByText("D:/Projects", { exact: true }).count(), 0);
  await page.getByLabel("筛选扫描结果").fill("missing-tool");
  await page.getByText("没有匹配结果", { exact: true }).waitFor();
  // Version conflicts of one installation must be visible with path and version.
  await page.getByLabel("筛选扫描结果").fill("");
  await page.getByRole("group", { name: "扫描分类", exact: true }).getByRole("button", { name: /运行时/ }).click();
  await page.locator('[data-scan-tool="Node.js"]').getByText("2 个入口版本不一致", { exact: true }).waitFor();
  await page.getByRole("button", { name: "展开 Node.js 扫描结果", exact: true }).click();
  await page.getByText("同一安装的其他入口版本不同（2 个）", { exact: true }).waitFor();
  await page.getByText("D:/Node/bin/node.cmd", { exact: true }).waitFor();
  await page.getByText("报告 20.11.0", { exact: true }).waitFor();
  await page.getByText(/无法读取版本（无法启动: 拒绝访问）/).waitFor();
  await page.setViewportSize({ width: 390, height: 740 });
  assert.equal(await page.evaluate(() => document.querySelector("main").scrollWidth > document.querySelector("main").clientWidth), false);
  await page.screenshot({ path: "tests/scan-conflicts-390.png", animations: "disabled" });
  assert.deepEqual(errors, []);
  console.log("PASS: grouped counts, same-version distinct installations, category/status/search, collapsed errors, module details, PATH preference, responsive layouts");
} finally {
  await browser.close();
}
