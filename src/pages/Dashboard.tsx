import { useEffect, useState } from "react";
import { HardDrive, Package, RefreshCw, Terminal, Zap, FolderOpen, Download as DownloadIcon } from "lucide-react";
import { PageShell } from "../components/layout/PageShell";
import { Card, CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { Spinner, Empty } from "../components/ui/Empty";
import { EnvTypeBadge } from "../components/EnvTypeBadge";
import { api } from "../lib/api";
import { toast } from "../lib/toast";
import { useAppStore } from "../lib/store";
import { formatBytes, cn } from "../lib/format";
import type { Overview } from "../lib/types";
import { ENV_TYPE_META } from "../lib/types";

export function Dashboard() {
  const [overview, setOverview] = useState<Overview | null>(null);
  const [loading, setLoading] = useState(true);
  const [rootSetting, setRootSetting] = useState(false);
  const [rootInput, setRootInput] = useState("");
  const refreshOverview = useAppStore((s) => s.refreshOverview);
  const overviewVersion = useAppStore((s) => s.overviewVersion);
  const setPage = useAppStore((s) => s.setPage);

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

  const applyRoot = async () => {
    if (!rootInput.trim()) return;
    try {
      await api.setRoot(rootInput.trim());
      toast.success("管理根目录已设置", rootInput.trim());
      setRootSetting(false);
      refreshOverview();
    } catch (e) {
      toast.error("设置失败", String(e));
    }
  };

  if (loading && !overview) return <Spinner className="py-24 flex justify-center" />;

  if (overview && !overview.rootExists) {
    // 首次使用引导
    return (
      <PageShell title="仪表盘" desc="欢迎使用 EnvCon">
        <Card>
          <Empty
            icon={<Harddrive />}
            title="设置管理根目录"
            desc="EnvCon 通过一个根目录统一管理所有开发环境(推荐空目录或沿用 D:\DevEnv)。环境将安装在 根目录/envs 下。"
            action={
              rootSetting ? (
                <div className="flex items-center gap-2">
                  <input
                    value={rootInput}
                    onChange={(e) => setRootInput(e.target.value)}
                    placeholder="D:\DevEnv"
                    className="h-9 w-72 rounded-lg border border-zinc-200 dark:border-zinc-700 bg-white dark:bg-zinc-800 px-3 text-sm outline-none focus:border-emerald-500"
                  />
                  <Button variant="primary" onClick={applyRoot}>
                    确定
                  </Button>
                  <Button onClick={() => setRootSetting(false)}>取消</Button>
                </div>
              ) : (
                <Button variant="primary" onClick={() => { setRootInput("D:\\DevEnvManager"); setRootSetting(true); }}>
                  选择根目录
                </Button>
              )
            }
          />
        </Card>
      </PageShell>
    );
  }

  const categories = overview?.categories ?? [];
  const managedCount = categories.reduce((n, c) => n + c.envs.length, 0);
  const totalSize = categories.reduce(
    (n, c) => n + c.envs.reduce((m, e) => m + (e.sizeBytes ?? 0), 0),
    0,
  );
  const currentEnvs = categories.filter((c) => c.current);

  return (
    <PageShell
      title="仪表盘"
      desc={overview ? `根目录:${overview.root}` : undefined}
      actions={
        <Button size="sm" onClick={load} loading={loading}>
          <RefreshCw className="size-3.5" />
          刷新
        </Button>
      }
    >
      {/* 统计卡片 */}
      <div className="grid grid-cols-3 gap-4 mb-5">
        <Card className="p-4">
          <div className="flex items-center gap-3">
            <div className="size-9 rounded-lg bg-emerald-50 dark:bg-emerald-950/60 flex items-center justify-center">
              <Package className="size-4.5 text-emerald-600 dark:text-emerald-400" />
            </div>
            <div>
              <div className="text-xl font-bold text-zinc-900 dark:text-white">{managedCount}</div>
              <div className="text-xs text-zinc-500">已管理环境</div>
            </div>
          </div>
        </Card>
        <Card className="p-4">
          <div className="flex items-center gap-3">
            <div className="size-9 rounded-lg bg-sky-50 dark:bg-sky-950/60 flex items-center justify-center">
              <HardDrive className="size-4.5 text-sky-600 dark:text-sky-400" />
            </div>
            <div>
              <div className="text-xl font-bold text-zinc-900 dark:text-white">{formatBytes(totalSize)}</div>
              <div className="text-xs text-zinc-500">环境占用空间</div>
            </div>
          </div>
        </Card>
        <Card className="p-4">
          <div className="flex items-center gap-3">
            <div className="size-9 rounded-lg bg-amber-50 dark:bg-amber-950/60 flex items-center justify-center">
              <Zap className="size-4.5 text-amber-600 dark:text-amber-400" />
            </div>
            <div>
              <div className="text-xl font-bold text-zinc-900 dark:text-white">{currentEnvs.length} / {categories.length}</div>
              <div className="text-xs text-zinc-500">已激活环境类型</div>
            </div>
          </div>
        </Card>
      </div>

      {/* 当前环境 */}
      <Card>
        <CardHeader
          title="当前激活环境"
          desc="通过 current 链接指向的版本,终端中使用的即是这些版本"
          actions={
            <Button size="sm" onClick={() => setPage("download")}>
              <DownloadIcon className="size-3.5" />
              下载新环境
            </Button>
          }
        />
        {currentEnvs.length === 0 ? (
          <Empty
            title="尚未激活任何环境"
            desc="在环境管理页点击“设为当前”即可激活,激活后终端立即可用"
            action={<Button size="sm" variant="primary" onClick={() => setPage("environments")}>去环境管理</Button>}
          />
        ) : (
          <div className="p-4 grid grid-cols-2 gap-3">
            {currentEnvs.map((c) => {
              const env = c.envs.find((e) => e.name === c.current);
              const meta = ENV_TYPE_META[c.envType];
              return (
                <div
                  key={c.envType}
                  className="rounded-lg border border-zinc-100 dark:border-zinc-800 p-3.5 hover:border-zinc-200 dark:hover:border-zinc-700 transition-colors"
                >
                  <div className="flex items-center justify-between">
                    <EnvTypeBadge type={c.envType} />
                    <Badge color="green">当前</Badge>
                  </div>
                  <div className="mt-2.5 flex items-baseline gap-2">
                    <span className="text-lg font-bold text-zinc-900 dark:text-white">
                      {env?.version ?? c.current}
                    </span>
                    <span className="text-xs text-zinc-400">{c.current}</span>
                  </div>
                  <div className="mt-1.5 flex items-center gap-3 text-xs text-zinc-500">
                    <span>{formatBytes(env?.sizeBytes)}</span>
                    <button
                      className="inline-flex items-center gap-1 hover:text-emerald-600 dark:hover:text-emerald-400 cursor-pointer"
                      onClick={() => env && api.openPath(env.path).catch((e) => toast.error("打开失败", String(e)))}
                    >
                      <FolderOpen className="size-3" />
                      {meta.label}目录
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </Card>

      {/* 快捷入口 */}
      <div className="grid grid-cols-3 gap-4 mt-5">
        {[
          { page: "environments" as const, icon: Package, title: "环境管理", desc: "切换版本 / 卸载" },
          { page: "download" as const, icon: DownloadIcon, title: "下载中心", desc: "国内镜像高速下载" },
          { page: "paths" as const, icon: Terminal, title: "路径管理", desc: "PATH 与缓存清理" },
        ].map(({ page, icon: Icon, title, desc }) => (
          <Card
            key={page}
            className="p-4 cursor-pointer hover:border-emerald-300 dark:hover:border-emerald-700 transition-colors"
            onClick={() => setPage(page)}
          >
            <div className="flex items-center gap-3">
              <div className="size-9 rounded-lg bg-zinc-100 dark:bg-zinc-800 flex items-center justify-center">
                <Icon className="size-4.5 text-zinc-500" />
              </div>
              <div>
                <div className="text-sm font-semibold">{title}</div>
                <div className="text-xs text-zinc-400">{desc}</div>
              </div>
            </div>
          </Card>
        ))}
      </div>

      {/* 便携模式指示 */}
      {overview?.portable && (
        <div className={cn(
          "mt-5 rounded-lg border border-emerald-200 dark:border-emerald-900",
          "bg-emerald-50 dark:bg-emerald-950/40 px-4 py-3 text-xs text-emerald-700 dark:text-emerald-400",
        )}>
          便携模式运行中 — 配置存储在 exe 旁的 data 目录,可直接放入 U 盘使用
        </div>
      )}
    </PageShell>
  );
}

function Harddrive() {
  return <HardDrive />;
}
