import { useEffect, useRef, useState } from "react";
import {
  RefreshCw,
  FolderOpen,
  Eraser,
  Wand2,
  Package,
  MoveRight,
} from "lucide-react";
import { PageShell } from "../components/layout/PageShell";
import { CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { Spinner } from "../components/ui/Empty";
import { ConfirmDialog } from "../components/ui/Modal";
import { api } from "../lib/api";
import { ToolMigration } from "../components/ToolMigration";
import { PipReinstall } from "../components/PipReinstall";
import { GlobalPackageReinstall } from "../components/GlobalPackageReinstall";
import { InstalledPackages } from "../components/InstalledPackages";
import { SelectionButton } from "../components/ui/SelectionButton";
import { toast } from "../lib/toast";
import { formatBytes, cn } from "../lib/format";
import type { CacheInfo, ToolConfig } from "../lib/types";
import { managers, managerDefinition } from "../lib/managers";
import { OldPackageScan } from "../components/OldPackageScan";

export function PackageManagers() {
  const [selectedTool, setSelectedTool] = useState("npm");
  const [view, setView] = useState<"installed" | "paths" | "scan">("installed");
  // 包管理器路径配置
  const [toolConfigs, setToolConfigs] = useState<ToolConfig[] | null>(null);
  const [toolBusy, setToolBusy] = useState<string | null>(null);
  const [configError, setConfigError] = useState("");
  const [configLoading, setConfigLoading] = useState(false);
  const configRequest = useRef(0);
  const [migrationTool, setMigrationTool] = useState<ToolConfig | null>(null);
  const [pipReinstallOpen, setPipReinstallOpen] = useState(false);
  const [globalReinstallTool, setGlobalReinstallTool] =
    useState<ToolConfig | null>(null);
  const [reinstallSource, setReinstallSource] = useState<string | undefined>();
  const [pipSource, setPipSource] = useState<string | undefined>();

  const openReinstall = (id: string, source?: string) => {
    const definition = managerDefinition(id);
    if (definition.reinstall === "venv") {
      setPipSource(source);
      setPipReinstallOpen(true);
    } else {
      const config = toolConfigs?.find((t) => t.tool === id) ?? {
        tool: id,
        name: definition.name,
        available: false,
        executablePath: null,
        error: null,
        globalPath: null,
        cachePath: null,
        defaultGlobal: null,
        defaultCache: null,
      };
      setReinstallSource(source);
      setGlobalReinstallTool(config);
    }
  };

  // 缓存
  const [caches, setCaches] = useState<CacheInfo[] | null>(null);
  const [cacheLoading, setCacheLoading] = useState(false);
  const [confirmClean, setConfirmClean] = useState<CacheInfo | null>(null);
  const [cleaning, setCleaning] = useState(false);

  const loadToolConfigs = async () => {
    const request = ++configRequest.current;
    setConfigLoading(true);
    setConfigError("");
    try {
      const configs = await api.getToolConfigs();
      if (request === configRequest.current) setToolConfigs(configs);
    } catch (e) {
      if (request === configRequest.current) {
        setConfigError(String(e));
        toast.error("读取包管理器配置失败", String(e));
      }
    } finally {
      if (request === configRequest.current) setConfigLoading(false);
    }
  };

  const loadCaches = async () => {
    setCacheLoading(true);
    try {
      setCaches(await api.getCaches());
    } catch (e) {
      toast.error("缓存检测失败", String(e));
    } finally {
      setCacheLoading(false);
    }
  };

  useEffect(() => {
    void loadToolConfigs();
  }, []);
  useEffect(() => {
    if (view === "paths" && !caches) void loadCaches();
  }, [view]);
  /** 应用单个工具的默认路径 */
  const applyTool = async (t: ToolConfig) => {
    setToolBusy(t.tool);
    try {
      const applied = await api.applyToolConfig(
        t.tool,
        t.defaultGlobal ?? null,
        t.defaultCache ?? null,
      );
      toast.success(`${t.name} 已配置`, applied.join("\n") || undefined);
    } catch (e) {
      toast.error(`${t.name} 配置失败`, String(e));
    } finally {
      setToolBusy(null);
      void loadToolConfigs();
      if (caches) void loadCaches();
    }
  };

  /** 一键:全部可用工具的路径指向 根目录\globals */
  const applyAllTools = async () => {
    if (!toolConfigs) return;
    const targets = toolConfigs.filter(
      (t) => t.available && (t.defaultGlobal || t.defaultCache),
    );
    if (targets.length === 0) {
      toast.info("没有可配置的工具", "未检测到支持路径配置的包管理器");
      return;
    }
    setToolBusy("__all__");
    let ok = 0;
    const failures: string[] = [];
    for (const t of targets) {
      try {
        await api.applyToolConfig(
          t.tool,
          t.defaultGlobal ?? null,
          t.defaultCache ?? null,
        );
        ok += 1;
      } catch (e) {
        failures.push(`${t.name}: ${String(e)}`);
      }
    }
    setToolBusy(null);
    if (failures.length)
      toast.error(`${failures.length} 个工具配置失败`, failures.join("\n"));
    if (ok > 0) {
      toast.success(
        `已配置 ${ok} 个工具`,
        "全局与缓存路径已指向 根目录\\globals",
      );
    }
    await loadToolConfigs();
    if (caches) void loadCaches();
  };

  const doClean = async () => {
    if (!confirmClean) return;
    setCleaning(true);
    try {
      const freed = await api.cleanCache(confirmClean.tool, confirmClean.path);
      toast.success("缓存已清理", `释放 ${formatBytes(freed)}`);
      setConfirmClean(null);
    } catch (e) {
      toast.error("清理失败", String(e));
    } finally {
      setCleaning(false);
      void loadCaches();
    }
  };

  return (
    <PageShell
      title="包管理器"
      actions={
        <Button
          size="sm"
          title="刷新包管理器配置"
          aria-label="刷新包管理器配置"
          onClick={loadToolConfigs}
          loading={configLoading}
          disabled={toolBusy !== null}
        >
          <RefreshCw className="size-3.5" />
          刷新
        </Button>
      }
    >
      <div
        role="group"
        aria-label="选择包管理器"
        className="flex gap-1.5 mb-4 flex-wrap"
      >
        {managers.map((definition) => {
          const tool = definition.id;
          const config = toolConfigs?.find((t) => t.tool === tool);
          const unknown = configLoading || !!configError || !toolConfigs;
          const status = unknown
            ? configLoading
              ? "检测中"
              : "检测结果未知"
            : config?.available
              ? "已安装且可用"
              : config?.executablePath
                ? "已发现，无法运行"
                : "未检测到";
          return (
            <SelectionButton
              key={tool}
              selected={selectedTool === tool}
              aria-label={`选择 ${tool}`}
              title={`${definition.name}：${status}`}
              onClick={() => setSelectedTool(tool)}
              indicator={
                unknown
                  ? "unknown"
                  : config?.available
                    ? "available"
                    : config?.executablePath
                      ? "unknown"
                      : "unavailable"
              }
              statusLabel={`${tool}：${status}`}
            >
              {definition.name}
            </SelectionButton>
          );
        })}
      </div>
      {configError && (
        <p role="alert" className="mb-4 text-xs text-red-600 break-all">
          {configError}
        </p>
      )}
      <div
        role="tablist"
        aria-label="包管理器视图"
        className="flex gap-4 border-b border-zinc-200 dark:border-zinc-800 mb-4"
      >
        {(
          [
            ["installed", "已安装包"],
            ["paths", "路径与迁移"],
            ["scan", "旧包扫描"],
          ] as const
        ).map(([id, title]) => (
          <button
            key={id}
            role="tab"
            aria-selected={view === id}
            onClick={() => setView(id)}
            className={cn(
              "py-2 text-sm border-b-2",
              view === id
                ? "border-emerald-600 text-emerald-700 dark:text-emerald-400"
                : "border-transparent text-zinc-500",
            )}
          >
            {title}
          </button>
        ))}
      </div>
      {view === "installed" && (
        <InstalledPackages
          tool={selectedTool}
          onReinstall={(source) => openReinstall(selectedTool, source)}
        />
      )}
      {view === "scan" && <OldPackageScan onReinstall={openReinstall} />}
      {globalReinstallTool && (
        <GlobalPackageReinstall
          tool={globalReinstallTool}
          initialSource={reinstallSource}
          onClose={() => {
            setGlobalReinstallTool(null);
            setReinstallSource(undefined);
          }}
        />
      )}
      {pipReinstallOpen && (
        <PipReinstall
          initialSource={pipSource}
          onClose={() => {
            setPipReinstallOpen(false);
            setPipSource(undefined);
          }}
        />
      )}
      <div className={view === "paths" ? "" : "hidden"}>
        {/* 包管理器路径配置 */}
        <section className="mt-5 border-y border-zinc-200 dark:border-zinc-800">
          <CardHeader
            title={
              <span className="flex items-center gap-1.5">
                <Package className="size-3.5 text-zinc-400" />
                包管理器路径
              </span>
            }
            actions={
              <>
                <Button
                  size="sm"
                  variant="primary"
                  onClick={applyAllTools}
                  loading={toolBusy === "__all__"}
                  disabled={toolBusy !== null || migrationTool !== null}
                >
                  <Wand2 className="size-3.5" />
                  一键配置全部
                </Button>
              </>
            }
          />
          {!toolConfigs && !configError ? (
            <Spinner />
          ) : (
            <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
              {(toolConfigs ?? [])
                .filter((t) => t.tool === selectedTool)
                .map((t) => (
                  <div
                    key={t.tool}
                    className="flex flex-wrap items-center gap-3 px-4 py-3"
                  >
                    <div className="w-28 shrink-0">
                      <div className="text-sm font-semibold">{t.name}</div>
                      {t.executablePath && (
                        <div
                          className="text-[10px] text-zinc-500 font-mono truncate"
                          title={t.executablePath}
                        >
                          {t.executablePath}
                        </div>
                      )}
                      {t.error && (
                        <div className="text-[10px] text-red-600 dark:text-red-400">
                          {t.error}
                        </div>
                      )}
                      {!t.available && (
                        <div className="text-[10px] text-zinc-400">不可用</div>
                      )}
                    </div>
                    <div className="min-w-0 w-full lg:w-auto lg:flex-1 grid grid-cols-1 sm:grid-cols-2 gap-x-4 gap-y-1">
                      <div>
                        <div className="text-[10px] text-zinc-400">
                          全局安装
                        </div>
                        <div
                          className="text-xs font-mono truncate selectable"
                          title={t.globalPath ?? "默认(用户目录)"}
                        >
                          {t.globalPath ?? (
                            <span className="text-zinc-400">
                              {managerDefinition(t.tool).globalConfig
                                ? "未自定义"
                                : "不适用"}
                            </span>
                          )}
                        </div>
                      </div>
                      <div>
                        <div className="text-[10px] text-zinc-400">缓存</div>
                        <div
                          className="text-xs font-mono truncate selectable"
                          title={t.cachePath ?? "默认(用户目录)"}
                        >
                          {t.cachePath ?? (
                            <span className="text-zinc-400">未自定义</span>
                          )}
                        </div>
                      </div>
                    </div>
                    <Button
                      size="sm"
                      disabled={
                        !t.available ||
                        (!t.defaultGlobal && !t.defaultCache) ||
                        toolBusy !== null ||
                        migrationTool !== null
                      }
                      loading={toolBusy === t.tool}
                      title={
                        t.defaultGlobal
                          ? `设为 ${t.defaultGlobal} / ${t.defaultCache}`
                          : undefined
                      }
                      onClick={() => applyTool(t)}
                    >
                      <Wand2 className="size-3.5" />
                      仅设置路径
                    </Button>
                    <Button
                      size="sm"
                      disabled={toolBusy !== null || migrationTool !== null}
                      title="迁移原目录中的全局包或缓存"
                      onClick={() => setMigrationTool(t)}
                    >
                      <MoveRight className="size-3.5" />
                      迁移数据
                    </Button>
                    <Button size="sm" onClick={() => openReinstall(t.tool)}>
                      <Package className="size-3.5" />
                      重装旧包
                    </Button>
                  </div>
                ))}
            </div>
          )}
        </section>

        {/* 缓存管理 */}
        {migrationTool && (
          <ToolMigration
            tool={migrationTool}
            onClose={() => setMigrationTool(null)}
            onComplete={() => {
              loadToolConfigs();
              loadCaches();
            }}
          />
        )}
        <section className="mt-5 border-y border-zinc-200 dark:border-zinc-800">
          <CardHeader
            title="开发缓存"
            actions={
              <Button size="sm" onClick={loadCaches} loading={cacheLoading}>
                <RefreshCw className="size-3.5" />
                重新统计
              </Button>
            }
          />
          {cacheLoading && !caches ? (
            <Spinner />
          ) : (
            <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
              {(caches ?? []).map((c) => (
                <div key={c.tool} className="flex items-center gap-3 px-4 py-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium">{c.name}</span>
                      {!c.exists && <Badge>未检测到</Badge>}
                    </div>
                    <div
                      className="mt-0.5 text-xs text-zinc-400 truncate selectable"
                      title={c.path}
                    >
                      {c.path}
                    </div>
                  </div>
                  <span
                    className={cn(
                      "text-sm font-semibold tabular-nums",
                      c.exists
                        ? "text-zinc-700 dark:text-zinc-200"
                        : "text-zinc-400",
                    )}
                  >
                    {c.exists ? formatBytes(c.sizeBytes) : "—"}
                    {c.exists && !c.sizeComplete && (
                      <span
                        className="ml-1 text-amber-600"
                        title={c.sizeDetail ?? "统计不完整"}
                      >
                        约
                      </span>
                    )}
                  </span>
                  <div className="flex items-center gap-1.5">
                    <Button
                      size="sm"
                      title={`打开 ${c.name} 缓存目录`}
                      aria-label={`打开 ${c.name} 缓存目录`}
                      onClick={() =>
                        api
                          .openPath(c.path)
                          .catch(() => toast.error("打开失败", "目录不存在"))
                      }
                    >
                      <FolderOpen className="size-3.5" />
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      className="text-red-500 hover:bg-red-50 dark:hover:bg-red-950 disabled:hidden"
                      disabled={!c.exists || (c.sizeBytes ?? 0) === 0}
                      onClick={() => setConfirmClean(c)}
                    >
                      <Eraser className="size-3.5" />
                      清理
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>

        <ConfirmDialog
          open={confirmClean !== null}
          onClose={() => setConfirmClean(null)}
          onConfirm={doClean}
          danger
          confirmText="清理"
          loading={cleaning}
          title={`清理 ${confirmClean?.name ?? ""}`}
          message={
            <>
              <p>
                将删除 <b className="selectable">{confirmClean?.path}</b>{" "}
                下的全部内容 (释放约{" "}
                {formatBytes(confirmClean?.sizeBytes ?? null)})。
              </p>
              <p className="mt-1.5 text-xs text-zinc-400">
                缓存清理是安全的,下次安装依赖时会自动重新下载。
              </p>
            </>
          }
        />
      </div>
    </PageShell>
  );
}
