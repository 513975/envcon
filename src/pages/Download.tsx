import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  RefreshCw,
  Search,
  Download as DownloadIcon,
  X,
  Pencil,
  Zap,
  Globe,
  Server,
} from "lucide-react";
import { PageShell } from "../components/layout/PageShell";
import { Card } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { ProgressBar } from "../components/ui/ProgressBar";
import { Spinner, Empty } from "../components/ui/Empty";
import { EnvTypeBadge } from "../components/EnvTypeBadge";
import { api } from "../lib/api";
import { toast } from "../lib/toast";
import { useInstallStore } from "../lib/installStore";
import { useAppStore } from "../lib/store";
import { formatBytes, formatSpeed, formatDate, cn } from "../lib/format";
import {
  ALL_ENV_TYPES,
  ENV_TYPE_META,
  type EnvType,
  type VersionInfo,
  type SourceInfo,
} from "../lib/types";

/** 默认目标文件夹名 */
function defaultTargetName(envType: EnvType, version: string): string {
  const v = version.replace("+", "_");
  switch (envType) {
    case "jdk":
      return `temurin-${v.split(".")[0]}`;
    case "python":
      return `python-${v}`;
    case "node":
      return `node-${v.split(".")[0]}`;
    case "go":
      return `go-${v}`;
    case "rust":
      return `rust-${v}`;
    case "maven":
      return `maven-${v}`;
    case "gradle":
      return `gradle-${v}`;
    case "php":
      return `php-${v}`;
    case "llvm":
      return `llvm-${v}`;
    case "zig":
      return `zig-${v}`;
    case "deno":
      return `deno-${v}`;
    case "bun":
      return `bun-${v}`;
    case "git":
      return `git-${v.split(".")[0]}`;
    case "gh":
      return `gh-${v}`;
    case "mingw":
      return `mingw-${v.split("-")[0]}`;
    case "adb":
      return `platform-tools-${v}`;
  }
}

