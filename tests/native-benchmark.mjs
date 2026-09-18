// Actual packaged Tauri/WebView2 benchmark. Only private copies/config/fixtures are written.
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createRequire } from "node:module";
import { createInterface } from "node:readline";
import fs from "node:fs/promises";
import path from "node:path";
import net from "node:net";
import os from "node:os";
import { createHash } from "node:crypto";
const [exeInput, label, modulePath, samplesArg = "3", validationFlag] = process.argv.slice(2);
if (!exeInput || !label || !modulePath || !/^[a-z0-9-]+$/.test(label)) throw new Error("Usage: node tests/native-benchmark.mjs <exe> <label> <playwright> [samples]");
const require = createRequire(import.meta.url);
const { chromium } = require(modulePath);
const benchmarkRoot = path.resolve("tests/artifacts/native-benchmark");
const runRoot = path.join(benchmarkRoot, `${label}-${Date.now()}`);
const exe = path.join(runRoot, "envcon.exe");
const data = path.join(runRoot, "data");
const fixture = path.join(benchmarkRoot, "fixture");
await fs.mkdir(data, { recursive: true });
await fs.copyFile(path.resolve(exeInput), exe);
// Three real directory layouts with 3000 files total. Version metadata avoids fake executables.
for (let version = 21; version <= 23; version++) {
  const jdk = path.join(fixture, `envs/jdks/jdk-${version}`);
  await fs.mkdir(path.join(jdk, "bin"), { recursive: true });
  await fs.writeFile(path.join(jdk, "release"), `JAVA_VERSION="${version}.0.0"\n`);
  await fs.writeFile(path.join(jdk, "bin/java.exe"), "fixture, never executed: release provides version");
  await fs.writeFile(path.join(jdk, "bin/javac.exe"), "fixture");
  const files = path.join(jdk, "fixture-files");
  await fs.mkdir(files, { recursive: true });
  await Promise.all(Array.from({ length: 1000 }, (_, i) => fs.writeFile(path.join(files, `${i}.txt`), Buffer.alloc(1024))));
}
await fs.writeFile(path.join(data, "config.json"), JSON.stringify({ schema_version: 1, root: fixture, downloads_dir: path.join(fixture, "downloads") }));
const records = [];
async function run(sample, cache) {
  if (cache === "cold") await fs.rm(path.join(data, "scan-cache-v1.json"), { force: true });
  const probe = net.createServer().listen(0, "127.0.0.1");
  await once(probe, "listening"); const port = probe.address().port;
  await new Promise(resolve => probe.close(resolve));
  const profile = path.join(runRoot, "webview-profile");
  const worker = spawn("powershell.exe", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", path.resolve("tests/benchmark-process.ps1"), "-Exe", exe], {
    windowsHide: true, stdio: ["pipe", "pipe", "pipe"],
    env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1 --disable-background-timer-throttling`, WEBVIEW2_USER_DATA_FOLDER: profile },
  });
  let stderr = "";
  worker.stderr.on("data", data => { stderr += data; });
  const queue = [], waiters = [];
  createInterface({ input: worker.stdout }).on("line", line => {
    try { const value = JSON.parse(line); const next = waiters.shift(); if (next) next(value); else queue.push(value); }
    catch { console.log("worker:", line); }
  });
  const receive = () => queue.length ? Promise.resolve(queue.shift()) : new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("Worker response timeout: " + stderr)), 45000);
    waiters.push(value => { clearTimeout(timer); resolve(value); });
  });
  const counters = async () => { worker.stdin.write("sample\n"); return await receive(); };
  let browser;
  try {
    const ready = await receive();
    if (ready.type !== "ready") throw new Error(JSON.stringify(ready));
    console.log("LAUNCHED", label, sample, cache, ready.pid, port);
    for (let i = 0; i < 300; i++) {
      try { if ((await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(200) })).ok) break; } catch {}
      if (worker.exitCode !== null) throw new Error(stderr);
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    for (let attempt = 0; attempt < 5; attempt++) {
      try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`); break; }
      catch (error) {
        console.log("CDP retry", attempt + 1, String(error), "worker", stderr);
        if (attempt === 4) throw error;
        await new Promise(resolve => setTimeout(resolve, 500));
      }
    }
    let page;
    for (let i = 0; i < 100; i++) {
      page = browser.contexts().flatMap(c => c.pages()).find(p => /tauri|localhost/.test(p.url()));
      if (page) break;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    if (!page) throw new Error("No Tauri page");
    console.log("WEBVIEW READY", page.url());
    page.setDefaultTimeout(180000);
    const errors = [];
    page.on("pageerror", e => errors.push(String(e)));
    await page.getByRole("button", { name: "环境管理", exact: true }).click();
    await page.getByText("jdk-21", { exact: true }).waitFor();
    const firstInteractiveMs = Date.now() - ready.launchTime;
    const firstInteractive = await counters();
    console.log("INTERACTIVE", firstInteractiveMs);
    // Visit both scan pages: button availability signals completed frontend loads.
    await page.getByRole("button", { name: "重新扫描", exact: true }).waitFor();
    await page.waitForFunction(() => [...document.querySelectorAll("button")].some(b => b.textContent.trim() === "重新扫描" && !b.disabled));
    await page.getByRole("button", { name: "包管理器", exact: true }).click();
    await page.getByRole("tab", { name: "路径与迁移", exact: true }).click();
    await page.waitForFunction(() => { const b = document.querySelector('[aria-label="刷新包管理器配置"]'); return b && !b.disabled; });
    await page.waitForFunction(() => [...document.querySelectorAll("button")].some(b => /^(重新统计|刷新缓存路径)$/.test(b.textContent.trim()) && !b.disabled));
    await page.waitForFunction(() => ![...document.querySelectorAll("span")].some(e => e.textContent === "正在核对最新结果..."));
    const commonScansMs = Date.now() - ready.launchTime;
    const commonScans = await counters();
    const discoveryCounts = await page.evaluate(() => ({ availableManagers: document.querySelectorAll('button[aria-label^="选择 "][title$="已安装且可用"]').length, cacheRows: [...document.querySelectorAll('button[aria-label]')].filter(b => /^打开 .* 缓存目录$/.test(b.ariaLabel)).length }));
    await page.screenshot({ path: path.join(runRoot, `${sample}-${cache}.png`), fullPage: true });
    const rescanStart = Date.now();
    await page.getByRole("button", { name: "刷新包管理器配置", exact: true }).click();
    await page.waitForFunction(() => !document.querySelector('[aria-label="刷新包管理器配置"]').disabled);
    await page.getByRole("button", { name: /^(重新统计|刷新缓存路径)$/ }).click();
    await page.waitForFunction(() => [...document.querySelectorAll("button")].some(b => /^(重新统计|刷新缓存路径)$/.test(b.textContent.trim()) && !b.disabled));
    await page.getByRole("button", { name: "环境管理", exact: true }).click();
    await page.getByRole("button", { name: "刷新", exact: true }).filter({ visible: true }).click();
    await page.waitForFunction(() => [...document.querySelectorAll("button")].some(b => b.textContent.trim() === "刷新" && b.getClientRects().length && !b.disabled));
    await page.getByRole("button", { name: "重新扫描", exact: true }).click();
    await page.waitForFunction(() => [...document.querySelectorAll("button")].some(b => b.textContent.trim() === "重新扫描" && !b.disabled));
    await page.waitForFunction(() => ![...document.querySelectorAll("span")].some(e => e.textContent === "正在核对最新结果..."));
    const manualRescanMs = Date.now() - rescanStart;
    const manualRescan = await counters();
    if (errors.length) throw new Error(errors.join("\n"));
    const failedRefreshes = await page.locator('[role="status"]').allTextContents();
    if (failedRefreshes.some(s => s.includes("刷新失败"))) throw new Error(failedRefreshes.join("\n"));
    const record = { sample, cache, firstInteractiveMs, commonScansMs, manualRescanMs, discoveryCounts, firstInteractive, commonScans, manualRescan };
    if (validationFlag === "--validate" && sample === Number(samplesArg) && cache === "warm") {
      const jdk = path.join(fixture, "envs/jdks/jdk-21");
      const parked = path.join(runRoot, "parked-jdk");
      for (const target of [jdk, parked]) {
        const relative = path.relative(benchmarkRoot, target);
        if (relative.startsWith("..") || path.isAbsolute(relative)) throw new Error("Fixture path escaped benchmark root");
      }
      let moved = false;
      try {
        await page.bringToFront();
        const changed = Date.now();
        await fs.writeFile(path.join(jdk, "release"), 'JAVA_VERSION="21.0.1"\n');
        await page.getByText("21.0.1", { exact: true }).filter({ visible: true }).waitFor({ timeout: 90000 });
        const periodicVersionChangeMs = Date.now() - changed;
        await fs.rename(jdk, parked); moved = true;
        const refresh = async () => {
          await page.getByRole("button", { name: "刷新", exact: true }).filter({ visible: true }).click();
          await page.waitForFunction(() => [...document.querySelectorAll("button")].some(b => b.textContent.trim() === "刷新" && b.getClientRects().length && !b.disabled));
        };
        await refresh();
        if (await page.getByText("jdk-21", { exact: true }).count()) throw new Error("Removed environment remained after force scan");
        await fs.rename(parked, jdk); moved = false;
        await refresh();
        await page.getByText("jdk-21", { exact: true }).waitFor();
        await page.getByRole("button", { name: "统计 jdk-21 大小", exact: true }).click();
        await page.waitForFunction(() => [...document.querySelectorAll("span")].some(e => e.textContent === "1003 文件 / 3 目录"));
        record.validation = { periodicVersionChangeMs, externalRemovalAndRestore: true, nativeSizeScanFiles: 1003 };
      } finally {
        if (moved) await fs.rename(parked, jdk);
        await fs.writeFile(path.join(jdk, "release"), 'JAVA_VERSION="21.0.0"\n');
      }
    }
    records.push(record); console.log(JSON.stringify(record));
    await fs.writeFile(path.join(runRoot, "results.json"), JSON.stringify({ label, exeSha256: createHash("sha256").update(await fs.readFile(exe)).digest("hex"), platform: `${os.type()} ${os.release()} ${os.arch()}`, cpu: os.cpus()[0].model, logicalCpus: os.cpus().length, memoryBytes: os.totalmem(), fixture: { jdks: 3, extraFiles: 3000, fileBytes: 1024 }, records }, null, 2));
  } finally {
    if (browser) await browser.close().catch(() => {});
    worker.stdin.write("stop\n");
    worker.stdin.end();
    if (worker.exitCode === null) await once(worker, "exit");
  }
}
for (let sample = 1; sample <= Number(samplesArg); sample++) {
  await run(sample, "cold");
  await run(sample, "warm");
}
console.log("RESULTS", path.join(runRoot, "results.json"));
