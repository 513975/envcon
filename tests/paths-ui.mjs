import assert from "node:assert/strict";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { chromium } = require(process.argv[3] || "playwright");
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1100, height: 900 } });
const errors = [];
page.on("pageerror", error => errors.push(String(error)));
try {
  await page.addInitScript(() => {
    window.calls = [];
    window.elevated = false;
    window.systemPath = "C:/Windows;%SystemRoot%/System32";
    window.systemJournal = "fixture-journal-1";
    window.pathSaveMode = "cancel";
    // 同一个变量名在两个 hive 里可以是不同的值——这正是必须分开显示的用例。
    window.envVars = [
      { name: "JAVA_HOME", label: "JDK 根目录", value: "D:/user/jdk", systemValue: "C:/system/jdk", suggested: null },
      { name: "GOPATH", label: "Go 工作区", value: null, systemValue: null, suggested: "D:/managed/gopath" },
    ];
    window.custom = {
      user: [{ name: "MY_USER_VAR", value: "user-value" }],
      system: [{ name: "MY_SYS_VAR", value: "sys-value" }],
    };
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        switch (command) {
          case "get_path_state": return { user: ["D:/user/bin"], system: window.systemPath.split(";") };
          case "preview_system_path_edit": {
            if (args.expectedPath !== null && args.expectedPath !== window.systemPath) throw "系统 PATH 已变化，请重新读取后编辑";
            const entries = args.entries ?? window.systemPath.split(";");
            if (entries.some(entry => entry.includes(";"))) throw "PATH 单项不能包含分号";
            return { expectedPath: window.systemPath, journalSnapshot: window.systemJournal, entries,
              resultingPath: entries.join(";"), sources: entries.map(entry => window.systemPath.split(";").includes(entry) ? "共享 / 来源未确认" : "本次手工新增 / 修改；不自动认领") };
          }
          case "save_system_path":
            if (args.plan.expectedPath !== window.systemPath || args.plan.journalSnapshot !== window.systemJournal) throw "PATH 或来源记录已变化，请重新读取并预览";
            if (window.pathSaveMode === "cancel") throw "已取消管理员授权，系统环境变量未修改";
            window.systemPath = args.plan.resultingPath;
            return null;
          case "get_env_vars": return window.envVars;
          case "get_custom_env_vars": return window.custom[args.scope] ?? [];
          case "is_elevated": return window.elevated;
          case "query_scan": return null;
          case "save_env_var": case "save_user_path": case "integrate_current_to_path": return null;
          default: return null;
        }
      },
    };
  });
  await page.goto(process.argv[2]);
  await page.locator("aside").getByRole("button", { name: "路径管理", exact: true }).click();
  await page.getByText("开发环境变量", { exact: true }).waitFor();

  // 默认是用户级；用户级与系统级是两套独立的值。
  const userTab = page.getByRole("button", { name: "用户级（HKCU）", exact: true });
  const systemTab = page.getByRole("button", { name: "系统级（HKLM）", exact: true });
  assert.equal(await userTab.getAttribute("aria-pressed"), "true");
  assert.equal(await systemTab.getAttribute("aria-pressed"), "false");
  const javaInput = page.getByLabel("JAVA_HOME 的值", { exact: true });
  assert.equal(await javaInput.inputValue(), "D:/user/jdk");
  // 用户级下不应出现提权提示。
  assert.equal(await page.getByText("写入时会弹出一次管理员授权", { exact: true }).count(), 0);
  assert.equal(await page.getByText("MY_USER_VAR", { exact: true }).count(), 1);
  assert.equal(await page.getByText("MY_SYS_VAR", { exact: true }).count(), 0);

  // 切到系统级：必须显示 systemValue，而不是沿用用户级的值。
  // 若沿用，用户会以为系统级等于用户级，进而覆盖掉真实的系统级配置。
  await systemTab.click();
  await page.getByText("写入时会弹出一次管理员授权", { exact: true }).waitFor();
  assert.equal(await systemTab.getAttribute("aria-pressed"), "true");
  assert.equal(await userTab.getAttribute("aria-pressed"), "false");
  await page.waitForFunction(() =>
    window.calls.some(c => c.command === "get_custom_env_vars" && c.args.scope === "system"));
  assert.equal(await javaInput.inputValue(), "C:/system/jdk");
  assert.equal(await page.getByText("MY_SYS_VAR", { exact: true }).count(), 1);
  assert.equal(await page.getByText("MY_USER_VAR", { exact: true }).count(), 0);

  // 系统级保存必须带上 scope=system，否则会写进用户级。
  await javaInput.fill("C:/edited/jdk");
  await page.getByRole("button", { name: "保存", exact: true }).first().click();
  await page.waitForFunction(() => window.calls.some(c => c.command === "save_env_var"));
  assert.deepEqual(
    await page.evaluate(() => window.calls.find(c => c.command === "save_env_var").args),
    { name: "JAVA_HOME", value: "C:/edited/jdk", scope: "system" },
  );

  // 新增自定义变量也跟随当前作用域。
  await page.getByLabel("新变量名", { exact: true }).fill("MY_NEW_VAR");
  await page.getByLabel("新变量值", { exact: true }).fill("D:/new");
  await page.getByRole("button", { name: "添加环境变量", exact: true }).click();
  await page.waitForFunction(() => window.calls.filter(c => c.command === "save_env_var").length >= 2);
  assert.deepEqual(
    await page.evaluate(() => window.calls.filter(c => c.command === "save_env_var")[1].args),
    { name: "MY_NEW_VAR", value: "D:/new", scope: "system" },
  );

  // 切回用户级：值必须切回来，且作用域传参随之改变。
  await userTab.click();
  await page.waitForFunction(() => window.calls.filter(c => c.command === "get_custom_env_vars").at(-1).args.scope === "user");
  assert.equal(await javaInput.inputValue(), "D:/user/jdk");
  assert.equal(await page.getByText("写入时会弹出一次管理员授权", { exact: true }).count(), 0);

  // 删除自定义变量同样带作用域。
  await page.getByRole("button", { name: "删除变量 MY_USER_VAR", exact: true }).click();
  await page.waitForFunction(() => window.calls.filter(c => c.command === "save_env_var").length >= 3);
  assert.deepEqual(
    await page.evaluate(() => window.calls.filter(c => c.command === "save_env_var").at(-1).args),
    { name: "MY_USER_VAR", value: "", scope: "user" },
  );

  await page.getByRole("button", { name: "编辑系统 PATH", exact: true }).click();
  const editor = page.getByRole("group", { name: "系统 PATH 编辑器", exact: true });
  assert.equal(await page.getByLabel("系统 PATH 第 1 项", { exact: true }).inputValue(), "C:/Windows");
  await page.getByLabel("新增系统 PATH", { exact: true }).fill("D:/tools/bin");
  await page.getByRole("button", { name: "添加系统路径", exact: true }).click();
  await page.getByRole("button", { name: "上移系统 PATH 第 3 项", exact: true }).click();
  await page.getByRole("button", { name: "预览系统 PATH", exact: true }).click();
  let dialog = page.getByRole("dialog", { name: "确认修改系统 PATH" });
  await dialog.waitFor();
  assert.equal(await dialog.getByText("共享 / 来源未确认", { exact: true }).count(), 2);
  const authorize = () => page.getByRole("button", { name: "授权并保存系统 PATH", exact: true });
  assert.equal(await authorize().isDisabled(), true);
  assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === "save_system_path").length), 0);
  await dialog.getByRole("checkbox").check();
  await authorize().click();
  await page.getByText("系统 PATH 未能确认保存", { exact: true }).waitFor();
  assert.equal(await editor.count(), 1);
  assert.equal(await page.evaluate(() => window.systemPath), "C:/Windows;%SystemRoot%/System32");
  assert.equal(await page.getByLabel("系统 PATH 第 2 项", { exact: true }).inputValue(), "D:/tools/bin");

  // A failed authorization preserves the draft; a new preview is required.
  await page.evaluate(() => { window.pathSaveMode = "success"; });
  await page.getByRole("button", { name: "预览系统 PATH", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "确认修改系统 PATH" });
  await dialog.getByRole("checkbox").check();
  await authorize().click();
  await page.getByRole("button", { name: "编辑系统 PATH", exact: true }).waitFor();
  assert.deepEqual(await page.evaluate(() => window.calls.filter(c => c.command === "save_system_path").at(-1).args), {
    plan: {
      expectedPath: "C:/Windows;%SystemRoot%/System32", journalSnapshot: "fixture-journal-1",
      entries: ["C:/Windows", "D:/tools/bin", "%SystemRoot%/System32"],
      resultingPath: "C:/Windows;D:/tools/bin;%SystemRoot%/System32",
      sources: ["共享 / 来源未确认", "本次手工新增 / 修改；不自动认领", "共享 / 来源未确认"],
    },
  });

  await page.getByRole("button", { name: "编辑系统 PATH", exact: true }).click();
  await page.getByRole("button", { name: "删除系统 PATH 第 2 项", exact: true }).click();
  await page.getByRole("button", { name: "预览系统 PATH", exact: true }).click();
  await page.evaluate(() => { window.systemPath += ";D:/other-installer"; });
  dialog = page.getByRole("dialog", { name: "确认修改系统 PATH" });
  await dialog.getByRole("checkbox").check();
  await authorize().click();
  await dialog.waitFor({ state: "hidden" });
  assert.equal(await page.evaluate(() => window.systemPath.endsWith(";D:/other-installer")), true);
  await page.getByRole("button", { name: "重新读取", exact: true }).click();
  await page.waitForFunction(() => window.calls.filter(c => c.command === "preview_system_path_edit").at(-1).args.entries === null);
  await page.getByLabel("系统 PATH 第 4 项", { exact: true }).waitFor();
  assert.equal(await page.getByLabel("系统 PATH 第 4 项", { exact: true }).inputValue(), "D:/other-installer");

  await page.screenshot({ path: "tests/artifacts/paths-env-var-desktop.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.equal(await page.locator("main").evaluate(el => el.scrollWidth <= el.clientWidth), true);
  await page.screenshot({ path: "tests/artifacts/paths-env-var-mobile.png", fullPage: true });
  assert.deepEqual(errors, []);
  console.log("PASS: env var scopes; system PATH row edits, preview consent, UAC cancellation, exact snapshot payload, stale save rejection and reload; narrow layout");
} finally {
  await browser.close();
}
