import { useEffect, useState } from "react";
import {
  RefreshCw,
  FolderOpen,
  Check,
  Trash2,
  ScanSearch,
  ExternalLink,
} from "lucide-react";
import { PageShell } from "../components/layout/PageShell";
import { Card, CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { Spinner, Empty } from "../components/ui/Empty";
import { ConfirmDialog } from "../components/ui/Modal";
import { EnvTypeBadge } from "../components/EnvTypeBadge";
import { api } from "../lib/api";
import { toast } from "../lib/toast";
import { useAppStore } from "../lib/store";
import { formatBytes, cn } from "../lib/format";
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
      setSystemEnvs(await api.scanSystem());
    } catch (e) {
      toast.error("扫描失败", String(e));
    } finally {
      setScanning(false);
    }
  };

  if (loading && !overview) return <Spinner className="py-24 flex justify-center" />;
  if (!overview || !overview.rootExists) {
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
      desc={`根目录:${overview.root}`}
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
            <button
              key={t}
              onClick={() => setTab(t)}
              className={cn(
                "h-8 px-3 rounded-lg text-xs font-medium transition-colors inline-flex items-center gap-1.5 cursor-pointer",
                tab === t
                  ? "bg-zinc-900 dark:bg-zinc-100 text-white dark:text-zinc-900"
                  : "bg-white dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-800 text-zinc-600 dark:text-zinc-300 hover:border-zinc-300",
              )}
            >
              {ENV_TYPE_META[t].label}
              {count > 0 && (
                <span
                  className={cn(
                    "rounded px-1 text-[10px]",
                    tab === t ? "bg-white/20" : "bg-zinc-100 dark:bg-zinc-800",
                  )}
                >
                  {count}
                </span>
              )}
              {active && <span className="size-1.5 rounded-full bg-emerald-500" />}
            </button>
          );
        })}
      </div>

      {/* 环境列表 */}
      <Card>
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
                  </div>
                  <div className="mt-0.5 flex items-center gap-3 text-xs text-zinc-500">
                    <span>{env.version ?? "未知版本"}</span>
                    <span>{formatBytes(env.sizeBytes)}</span>
                    <span className="truncate max-w-64 selectable" title={env.path}>
                      {env.path}
                    </span>
                  </div>
                </div>
                <div className="flex items-center gap-1.5 shrink-0">
                  {!env.isCurrent && (
                    <Button
                      size="sm"
                      variant="primary"
                      loading={busy === `switch:${env.name}`}
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
              系统散装环境
            </span>
          }
          desc="扫描 PATH 中由其他方式安装的工具(不受 EnvCon 管理)"
          actions={
            <Button size="sm" onClick={doScanSystem} loading={scanning}>
              {systemEnvs ? "重新扫描" : "开始扫描"}
            </Button>
          }
        />
        {systemEnvs === null ? (
          <Empty
            title="尚未扫描"
            desc="点击“开始扫描”检测本机 PATH 中的外部开发环境"
          />
        ) : systemEnvs.length === 0 ? (
          <Empty title="未发现散装环境" desc="PATH 中没有 EnvCon 之外安装的开发工具" />
        ) : (
          <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
            {systemEnvs.map((env, i) => (
              <div key={i} className="flex items-center gap-3 px-4 py-2.5">
                <Badge color="blue">{env.tool}</Badge>
                <span className="text-xs font-mono text-zinc-600 dark:text-zinc-300">
                  {env.version ?? "—"}
                </span>
                <span className="flex-1 truncate text-xs text-zinc-400 selectable" title={env.path ?? ""}>
                  {env.path}
                </span>
                <button
                  className="text-zinc-400 hover:text-emerald-600 cursor-pointer"
                  title="打开所在目录"
                  onClick={() =>
                    env.path && api.openPath(env.path).catch((e) => toast.error("打开失败", String(e)))
                  }
                >
                  <ExternalLink className="size-3.5" />
                </button>
              </div>
            ))}
          </div>
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
