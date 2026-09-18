import assert from "node:assert/strict";
import fs from "node:fs/promises";
import ts from "typescript";

const source = await fs.readFile(new URL("../src/lib/readCache.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext } });
const { cachedRead, invalidateReads } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`);
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((a, b) => { resolve = a; reject = b; });
  return { promise, resolve, reject };
};

let calls = 0;
const pending = deferred();
const read = () => { calls++; return pending.promise; };
const a = cachedRead("shared", read);
const b = cachedRead("shared", read);
pending.resolve(1);
assert.deepEqual(await Promise.all([a, b]), [1, 1]);
assert.equal(calls, 1);

const old = deferred();
const first = cachedRead("refresh", () => old.promise);
const newest = cachedRead("refresh", async () => 2, true);
old.resolve(0);
assert.deepEqual(await Promise.all([first, newest]), [2, 2]);

const invalidated = deferred();
let revisions = 0;
const stale = cachedRead("mutation", () => ++revisions === 1 ? invalidated.promise : Promise.resolve(3));
await Promise.resolve();
invalidateReads();
invalidated.resolve(0);
assert.equal(await stale, 3);
assert.equal(revisions, 2);

const failed = deferred();
const obsoleteFailure = cachedRead("failed", () => failed.promise);
await Promise.resolve();
const retry = cachedRead("failed", async () => 4, true);
failed.reject(new Error("obsolete"));
assert.deepEqual(await Promise.all([obsoleteFailure, retry]), [4, 4]);
await assert.rejects(cachedRead("sync", () => { throw new Error("sync failure"); }), /sync failure/);
assert.equal(await cachedRead("sync", async () => 5), 5);
console.log("PASS: shared reads, forced refresh ordering, mutation invalidation, obsolete errors and retry");
