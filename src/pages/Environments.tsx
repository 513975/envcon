import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  RefreshCw,
  FolderOpen,
  Check,
  Trash2,
  ScanSearch,
  X,
  FolderPlus,
} from "lucide-react";
import { PageShell } from "../components/layout/PageShell";
import { Card, CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { Spinner, Empty } from "../components/ui/Empty";
import { ConfirmDialog } from "../components/ui/Modal";
import { EnvTypeBadge } from "../components/EnvTypeBadge";
import { ScanResults } from "../components/ScanResults";
import { SelectionButton } from "../components/ui/SelectionButton";
import { api } from "../lib/api";
import { toast } from "../lib/toast";
import { useAppStore } from "../lib/store";
import { formatBytes } from "../lib/format";
import {
  ALL_ENV_TYPES,
  ENV_TYPE_META,
  type EnvType,
  type Overview,
  type ExternalEnv,
  type ManagedEnv,
} from "../lib/types";

export function Environments() {
  const [overview, setOverview] = useState<Overview | null>(null);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState<EnvType>("jdk");
  const [busy, setBusy] = useState<string | null>(null);
  const [confirmUninstall, setConfirmUninstall] = useState<ManagedEnv | null>(null);
  const [systemEnvs, setSystemEnvs] = useState<ExternalEnv[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const [projectDirs, setProjectDirs] = useState<string[]>(() => {
    try {
      const saved = localStorage.getItem("envcon.scan.projectDirs");
      const parsed = saved ? JSON.parse(saved) : [];
      return Array.isArray(parsed) ? parsed.filter((p): p is string => typeof p === "string").slice(0, 8) : [];
    } catch { return []; }
  });
  const [scanWarnings, setScanWarnings] = useState<string[]>([]);
  const [integrating, setIntegrating] = useState<string | null>(null);

  const refreshOverview = useAppStore((s) => s.refreshOverview);
  const overviewVersion = useAppStore((s) => s.overviewVersion);

  const load = async () => {
    setLoading(true);
    try {
      setOverview(await api.getOverview());
    } catch (e) {
      toast.error("加载失败", String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [overviewVersion]);

  useEffect(() => {
    localStorage.setItem("envcon.scan.projectDirs", JSON.stringify(projectDirs));
  }, [projectDirs]);

  const doSwitch = async (env: ManagedEnv) => {
    setBusy(`switch:${env.name}`);
    try {
      await api.switchEnv(env.envType, env.name);
      toast.success(`已切换 ${ENV_TYPE_META[env.envType].label}`, `${env.name} 已设为当前版本`);
      refreshOverview();
    } catch (e) {
      toast.error("切换失败", String(e));
    } finally {
      setBusy(null);
    }
  };

  const doUninstall = async () => {
    if (!confirmUninstall) return;
    const env = confirmUninstall;
    setBusy(`uninstall:${env.name}`);
    try {
      await api.uninstallEnv(env.envType, env.name);
      toast.success("已卸载", `${env.name} 已删除`);
      setConfirmUninstall(null);
      refreshOverview();
    } catch (e) {
      toast.error("卸载失败", String(e));
    } finally {
      setBusy(null);
    }
  };

  const doScanSystem = async () => {
    setScanning(true);
    try {
      const report = await api.scanSystem(projectDirs);
      setSystemEnvs(report.tools);
      setScanWarnings(report.warnings);
    } catch (e) {
      toast.error("扫描失败", String(e));
    } finally {
      setScanning(false);
    }
  };

  const addProject = async () => {
    try {
      const dir = await open({ directory: true, title: "选择项目或虚拟环境目录" });
      if (typeof dir === "string") {
        setProjectDirs((dirs) => dirs.includes(dir) ? dirs : [...dirs, dir].slice(0, 8));
      }
    } catch (error) { toast.error("选择目录失败", String(error)); }
  };

  const doIntegrate = async (env: ExternalEnv) => {
    if (!env.envType || !env.path) return;
    setIntegrating(env.path);
    try {
      const name = await api.integrateExternalEnv(env.envType, env.path, env.version);
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

  if (loading && !overview) return <Spinner className="py-24 flex justify-center" />;
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

  return (
    <PageShell
      title="环境管理"
      desc={overview.rootExists ? `根目录:${overview.root}` : "尚未设置管理根目录"}
      actions={
        <Button size="sm" onClick={load} loading={loading}>
          <RefreshCw className="size-3.5" />
          刷新
        </Button>
      }
    >
      {/* 类型 Tab */}
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
      <Card>
        {category?.scanWarning && <div className="px-4 py-2 text-xs text-amber-700 dark:text-amber-300 border-b border-zinc-100 dark:border-zinc-800">{category.scanWarning}</div>}
        {category?.currentError && <div className="px-4 py-2 text-xs text-red-600 dark:text-red-400 border-b border-zinc-100 dark:border-zinc-800">{category.currentError}</div>}
        {category && category.envs.length > 0 ? (
          <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
            {category.envs.map((env) => (
              <div key={env.name} className="flex items-center gap-4 px-4 py-3.5">
                <EnvTypeBadge type={env.envType} size="sm" />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-semibold text-zinc-800 dark:text-zinc-100">
                      {env.name}
                    </span>
                    {env.isCurrent && <Badge color="green">当前</Badge>}
                    {env.status !== "available" && <Badge color="red">{env.status === "inaccessible" ? "不可访问" : "损坏"}</Badge>}
                    {env.isExternalLink && <Badge color="blue">外部链接</Badge>}
                  </div>
                  <div className="mt-0.5 flex items-center gap-3 text-xs text-zinc-500">
                    <span>{env.version ?? "未知版本"}</span>
                    <span>{formatBytes(env.sizeBytes)}</span>
                    {!env.sizeComplete && <span className="text-amber-600 dark:text-amber-400">大小统计不完整</span>}
                    <span className="truncate max-w-64 selectable" title={env.path}>
                      {env.path}
                    </span>
                  </div>
                  {env.statusDetail && <div className="mt-1 text-xs text-red-600 dark:text-red-400">{env.statusDetail}</div>}
                </div>
                <div className="flex items-center gap-1.5 shrink-0">
                  {!env.isCurrent && (
                    <Button
                      size="sm"
                      variant="primary"
                      loading={busy === `switch:${env.name}`}
                      disabled={env.status !== "available"}
                      onClick={() => doSwitch(env)}
                    >
                      <Check className="size-3.5" />
                      设为当前
                    </Button>
                  )}
                  <Button
                    size="sm"
                    onClick={() => api.openPath(env.path).catch((e) => toast.error("打开失败", String(e)))}
                  >
                    <FolderOpen className="size-3.5" />
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    className="text-red-500 hover:bg-red-50 dark:hover:bg-red-950"
                    onClick={() => setConfirmUninstall(env)}
                  >
                    <Trash2 className="size-3.5" />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <Empty
            title={`暂无 ${ENV_TYPE_META[tab].label} 环境`}
            desc="去下载中心安装一个吧"
          />
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
            <Button size="sm" onClick={addProject} disabled={scanning || projectDirs.length >= 8}>
              <FolderPlus className="size-3.5" />项目目录
            </Button>
            <Button size="sm" onClick={doScanSystem} loading={scanning}>
              <ScanSearch className="size-3.5" />
              {systemEnvs ? "重新扫描" : "开始扫描"}
            </Button>
            </div>
          }
        />
        {projectDirs.length > 0 && <div className="px-4 py-2 space-y-1">
          {projectDirs.map((dir) => <div key={dir} className="flex items-center gap-2 text-xs text-zinc-500">
            <span className="min-w-0 flex-1 break-all">{dir}</span>
            <button disabled={scanning} title="移除扫描目录" aria-label={`移除 ${dir}`}
              onClick={() => setProjectDirs((dirs) => dirs.filter((p) => p !== dir))}>
              <X className="size-3.5" />
            </button>
          </div>)}
        </div>}
        {scanWarnings.length > 0 && <details className="px-4 py-2 text-xs text-amber-700 dark:text-amber-400 space-y-1">
          <summary className="cursor-pointer">扫描提示（{scanWarnings.length}）</summary>
          {scanWarnings.map((warning, i) => <p key={i} className="break-all">{warning}</p>)}
        </details>}
        {systemEnvs === null ? (
          <Empty
            title="尚未扫描"
          />
        ) : systemEnvs.length === 0 ? (
          <Empty title="未发现可用工具" />
        ) : (
          <ScanResults rows={systemEnvs} canIntegrate={overview.rootExists} integrating={integrating} onIntegrate={doIntegrate} />
        )}
      </Card>

      {/* 卸载确认 */}
      <ConfirmDialog
        open={confirmUninstall !== null}
        onClose={() => setConfirmUninstall(null)}
        onConfirm={doUninstall}
        danger
        confirmText="卸载"
        loading={busy?.startsWith("uninstall:") ?? false}
        title={`卸载 ${confirmUninstall?.name ?? ""}`}
        message={
          <>
            <p>
              将永久删除 <b>{confirmUninstall?.path}</b>
            </p>
            <p className="mt-1.5 text-xs text-zinc-400">
              占用 {formatBytes(confirmUninstall?.sizeBytes ?? null)}
              {confirmUninstall?.isCurrent ? ",当前链接也会一并移除" : ""}
              ,此操作不可恢复。
            </p>
          </>
        }
      />
    </PageShell>
  );
}
