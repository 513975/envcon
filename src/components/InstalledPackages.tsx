import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, RefreshCw, Search, Package } from "lucide-react";
import { api } from "../lib/api";
import type { PackageList } from "../lib/types";
import { Button } from "./ui/Button";
import { Spinner } from "./ui/Empty";
import { GlobalSourceSelect } from "./GlobalSourceSelect";
import { managerDefinition } from "../lib/managers";

export function InstalledPackages({
  tool,
  onReinstall,
}: {
  tool: string;
  onReinstall?: (source: string) => void;
}) {
  const [selectedScope, setScope] = useState("global");
  const definition = managerDefinition(tool);
  const scope =
    !definition.projectQuery && selectedScope === "directory"
      ? "global"
      : selectedScope;
  const [directory, setDirectory] = useState("");
  const [filter, setFilter] = useState("");
  const [result, setResult] = useState<PackageList | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const generation = useRef(0);
  const load = async () => {
    const request = ++generation.current;
    setBusy(true);
    setError("");
    setResult(null);
    try {
      const data = await api.listInstalledPackages(
        tool,
        scope === "global" ? null : directory.trim(),
        scope === "historical",
      );
      if (generation.current === request) setResult(data);
    } catch (e) {
      if (generation.current === request) setError(String(e));
    } finally {
      if (generation.current === request) setBusy(false);
    }
  };
  useEffect(() => {
    setResult(null);
    setError("");
    setBusy(false);
    setFilter("");
    if (scope === "global") void load();
    return () => {
      generation.current += 1;
    };
  }, [tool, scope]);
  const browse = async () => {
    try {
      const path = await open({
        directory: true,
        title:
          scope === "historical"
            ? tool === "pip"
              ? "选择历史 site-packages"
              : "选择历史全局目录"
            : tool === "pip"
              ? "选择虚拟环境目录"
              : "选择项目目录",
      });
      if (typeof path === "string") {
        setDirectory(path);
        setResult(null);
        setError("");
        generation.current += 1;
        setBusy(false);
      }
    } catch (e) {
      setError(String(e));
    }
  };
  const rows = useMemo(
    () =>
      (result?.packages ?? []).filter((p) =>
        `${p.name} ${p.version ?? ""} ${p.path ?? ""}`
          .toLowerCase()
          .includes(filter.toLowerCase()),
      ),
    [result, filter],
  );
  const openPath = async (path: string) => {
    try {
      await api.openPath(path);
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <section aria-label="已安装包" className="space-y-4">
      <div className="flex flex-wrap items-end gap-3">
        <label className="text-xs">
          范围
          <select
            aria-label="范围"
            value={scope}
            onChange={(e) => setScope(e.target.value)}
            className="block mt-1 h-9 border rounded bg-white dark:bg-zinc-900 border-zinc-300 dark:border-zinc-700 px-2 text-sm"
          >
            <option value="global">
              {tool === "pip" ? "当前 Python 环境" : "全局包"}
            </option>
            {definition.projectQuery && (
              <option value="directory">
                {tool === "pip" ? "指定虚拟环境" : "项目直接依赖"}
              </option>
            )}
            <option value="historical">
              {tool === "pip" ? "历史 site-packages" : "历史全局目录"}
            </option>
          </select>
        </label>
        <Button
          loading={busy}
          disabled={scope !== "global" && !directory.trim()}
          onClick={load}
          title="刷新已安装包"
        >
          <RefreshCw className="size-4" />
          刷新
        </Button>
      </div>
      {scope === "historical" && (
        <GlobalSourceSelect
          tool={tool}
          onSelect={(path) => {
            setDirectory(path);
            setResult(null);
            setError("");
            generation.current += 1;
            setBusy(false);
          }}
        />
      )}
      {scope !== "global" && (
        <label className="block text-xs">
          {scope === "historical"
            ? tool === "pip"
              ? "历史 site-packages"
              : "历史全局目录"
            : tool === "pip"
              ? "虚拟环境目录"
              : "项目目录"}
          <div className="mt-1 flex gap-2">
            <input
              aria-label="查询目录"
              value={directory}
              onChange={(e) => {
                setDirectory(e.target.value);
                setResult(null);
                setError("");
                generation.current += 1;
                setBusy(false);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" && directory.trim()) void load();
              }}
              className="min-w-0 flex-1 border rounded p-2 bg-transparent border-zinc-300 dark:border-zinc-700 font-mono"
            />
            <Button
              title="选择查询目录"
              aria-label="选择查询目录"
              onClick={browse}
            >
              <FolderOpen className="size-4" />
            </Button>
          </div>
        </label>
      )}
      {error && (
        <p
          role="alert"
          className="text-sm text-red-600 dark:text-red-400 whitespace-pre-wrap break-all"
        >
          {error}
        </p>
      )}
      {busy && <Spinner />}
      {result && (
        <>
          <div className="text-xs text-zinc-500 break-all space-y-1">
            {result.executable ? (
              <p>
                启动器：
                <span className="font-mono selectable">
                  {result.executable}
                </span>
              </p>
            ) : (
              <p>来源：磁盘安装元数据</p>
            )}
            {result.source && (
              <p>
                目录：
                <span className="font-mono selectable">{result.source}</span>
              </p>
            )}
          </div>
          {scope === "historical" &&
            result.source &&
            result.packages.length > 0 &&
            onReinstall && (
              <Button onClick={() => onReinstall(result.source!)}>
                <Package className="size-4" />
                重装此目录的包
              </Button>
            )}
          {result.warnings.map((warning, i) => (
            <p
              role="alert"
              key={i}
              className="text-xs text-amber-700 dark:text-amber-300 whitespace-pre-wrap break-all"
            >
              {warning}
            </p>
          ))}
          <div className="flex flex-wrap items-center gap-3">
            <label className="flex min-w-0 flex-1 items-center gap-2 border-b border-zinc-300 dark:border-zinc-700 py-2">
              <Search className="size-4 shrink-0 text-zinc-400" />
              <input
                aria-label="搜索包"
                placeholder="搜索包名、版本、路径"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                className="min-w-0 flex-1 bg-transparent outline-none text-sm"
              />
            </label>
            <span className="text-xs tabular-nums">
              {rows.length} / {result.packages.length} 个包
            </span>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full table-fixed text-xs text-left">
              <thead className="border-b border-zinc-200 dark:border-zinc-700">
                <tr>
                  <th className="py-2 w-[28%]">包名</th>
                  <th className="py-2 w-[18%]">版本</th>
                  <th className="py-2">安装位置 / 状态</th>
                  <th className="w-10">
                    <span className="sr-only">操作</span>
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-zinc-200 dark:divide-zinc-800">
                {rows.map((p, i) => (
                  <tr key={`${p.name}-${p.path}-${i}`}>
                    <td className="py-3 pr-2 break-all font-mono selectable align-top">
                      {p.name}
                    </td>
                    <td className="py-3 pr-2 break-all font-mono selectable align-top">
                      {p.version ?? "未知"}
                    </td>
                    <td className="py-3 pr-2 break-all align-top">
                      <span className="font-mono selectable">
                        {p.path ?? "路径不可获取"}
                      </span>
                      {p.detail && (
                        <p className="text-amber-700 dark:text-amber-300 mt-1">
                          {p.detail}
                        </p>
                      )}
                    </td>
                    <td className="py-2 align-top">
                      {p.path && (
                        <Button
                          size="sm"
                          title={`打开 ${p.name} 的安装位置`}
                          aria-label={`打开 ${p.name} 的安装位置`}
                          onClick={() => openPath(p.path!)}
                        >
                          <FolderOpen className="size-3.5" />
                        </Button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {rows.length === 0 && (
            <p className="py-6 text-center text-sm text-zinc-500">
              {result.packages.length ? "没有匹配的包" : "未发现已安装的包"}
            </p>
          )}
        </>
      )}
    </section>
  );
}
