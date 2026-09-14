import { useMemo, useState } from "react";
import { ChevronDown, FolderOpen, Import, Search } from "lucide-react";
import type { ExternalEnv } from "../lib/types";
import { ENV_TYPE_META } from "../lib/types";
import { groupScanResults, scanHealthy, scanMatches, SCAN_CATEGORIES, type ScanCategory } from "../lib/scanGroups";
import { api } from "../lib/api";
import { toast } from "../lib/toast";
import { Badge } from "./ui/Badge";
import { Button } from "./ui/Button";
import { Empty } from "./ui/Empty";
import { SelectionButton } from "./ui/SelectionButton";

interface Props {
  rows: ExternalEnv[];
  canIntegrate: boolean;
  integrating: string | null;
  onIntegrate: (env: ExternalEnv) => void;
}

export function ScanResults({ rows, canIntegrate, integrating, onIntegrate }: Props) {
  const [category, setCategory] = useState<ScanCategory | "全部">("全部");
  const [status, setStatus] = useState("all");
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const groups = useMemo(() => groupScanResults(rows), [rows]);
  const healthyTools = groups.filter(g => g.entries.some(scanHealthy)).length;
  const failedEntries = rows.filter(e => !scanHealthy(e)).length;
  const filtered = groups.filter(g => category === "全部" || g.category === category).map(g => ({ ...g, entries: g.entries.filter(e =>
    scanMatches(e, query) && (status === "all" || (status === "healthy" ? scanHealthy(e) : !scanHealthy(e))),
  ) })).filter(g => g.entries.length);
  const filtering = !!query.trim() || status !== "all";
  const toggle = (tool: string) => setExpanded(previous => {
    const next = new Set(previous);
    if (next.has(tool)) next.delete(tool); else next.add(tool);
    return next;
  });

  return <div aria-label="扫描结果" className="px-4 pb-4">
    <div role="status" className="flex flex-wrap gap-x-4 gap-y-1 py-3 text-xs text-zinc-500">
      <span>发现 <b className="text-zinc-800 dark:text-zinc-100">{groups.length}</b> 种工具</span>
      <span>{healthyTools} 种可用</span><span>{rows.length} 个入口</span>
      {failedEntries > 0 && <span className="text-amber-700 dark:text-amber-400">{failedEntries} 个入口待检查</span>}
    </div>
    <div role="group" aria-label="扫描分类" className="flex flex-wrap gap-1.5 mb-3">
      {(["全部", ...SCAN_CATEGORIES] as const).map(name => <SelectionButton key={name} selected={category === name} onClick={() => setCategory(name)} count={name === "全部" ? groups.length : groups.filter(g => g.category === name).length}>{name}</SelectionButton>)}
    </div>
    <div className="flex flex-wrap items-center gap-2 mb-3">
      <label className="flex w-full sm:w-auto min-w-0 sm:flex-1 items-center gap-2 h-8 border rounded border-zinc-200 dark:border-zinc-700 px-2"><Search className="size-3.5 shrink-0 text-zinc-400" /><input aria-label="筛选扫描结果" placeholder="工具、版本或路径" value={query} onChange={e => setQuery(e.target.value)} className="w-full min-w-0 bg-transparent text-xs outline-none" /></label>
      <select aria-label="扫描状态" value={status} onChange={e => setStatus(e.target.value)} className="h-8 text-xs border rounded px-2 bg-white dark:bg-zinc-900 border-zinc-200 dark:border-zinc-700"><option value="all">全部状态</option><option value="healthy">可用入口</option><option value="attention">待检查入口</option></select>
      <span className="text-xs text-zinc-500">{filtered.length} 种 / {filtered.reduce((count, group) => count + group.entries.length, 0)} 个入口</span>
    </div>
    {filtered.length === 0 && <Empty title="没有匹配结果" />}
    {SCAN_CATEGORIES.map(name => {
      const matches = filtered.filter(g => g.category === name);
      if (!matches.length) return null;
      return <section key={name} aria-label={name} className="mb-4 last:mb-0">
        <h3 className="py-2 text-xs font-medium text-zinc-500 border-b border-zinc-200 dark:border-zinc-800">{name}</h3>
        {matches.map(group => {
          const open = filtering || expanded.has(group.tool);
          const good = group.entries.filter(scanHealthy);
          const errors = group.entries.length - good.length;
          const versions = [...new Set(good.map(e => e.version!))];
          const preferred = group.entries.find(e => e.isPreferred);
          return <div key={group.tool} className="border-b border-zinc-100 dark:border-zinc-800" data-scan-tool={group.tool}>
            <button type="button" aria-label={`${open ? "收起" : "展开"} ${group.tool} 扫描结果`} aria-expanded={open} onClick={() => toggle(group.tool)} disabled={filtering} className="w-full flex items-start gap-2 py-3 text-left cursor-pointer disabled:cursor-default">
              <ChevronDown className={`size-4 mt-0.5 shrink-0 text-zinc-400 transition-transform ${open ? "" : "-rotate-90"}`} />
              <span className="min-w-0 flex-1">
                <span className="flex flex-wrap items-center gap-2 text-xs"><strong className="text-sm">{group.tool}</strong><span role="img" aria-label={good.length ? "存在可用入口" : "无可用入口"} className={`size-1.5 rounded-full ${good.length ? "bg-emerald-500" : "bg-amber-500"}`} /><span className="text-zinc-500">{group.entries.length} 个入口</span>{errors > 0 && <span className="text-amber-700 dark:text-amber-400">{errors} 项待检查</span>}</span>
                <span className="block mt-1 text-xs text-zinc-600 dark:text-zinc-300 break-all">{versions.slice(0, 3).join(" / ") || "未取得可用版本"}{versions.length > 3 ? ` 等 ${versions.length} 个版本` : ""}</span>
                {preferred?.path && <span className="block mt-1 text-xs text-zinc-500 truncate" title={preferred.path}>PATH 首选：{preferred.path}</span>}
              </span>
            </button>
            {open && <div className="pl-2 sm:pl-6 pb-2 divide-y divide-zinc-100 dark:divide-zinc-800">
              {group.entries.map((env, index) => <div key={`${env.path}:${env.command}:${index}`} className="py-3 text-xs">
                <div className="flex flex-wrap items-center gap-2 mb-1.5">
                  <span className="font-mono break-all">{env.version ?? "版本未知"}</span>
                  <Badge color={scanHealthy(env) ? "green" : "red"}>{scanHealthy(env) ? "可用" : "待检查"}</Badge>
                  {env.isPreferred && <span title="本次系统与用户 PATH 中优先命中；终端别名及激活的虚拟环境可能不同"><Badge color="green">PATH 首选</Badge></span>}
                  <Badge>{env.source}</Badge>
                  {env.tool === "pip (Python 模块)" && <Badge color="blue">Python 模块</Badge>}
                </div>
                <div className="font-mono break-all selectable text-zinc-600 dark:text-zinc-300">{env.path ?? "路径未知"}</div>
                {env.error && <p className="mt-2 text-red-600 dark:text-red-400 whitespace-pre-wrap break-all">{env.error}</p>}
                <div className="flex flex-wrap items-center gap-2 mt-2">
                  <details className="min-w-0 flex-1 text-zinc-500">
                    <summary className="cursor-pointer">探测详情</summary>
                    <dl className="mt-2 space-y-1 break-all"><div><dt className="inline">命令：</dt><dd className="inline font-mono selectable">{env.command}</dd></div>{env.installRoot && <div><dt className="inline">安装根目录：</dt><dd className="inline selectable">{env.installRoot}</dd></div>}{env.identityPath && <div><dt className="inline">实际文件：</dt><dd className="inline selectable">{env.identityPath}</dd></div>}</dl>
                  </details>
                  {env.envType && canIntegrate && scanHealthy(env) && <Button size="sm" variant="primary" loading={integrating === env.path} disabled={integrating !== null} onClick={() => onIntegrate(env)} title={`以链接方式注册到 ${ENV_TYPE_META[env.envType].label}`}><Import className="size-3.5" />纳入管理</Button>}
                  {env.path && <Button size="sm" title="打开所在目录" aria-label={`打开 ${env.tool} 所在目录`} onClick={() => api.openPath(env.path!.replace(/[\\/][^\\/]+$/, "")).catch(e => toast.error("打开失败", String(e)))}><FolderOpen className="size-3.5" /></Button>}
                </div>
              </div>)}
            </div>}
          </div>;
        })}
      </section>;
    })}
  </div>;
}
