import { spawn } from "node:child_process";
import net from "node:net";
import { once } from "node:events";
import path from "node:path";

const [playwrightPath, ...tests] = process.argv.slice(2);
if (!playwrightPath || !tests.length) {
  throw new Error("Usage: node tests/run-ui.mjs <playwright-module> <test.mjs> ...");
}
const probe = net.createServer().listen(0, "127.0.0.1");
await once(probe, "listening");
const port = probe.address().port;
await new Promise(resolve => probe.close(resolve));
const url = "http://127.0.0.1:" + port;
const server = spawn(process.execPath, [
  "node_modules/vite/bin/vite.js", "--host", "127.0.0.1", "--port", String(port), "--strictPort",
], { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
let serverLog = "";
server.stdout.on("data", data => { serverLog += data; });
server.stderr.on("data", data => { serverLog += data; });
try {
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    if (server.exitCode !== null) throw new Error(serverLog);
    try {
      if ((await fetch(url, { signal: AbortSignal.timeout(1000) })).ok) { ready = true; break; }
    } catch {}
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  if (!ready) throw new Error("Vite readiness timeout: " + serverLog);
  console.log("UI test server:", url);
  for (const test of tests) {
    const child = spawn(process.execPath, [path.resolve(test), url, playwrightPath], {
      windowsHide: true, stdio: "inherit",
    });
    const [code] = await once(child, "exit");
    if (code !== 0) throw new Error(test + " failed: " + code);
  }
} finally {
  if (server.exitCode === null) {
    server.kill();
    await once(server, "exit");
  }
}
