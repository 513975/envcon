import { invoke } from "@tauri-apps/api/core";
import { useEffect, useSyncExternalStore } from "react";

type Kind = "overview" | "system" | "tools" | "caches";
export interface ScanSnapshot<T = unknown> {
  value: T;
  updatedAt: number;
  durationMs: number;
  warning: string | null;
}
export interface ScanState {
  snapshot?: ScanSnapshot;
  refreshing: boolean;
  error?: string;
  revision: number;
}
interface Entry {
  kind: Kind;
  projects: string[];
  state: ScanState;
  pending?: Promise<unknown>;
  epoch: number;
}
const entries = new Map<string, Entry>();
const listeners = new Set<() => void>();
let revision = 0;
let mutationCount = 0;
const notify = () => { revision++; listeners.forEach(fn => fn()); };
export const scanKey = (kind: Kind, projects: string[] = []) => `${kind}:${JSON.stringify(projects)}`;
export function useScanRevision() {
  return useSyncExternalStore(fn => { listeners.add(fn); return () => listeners.delete(fn); }, () => revision);
}
export function scanState(kind: Kind, projects: string[] = []): ScanState | undefined {
  return entries.get(scanKey(kind, projects))?.state;
}

export function useScanResult<T>(kind: Kind, projects: string[], apply: (value: T) => void) {
  const version = useScanRevision();
  const key = scanKey(kind, projects);
  useEffect(() => {
    const snapshot = entries.get(key)?.state.snapshot;
    if (snapshot) apply(snapshot.value as T);
  }, [key, version, apply]);
}

async function refresh<T>(entry: Entry, force: boolean): Promise<T> {
  if (entry.pending) return entry.pending as Promise<T>;
  const epoch = entry.epoch;
  entry.state = { ...entry.state, refreshing: true, error: undefined };
  notify();
  const promise = (async () => {
    const snapshot = await invoke<ScanSnapshot<T> | null>("query_scan", {
      kind: entry.kind, projectDirs: entry.projects, force, cachedOnly: false,
    });
    if (!snapshot) throw new Error("扫描未返回有效结果");
    const result = snapshot;
    if (epoch !== entry.epoch) throw new Error("扫描期间环境发生变更，请刷新");
    entry.state = { snapshot: result, refreshing: false, revision: entry.state.revision + 1 };
    return result.value;
  })();
  entry.pending = promise;
  try { return await promise; }
  catch (error) {
    if (epoch === entry.epoch) entry.state = { ...entry.state, error: String(error), refreshing: false };
    throw error;
  } finally {
    if (entry.pending === promise) entry.pending = undefined;
    notify();
  }
}

export async function scanRead<T>(kind: Kind, projects: string[], force: boolean): Promise<T> {
  const key = scanKey(kind, projects);
  if (kind === "system") {
    for (const [otherKey, other] of entries) {
      if (other.kind === "system" && otherKey !== key) { other.epoch++; entries.delete(otherKey); }
    }
  }
  let entry = entries.get(key);
  if (!entry) {
    entry = { kind, projects, state: { refreshing: false, revision: 0 }, epoch: 0 };
    entries.set(key, entry);
    // Coalesce disk peeks as well as active scans.
    const target = entry;
    const epoch = target.epoch;
    const peek = invoke<ScanSnapshot<T> | null>("query_scan", { kind, projectDirs: projects, cachedOnly: true, force: false });
    target.pending = peek.then(value => value?.value);
    let snapshot: ScanSnapshot<T> | null;
    try { snapshot = await peek; } catch { snapshot = null; }
    target.pending = undefined;
    if (epoch !== target.epoch) throw new Error("扫描输入已改变，请重试");
    if (snapshot && !force) {
      target.state = { snapshot, refreshing: false, revision: 1 };
      notify();
      void refresh(target, false).catch(() => {});
      return snapshot.value;
    }
  } else if (entry.pending) {
    const result = await entry.pending;
    if (result !== undefined) return result as T;
  }
  if (!force && entry.state.snapshot) return entry.state.snapshot.value as T;
  return refresh(entry, force);
}

export async function invalidateScanReads(start: boolean) {
  mutationCount += start ? 1 : -1;
  for (const entry of entries.values()) {
    entry.epoch++;
    entry.pending = undefined;
    entry.state = { refreshing: false, revision: entry.state.revision + 1 };
  }
  notify();
  try { await invoke("invalidate_scans"); }
  catch (error) {
    if (start) mutationCount--;
    throw error;
  }
}

export function startScanRevalidation(readers: Record<Kind, (force?: boolean) => Promise<unknown>>) {
  const tick = () => {
    if (mutationCount > 0 || document.hidden) return;
    // Stagger groups so background validation cannot flood version processes.
    const next = [...entries.values()].find(e => !e.pending && (!e.state.snapshot || Date.now() - e.state.snapshot.updatedAt >= 30_000));
    if (!next) return;
    if (next.kind === "system") {
      void refresh(next, true).catch(() => {});
    } else {
      void readers[next.kind](true).catch(() => {});
    }
  };
  const timer = window.setInterval(tick, 5000);
  window.addEventListener("focus", tick);
  return () => { window.clearInterval(timer); window.removeEventListener("focus", tick); };
}
