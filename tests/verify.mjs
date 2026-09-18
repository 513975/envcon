// Cross-project entry point; fixture tests never modify real packages or PATH.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = fileURLToPath(new URL("..", import.meta.url));
const flags = process.argv.slice(2);
if (flags.some(arg => arg !== "--skip-ui")) throw new Error("Usage: node tests/verify.mjs [--skip-ui]");
function run(program, args, cwd = root) {
  console.log(`\n> ${program} ${args.join(" ")}`);
  const result = spawnSync(program, args, { cwd, stdio: "inherit", windowsHide: true });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status || 1);
}
run(process.execPath, ["node_modules/typescript/bin/tsc"]);
run(process.execPath, ["node_modules/vite/bin/vite.js", "build"]);
run("cargo", ["test", "--offline", "--lib", "--", "--test-threads=1"], path.join(root, "src-tauri"));
run(process.execPath, ["tests/read-cache.mjs"]);
if (flags.includes("--skip-ui")) {
  console.log("SKIPPED: browser UI tests (--skip-ui)");
} else {
  run(process.execPath, ["tests/run-ui.mjs", process.env.ENVCON_PLAYWRIGHT_MODULE || "playwright",
    "tests/settings-ui.mjs", "tests/startup-ui.mjs", "tests/release-052-ui.mjs", "tests/download-ui.mjs"]);
}
console.log("Verification completed. Ignored Rust integration tests and host benchmarks require an explicit separate run.");
