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
import { useScanResult } from "../lib/scanClient";
import { ScanStatus } from "../components/ScanStatus";
import { DirectorySize } from "../components/DirectorySize";
import { ToolMigration } from "../components/ToolMigration";
import { PipReinstall } from "../components/PipReinstall";
import { GlobalPackageReinstall } from "../components/GlobalPackageReinstall";
import { InstalledPackages } from "../components/InstalledPackages";
import { SelectionButton } from "../components/ui/SelectionButton";
import { toast } from "../lib/toast";
import { formatBytes, cn } from "../lib/format";
import type { CacheInfo, PackageManagerBatchResult, PackageManagerUpdateBatchPreview, PackageManagerUpdatePreview, PackageManagerUpdateRecord, ToolConfig, ToolConfigBatchPreview, ToolConfigPreview, UvLegacyLayout } from "../lib/types";
import { managers, managerDefinition } from "../lib/managers";
import { OldPackageScan } from "../components/OldPackageScan";

const updateRecordStatus: Record<string, string> = {
  planned: "已开始但未确认结果",
  completed: "命令完成",
  unverified: "已执行但版本未确认",
  failed: "命令失败",
};

const batchOutcomeLabel: Record<string, string> = {
  updated: "版本已变化",
  unchanged: "版本未变化",
  unverified: "待确认",
  failed: "失败",
  skipped: "跳过",
};

