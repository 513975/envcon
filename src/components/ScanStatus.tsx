import { scanState, useScanRevision } from "../lib/scanClient";

export function ScanStatus({ kind, projects = [] }: { kind: "overview" | "system" | "tools" | "caches"; projects?: string[] }) {
  useScanRevision();
  const state = scanState(kind, projects);
  if (!state) return null;
  return <div className="my-2 text-xs text-zinc-500 flex flex-wrap gap-x-3 gap-y-1" role="status">
    {state.snapshot && <span>结果时间：{new Date(state.snapshot.updatedAt).toLocaleString()}</span>}
    {state.refreshing && <span>正在核对最新结果...</span>}
    {state.error && <span className="text-amber-700 dark:text-amber-400">刷新失败：{state.error}</span>}
    {state.snapshot?.warning && <span className="text-amber-700 dark:text-amber-400">{state.snapshot.warning}</span>}
  </div>;
}