export function Download() {
  const [envType, setEnvType] = useState<EnvType>("node");
  const [source, setSource] = useState<string>("mirror");
  const [sourceList, setSourceList] = useState<{
    envType: EnvType; items: SourceInfo[];
  } | null>(null);
  const sources = sourceList?.envType === envType ? sourceList.items : [];
  const sourcesReady = sourceList?.envType === envType;
  const [versionList, setVersionList] = useState<{
    envType: EnvType; source: string; items: VersionInfo[];
  } | null>(null);
  const versions = versionList?.envType === envType && versionList.source === source
    ? versionList.items : null;
  const requestId = useRef(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [visibleCount, setVisibleCount] = useState(60);
  useEffect(() => { setVisibleCount(60); }, [envType, source, search]);
  const [editing, setEditing] = useState<string | null>(null); // 正在编辑目标名的 version
  const [targetNames, setTargetNames] = useState<Record<string, string>>({});
  const tasks = useInstallStore((s) => s.tasks);
  const refreshOverview = useAppStore((s) => s.refreshOverview);
  const overviewVersion = useAppStore((s) => s.overviewVersion);

  const load = useCallback(async (t: EnvType, src: string) => {
    const id = ++requestId.current;
    setLoading(true);
    setError(null);
    setVersionList(null);
    try {
      const items = await api.listVersions(t, src);
      if (id === requestId.current) setVersionList({ envType: t, source: src, items });
    } catch (e) {
      if (id === requestId.current) setError(String(e));
    } finally {
      if (id === requestId.current) setLoading(false);
    }
  }, []);

  // 切换类型:加载该类型可用源,默认取第一个
  useEffect(() => {
    let canceled = false;
    (async () => {
      try {
        const list = await api.getSources(envType);
        if (canceled) return;
        setSourceList({ envType, items: list });
        const def = list.find((s) => s.id === "official") ?? list[0];
        setSource(def?.id ?? "mirror");
      } catch {
        if (canceled) return;
        setSourceList({ envType, items: [] });
        setSource("mirror");
      }
    })();
    setSearch("");
    setTargetNames({});
    return () => {
      canceled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [envType]);

  // 类型或源变化时加载版本列表
  useEffect(() => {
    if (sourcesReady) void load(envType, source);
    return () => { requestId.current += 1; };
  }, [envType, source, sourcesReady, load]);

  const doneToastFired = useMemo(() => new Set<number>(), [envType]);
  useEffect(() => {
    // 任务完成/失败 toast(只触发一次)
    for (const t of Object.values(tasks)) {
      if (t.status === "done" && !doneToastFired.has(t.id)) {
        doneToastFired.add(t.id);
        toast.success("安装完成", `${t.targetName}(${t.version})已就绪`);
        refreshOverview();
        setTimeout(() => useInstallStore.getState().remove(t.id), 3000);
      }
      if (t.status === "error" && !doneToastFired.has(t.id)) {
        doneToastFired.add(t.id);
        toast.error("安装失败", `${t.targetName}: ${t.message}`);
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tasks, doneToastFired, overviewVersion]);

  const filtered = (versions ?? []).filter((v) =>
    v.version.toLowerCase().includes(search.toLowerCase()),
  );

  const start = async (v: VersionInfo) => {
    if (!sourcesReady || loading || !versions?.includes(v)) return;
    const name = targetNames[v.version] ?? defaultTargetName(envType, v.version);
    try {
      await api.startInstall(envType, v.version, name, source, v.url);
      toast.info("已开始下载", `${ENV_TYPE_META[envType].label} ${v.version} → ${name}`);
    } catch (e) {
      toast.error("启动失败", String(e));
    }
  };

  const activeTasks = Object.values(tasks).filter(
    (t) => t.status === "downloading" || t.status === "installing",
  );

  return (
    <PageShell
      title="下载中心"
      desc="镜像/官方双源下载,支持自定义安装目录名"
      actions={
        <Button size="sm" onClick={() => load(envType, source)} loading={loading || !sourcesReady}>
          <RefreshCw className="size-3.5" />
          刷新
        </Button>
      }
    >
      {/* 类型选择:全名称按钮 */}
      <div className="grid grid-cols-3 sm:grid-cols-4 lg:grid-cols-5 xl:grid-cols-8 gap-2 mb-4">
        {ALL_ENV_TYPES.map((t) => {
          const meta = ENV_TYPE_META[t];
          return (
            <button
              key={t}
              onClick={() => setEnvType(t)}
              className={cn(
                "h-9 rounded-lg border text-xs font-semibold transition-colors cursor-pointer",
                envType === t
                  ? cn("border-transparent text-zinc-900 dark:text-zinc-50", meta.color)
                  : "border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 text-zinc-600 dark:text-zinc-300 hover:border-zinc-300 dark:hover:border-zinc-700",
              )}
            >
              {meta.label}
            </button>
          );
        })}
      </div>

      {/* 进行中的任务 */}
      {activeTasks.length > 0 && (
        <Card className="mb-4 divide-y divide-zinc-100 dark:divide-zinc-800">
          {activeTasks.map((t) => (
            <div key={t.id} className="px-4 py-3">
              <div className="flex items-center justify-between gap-3">
                <div className="flex items-center gap-2 min-w-0">
                  <EnvTypeBadge type={t.envType} size="sm" />
                  <span className="text-sm font-medium">{t.targetName}</span>
                  <span className="text-xs text-zinc-400">{t.version}</span>
                </div>
                <Button
                  size="sm"
                  variant="ghost"
                  className="text-red-500"
                  disabled={t.status === "installing" && !t.cancelable}
                  title={t.status === "installing" && !t.cancelable ? "安装器正在执行，当前阶段无法取消" : "取消安装"}
                  onClick={() =>
                    api.cancelInstall(t.id).catch((e) => toast.error("取消失败", String(e)))
                  }
                >
                  <X className="size-3.5" />
                  取消
                </Button>
              </div>
              <div className="mt-2">
                {t.status === "downloading" ? (
                  <>
                    <ProgressBar
                      value={t.total ? t.downloaded / t.total : 0}
                      indeterminate={!t.total}
                    />
                    <div className="mt-1 flex justify-between text-xs text-zinc-500">
                      <span>
                        {formatBytes(t.downloaded)}
                        {t.total ? ` / ${formatBytes(t.total)}` : ""}
                      </span>
                      <span>{t.speed > 0 ? formatSpeed(t.speed) : "连接中…"}</span>
                    </div>
                  </>
                ) : (
                  <>
                    <ProgressBar value={1} indeterminate />
                    <div className="mt-1 text-xs text-zinc-500 truncate">{t.message}</div>
                  </>
                )}
              </div>
            </div>
          ))}
        </Card>
      )}

      {/* 版本列表 */}
      <Card>
        <div className="px-4 py-3 border-b border-zinc-100 dark:border-zinc-800 flex items-center gap-3">
          <div className="relative flex-1 max-w-64">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-zinc-400" />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder={`搜索 ${ENV_TYPE_META[envType].label} 版本…`}
              className="h-8 w-full rounded-lg border border-zinc-200 dark:border-zinc-700 bg-transparent pl-8 pr-3 text-xs outline-none focus:border-emerald-500"
            />
          </div>
          <span className="text-xs text-zinc-400">
            {versions ? `${filtered.length} 个版本` : ""}
          </span>

          {/* 下载源切换 */}
          {sources.length > 1 && (
            <div className="flex items-center gap-1 rounded-lg bg-zinc-100 dark:bg-zinc-800 p-0.5">
              {sources.map((s) => (
                <button
                  key={s.id}
                  onClick={() => setSource(s.id)}
                  className={cn(
                    "h-7 px-2.5 rounded-md text-xs font-medium transition-colors inline-flex items-center gap-1 cursor-pointer",
                    source === s.id
                      ? "bg-white dark:bg-zinc-900 shadow-sm text-zinc-800 dark:text-zinc-100"
                      : "text-zinc-500 hover:text-zinc-700 dark:hover:text-zinc-300",
                  )}
                  title={s.id === "mirror" ? "镜像可能存在同步延迟，可切换官方源核对" : "查询上游发布列表（仅展示受支持的 Windows 构建）"}
                >
                  {s.id === "mirror" ? (
                    <Server className="size-3" />
                  ) : (
                    <Globe className="size-3" />
                  )}
                  {s.label}
                </button>
              ))}
            </div>
          )}
        </div>

        <div className="px-4 py-2 text-xs text-zinc-500 space-y-1" role="status">
          <p>仅列出当前适配器支持的 Windows x64 版本。镜像可能滞后；刷新会重新联网查询。</p>
          {envType === "jdk" && <p>{source === "mirror" ? "镜像列出常用 LTS 主版本的最新构建；更多主版本请切换官方源。" : "官方列出可用主版本的最新构建，不包含每个历史补丁。"}</p>}
          {envType === "rust" && <p>Rust 按 stable / beta / nightly 渠道安装，由 rustup 获取对应最新版本。</p>}
          {["llvm", "deno", "bun", "git", "gh", "mingw"].includes(envType) && <p>GitHub 源查询最近 100 个发布记录后筛选兼容安装包，不代表完整历史归档。</p>}
          {envType === "adb" && <p>Android SDK Platform Tools，包含 ADB 和 Fastboot。仅列出 Google 当前索引提供的 Windows 稳定版，不代表完整历史版本。使用需遵守 Android SDK 许可条款。</p>}
          {envType === "python" && <p>Python 列表来自版本目录；仅源码发布的安全维护版本可能没有 Windows 安装器。</p>}
          {envType === "maven" && <p>Maven 仅适配 3.x；官方当前分发目录不包含全部历史版本。</p>}
          {versions?.some(v => v.note?.includes("离线缓存")) && <p role="alert" className="text-amber-700 dark:text-amber-400">在线查询失败，正在展示离线缓存，版本可能过期。请检查网络后刷新或切换源。</p>}
        </div>
        {loading || !sourcesReady ? (
          <Spinner />
        ) : error ? (
          <Empty
            title="版本列表获取失败"
            desc={error}
            action={
              <Button size="sm" variant="primary" onClick={() => load(envType, source)}>
                重试
              </Button>
            }
          />
        ) : filtered.length === 0 ? (
          <Empty title="没有匹配的版本" />
        ) : (
          <div className="max-h-[420px] overflow-y-auto divide-y divide-zinc-50 dark:divide-zinc-800/50">
            {filtered.slice(0, visibleCount).map((v) => {
              const name = targetNames[v.version] ?? defaultTargetName(envType, v.version);
              return (
                <div
                  key={v.version}
                  className="flex items-center gap-3 px-4 py-2.5 hover:bg-zinc-50 dark:hover:bg-zinc-800/50 transition-colors"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-sm font-medium text-zinc-800 dark:text-zinc-100">
                        {v.version}
                      </span>
                      {v.note && (
                        <Badge color={v.note === "LTS" ? "green" : "gray"}>{v.note}</Badge>
                      )}
                    </div>
                    <div className="mt-0.5 flex items-center gap-3 text-[11px] text-zinc-400">
                      {v.date && <span>{formatDate(v.date)}</span>}
                      {v.sizeBytes && <span>{formatBytes(v.sizeBytes)}</span>}
                    </div>
                  </div>

                  {/* 目标名(可编辑) */}
                  {editing === v.version ? (
                    <input
                      autoFocus
                      value={name}
                      onChange={(e) =>
                        setTargetNames((m) => ({ ...m, [v.version]: e.target.value }))
                      }
                      onBlur={() => setEditing(null)}
                      onKeyDown={(e) => e.key === "Enter" && setEditing(null)}
                      className="h-7 w-40 rounded-lg border border-emerald-500 bg-white dark:bg-zinc-800 px-2 text-xs outline-none"
                    />
                  ) : (
                    <button
                      onClick={() => setEditing(v.version)}
                      className="flex items-center gap-1 text-xs text-zinc-400 hover:text-emerald-600 cursor-pointer"
                      title="点击修改安装目录名"
                    >
                      <span className="max-w-36 truncate font-mono">{name}</span>
                      <Pencil className="size-3" />
                    </button>
                  )}

                  <Button size="sm" variant="primary" onClick={() => start(v)}>
                    <DownloadIcon className="size-3.5" />
                    下载
                  </Button>
                </div>
              );
            })}
            {visibleCount < filtered.length && <div className="p-3 text-center"><Button onClick={() => setVisibleCount(n => n + 60)}>显示更多（已显示 {Math.min(visibleCount, filtered.length)} / {filtered.length}）</Button></div>}
          </div>
        )}
      </Card>

      <div className="mt-4 flex items-start gap-2 text-xs text-zinc-400 px-1">
        <Zap className="size-3.5 mt-0.5 shrink-0" />
        <p>
          安装位置为 根目录/envs/{ENV_TYPE_META[envType].folder}/{`<目标名>`},安装完成后可在环境管理页设为当前版本。
        </p>
      </div>
    </PageShell>
  );
}