export function PackageManagers() {
  const [selectedTool, setSelectedTool] = useState("npm");
  const [view, setView] = useState<"installed" | "paths" | "scan">("installed");
  // 包管理器路径配置
  const [toolConfigs, setToolConfigs] = useState<ToolConfig[] | null>(null);
  const [toolBusy, setToolBusy] = useState<string | null>(null);
  const [updating, setUpdating] = useState(false);
  const [updatePreview, setUpdatePreview] =
    useState<PackageManagerUpdatePreview | null>(null);
  const [updateRecord, setUpdateRecord] =
    useState<PackageManagerUpdateRecord | null>(null);
  const [configError, setConfigError] = useState("");
  const [configLoading, setConfigLoading] = useState(false);
  const configRequest = useRef(0);
  const [uvLayout, setUvLayout] = useState<UvLegacyLayout | null>(null);
  const [configPreview, setConfigPreview] = useState<{
    config: ToolConfig;
    preview: ToolConfigPreview;
  } | null>(null);
  const [applyingConfig, setApplyingConfig] = useState(false);
  const [batchPreview, setBatchPreview] =
    useState<ToolConfigBatchPreview | null>(null);
  // 包管理器批量更新
  const [updateBatch, setUpdateBatch] =
    useState<PackageManagerUpdateBatchPreview | null>(null);
  const [batchUpdateResult, setBatchUpdateResult] =
    useState<PackageManagerBatchResult | null>(null);
  const [batchUpdating, setBatchUpdating] = useState(false);
  const [uvRecoveryOpen, setUvRecoveryOpen] = useState(false);
  const [recoveringUv, setRecoveringUv] = useState(false);
  const uvLayoutRequest = useRef(0);
  const [migrationTool, setMigrationTool] = useState<ToolConfig | null>(null);
  const [pipReinstallOpen, setPipReinstallOpen] = useState(false);
  const [globalReinstallTool, setGlobalReinstallTool] =
    useState<ToolConfig | null>(null);
  const [reinstallSource, setReinstallSource] = useState<string | undefined>();
  const [pipSource, setPipSource] = useState<string | undefined>();
  const selectedConfig = toolConfigs?.find((t) => t.tool === selectedTool);
  const selectedDefinition = managerDefinition(selectedTool);
  const selectedVersionStatus = configError
    ? "检测失败"
    : configLoading || !toolConfigs
      ? "检测中"
      : selectedConfig?.available
        ? selectedConfig.version ?? "版本无法识别"
        : selectedConfig?.executablePath
          ? "已发现，但无法运行"
          : "未检测到";

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
        version: null,
        executablePath: null,
        error: null,
        globalPath: null,
        cachePath: null,
        defaultGlobal: null,
        defaultCache: null,
        commandPath: null,
        commandPathSource: null,
        layoutWarning: null,
        selfUpdateBlocker: null,
      };
      setReinstallSource(source);
      setGlobalReinstallTool(config);
    }
  };

  // 缓存
  const [caches, setCaches] = useState<CacheInfo[] | null>(null);
  useScanResult<ToolConfig[]>("tools", [], setToolConfigs);
  useScanResult<CacheInfo[]>("caches", [], setCaches);
  const [cacheLoading, setCacheLoading] = useState(false);
  const [confirmClean, setConfirmClean] = useState<CacheInfo | null>(null);
  const [cleaning, setCleaning] = useState(false);

  const loadToolConfigs = async (force = false) => {
    const request = ++configRequest.current;
    setConfigLoading(true);
    setConfigError("");
    try {
      const configs = await api.getToolConfigs(force);
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

  const loadCaches = async (force = false) => {
    setCacheLoading(true);
    try {
      setCaches(await api.getCaches(force));
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
  useEffect(() => {
    const config = toolConfigs?.find((t) => t.tool === "uv");
    const path = config?.globalPath ?? config?.defaultGlobal;
    const request = ++uvLayoutRequest.current;
    if (selectedTool !== "uv" || !config?.layoutWarning || !path) {
      setUvLayout(null);
      setUvRecoveryOpen(false);
      return;
    }
    api
      .inspectUvLayout(path)
      .then((layout) => {
        if (request === uvLayoutRequest.current) setUvLayout(layout);
      })
      .catch(() => {
        if (request === uvLayoutRequest.current) setUvLayout(null);
      });
  }, [selectedTool, toolConfigs]);
  /** 先只读预览差异，用户确认后才写入 */
  const openConfigPreview = async (t: ToolConfig) => {
    setToolBusy(t.tool);
    try {
      const preview = await api.previewToolConfig(
        t.tool,
        t.defaultGlobal ?? null,
        t.defaultCache ?? null,
      );
      setConfigPreview({ config: t, preview });
    } catch (e) {
      toast.error(`${t.name} 预览失败`, String(e));
    } finally {
      setToolBusy(null);
    }
  };

  const applyPreviewedConfig = async () => {
    if (!configPreview) return;
    const { config: t } = configPreview;
    setApplyingConfig(true);
    try {
      const applied = await api.applyToolConfig(
        t.tool,
        t.defaultGlobal ?? null,
        t.defaultCache ?? null,
      );
      toast.success(`${t.name} 已配置`, applied.join("\n") || undefined);
      setConfigPreview(null);
    } catch (e) {
      toast.error(`${t.name} 配置失败`, String(e));
    } finally {
      setApplyingConfig(false);
      void loadToolConfigs();
      if (caches) void loadCaches();
    }
  };

  const openUvRecovery = async () => {
    const config = toolConfigs?.find((t) => t.tool === "uv");
    const path = config?.globalPath ?? config?.defaultGlobal;
    if (!config?.executablePath || !path) {
      toast.error(
        "无法启动 uv 恢复",
        "未检测到 uv 可执行文件或工具目录",
      );
      return;
    }
    try {
      const layout = await api.inspectUvLayout(path);
      setUvLayout(layout);
      setUvRecoveryOpen(true);
    } catch (e) {
      toast.error("读取 uv 布局失败", String(e));
    }
  };

  const doRecoverUv = async () => {
    const config = toolConfigs?.find((t) => t.tool === "uv");
    if (!uvLayout || !config?.executablePath) return;
    setRecoveringUv(true);
    try {
      const report = await api.recoverUvLayout(
        uvLayout,
        config.executablePath,
      );
      setUvLayout(report.layout);
      setUvRecoveryOpen(false);
      const summary = report.items
        .map(
          (item) =>
            `${item.name}: ${item.status}${item.detail ? `（${item.detail}）` : ""}`,
        )
        .concat(report.retained)
        .join("\n");
      if (report.status === "complete") {
        toast.success("uv 旧布局已恢复", summary || undefined);
      } else if (report.status === "partial") {
        toast.info("uv 布局部分恢复", summary || undefined);
      } else {
        toast.error("uv 布局恢复失败", summary || undefined);
      }
      await loadToolConfigs(true);
    } catch (e) {
      toast.error("uv 布局恢复失败", String(e));
    } finally {
      setRecoveringUv(false);
    }
  };

  /** 一键:先只读汇总所有可用工具的差异，确认后再写入 */
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
    try {
      const preview = await api.previewToolConfigs(
        targets.map((t) => ({
          tool: t.tool,
          globalPath: t.defaultGlobal ?? null,
          cachePath: t.defaultCache ?? null,
        })),
      );
      setBatchPreview(preview);
    } catch (e) {
      toast.error("批量预览失败", String(e));
    } finally {
      setToolBusy(null);
    }
  };

  /** 只写入预览中未阻止的工具，并逐项报告结果 */
  const applyPreviewedBatch = async () => {
    if (!batchPreview) return;
    const ready = new Set(batchPreview.ready);
    setApplyingConfig(true);
    let ok = 0;
    const failures: string[] = [];
    for (const item of batchPreview.items) {
      if (!ready.has(item.tool)) continue;
      const target = toolConfigs?.find((t) => t.tool === item.tool);
      if (!target) continue;
      try {
        await api.applyToolConfig(
          target.tool,
          target.defaultGlobal ?? null,
          target.defaultCache ?? null,
        );
        ok += 1;
      } catch (e) {
        failures.push(`${target.name}: ${String(e)}`);
      }
    }
    setApplyingConfig(false);
    setBatchPreview(null);
    const skipped = batchPreview.blocked.length;
    if (failures.length)
      toast.error(`${failures.length} 个工具配置失败`, failures.join("\n"));
    if (skipped > 0)
      toast.info(
        `${skipped} 个工具已跳过`,
        batchPreview.blocked.join("、") + "（预览中有阻止项）",
      );
    if (ok > 0) {
      toast.success(
        `已配置 ${ok} 个工具`,
        "全局与缓存路径已指向 根目录\\globals",
      );
    }
    await loadToolConfigs();
    if (caches) void loadCaches();
  };

  const openUpdatePreview = async () => {
    if (!selectedTool || updating) return;
    setUpdating(true);
    try {
      const preview = await api.previewPackageManagerUpdate(selectedTool);
      setUpdatePreview(preview);
    } catch (e) {
      toast.error("无法预览包管理器更新", String(e));
    } finally {
      setUpdating(false);
    }
  };

  const loadUpdateRecord = async () => {
    if (!selectedTool) return;
    try {
      setUpdateRecord(await api.packageManagerUpdateRecord(selectedTool));
    } catch {
      // A missing or unreadable journal must not break the page; the section
      // is simply omitted rather than claiming there was no update.
      setUpdateRecord(null);
    }
  };

  // Declared after `loadUpdateRecord` so the hook body cannot reference the
  // const before its initializer runs.
  useEffect(() => {
    void loadUpdateRecord();
  }, [selectedTool]);

  const applyUpdate = async () => {
    if (!selectedTool || updating) return;
    setUpdating(true);
    try {
      const result = await api.updatePackageManager(selectedTool);
      const detail = result.status === "commandFailed"
        ? `更新命令失败：${result.error ?? "未提供原因"}`
        : result.status === "verificationFailed"
          ? `更新命令已成功，但二次版本复查失败；更新可能已执行：${result.error ?? "未提供原因"}`
          : result.status === "versionChanged"
        ? `版本已变化：${result.beforeVersion ?? "未知"} → ${result.afterVersion ?? "未知"}`
        : result.status === "versionUnchanged"
          ? `命令已成功执行，但版本未变化（${result.afterVersion ?? "未知"}）。这不能证明已是最新版。`
          : "命令已成功执行，但无法从输出确认版本变化。";
      const message = `${detail}${result.output ? `\n${result.output}` : ""}`;
      const name = managerDefinition(selectedTool).name;
      if (result.status === "versionChanged") {
        toast.success(`${name} 版本已变化`, message);
      } else if (result.status === "commandFailed") {
        toast.error(`${name} 更新命令失败`, message);
      } else {
        toast.info(`${name} 更新结果待确认`, message);
      }
      if (result.status !== "commandFailed") await loadToolConfigs(true);
    } catch (e) {
      toast.error("包管理器更新失败", String(e));
    } finally {
      setUpdating(false);
      setUpdatePreview(null);
      void loadUpdateRecord();
    }
  };

  const openBatchUpdatePreview = async () => {
    if (updating || batchUpdating) return;
    setBatchUpdating(true);
    try {
      const preview = await api.previewPackageManagerUpdates();
      setUpdateBatch(preview);
    } catch (e) {
      toast.error("无法预览批量更新", String(e));
    } finally {
      setBatchUpdating(false);
    }
  };

  /** 只执行预览中未被阻止的工具；跳过项不算失败 */
  const applyBatchUpdate = async () => {
    if (!updateBatch || batchUpdating) return;
    setBatchUpdating(true);
    try {
      const result = await api.updatePackageManagers(updateBatch.ready);
      setBatchUpdateResult(result);
      const updated = result.items.filter((item) => item.outcome === "updated").length;
      const unchanged = result.items.filter((item) => item.outcome === "unchanged").length;
      const unverified = result.items.filter((item) => item.outcome === "unverified").length;
      const failed = result.retryable.length;
      const skipped = updateBatch.blocked.length;
      const summary = [
        updated ? `${updated} 个版本已变化` : "",
        unchanged ? `${unchanged} 个版本未变化` : "",
        unverified ? `${unverified} 个待确认` : "",
        failed ? `${failed} 个失败` : "",
        skipped ? `${skipped} 个跳过` : "",
      ]
        .filter(Boolean)
        .join("、");
      if (failed) {
        toast.error(`${failed} 个包管理器更新失败`, `可重试：${result.retryable.join("、")}`);
      }
      if (updated || unchanged) {
        toast.success("批量更新已执行", summary);
      } else if (!failed) {
        toast.info("批量更新已执行", summary || "没有可执行的更新");
      }
      await loadToolConfigs(true);
    } catch (e) {
      toast.error("批量更新失败", String(e));
    } finally {
      setBatchUpdating(false);
      // The preview is finished once the batch ran; the result dialog replaces
      // it instead of stacking a second modal on top.
      setUpdateBatch(null);
      void loadUpdateRecord();
    }
  };

  /** 只重试批次中失败的工具，其余保持不变 */
  const retryFailedUpdates = async () => {
    if (!batchUpdateResult || batchUpdating) return;
    const retry = batchUpdateResult.retryable;
    if (retry.length === 0) return;
    setBatchUpdating(true);
    try {
      const result = await api.updatePackageManagers(retry);
      // 合并：重试项覆盖原结果，其余保留，避免丢失首次运行的信息
      const byTool = new Map(result.items.map((item) => [item.tool, item]));
      const merged = batchUpdateResult.items.map(
        (item) => byTool.get(item.tool) ?? item,
      );
      setBatchUpdateResult({
        items: merged,
        retryable: merged
          .filter((item) => item.outcome === "failed")
          .map((item) => item.tool),
      });
      const stillFailed = merged.filter((item) => item.outcome === "failed").length;
      if (stillFailed) {
        toast.error("重试后仍有失败", `仍失败：${merged
          .filter((item) => item.outcome === "failed")
          .map((item) => item.tool)
          .join("、")}`);
      } else {
        toast.success("失败项已重试", "全部重试项均已执行");
      }
      await loadToolConfigs(true);
    } catch (e) {
      toast.error("重试失败", String(e));
    } finally {
      setBatchUpdating(false);
      void loadUpdateRecord();
    }
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
        <div className="flex gap-2">
          <Button
            size="sm"
            title="刷新包管理器配置"
            aria-label="刷新包管理器配置"
            onClick={() => loadToolConfigs(true)}
            loading={configLoading}
            disabled={toolBusy !== null}
          >
            <RefreshCw className="size-3.5" />
            刷新
          </Button>
          <Button
            size="sm"
            onClick={openUpdatePreview}
            loading={updating}
            disabled={
              toolBusy !== null ||
              configLoading ||
              !selectedDefinition.selfUpdate ||
              !!selectedConfig?.selfUpdateBlocker ||
              !toolConfigs?.find((t) => t.tool === selectedTool)?.available
            }
            title={
              selectedConfig?.selfUpdateBlocker
                ? selectedConfig.selfUpdateBlocker
                : selectedDefinition.selfUpdate
                  ? "先查看将执行的命令与当前版本，再确认更新；执行成功不保证是官方最新版"
                  : "此工具不支持通用自更新，请使用原安装方式更新"
            }
          >
            <RefreshCw className="size-3.5" />
            更新当前工具
          </Button>
          <Button
            size="sm"
            onClick={openBatchUpdatePreview}
            loading={batchUpdating}
            disabled={toolBusy !== null || configLoading || updating}
            title="先只读汇总所有可更新的包管理器，再一次性确认执行；不支持的会跳过"
          >
            <RefreshCw className="size-3.5" />
            批量更新
          </Button>
        </div>
      }
    >
      <ScanStatus kind="tools" />
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
      <section
        aria-label="当前包管理器版本"
        className="mb-4 flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border border-zinc-200 bg-white px-3 py-2 text-xs dark:border-zinc-800 dark:bg-zinc-900"
      >
        <span className="font-semibold">{managerDefinition(selectedTool).name}</span>
        <span className="text-zinc-500">当前版本</span>
        <span className="font-mono font-medium text-emerald-700 dark:text-emerald-400">
          {selectedVersionStatus}
        </span>
        {selectedConfig?.executablePath && (
          <span
            className="min-w-0 flex-1 truncate font-mono text-[10px] text-zinc-500"
            title={selectedConfig.executablePath}
          >
            {selectedConfig.executablePath}
          </span>
        )}
        <span className="w-full text-zinc-500">
          {selectedDefinition.selfUpdate
            ? "自更新：有条件支持，取决于原安装方式；执行后比较当前命令版本。"
            : "自更新：不支持通用命令，请通过原安装方式或所属 SDK 更新。"}
        </span>
        {selectedConfig?.selfUpdateBlocker && (
          <span className="w-full break-all text-amber-700 dark:text-amber-300">
            此安装的自更新不可用：{selectedConfig.selfUpdateBlocker}
          </span>
        )}
        <span className="w-full text-zinc-500">
          受限：不支持指定版本降级；全局包的依赖变化由管理器自行解析，不会给出影响范围；旧版本可能已从来源下架，无法重新安装。
        </span>
        {updateRecord && (
          <span className="w-full break-all text-zinc-500">
            最近一次更新记录：
            <span className="font-mono">
              {updateRecordStatus[updateRecord.status] ?? updateRecord.status}
            </span>
            {" · 版本 "}
            <span className="font-mono">
              {updateRecord.beforeVersion ?? "未知"}
              {updateRecord.afterVersion ? ` → ${updateRecord.afterVersion}` : "（未确认新版本）"}
            </span>
            {" · "}
            <span className="font-mono">{updateRecord.program}</span>
            {" ｜ 该记录仅用于审计，应用不会自动恢复到此版本。"}
          </span>
        )}
      </section>
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
      <div className={view === "scan" ? "" : "hidden"}>
        <OldPackageScan onReinstall={openReinstall} />
      </div>
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
                      {t.version && (
                        <div className="text-xs font-mono text-emerald-700 dark:text-emerald-400">
                          {t.version}
                        </div>
                      )}
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
                    <div className="min-w-0 w-full lg:w-auto lg:flex-1 grid grid-cols-1 sm:grid-cols-3 gap-x-4 gap-y-1">
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
                        <div className="flex items-center gap-1.5 text-[10px] text-zinc-400">
                          <span>命令入口</span>
                          {t.commandPathSource === "layout" && (
                            <span
                              className="rounded bg-zinc-100 px-1 py-px text-[9px] text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400"
                              title="由声明布局推导，尚未从包管理器实际查询到"
                            >
                              布局推导
                            </span>
                          )}
                          {t.commandPathSource === "queried" && (
                            <span
                              className="text-[9px] text-emerald-600 dark:text-emerald-400"
                              title="由当前包管理器命令实际返回"
                            >
                              实际查询
                            </span>
                          )}
                        </div>
                        <div
                          className="text-xs font-mono truncate selectable"
                          title={t.commandPath ?? "不适用"}
                        >
                          {t.commandPath ?? (
                            <span className="text-zinc-400">不适用</span>
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
                    {t.layoutWarning && (
                      <p
                        role="status"
                        className="w-full rounded-md border border-amber-200 bg-amber-50 px-2.5 py-2 text-[11px] leading-relaxed text-amber-900 dark:border-amber-900/60 dark:bg-amber-950/40 dark:text-amber-200"
                      >
                        {t.layoutWarning}
                      </p>
                    )}
                    {t.tool === "uv" &&
                      uvLayout &&
                      uvLayout.status !== "clean" && (
                        <div className="w-full flex flex-wrap items-center gap-2">
                          <span className="text-[11px] text-zinc-500">
                            {uvLayout.tools.filter(
                              (tool) => tool.needsRecovery && tool.recoverable,
                            ).length > 0
                              ? "恢复会按 receipt 中的精确版本重新安装到独立入口目录；无法还原的工具保留。"
                              : "没有可自动恢复的工具，请按预览中的问题人工处理。"}
                          </span>
                          <Button
                            size="sm"
                            variant="primary"
                            aria-label="恢复 uv 旧布局"
                            disabled={
                              !t.available ||
                              !t.executablePath ||
                              uvLayout.tools.filter(
                                (tool) =>
                                  tool.needsRecovery && tool.recoverable,
                              ).length === 0 ||
                              toolBusy !== null ||
                              migrationTool !== null
                            }
                            onClick={openUvRecovery}
                          >
                            <Wand2 className="size-3.5" />
                            恢复 uv 布局
                          </Button>
                        </div>
                      )}
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
                      onClick={() => openConfigPreview(t)}
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
              <Button
                size="sm"
                onClick={() => loadCaches(true)}
                loading={cacheLoading}
              >
                <RefreshCw className="size-3.5" />
                刷新缓存路径
              </Button>
            }
          />
          <ScanStatus kind="caches" />
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
                    {c.exists && <DirectorySize path={c.path} cacheTool={c.tool} />}
                  </div>
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
                      disabled={!c.exists || c.sizeBytes === 0}
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
          open={updatePreview !== null}
          onClose={() => (updating ? {} : setUpdatePreview(null))}
          onConfirm={applyUpdate}
          confirmText="执行更新"
          loading={updating}
          confirmDisabled={!!updatePreview?.blocked}
          title={`${managerDefinition(selectedTool).name} 更新预览`}
          message={
            updatePreview && (
              <>
                <p className="text-xs text-zinc-500">
                  以下为只读预览，尚未执行任何更新命令。
                </p>
                {updatePreview.blocked ? (
                  <p className="mt-3 text-xs text-amber-600 dark:text-amber-400">
                    {updatePreview.blocked}
                  </p>
                ) : (
                  <div className="mt-3 space-y-2 text-xs">
                    <p>
                      <span className="text-zinc-500">当前版本：</span>
                      <span className="font-mono">
                        {updatePreview.currentVersion ?? "无法解析"}
                      </span>
                    </p>
                    <p className="break-all">
                      <span className="text-zinc-500">安装位置：</span>
                      <span className="font-mono">
                        {updatePreview.executablePath ?? "未找到"}
                      </span>
                    </p>
                    <p className="break-all">
                      <span className="text-zinc-500">将执行：</span>
                      <span className="font-mono">
                        {[
                          updatePreview.updateProgram,
                          ...updatePreview.updateArguments,
                        ]
                          .filter(Boolean)
                          .join(" ")}
                      </span>
                    </p>
                    <p>
                      <span className="text-zinc-500">目标版本：</span>
                      无法在更新前得知（这些管理器不提供远程最新版查询）
                    </p>
                    {updatePreview.capabilities.latestOnly && (
                      <p>
                        <span className="text-zinc-500">版本范围：</span>
                        此入口只安装 latest，不接受版本参数，因此不能固定或降级版本
                      </p>
                    )}
                    {updatePreview.capabilities.manualPinnedCommand && (
                      <p className="break-all">
                        <span className="text-zinc-500">指定版本需手动执行：</span>
                        <span className="font-mono">
                          {updatePreview.capabilities.manualPinnedCommand}
                        </span>
                      </p>
                    )}
                    {updatePreview.notes.map((note) => (
                      <p key={note} className="text-zinc-500">
                        {note}
                      </p>
                    ))}
                  </div>
                )}
              </>
            )
          }
        />

        <ConfirmDialog
          open={configPreview !== null}
          onClose={() => (applyingConfig ? {} : setConfigPreview(null))}
          onConfirm={applyPreviewedConfig}
          confirmText="写入配置"
          loading={applyingConfig}
          confirmDisabled={
            !configPreview?.preview.available ||
            (configPreview?.preview.blocked.length ?? 1) > 0
          }
          title={`${configPreview?.config.name ?? ""} 路径配置预览`}
          message={
            configPreview && (
              <>
                <p className="text-xs text-zinc-500">
                  以下为只读预览，尚未写入任何配置或目录。
                </p>
                {configPreview.preview.changes.length > 0 && (
                  <div className="mt-3 space-y-2">
                    {configPreview.preview.changes.map((change) => (
                      <div
                        key={change.key}
                        className="rounded-md border border-zinc-200 px-2.5 py-2 text-xs dark:border-zinc-800"
                      >
                        <div className="flex items-center gap-1.5">
                          <span className="font-medium font-mono">
                            {change.key}
                          </span>
                          <span className="rounded bg-zinc-100 px-1 py-px text-[9px] text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400">
                            {change.role === "global"
                              ? "全局安装"
                              : change.role === "command"
                                ? "命令入口"
                                : change.role === "cache"
                                  ? "缓存/存储"
                                  : "配置"}
                          </span>
                        </div>
                        <div className="mt-1 break-all font-mono text-zinc-500">
                          旧值：{change.current ?? "未设置"}
                        </div>
                        <div className="break-all font-mono text-emerald-700 dark:text-emerald-400">
                          新值：{change.next}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
                <dl className="mt-3 space-y-1 text-xs">
                  <div className="flex gap-1.5">
                    <dt className="shrink-0 text-zinc-500">命令入口：</dt>
                    <dd className="break-all font-mono">
                      {configPreview.preview.commandEntry || "不适用"}
                      {configPreview.preview.commandEntryChanged
                        ? "（将变化）"
                        : ""}
                    </dd>
                  </div>
                  <div className="flex gap-1.5">
                    <dt className="shrink-0 text-zinc-500">入口在 PATH：</dt>
                    <dd>
                      {configPreview.preview.entryInPath ? "是" : "否"}
                    </dd>
                  </div>
                  <div className="flex gap-1.5">
                    <dt className="shrink-0 text-zinc-500">命令解析：</dt>
                    <dd className="break-all">
                      {configPreview.preview.commandStatus === "managed"
                        ? "命中受管入口"
                        : configPreview.preview.commandStatus === "shadowed"
                          ? "被 PATH 靠前位置覆盖"
                          : configPreview.preview.commandStatus === "missing"
                            ? "未在 PATH 中解析到"
                            : "无法确认"}
                    </dd>
                  </div>
                  <div className="flex gap-1.5">
                    <dt className="shrink-0 text-zinc-500">已装包：</dt>
                    <dd>
                      {configPreview.preview.movesExistingPackages
                        ? "会搬移"
                        : "保留原位置，不搬移"}
                    </dd>
                  </div>
                </dl>
                <div className="mt-3 rounded-md border border-zinc-200 px-2.5 py-2 text-xs dark:border-zinc-800">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-zinc-500">
                      受影响的全局包（当前全局目录）
                    </span>
                    {configPreview.preview.affectedPackages
                      .unresolvedCount > 0 && (
                      <span className="text-[10px] text-amber-700 dark:text-amber-300">
                        {configPreview.preview.affectedPackages.unresolvedCount}{" "}
                        个可能不再解析
                      </span>
                    )}
                  </div>
                  <p className="mt-1 text-zinc-400">
                    {configPreview.preview.affectedPackages.note}
                  </p>
                  {configPreview.preview.affectedPackages.packages.length >
                    0 && (
                    <ul className="mt-1.5 max-h-40 space-y-0.5 overflow-y-auto font-mono break-all">
                      {configPreview.preview.affectedPackages.packages.map(
                        (pkg) => (
                          <li
                            key={pkg.name}
                            className={
                              pkg.status === "unresolved"
                                ? "text-amber-700 dark:text-amber-300"
                                : "text-zinc-500"
                            }
                          >
                            {pkg.name}
                            {pkg.version ? ` ${pkg.version}` : ""}
                            {pkg.status === "unresolved" ? "（入口可能失效）" : ""}
                            {pkg.detail ? ` · ${pkg.detail}` : ""}
                          </li>
                        ),
                      )}
                    </ul>
                  )}
                </div>
                {configPreview.preview.warnings.length > 0 && (
                  <ul className="mt-3 space-y-1 text-xs text-amber-700 dark:text-amber-300">
                    {configPreview.preview.warnings.map((warning) => (
                      <li key={warning}>{warning}</li>
                    ))}
                  </ul>
                )}
                {configPreview.preview.blocked.length > 0 && (
                  <ul
                    role="alert"
                    className="mt-3 space-y-1 text-xs text-red-600 dark:text-red-400"
                  >
                    {configPreview.preview.blocked.map((reason) => (
                      <li key={reason}>阻止写入：{reason}</li>
                    ))}
                  </ul>
                )}
                {configPreview.preview.notes.length > 0 && (
                  <ul className="mt-3 space-y-1 text-xs text-zinc-400">
                    {configPreview.preview.notes.map((note) => (
                      <li key={note}>{note}</li>
                    ))}
                  </ul>
                )}
              </>
            )
          }
        />

        <ConfirmDialog
          open={batchPreview !== null}
          onClose={() => (applyingConfig ? {} : setBatchPreview(null))}
          onConfirm={applyPreviewedBatch}
          confirmText="写入可用项"
          loading={applyingConfig}
          confirmDisabled={(batchPreview?.ready.length ?? 0) === 0}
          title="一键配置预览"
          message={
            batchPreview && (
              <>
                <p className="text-xs text-zinc-500">
                  以下为只读汇总，尚未写入任何配置或目录。共{" "}
                  {batchPreview.items.length} 个工具、
                  {batchPreview.totalChanges} 项将变化；
                  {batchPreview.ready.length} 个可写入，
                  {batchPreview.blocked.length} 个因阻止项跳过。
                </p>
                <ul className="mt-3 space-y-2">
                  {batchPreview.items.map((item) => (
                    <li
                      key={item.tool}
                      className="rounded-md border border-zinc-200 px-2.5 py-2 text-xs dark:border-zinc-800"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-medium">
                          {toolConfigs?.find((t) => t.tool === item.tool)?.name ??
                            item.tool}
                        </span>
                        <span
                          className={
                            item.blocked.length > 0
                              ? "text-[10px] text-red-600 dark:text-red-400"
                              : "text-[10px] text-emerald-700 dark:text-emerald-400"
                          }
                        >
                          {item.blocked.length > 0
                            ? "跳过"
                            : item.changedCount > 0
                              ? `${item.changedCount} 项变化`
                              : "无需变化"}
                        </span>
                      </div>
                      {item.blocked.length > 0 ? (
                        <ul className="mt-1 space-y-0.5 text-red-600 dark:text-red-400">
                          {item.blocked.map((reason) => (
                            <li key={reason}>{reason}</li>
                          ))}
                        </ul>
                      ) : (
                        item.changes.length > 0 && (
                          <ul className="mt-1 space-y-0.5 font-mono break-all text-zinc-500">
                            {item.changes.map((change) => (
                              <li key={change.key}>
                                {change.key}: {change.current ?? "未设置"} →{" "}
                                {change.next}
                              </li>
                            ))}
                          </ul>
                        )
                      )}
                      {item.warnings.length > 0 && (
                        <ul className="mt-1 space-y-0.5 text-amber-700 dark:text-amber-300">
                          {item.warnings.map((warning) => (
                            <li key={warning}>{warning}</li>
                          ))}
                        </ul>
                      )}
                      {item.affectedPackages.packages.length > 0 && (
                        <div className="mt-1.5 border-t border-zinc-100 pt-1.5 dark:border-zinc-800">
                          <div className="flex items-center justify-between gap-2 text-zinc-500">
                            <span>受影响的全局包</span>
                            {item.affectedPackages.unresolvedCount > 0 && (
                              <span className="text-[10px] text-amber-700 dark:text-amber-300">
                                {item.affectedPackages.unresolvedCount} 个可能不再解析
                              </span>
                            )}
                          </div>
                          <ul className="mt-0.5 max-h-28 space-y-0.5 overflow-y-auto font-mono break-all">
                            {item.affectedPackages.packages.map((pkg) => (
                              <li
                                key={`${item.tool}-${pkg.name}`}
                                className={
                                  pkg.status === "unresolved"
                                    ? "text-amber-700 dark:text-amber-300"
                                    : "text-zinc-500"
                                }
                              >
                                {pkg.name}
                                {pkg.version ? ` ${pkg.version}` : ""}
                                {pkg.status === "unresolved"
                                  ? "（入口可能失效）"
                                  : ""}
                              </li>
                            ))}
                          </ul>
                        </div>
                      )}
                      {item.affectedPackages.packages.length === 0 &&
                        item.blocked.length === 0 && (
                          <p className="mt-1 text-[11px] text-zinc-400">
                            {item.affectedPackages.note}
                          </p>
                        )}
                    </li>
                  ))}
                </ul>
                <p className="mt-3 text-xs text-zinc-400">
                  仅写入未阻止的工具；已安装的全局包保留在原位置，不会搬移。
                </p>
              </>
            )
          }
        />

        <ConfirmDialog
          open={updateBatch !== null}
          onClose={() => (batchUpdating ? {} : setUpdateBatch(null))}
          onConfirm={applyBatchUpdate}
          confirmText="执行可用项"
          loading={batchUpdating}
          confirmDisabled={(updateBatch?.ready.length ?? 0) === 0}
          title="批量更新预览"
          message={
            updateBatch && (
              <>
                <p className="text-xs text-zinc-500">
                  以下为只读汇总，尚未执行任何更新命令。共{" "}
                  {updateBatch.items.length} 个包管理器：
                  {updateBatch.ready.length} 个可更新，
                  {updateBatch.blocked.length} 个将跳过。
                </p>
                <ul className="mt-3 max-h-72 space-y-2 overflow-y-auto">
                  {updateBatch.items.map((item) => (
                    <li
                      key={item.tool}
                      className="rounded-md border border-zinc-200 px-2.5 py-2 text-xs dark:border-zinc-800"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-medium">
                          {managerDefinition(item.tool).name}
                        </span>
                        <span
                          className={
                            item.blocked
                              ? "text-[10px] text-red-600 dark:text-red-400"
                              : "text-[10px] text-emerald-700 dark:text-emerald-400"
                          }
                        >
                          {item.blocked ? "跳过" : "将更新"}
                        </span>
                      </div>
                      <div className="mt-0.5 font-mono break-all text-zinc-500">
                        当前版本 {item.currentVersion ?? "无法解析"}
                        {item.executablePath ? `｜${item.executablePath}` : ""}
                      </div>
                      {item.blocked ? (
                        <p className="mt-1 text-red-600 dark:text-red-400">
                          {item.blocked}
                        </p>
                      ) : (
                        item.updateProgram && (
                          <p className="mt-1 font-mono break-all text-zinc-500">
                            {[item.updateProgram, ...item.updateArguments].join(" ")}
                          </p>
                        )
                      )}
                    </li>
                  ))}
                </ul>
                <p className="mt-3 text-xs text-zinc-400">
                  仅执行未被阻止的工具；执行成功不保证版本变化，执行后会逐个复查版本。
                </p>
              </>
            )
          }
        />

        <ConfirmDialog
          open={batchUpdateResult !== null}
          onClose={() => (batchUpdating ? {} : setBatchUpdateResult(null))}
          onConfirm={retryFailedUpdates}
          confirmText="重试失败项"
          loading={batchUpdating}
          confirmDisabled={(batchUpdateResult?.retryable.length ?? 0) === 0}
          title="批量更新结果"
          message={
            batchUpdateResult && (
              <>
                <p className="text-xs text-zinc-500">
                  共 {batchUpdateResult.items.length} 项；
                  {batchUpdateResult.retryable.length} 项失败可重试。
                </p>
                <ul className="mt-3 max-h-72 space-y-2 overflow-y-auto">
                  {batchUpdateResult.items.map((item) => (
                    <li
                      key={item.tool}
                      className="rounded-md border border-zinc-200 px-2.5 py-2 text-xs dark:border-zinc-800"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-medium">
                          {managerDefinition(item.tool).name}
                        </span>
                        <span
                          className={
                            item.outcome === "failed"
                              ? "text-[10px] text-red-600 dark:text-red-400"
                              : item.outcome === "updated"
                                ? "text-[10px] text-emerald-700 dark:text-emerald-400"
                                : "text-[10px] text-amber-700 dark:text-amber-300"
                          }
                        >
                          {batchOutcomeLabel[item.outcome] ?? item.outcome}
                        </span>
                      </div>
                      {item.outcome === "skipped" ? (
                        <p className="mt-1 text-zinc-500">
                          {item.skippedReason ?? "已跳过"}
                        </p>
                      ) : (
                        item.result && (
                          <p className="mt-1 font-mono break-all text-zinc-500">
                            {item.result.beforeVersion ?? "未知"} →{" "}
                            {item.result.afterVersion ?? "未确认新版本"}
                            {item.result.error ? `｜${item.result.error}` : ""}
                          </p>
                        )
                      )}
                    </li>
                  ))}
                </ul>
                <p className="mt-3 text-xs text-zinc-400">
                  “跳过”是执行前的决定，不计为失败；“待确认”表示命令可能已执行但读不到新版本。
                </p>
              </>
            )
          }
        />

        <ConfirmDialog
          open={uvRecoveryOpen}
          onClose={() => setUvRecoveryOpen(false)}
          onConfirm={doRecoverUv}
          confirmText="执行恢复"
          loading={recoveringUv}
          confirmDisabled={
            !uvLayout ||
            uvLayout.tools.filter(
              (tool) => tool.needsRecovery && tool.recoverable,
            ).length === 0
          }
          title="恢复 uv 旧布局"
          message={
            <>
              <p>
                保持 uv 工具环境目录不变，对每个可恢复工具按安装记录中的精确版本
                重新执行安装，并将启动器写入独立入口目录。
              </p>
              <div className="mt-3 space-y-2">
                {uvLayout?.tools
                  .filter((tool) => tool.needsRecovery)
                  .map((tool) => (
                    <div
                      key={tool.name}
                      className="rounded-md border border-zinc-200 px-2.5 py-2 text-xs dark:border-zinc-800"
                    >
                      <div className="font-medium">
                        {tool.name}
                        {tool.version ? ` ${tool.version}` : ""}
                      </div>
                      <div className="mt-1 text-zinc-500">
                        {tool.recoverable
                          ? `可恢复：${tool.requirement ?? "精确版本要求"}`
                          : `需人工核对：${tool.issues.join("；") || "安装记录不完整"}`}
                      </div>
                    </div>
                  ))}
              </div>
              {uvLayout?.issues.length ? (
                <p className="mt-3 text-xs text-amber-700 dark:text-amber-300">
                  布局问题：{uvLayout.issues.join("；")}
                </p>
              ) : null}
              <p className="mt-3 text-xs text-zinc-400">
                恢复过程会重新解析依赖，可能更新传递依赖。旧启动器只在新安装记录和文件核验通过后删除；
                无法还原的工具、旧入口和共享缓存会保留并显示在结果中。此操作会改写真实 uv 环境。
              </p>
            </>
          }
        />

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
                下的全部内容。实际释放空间将在清理完成后显示。
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
