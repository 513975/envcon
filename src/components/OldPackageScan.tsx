import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ChevronDown,
  ChevronRight,
  FolderOpen,
  Package,
  Search,
  Square,
} from "lucide-react";
import { api } from "../lib/api";
import { managers, managerDefinition } from "../lib/managers";
import { useTaskStatus } from "../lib/useTaskStatus";
import type { PackageList } from "../lib/types";
import { Button } from "./ui/Button";

export function OldPackageScan({
  onReinstall,
}: {
  onReinstall: (tool: string, source: string) => void;
}) {
  const [mode, setMode] = useState<"common" | "drives" | "directory">("common");
  const [directory, setDirectory] = useState("");
  const [filter, setFilter] = useState("");
  const [manager, setManager] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [expanded, setExpanded] = useState("");
  const [details, setDetails] = useState<PackageList | null>(null);
  const [loadingDetails, setLoadingDetails] = useState(false);
  const request = useRef(0);
  const {
    task,
    setTask,
    initializing,
    statusError,
    beginMutation,
    endMutation,
  } = useTaskStatus("package-scan", api.packageScanStatus);
  const running = task?.status === "running";
  useEffect(
    () => () => {
      request.current += 1;
    },
    [],
  );
  const scan = async () => {
    if (busy || running || initializing) return;
    beginMutation();
    setBusy(true);
    setError("");
    setExpanded("");
    setDetails(null);
    request.current += 1;
    try {
      await api.startPackageScan(
        mode,
        mode === "directory" ? directory.trim() : null,
      );
      setTask(await api.packageScanStatus());
    } catch (e) {
      setError(String(e));
    } finally {
      endMutation();
      setBusy(false);
    }
  };
  const browse = async () => {
    try {
      const path = await open({ directory: true, title: "选择旧包扫描目录" });
      if (typeof path === "string") setDirectory(path);
    } catch (e) {
      setError(String(e));
    }
  };
  const inspect = async (tool: string, path: string) => {
    const id = `${tool}:${path}`;
    const current = ++request.current;
    setDetails(null);
    setError("");
    if (expanded === id) {
      setExpanded("");
      setLoadingDetails(false);
      return;
    }
    setExpanded(id);
    setLoadingDetails(true);
    try {
      const result = await api.listInstalledPackages(tool, path, true);
      if (request.current === current) setDetails(result);
    } catch (e) {
      if (request.current === current) setError(String(e));
    } finally {
      if (request.current === current) setLoadingDetails(false);
    }
  };
  const rows = (task?.sources ?? []).filter(
    (row) =>
      (!manager || row.tool === manager) &&
      `${row.tool} ${row.path}`.toLowerCase().includes(filter.toLowerCase()),
  );
  const states: Record<string, string> = {
    running: "正在扫描",
    done: "扫描完成",
    partial: "扫描结束，有未覆盖位置",
    canceled: "已停止，保留已发现结果",
    error: "扫描失败",
  };
  return (
    <section aria-label="旧包扫描" className="space-y-4">
      <div className="flex flex-wrap items-end gap-3">
        <label className="text-xs">
          扫描范围
          <select
            aria-label="扫描范围"
            value={mode}
            disabled={busy || running}
            onChange={(e) => setMode(e.target.value as typeof mode)}
            className="block mt-1 h-9 rounded border border-zinc-300 dark:border-zinc-700 bg-white dark:bg-zinc-900 px-2 text-sm"
          >
            <option value="common">常见位置</option>
            <option value="drives">全部本地磁盘</option>
            <option value="directory">指定目录</option>
          </select>
        </label>
        {running ? (
          <Button
            onClick={() =>
              api.cancelPackageScan().catch((e) => setError(String(e)))
            }
          >
            <Square className="size-4" />
            停止扫描
          </Button>
        ) : (
          <Button
            variant="primary"
            loading={busy}
            disabled={
              initializing || (mode === "directory" && !directory.trim())
            }
            onClick={scan}
          >
            <Search className="size-4" />
            扫描旧包
          </Button>
        )}
      </div>
      {mode === "directory" && (
        <div className="flex gap-2">
          <input
            aria-label="扫描目录"
            value={directory}
            disabled={busy || running}
            onChange={(e) => setDirectory(e.target.value)}
            className="min-w-0 flex-1 rounded border border-zinc-300 dark:border-zinc-700 bg-transparent p-2 font-mono text-xs"
          />
          <Button
            title="选择扫描目录"
            aria-label="选择扫描目录"
            disabled={busy || running}
            onClick={browse}
          >
            <FolderOpen className="size-4" />
          </Button>
        </div>
      )}
      {(error || statusError) && (
        <p
          role="alert"
          className="text-xs text-red-600 break-all whitespace-pre-wrap"
        >
          {error || statusError}
        </p>
      )}
      {task && (
        <>
          <div className="text-xs space-y-1">
            <p role="status">
              {states[task.status] ?? task.status} · {task.visited} 个目录 ·{" "}
              {task.sources.length} 个包来源 ·{" "}
              {task.sources.reduce((sum, row) => sum + row.packages, 0)}{" "}
              个安装记录
            </p>
            {task.currentPath && (
              <p
                className="font-mono truncate text-zinc-500"
                title={task.currentPath}
              >
                {task.currentPath}
              </p>
            )}
          </div>
          <div className="flex flex-wrap gap-3">
            <select
              aria-label="筛选管理器"
              value={manager}
              onChange={(e) => setManager(e.target.value)}
              className="h-9 max-w-full rounded border border-zinc-300 dark:border-zinc-700 bg-white dark:bg-zinc-900 px-2 text-xs"
            >
              <option value="">全部管理器</option>
              {managers.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                </option>
              ))}
            </select>
            <input
              aria-label="搜索来源目录"
              placeholder="搜索来源目录"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              className="min-w-0 flex-1 border-b border-zinc-300 dark:border-zinc-700 bg-transparent text-sm"
            />
          </div>
          <div className="divide-y divide-zinc-200 dark:divide-zinc-800 border-y border-zinc-200 dark:border-zinc-800">
            {rows.map((row) => {
              const id = `${row.tool}:${row.path}`;
              const active = expanded === id;
              return (
                <div key={id} className="py-3 space-y-2">
                  <div className="flex gap-2 items-start">
                    <button
                      disabled={running}
                      aria-label={`查看 ${row.tool} ${row.path}`}
                      aria-expanded={active}
                      onClick={() => inspect(row.tool, row.path)}
                      className="shrink-0 p-1 disabled:opacity-40"
                    >
                      {active ? (
                        <ChevronDown className="size-4" />
                      ) : (
                        <ChevronRight className="size-4" />
                      )}
                    </button>
                    <div className="flex-1 min-w-0">
                      <p className="text-sm font-medium">
                        {managerDefinition(row.tool).name}
                        <span className="ml-2 text-xs font-normal text-zinc-500">
                          {row.current ? "当前配置" : "其他来源，待核对"}
                        </span>
                      </p>
                      <p className="text-xs font-mono break-all selectable">
                        {row.path}
                      </p>
                      <p className="mt-1 text-xs text-zinc-500">
                        {row.error
                          ? "包清单不可读取"
                          : `${row.packages} 个包 · ${row.reinstallable} 个可重装`}
                      </p>
                    </div>
                  </div>
                  {row.error && (
                    <p className="text-xs text-amber-700 break-all">
                      {row.error}
                    </p>
                  )}
                  {active && (
                    <div className="pl-7 space-y-2">
                      {loadingDetails && (
                        <p role="status" className="text-xs">
                          正在读取包清单
                        </p>
                      )}
                      {details && (
                        <>
                          <div className="max-h-72 overflow-auto">
                            <table className="w-full table-fixed text-xs text-left">
                              <thead>
                                <tr>
                                  <th className="w-[35%] py-2">包名</th>
                                  <th className="w-[20%]">版本</th>
                                  <th>重装状态</th>
                                </tr>
                              </thead>
                              <tbody>
                                {details.packages.map((p, i) => (
                                  <tr
                                    key={`${p.name}:${i}`}
                                    className="border-t border-zinc-100 dark:border-zinc-800"
                                  >
                                    <td className="py-2 pr-2 font-mono break-all">
                                      {p.name}
                                    </td>
                                    <td className="py-2 pr-2 font-mono break-all">
                                      {p.version ?? "未知"}
                                    </td>
                                    <td className="py-2 break-all">
                                      {p.detail ?? "可重装"}
                                    </td>
                                  </tr>
                                ))}
                              </tbody>
                            </table>
                          </div>
                          <Button
                            disabled={running || !details.packages.length}
                            onClick={() => onReinstall(row.tool, row.path)}
                          >
                            <Package className="size-4" />
                            重装此目录的包
                          </Button>
                        </>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
          {!rows.length && (
            <p className="text-sm text-zinc-500 py-4">
              {running ? "正在查找包目录" : "当前范围没有匹配的包来源"}
            </p>
          )}
          <details className="text-xs">
            <summary className="cursor-pointer">
              扫描记录 · {task.warningCount} 项诊断 · {task.skippedProjects}{" "}
              个项目目录已排除
            </summary>
            <div className="mt-2 max-h-56 overflow-auto space-y-2 break-all">
              {task.warnings.map((text, i) => (
                <p key={i}>{text}</p>
              ))}
              {task.warningCount > task.warnings.length && (
                <p>
                  其余 {task.warningCount - task.warnings.length} 项诊断未展开
                </p>
              )}
              <p>扫描根目录</p>
              {task.roots.map((path, i) => (
                <p className="font-mono" key={i}>
                  {path}
                </p>
              ))}
            </div>
          </details>
        </>
      )}
    </section>
  );
}
