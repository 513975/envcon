import { useCallback, useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  RefreshCw,
  FolderOpen,
  Check,
  Trash2,
  ScanSearch,
  X,
  FolderPlus,
  Wand2,
  FileDown,
  FileUp,
} from "lucide-react";
import { PageShell } from "../components/layout/PageShell";
import { Card, CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { Spinner, Empty } from "../components/ui/Empty";
import { ConfirmDialog, Modal } from "../components/ui/Modal";
import { EnvTypeBadge } from "../components/EnvTypeBadge";
import { ScanResults } from "../components/ScanResults";
import { SelectionButton } from "../components/ui/SelectionButton";
import { api } from "../lib/api";
import { useScanResult } from "../lib/scanClient";
import { ScanStatus } from "../components/ScanStatus";
import { DirectorySize } from "../components/DirectorySize";
import { savedProjectDirs } from "../lib/scanPreferences";
import { toast } from "../lib/toast";
import { useAppStore } from "../lib/store";
import {
  ALL_ENV_TYPES,
  ENV_TYPE_META,
  type EnvType,
  type Overview,
  type ExternalEnv,
  type ManagedEnv,
  type PathRepairPreview,
  type ScanReport,
  type ToolScanSummary,
  type UninstallPreview,
  type ManifestComparison,
} from "../lib/types";

/** 环境列表的排序方式。只影响本地视图顺序，不改变后端返回。 */
type EnvSort = "name" | "version" | "status";

/** 按健康状态筛选。`issues` 同时包含 broken 与 inaccessible。 */
type EnvHealthFilter = "all" | "available" | "issues";

const ENV_SORT_LABEL: Record<EnvSort, string> = {
  name: "名称",
  version: "版本",
  status: "状态",
};

const ENV_HEALTH_LABEL: Record<EnvHealthFilter, string> = {
  all: "全部",
  available: "可用",
  issues: "有问题",
};

/**
 * 对某个分类的环境列表应用搜索、健康过滤与排序。
 *
 * 抽成纯函数是为了能直接测：列表视图的规则一旦写错，用户会看到
 * 「明明装了却搜不到」或「损坏项被排序挤到看不见」，而这类问题
 * 在 UI 上很难复现。纯函数不依赖 React，也不碰后端。
 */
function viewEnvs(
  envs: ManagedEnv[],
  query: string,
  health: EnvHealthFilter,
  sort: EnvSort,
): ManagedEnv[] {
  const needle = query.trim().toLowerCase();
  const filtered = envs.filter((env) => {
    if (health === "available" && env.status !== "available") return false;
    if (health === "issues" && env.status === "available") return false;
    if (!needle) return true;
    // 名称之外也匹配路径与版本：用户常常只记得「装在 D 盘」或某个版本号。
    return (
      env.name.toLowerCase().includes(needle) ||
      env.path.toLowerCase().includes(needle) ||
      (env.version ?? "").toLowerCase().includes(needle)
    );
  });
  const sorted = [...filtered];
  sorted.sort((a, b) => {
    if (sort === "version") {
      // 缺版本的一律排后面，避免 null 混进比较。
      const va = a.version ?? "";
      const vb = b.version ?? "";
      if (!va && !vb) return a.name.localeCompare(b.name);
      if (!va) return 1;
      if (!vb) return -1;
      return vb.localeCompare(va, undefined, { numeric: true }) || a.name.localeCompare(b.name);
    }
    if (sort === "status") {
      // 有问题的排前面：需要处理的项不该排在列表底部。
      const rank = (env: ManagedEnv) => (env.status === "available" ? 1 : 0);
      return rank(a) - rank(b) || a.name.localeCompare(b.name);
    }
    return a.name.localeCompare(b.name);
  });
  return sorted;
}

export function Environments() {
  const [overview, setOverview] = useState<Overview | null>(null);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState<EnvType>("jdk");
  const [busy, setBusy] = useState<string | null>(null);
  const [pathRepair, setPathRepair] = useState<PathRepairPreview | null>(null);
  const [confirmUninstall, setConfirmUninstall] = useState<ManagedEnv | null>(
    null,
  );
  const [uninstallPreview, setUninstallPreview] = useState<UninstallPreview | null>(null);
  const [previewingUninstall, setPreviewingUninstall] = useState(false);
  // 卸载会删除内容；停用只解除管理并保留全部文件。默认仍是历史行为。
  const [uninstallMode, setUninstallMode] = useState<"uninstall" | "detach">("uninstall");
  const [manifestComparison, setManifestComparison] =
    useState<ManifestComparison | null>(null);
  const [uninstallPreviewError, setUninstallPreviewError] = useState<string | null>(null);
  const [confirmedLegacyLinks, setConfirmedLegacyLinks] = useState<string[]>([]);
  const [confirmedLegacyPathEntries, setConfirmedLegacyPathEntries] = useState<string[]>([]);
  const legacyConfirmationsReady = (uninstallPreview?.legacyLinks ?? []).every(
    (item) => item.path !== null && confirmedLegacyLinks.includes(item.path),
  );
  const legacyPathConfirmationsReady = confirmedLegacyPathEntries.every(
    (path) => (uninstallPreview?.legacyPathEntries ?? []).some((item) => item.path === path),
  );
  const installerReady = uninstallPreview?.installerPlan?.state === "ready";
  const [systemEnvs, setSystemEnvs] = useState<ExternalEnv[] | null>(null);
  const [systemCoreTools, setSystemCoreTools] = useState<ToolScanSummary[]>([]);
  const [systemScanStatus, setSystemScanStatus] = useState<ScanReport["status"]>("complete");
  const [systemScanError, setSystemScanError] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [projectDirs, setProjectDirs] = useState<string[]>(savedProjectDirs);
  const [scanWarnings, setScanWarnings] = useState<string[]>([]);
  const [integrating, setIntegrating] = useState<string | null>(null);
  // 环境列表的本地视图选项。这些只在已加载的列表上筛选/排序，
  // 不触发重新扫描，也不改动后端返回的顺序。
  const [envQuery, setEnvQuery] = useState("");
  const [envSort, setEnvSort] = useState<EnvSort>("name");
  const [envHealth, setEnvHealth] = useState<EnvHealthFilter>("all");
  useScanResult<Overview>("overview", [], setOverview);
  useScanResult<ScanReport>("system", projectDirs, useCallback((report: ScanReport) => {
    setSystemEnvs(report.tools);
    setSystemCoreTools(report.coreTools ?? []);
    setSystemScanStatus(report.status ?? (report.warnings?.length ? "partial" : "complete"));
    setScanWarnings(report.warnings);
    setSystemScanError(null);
  }, []));

  const refreshOverview = useAppStore((s) => s.refreshOverview);
  const overviewVersion = useAppStore((s) => s.overviewVersion);

  const load = async (force = false) => {
    setLoading(true);
    try {
      setOverview(await api.getOverview(force));
    } catch (e) {
      toast.error("加载失败", String(e));
    } finally {
      setLoading(false);
    }
  };

  /** 导出只读清单；不创建、不还原、不搬移任何环境。 */
  const exportManifest = async () => {
    try {
      const target = await save({
        title: "导出环境清单",
        defaultPath: "envcon-manifest.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!target) return;
      const saved = await api.exportEnvironmentManifest(target);
      toast.success(
        "环境清单已导出",
        `已写入 ${saved}。清单只记录本机路径，不是可还原的备份。`,
      );
    } catch (e) {
      toast.error("导出失败", String(e));
    }
  };

  /** 读取清单并只读比对；不会创建、还原或搬移任何环境。 */
  const importManifest = async () => {
    try {
      const source = await open({
        title: "导入环境清单并比对",
        multiple: false,
        directory: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!source || Array.isArray(source)) return;
      const comparison = await api.importEnvironmentManifest(source);
      setManifestComparison(comparison);
    } catch (e) {
      toast.error("清单比对失败", String(e));
    }
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [overviewVersion]);

  useEffect(() => {
    localStorage.setItem(
      "envcon.scan.projectDirs",
      JSON.stringify(projectDirs),
    );
  }, [projectDirs]);

  const doSwitch = async (env: ManagedEnv) => {
    if (busy) return;
    setBusy(`switch:${env.name}`);
    try {
      const diagnosis = await api.switchEnv(env.envType, env.name);
      const shadowed = diagnosis.commands.filter((c) => c.status === "shadowed");
      const missing = diagnosis.commands.filter((c) => c.status === "missing");
      const version = diagnosis.version ? ` ${diagnosis.version}` : "";
      const detail = [
        diagnosis.currentTarget
          ? `当前链接：${diagnosis.currentTarget}${version}`
          : "已设为当前版本",
        diagnosis.commands.length > 0
          ? `命令解析：${diagnosis.commands
              .map((c) => `${c.command} ${c.status === "managed" ? "命中受管入口" : c.status === "shadowed" ? "被靠前安装覆盖" : "未解析到"}`)
              .join("；")}`
          : "",
        shadowed.length > 0
          ? `警告：${shadowed.map((c) => c.command).join("、")} 仍会命中 ${shadowed[0].resolved ?? "其他安装"}`
          : "",
        missing.length > 0
          ? `提示：${missing.map((c) => c.command).join("、")} 不在 PATH 中`
          : "",
      ]
        .filter(Boolean)
        .join("\n");
      if (shadowed.length > 0 || missing.length > 0) {
        toast.info(
          `已切换 ${ENV_TYPE_META[env.envType].label}，命令生效待确认`,
          detail,
        );
      } else {
        toast.success(
          `已切换 ${ENV_TYPE_META[env.envType].label}`,
          detail || `${env.name} 已设为当前版本`,
        );
      }
      refreshOverview();
    } catch (e) {
      toast.error("切换失败", String(e));
    } finally {
      setBusy(null);
    }
  };

  /** 只重新读取一个环境，用于修好目录后确认它是否已恢复。 */
  const redetect = async (env: ManagedEnv) => {
    if (busy) return;
    setBusy(`redetect:${env.envType}:${env.name}`);
    try {
      const result = await api.redetectEnvironment(env.envType, env.name);
      if (result.status === "available") {
        toast.success(
          `${env.name} 重新检测通过`,
          result.version ? `版本 ${result.version}` : "已可正常识别",
        );
      } else {
        toast.error(
          `${env.name} 仍未恢复`,
          result.statusDetail ?? "状态仍不可用",
        );
      }
      await load();
    } catch (e) {
      toast.error("重新检测失败", String(e));
    } finally {
      setBusy(null);
    }
  };

  const openPathRepair = async (envType: EnvType) => {
    if (busy) return;
    setBusy(`repair:${envType}`);
    try {
      const preview = await api.previewPathRepair(envType);
      if (preview.mode === "none") {
        toast.info("无需修复 PATH 顺序", preview.summary);
        return;
      }
      setPathRepair(preview);
    } catch (e) {
      toast.error("读取 PATH 修复预览失败", String(e));
    } finally {
      setBusy(null);
    }
  };

  const doApplyPathRepair = async () => {
    if (!pathRepair?.plan || busy) return;
    setBusy("repair:apply");
    try {
      const result = await api.applyPathRepair(pathRepair.plan);
      if (result.verified) {
        toast.success(
          "已调整 PATH 顺序并核验",
          `受管入口已在用户 PATH 首位；未增删任何条目。${result.commands
            .map((c) => `${c.command} ${c.status === "managed" ? "命中受管入口" : c.status}`)
            .join("；")}。请在新终端核对命令`,
        );
      } else {
        toast.info(
          "PATH 顺序已写入，但核验未通过",
          [
            ...result.problems,
            "可能原因：已有终端仍使用旧环境，或系统 PATH 中另有更早的同名命令",
          ].join("\n"),
        );
      }
      setPathRepair(null);
      refreshOverview();
    } catch (e) {
      toast.error("PATH 修复失败", String(e));
      setPathRepair(null);
    } finally {
      setBusy(null);
    }
  };

  const doUninstall = async () => {    if (!confirmUninstall || busy || !uninstallPreview || !uninstallPreview.scopeComplete || uninstallPreview.blocked.length > 0 || !legacyConfirmationsReady || !legacyPathConfirmationsReady) return;
    const env = confirmUninstall;
    setBusy(`uninstall:${env.name}`);
    try {
      const result = await api.uninstallEnv(env.envType, env.name, {
        ...uninstallPreview, confirmedLegacyLinks, confirmedLegacyPathEntries,
      });
      if (result.status === "partial") {
        const removed = result.removed.map((item) => item.resource).join("、") || "无可核验的移除项";
        toast.error(
          "移除部分失败",
          [
            `已核验移除：${removed}`,
            ...result.failed.map((item) => `${item.resource}：${item.details}`),
            result.unverified.length > 0 ? `仍未验证 ${result.unverified.length} 类目录外副作用` : "",
          ].filter(Boolean).join("\n"),
        );
      } else {
        const details = [
          `已核验移除 ${result.removed.map((item) => item.resource).join("、") || "所选受管范围"}`,
          result.retained.length > 0 ? `保留 ${result.retained.length} 类目录外/外部资源` : "",
          result.unverified.length > 0 ? `${result.unverified.length} 类目录外副作用仍未验证` : "",
        ].filter(Boolean).join("；");
        toast.success("受管范围已移除并核验", details);
      }
      setConfirmUninstall(null);
    } catch (e) {
      toast.error("移除失败", String(e));
    } finally {
      setBusy(null);
      // A failed removal may already have detached current or removed some files.
      refreshOverview();
    }
  };

  const openUninstallPreview = async (env: ManagedEnv, mode: "uninstall" | "detach" = "uninstall") => {
    if (busy) return;
    setUninstallMode(mode);
    setConfirmUninstall(env);
    setUninstallPreview(null);
    setConfirmedLegacyLinks([]);
    setConfirmedLegacyPathEntries([]);
    setUninstallPreviewError(null);
    setPreviewingUninstall(true);
    try {
      setUninstallPreview(await api.previewUninstallEnv(env.envType, env.name, mode));
    } catch (error) {
      setUninstallPreviewError(String(error));
    } finally {
      setPreviewingUninstall(false);
    }
  };

  /** 在同一个对话框内切换模式：必须重新预览，旧预览绑定的是另一种范围。 */
  const switchUninstallMode = async (mode: "uninstall" | "detach") => {
    const env = confirmUninstall;
    if (!env || busy || mode === uninstallMode) return;
    setUninstallMode(mode);
    setUninstallPreview(null);
    setConfirmedLegacyLinks([]);
    setConfirmedLegacyPathEntries([]);
    setUninstallPreviewError(null);
    setPreviewingUninstall(true);
    try {
      setUninstallPreview(await api.previewUninstallEnv(env.envType, env.name, mode));
    } catch (error) {
      setUninstallPreviewError(String(error));
    } finally {
      setPreviewingUninstall(false);
    }
  };

  const doScanSystem = async (force = true) => {
    setScanning(true);
    setSystemScanError(null);
    try {
      const report = await api.scanSystem(projectDirs, force);
      setSystemEnvs(report.tools);
      setSystemCoreTools(report.coreTools ?? []);
      setSystemScanStatus(report.status ?? (report.warnings?.length ? "partial" : "complete"));
      setScanWarnings(report.warnings);
    } catch (e) {
      setSystemScanStatus("failed");
      setSystemScanError(String(e));
      toast.error("扫描失败", String(e));
    } finally {
      setScanning(false);
    }
  };

  useEffect(() => {
    void doScanSystem(false);
  }, [JSON.stringify(projectDirs), overviewVersion]);

  const addProject = async () => {
    try {
      const dir = await open({
        directory: true,
        title: "选择项目或虚拟环境目录",
      });
      if (typeof dir === "string") {
        setProjectDirs((dirs) =>
          dirs.includes(dir) ? dirs : [...dirs, dir].slice(0, 8),
        );
      }
    } catch (error) {
      toast.error("选择目录失败", String(error));
    }
  };

  const doIntegrate = async (env: ExternalEnv) => {
    if (!env.envType || !env.path) return;
    setIntegrating(env.path);
    try {
      const name = await api.integrateExternalEnv(
        env.envType,
        env.path,
        env.version,
      );
      toast.success(
        `${env.tool} 已纳入管理`,
        `已注册为 ${name},可在上方列表中设为当前。建议在路径管理中清理原 PATH 条目,避免版本冲突`,
      );
      await doScanSystem();
      refreshOverview();
    } catch (e) {
      toast.error("纳入管理失败", String(e));
    } finally {
      setIntegrating(null);
    }
  };

  if (loading && !overview)
    return <Spinner className="py-24 flex justify-center" />;
  if (!overview) {
    return (
      <PageShell title="环境管理">
        <Card>
          <Empty title="请先在仪表盘设置管理根目录" />
        </Card>
      </PageShell>
    );
  }

  const category = overview.categories.find((c) => c.envType === tab);
  // 视图层的筛选与排序结果。后端返回的顺序保持不变，
  // 这里只决定「当前这一屏显示什么、按什么顺序」。
  const visibleEnvs = category
    ? viewEnvs(category.envs, envQuery, envHealth, envSort)
    : [];

  return (
    <PageShell
      title="环境管理"
      desc={
        overview.rootExists ? `根目录:${overview.root}` : "尚未设置管理根目录"
      }
      actions={
        <div className="flex gap-2">
          <Button
            size="sm"
            onClick={exportManifest}
            title="导出只读环境清单：记录本机路径与版本，不创建、不还原、不搬移任何环境"
          >
            <FileDown className="size-3.5" />
            导出清单
          </Button>
          <Button
            size="sm"
            onClick={importManifest}
            title="读取已导出的清单并与本机现状比对：只读，不创建、不还原、不搬移任何环境"
          >
            <FileUp className="size-3.5" />
            导入比对
          </Button>
          <Button size="sm" onClick={() => load(true)} loading={loading}>
            <RefreshCw className="size-3.5" />
            刷新
          </Button>
        </div>
      }
    >
      {/* 类型 Tab */}
      <ScanStatus kind="overview" />
      <div className="flex gap-1.5 mb-4 flex-wrap">
        {ALL_ENV_TYPES.map((t) => {
          const cat = overview.categories.find((c) => c.envType === t);
          const count = cat?.envs.length ?? 0;
          const active = cat?.current != null;
          return (
            <SelectionButton
              key={t}
              onClick={() => setTab(t)}
              selected={tab === t}
              count={count}
              indicator={active ? "available" : undefined}
              statusLabel="已设置当前环境"
            >
              {ENV_TYPE_META[t].label}
            </SelectionButton>
          );
        })}
      </div>

      {/* 环境列表 */}
      {tab === "adb" && <p className="mb-3 text-xs text-zinc-500">管理 Android Platform Tools（ADB / Fastboot），已有 SDK 仅链接其 platform-tools 目录。切换版本不重启已运行的 ADB 服务；若版本未生效或卸载提示占用，可自行执行 adb kill-server 后重试。</p>}
      {/* 搜索、排序与健康过滤：只在已加载的列表上生效，不触发重新扫描 */}
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <input
          type="search"
          value={envQuery}
          onChange={(e) => setEnvQuery(e.target.value)}
          placeholder="搜索名称、路径或版本"
          aria-label="搜索环境"
          className="h-8 min-w-40 flex-1 rounded-md border border-zinc-200 bg-white px-2.5 text-xs text-zinc-700 outline-none placeholder:text-zinc-400 focus:border-zinc-400 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200"
        />
        <div className="flex gap-1.5" role="group" aria-label="按健康状态筛选">
          {(Object.keys(ENV_HEALTH_LABEL) as EnvHealthFilter[]).map((h) => (
            <SelectionButton
              key={h}
              onClick={() => setEnvHealth(h)}
              selected={envHealth === h}
            >
              {ENV_HEALTH_LABEL[h]}
            </SelectionButton>
          ))}
        </div>
        <label className="flex items-center gap-1.5 text-xs text-zinc-500">
          排序
          <select
            value={envSort}
            onChange={(e) => setEnvSort(e.target.value as EnvSort)}
            aria-label="环境排序方式"
            className="h-8 rounded-md border border-zinc-200 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200"
          >
            {(Object.keys(ENV_SORT_LABEL) as EnvSort[]).map((s) => (
              <option key={s} value={s}>
                {ENV_SORT_LABEL[s]}
              </option>
            ))}
          </select>
        </label>
      </div>
      <Card>
        {category?.scanWarning && (
          <div className="px-4 py-2 text-xs text-amber-700 dark:text-amber-300 border-b border-zinc-100 dark:border-zinc-800">
            {category.scanWarning}
          </div>
        )}
        {category?.currentError && (
          <div className="px-4 py-2 text-xs text-red-600 dark:text-red-400 border-b border-zinc-100 dark:border-zinc-800">
            {category.currentError}
          </div>
        )}
        {visibleEnvs.length > 0 ? (
          <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
            {visibleEnvs.map((env) => (
              <div
                key={env.name}
                className="grid grid-cols-[auto_minmax(0,1fr)] sm:grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-3 px-4 py-3.5"
              >
                <EnvTypeBadge type={env.envType} size="sm" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-semibold text-zinc-800 dark:text-zinc-100 break-all">
                      {env.name}
                    </span>
                    {env.isCurrent && <Badge color="green">当前</Badge>}
                    {env.status !== "available" && (
                      <Badge color="red">
                        {env.status === "inaccessible" ? "不可访问" : "损坏"}
                      </Badge>
                    )}
                    {env.isExternalLink && <Badge color="blue">外部链接</Badge>}
                    {env.isLinkEntry && !env.isExternalLink && <Badge color="blue">受管链接</Badge>}
                  </div>
                  <div className="mt-0.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-zinc-500">
                    <span>{env.version ?? "未知版本"}</span>
                    <span
                      className="truncate max-w-64 selectable"
                      title={env.path}
                    >
                      {env.path}
                    </span>
                  </div>
                  <DirectorySize path={env.path} envType={env.envType} name={env.name} />
                  {env.statusDetail && (
                    <div className="mt-1 text-xs text-red-600 dark:text-red-400">
                      {env.statusDetail}
                    </div>
                  )}
                </div>
                <div className="col-span-2 sm:col-span-1 flex flex-wrap justify-end items-center gap-1.5">
                  {!env.isCurrent && (
                    <Button
                      size="sm"
                      variant="primary"
                      loading={busy === `switch:${env.name}`}
                      disabled={busy !== null || env.status !== "available"}
                      onClick={() => doSwitch(env)}
                    >
                      <Check className="size-3.5" />
                      设为当前
                    </Button>
                  )}
                  {env.isCurrent && (
                    <Button
                      size="sm"
                      loading={busy === `repair:${env.envType}`}
                      disabled={busy !== null || env.status !== "available"}
                      onClick={() => openPathRepair(env.envType)}
                    >
                      <Wand2 className="size-3.5" />
                      预览 PATH 修复
                    </Button>
                  )}
                  <Button
                    size="sm"
                    title={`重新读取 ${env.name} 的结构与版本（只读）`}
                    aria-label={`重新检测 ${env.name}`}
                    loading={busy === `redetect:${env.envType}:${env.name}`}
                    disabled={busy !== null}
                    onClick={() => void redetect(env)}
                  >
                    <ScanSearch className="size-3.5" />
                  </Button>
                  <Button
                    size="sm"
                    title={`打开 ${env.name} 安装目录`}
                    aria-label={`打开 ${env.name} 安装目录`}
                    onClick={() =>
                      api
                        .openPath(env.path)
                        .catch((e) => toast.error("打开失败", String(e)))
                    }
                  >
                    <FolderOpen className="size-3.5" />
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    className="text-red-500 hover:bg-red-50 dark:hover:bg-red-950"
                    disabled={busy !== null}
                    aria-label={`${env.isExternalLink ? "移除管理" : env.isLinkEntry ? "移除链接" : "删除受管目录"} ${env.name}`}
                    onClick={() => void openUninstallPreview(env)}
                  >
                    <Trash2 className="size-3.5" />
                    {env.isExternalLink ? "移除管理" : env.isLinkEntry ? "移除链接" : "删除目录"}
                  </Button>
                </div>
              </div>
            ))}
          </div>
        ) : (
          // 两种「空」必须区分：本来就没有环境 vs 筛选后没有匹配。
          // 混为一谈会让用户以为环境丢了，或按错误的提示去下载中心。
          category && category.envs.length > 0 ? (
            <Empty
              title="没有符合筛选条件的环境"
              desc={`当前筛选：${ENV_HEALTH_LABEL[envHealth]}${envQuery.trim() ? `，关键词「${envQuery.trim()}」` : ""}。共 ${category.envs.length} 个环境被隐藏。`}
            />
          ) : (
            <Empty
              title={`暂无 ${ENV_TYPE_META[tab].label} 环境`}
              desc="去下载中心安装一个吧"
            />
          )
        )}
      </Card>

      {/* 系统散装环境 */}
      <Card className="mt-5">
        <CardHeader
          title={
            <span className="flex items-center gap-1.5">
              <ScanSearch className="size-3.5 text-zinc-400" />
              本机工具与包管理器
            </span>
          }
          actions={
            <div className="flex gap-2">
              <Button
                size="sm"
                onClick={addProject}
                disabled={scanning || projectDirs.length >= 8}
              >
                <FolderPlus className="size-3.5" />
                项目目录
              </Button>
              <Button
                size="sm"
                onClick={() => doScanSystem()}
                loading={scanning}
              >
                <ScanSearch className="size-3.5" />
                {systemEnvs ? "重新扫描" : "开始扫描"}
              </Button>
            </div>
          }
        />
        <div className="px-4"><ScanStatus kind="system" projects={projectDirs} /></div>
        {systemScanError && <p role="alert" className="mx-4 mb-2 rounded bg-rose-50 px-3 py-2 text-xs text-rose-700 dark:bg-rose-950/40 dark:text-rose-300">
          最近一次扫描未完成：{systemScanError}{systemEnvs !== null && "；仍显示上次成功结果"}
        </p>}
        {projectDirs.length > 0 && (
          <div className="px-4 py-2 space-y-1">
            {projectDirs.map((dir) => (
              <div
                key={dir}
                className="flex items-center gap-2 text-xs text-zinc-500"
              >
                <span className="min-w-0 flex-1 break-all">{dir}</span>
                <button
                  disabled={scanning}
                  title="移除扫描目录"
                  aria-label={`移除 ${dir}`}
                  onClick={() =>
                    setProjectDirs((dirs) => dirs.filter((p) => p !== dir))
                  }
                >
                  <X className="size-3.5" />
                </button>
              </div>
            ))}
          </div>
        )}
        {scanWarnings.length > 0 && (
          <details className="px-4 py-2 text-xs text-amber-700 dark:text-amber-400 space-y-1">
            <summary className="cursor-pointer">
              扫描提示（{scanWarnings.length}）
            </summary>
            {scanWarnings.map((warning, i) => (
              <p key={i} className="break-all">
                {warning}
              </p>
            ))}
          </details>
        )}
        {systemEnvs === null ? (
          <Empty
            title={scanning ? "正在扫描" : systemScanError ? "扫描未完成" : "尚未扫描"}
            desc={systemScanError ?? "扫描前不会把工具状态判定为未安装。"}
          />
        ) : (
          <ScanResults
            rows={systemEnvs}
            coreTools={systemCoreTools}
            scanRunStatus={systemScanStatus}
            canIntegrate={overview.rootExists}
            integrating={integrating}
            onIntegrate={doIntegrate}
          />
        )}
      </Card>

      {/* 卸载确认 */}
      <ConfirmDialog
        open={pathRepair !== null}
        onClose={() => setPathRepair(null)}
        onConfirm={doApplyPathRepair}
        confirmText="调整顺序"
        loading={busy === "repair:apply"}
        confirmDisabled={!pathRepair?.plan || pathRepair.mode !== "reorder"}
        title="PATH 顺序修复预览"
        message={
          pathRepair && (
            <>
              <p className="text-xs text-zinc-500">
                只读预览，尚未修改用户 PATH。
              </p>
              <p className="mt-2">{pathRepair.summary}</p>
              {pathRepair.mode === "reorder" && pathRepair.plan && (
                <>
                  <div className="mt-3 rounded-md border border-zinc-200 px-2.5 py-2 text-xs dark:border-zinc-800">
                    <div className="text-zinc-500">将移到最前的受管入口</div>
                    <div className="break-all font-mono">{pathRepair.entry}</div>
                    <div className="mt-2 text-zinc-500">当前覆盖它的目录</div>
                    <ul className="break-all font-mono text-amber-700 dark:text-amber-300">
                      {pathRepair.shadowing.map((item) => (
                        <li key={item}>{item}</li>
                      ))}
                    </ul>
                  </div>
                  <div className="mt-3 text-xs">
                    <div className="text-zinc-500">调整后的用户 PATH</div>
                    <div className="mt-1 break-all font-mono text-emerald-700 dark:text-emerald-400">
                      {pathRepair.resultingPath}
                    </div>
                  </div>
                </>
              )}
              {(pathRepair.mode === "manual" ||
                pathRepair.blocked.length > 0) && (
                <ul
                  role="alert"
                  className="mt-3 space-y-1 text-xs text-red-600 dark:text-red-400"
                >
                  {pathRepair.blocked.map((reason) => (
                    <li key={reason}>{reason}</li>
                  ))}
                  {pathRepair.manualEntries.map((entry) => (
                    <li key={entry}>需手动处理：{entry}</li>
                  ))}
                </ul>
              )}
              <ul className="mt-3 space-y-1 text-xs text-zinc-400">
                {pathRepair.notes.map((note) => (
                  <li key={note}>{note}</li>
                ))}
              </ul>
            </>
          )
        }
      />

      {/* 清单只读比对：没有确认按钮，因为这里不执行任何恢复动作 */}
      <Modal
        open={manifestComparison !== null}
        onClose={() => setManifestComparison(null)}
        title="环境清单比对"
        footer={
          <Button size="sm" onClick={() => setManifestComparison(null)}>
            关闭
          </Button>
        }
      >
        {manifestComparison && (
          <>
            <p className="text-xs text-zinc-500">
              只读比对，未创建、还原或搬移任何环境。
            </p>
            <p className="mt-2 text-xs">
              清单生成于 {manifestComparison.recordedGeneratedAt}；记录根目录{" "}
              {manifestComparison.recordedRoot ?? "（无）"}，本机根目录{" "}
              {manifestComparison.currentRoot ?? "（无）"}
            </p>
            {!manifestComparison.sameRoot && (
              <p
                role="alert"
                className="mt-2 text-xs text-amber-700 dark:text-amber-300"
              >
                根目录不同：这是跨机器比对，差异不代表环境被改动。
              </p>
            )}
            <div className="mt-3 grid grid-cols-4 gap-2 text-center text-xs">
              {(
                [
                  ["未变", manifestComparison.summary.unchanged],
                  ["新增", manifestComparison.summary.added],
                  ["缺失", manifestComparison.summary.missing],
                  ["已变更", manifestComparison.summary.changed],
                ] as const
              ).map(([label, value]) => (
                <div
                  key={label}
                  className="rounded-md border border-zinc-200 py-1.5 dark:border-zinc-800"
                >
                  <div className="text-zinc-500">{label}</div>
                  <div className="text-sm font-medium">{value}</div>
                </div>
              ))}
            </div>
            {manifestComparison.rows.filter((row) => row.kind !== "unchanged")
              .length > 0 && (
              <ul className="mt-3 max-h-64 space-y-1.5 overflow-auto text-xs">
                {manifestComparison.rows
                  .filter((row) => row.kind !== "unchanged")
                  .map((row) => (
                    <li
                      key={`${row.envType}:${row.name}`}
                      className="rounded-md border border-zinc-200 px-2.5 py-1.5 dark:border-zinc-800"
                    >
                      <div className="flex items-center gap-2">
                        <span className="font-medium">
                          {ENV_TYPE_META[row.envType].label}
                        </span>
                        <span>{row.name}</span>
                        <span className="text-zinc-500">
                          {row.kind === "added"
                            ? "本机新增"
                            : row.kind === "missing"
                              ? "本机已无"
                              : `已变更：${row.differences.join("、")}`}
                        </span>
                      </div>
                      <div className="mt-1 break-all font-mono text-zinc-500">
                        {row.kind !== "added" && (
                          <div>
                            清单：{row.recordedStatus} ·{" "}
                            {row.recordedVersion ?? "版本未记录"} ·{" "}
                            {row.recordedPath}
                          </div>
                        )}
                        {row.kind !== "missing" && (
                          <div>
                            本机：{row.currentStatus} ·{" "}
                            {row.currentVersion ?? "版本未探测"} ·{" "}
                            {row.currentPath}
                          </div>
                        )}
                      </div>
                    </li>
                  ))}
              </ul>
            )}
            <ul className="mt-3 space-y-1 text-xs text-zinc-400">
              {manifestComparison.limitations.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          </>
        )}
      </Modal>

      <ConfirmDialog
        open={confirmUninstall !== null}
        onClose={() => {
          if (!busy) {
            setConfirmUninstall(null);
            setUninstallPreview(null);
            setConfirmedLegacyLinks([]);
            setConfirmedLegacyPathEntries([]);
            setUninstallPreviewError(null);
          }
        }}
        onConfirm={doUninstall}
        danger
        confirmText={uninstallMode === "detach" ? "确认停用" : confirmUninstall?.isExternalLink ? "移除管理" : confirmUninstall?.isLinkEntry ? "移除链接" : installerReady ? "执行原厂卸载" : "确认删除目录"}
        loading={busy?.startsWith("uninstall:") ?? false}
        confirmDisabled={previewingUninstall || !uninstallPreview || !!uninstallPreviewError || !uninstallPreview.scopeComplete || uninstallPreview.blocked.length > 0 || !legacyConfirmationsReady || !legacyPathConfirmationsReady}
         title={`${confirmUninstall?.isExternalLink ? "移除管理" : confirmUninstall?.isLinkEntry ? "移除受管链接" : installerReady ? "原厂卸载" : "删除受管目录"} ${confirmUninstall?.name ?? ""}`}
        message={
          <>
            {previewingUninstall && <p>正在核对可移除范围，请稍候...</p>}
            {uninstallPreviewError && <p role="alert" className="text-red-600 dark:text-red-400">无法生成卸载预览：{uninstallPreviewError}</p>}
            <div role="group" aria-label="选择处理方式" className="mb-3 flex gap-1.5">
              <button
                type="button"
                aria-pressed={uninstallMode === "uninstall"}
                disabled={busy !== null || previewingUninstall}
                onClick={() => void switchUninstallMode("uninstall")}
                className={`rounded-md border px-2.5 py-1 text-xs ${uninstallMode === "uninstall" ? "border-red-500 bg-red-50 font-medium text-red-700 dark:bg-red-950 dark:text-red-300" : "border-zinc-200 text-zinc-600 dark:border-zinc-800 dark:text-zinc-400"}`}
              >
                卸载（删除内容）
              </button>
              <button
                type="button"
                aria-pressed={uninstallMode === "detach"}
                disabled={busy !== null || previewingUninstall}
                onClick={() => void switchUninstallMode("detach")}
                className={`rounded-md border px-2.5 py-1 text-xs ${uninstallMode === "detach" ? "border-emerald-500 bg-emerald-50 font-medium text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300" : "border-zinc-200 text-zinc-600 dark:border-zinc-800 dark:text-zinc-400"}`}
              >
                停用（保留文件）
              </button>
            </div>
            {uninstallMode === "detach" && (
              <p className="mb-3 text-xs text-emerald-700 dark:text-emerald-400">
                停用只解除 EnvCon 的管理记录与当前版本链接，磁盘上的文件全部保留；不会调用原厂卸载器。
              </p>
            )}
            {uninstallPreview && <p className="text-xs text-amber-700 dark:text-amber-400">
              {installerReady
                ? "执行前会再次核对预览、原厂登记和卸载器文件哈希，范围变化时停止。将运行精确匹配的原厂卸载器；只在可识别范围内核验登记和运行时目录，目录外关联项仍可能未验证。"
                : "执行前会再次核对预览，范围变化时停止。仅移除所列目录、链接和有来源证据的用户 PATH 条目；来源未知的链接或历史 PATH 如需清理，必须逐项确认。不调用原厂卸载器，目录外关联项仍可能未验证，因此不等于完整卸载证明。"}
            </p>}
            <p>
              {uninstallMode === "detach"
                ? "停用后目录与文件全部原地保留，仅解除管理记录："
                : confirmUninstall?.isExternalLink || confirmUninstall?.isLinkEntry
                  ? "仅移除管理链接："
                  : installerReady
                    ? "将运行原厂卸载器并核验所选安装路径："
                    : "将永久删除安装目录及其中的文件（包括在该目录安装的包）："}
              <b className="break-all">{confirmUninstall?.path}</b>
            </p>
            {(confirmUninstall?.isExternalLink || confirmUninstall?.isLinkEntry) && <p className="mt-2 break-all">原安装目录保留：{confirmUninstall.identityPath}</p>}
            {uninstallPreview && <div className="mt-3 space-y-2 text-xs">
              <p className={uninstallPreview.scopeComplete && uninstallPreview.blocked.length === 0 ? "text-emerald-700 dark:text-emerald-400" : "text-red-600 dark:text-red-400"}>
                {uninstallPreview.scopeComplete && uninstallPreview.blocked.length === 0 ? "已生成可核对的卸载范围。" : "卸载范围不完整，已禁用执行。"}
              </p>
              {([
                ["将移除", uninstallPreview.removable, "text-red-600 dark:text-red-400"],
                ["明确保留", uninstallPreview.retained, "text-emerald-700 dark:text-emerald-400"],
                ["未验证/不自动处理", uninstallPreview.unverified, "text-amber-700 dark:text-amber-400"],
                ["阻止执行", uninstallPreview.blocked, "text-red-600 dark:text-red-400"],
              ] as const).map(([title, items, color]) => items.length > 0 && <section key={title}>
                <h4 className={`font-medium ${color}`}>{title}</h4>
                <ul className="mt-1 space-y-1">
                  {items.map((item, index) => <li key={`${item.resource}:${index}`} className="break-all">
                    <span className="font-medium">{item.resource}</span>
                    {item.path && <span className="font-mono"> — {item.path}</span>}
                    <span className="block text-zinc-500">{item.details}{item.count != null ? `（${item.count} 项）` : ""}</span>
                  </li>)}
                </ul>
              </section>)}
              {(uninstallPreview.legacyLinks?.length ?? 0) > 0 && <section className="rounded border border-amber-500/40 p-2">
                <h4 className="font-medium text-amber-700 dark:text-amber-400">来源未知的链接：需要逐项确认</h4>
                <p className="mt-1 text-zinc-500">默认保留这些入口。勾选只授权本次移除所列链接，不表示它们由 EnvCon 创建；不会删除链接的外部目标。</p>
                {uninstallPreview.legacyLinks?.map((item) => <label key={item.path} className="mt-2 flex items-start gap-2 break-all">
                  <input
                    type="checkbox"
                    className="mt-0.5 shrink-0"
                    checked={item.path !== null && confirmedLegacyLinks.includes(item.path)}
                    disabled={busy !== null || item.path === null}
                    onChange={(event) => {
                      const path = item.path;
                      if (!path) return;
                      const checked = event.target.checked;
                      setConfirmedLegacyLinks((current) => checked
                        ? [...current.filter((entry) => entry !== path), path]
                        : current.filter((entry) => entry !== path));
                    }}
                  />
                  <span>
                    <span className="font-medium">确认移除 {item.resource}</span>
                    <span className="block font-mono">{item.path}</span>
                    <span className="block text-zinc-500">{item.details}</span>
                  </span>
                </label>)}
              </section>}
              {(uninstallPreview.legacyPathEntries?.length ?? 0) > 0 && <section className="rounded border border-amber-500/40 p-2">
                <h4 className="font-medium text-amber-700 dark:text-amber-400">来源未知的历史 PATH：默认保留</h4>
                <p className="mt-1 text-zinc-500">逐项勾选仅授权本次移除预览列出的精确 PATH 字符串；不表示该项由 EnvCon 创建。共享依赖、重复或已变化的条目不会清理。</p>
                {uninstallPreview.legacyPathEntries?.map((item) => <label key={item.path} className="mt-2 flex items-start gap-2 break-all">
                  <input
                    type="checkbox"
                    className="mt-0.5 shrink-0"
                    checked={item.path !== null && confirmedLegacyPathEntries.includes(item.path)}
                    disabled={busy !== null || item.path === null}
                    onChange={(event) => {
                      const path = item.path;
                      if (!path) return;
                      const checked = event.target.checked;
                      setConfirmedLegacyPathEntries((current) => checked
                        ? [...current.filter((entry) => entry !== path), path]
                        : current.filter((entry) => entry !== path));
                    }}
                  />
                  <span>
                    <span className="font-medium">确认移除此历史 PATH 条目</span>
                    <span className="block font-mono">{item.path}</span>
                    <span className="block text-zinc-500">{item.details}</span>
                  </span>
                </label>)}
              </section>}
            </div>}
            <p className="mt-1.5 text-xs text-zinc-400">
              {confirmUninstall?.isCurrent ? "当前链接也会一并移除，需要重新选择当前环境。" : ""}
              {confirmUninstall?.isExternalLink || confirmUninstall?.isLinkEntry ? "之后可以重新扫描并纳入管理。" : "此操作不可恢复，请先关闭使用此环境的程序。"}
               仅自动清理有 EnvCon 来源证据且无共享依赖的用户 PATH 项；来源未知的历史固定项默认保留，需逐项授权。
               {!confirmUninstall?.isExternalLink && !confirmUninstall?.isLinkEntry && (installerReady
                 ? " 原厂卸载成功后才会清理有来源证据的 PATH；用户级包、外部 venv、共享缓存及目录外数据仍可能保留。"
                 : " 此操作不调用原厂卸载器；安装器登记、用户级包、缓存及目录外数据可能保留，不能视为完整卸载。")}
            </p>
          </>
        }
      />
    </PageShell>
  );
}
