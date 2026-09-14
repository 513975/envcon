import { useEffect, useState } from "react";
import {
  FolderOpen,
  HardDrive,
  Usb,
  History,
  RotateCcw,
  Package,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { PageShell } from "../components/layout/PageShell";
import { Card, CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { Empty } from "../components/ui/Empty";
import { api } from "../lib/api";
import { toast } from "../lib/toast";
import { APP_VERSION } from "../lib/version";
import { useAppStore } from "../lib/store";
import { cn } from "../lib/format";

interface SettingsView {
  root: string | null;
  downloadsDir: string | null;
  portable: boolean;
  dataDir: string;
}

interface Backup {
  file: string;
  time: string;
}

export function Settings() {
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [backups, setBackups] = useState<Backup[]>([]);
  const [rootInput, setRootInput] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const refreshOverview = useAppStore((s) => s.refreshOverview);

  const load = async () => {
    try {
      setSettings(await api.getSettings());
      setBackups(await api.listPathBackups());
    } catch (e) {
      toast.error("加载设置失败", String(e));
    }
  };

  useEffect(() => {
    load();
  }, []);

  const pickRoot = async () => {
    const dir = await open({ directory: true, title: "选择管理根目录" });
    if (typeof dir === "string") {
      setRootInput(dir);
    }
  };

  const applyRoot = async () => {
    if (!rootInput.trim()) return;
    setBusy("root");
    try {
      await api.setRoot(rootInput.trim());
      toast.success("根目录已更新", rootInput.trim());
      setRootInput("");
      refreshOverview();
      await load();
    } catch (e) {
      toast.error("设置失败", String(e));
    } finally {
      setBusy(null);
    }
  };

  const pickDownloads = async () => {
    const dir = await open({ directory: true, title: "选择下载临时目录" });
    if (typeof dir === "string") {
      setBusy("dl");
      try {
        await api.setDownloadsDir(dir);
        toast.success("下载目录已更新", dir);
        await load();
      } catch (e) {
        toast.error("设置失败", String(e));
      } finally {
        setBusy(null);
      }
    }
  };

  const restore = async (b: Backup) => {
    setBusy(`restore:${b.file}`);
    try {
      await api.restorePathBackup(b.file);
      toast.success("PATH 已恢复", `已还原到 ${b.time} 的备份`);
    } catch (e) {
      toast.error("恢复失败", String(e));
    } finally {
      setBusy(null);
    }
  };

  if (!settings) return null;

  return (
    <PageShell title="设置" desc="管理根目录 / 存储位置 / PATH 备份">
      {/* 管理根目录 */}
      <Card>
        <CardHeader
          title={
            <span className="flex items-center gap-1.5">
              <HardDrive className="size-3.5 text-zinc-400" />
              管理根目录
            </span>
          }
          desc="所有环境安装在 根目录/envs 下,current 链接在 根目录/current 下"
        />
        <div className="p-4">
          <div className="flex items-center gap-2 mb-3">
            <code className="text-xs bg-zinc-100 dark:bg-zinc-800 rounded-md px-2.5 py-1.5 font-mono selectable">
              {settings.root ?? "未设置"}
            </code>
            {settings.root && (
              <Button
                size="sm"
                onClick={() => api.openPath(settings.root!).catch((e) => toast.error("打开失败", String(e)))}
              >
                <FolderOpen className="size-3.5" />
              </Button>
            )}
          </div>
          <div className="flex items-center gap-2">
            <input
              value={rootInput}
              onChange={(e) => setRootInput(e.target.value)}
              placeholder="D:\DevEnv"
              className="h-9 flex-1 rounded-lg border border-zinc-200 dark:border-zinc-700 bg-white dark:bg-zinc-800 px-3 text-sm outline-none focus:border-emerald-500"
            />
            <Button onClick={pickRoot}>浏览…</Button>
            <Button variant="primary" onClick={applyRoot} loading={busy === "root"} disabled={!rootInput.trim()}>
              应用
            </Button>
          </div>
          <p className="mt-2 text-xs text-zinc-400">更换根目录后,旧目录中的环境不会被移动,新环境将安装到新位置。</p>
        </div>
      </Card>

      {/* 存储位置 */}
      <Card className="mt-5">
        <CardHeader
          title={
            <span className="flex items-center gap-1.5">
              <Usb className="size-3.5 text-zinc-400" />
              存储位置
            </span>
          }
          desc="配置文件与下载缓存的位置"
        />
        <div className="p-4 space-y-3">
          <div className="flex items-center justify-between gap-3">
            <div>
              <div className="text-sm font-medium">配置数据目录</div>
              <div className="text-xs text-zinc-400 font-mono selectable">{settings.dataDir}</div>
            </div>
            <Badge color={settings.portable ? "green" : "gray"}>
              {settings.portable ? "便携模式" : "标准模式"}
            </Badge>
          </div>
          <div className="flex items-center justify-between gap-3">
            <div>
              <div className="text-sm font-medium">下载临时目录</div>
              <div className="text-xs text-zinc-400 font-mono selectable">
                {settings.downloadsDir ?? `${settings.root ?? ""}\\downloads(默认)`}
              </div>
            </div>
            <Button size="sm" onClick={pickDownloads} loading={busy === "dl"}>
              更改
            </Button>
          </div>
          {settings.portable && (
            <div className={cn(
              "rounded-lg border border-emerald-200 dark:border-emerald-900",
              "bg-emerald-50 dark:bg-emerald-950/40 px-3.5 py-2.5 text-xs text-emerald-700 dark:text-emerald-400",
            )}>
              便携模式:把 EnvCon.exe 放入 U 盘任意目录即可携带配置,环境安装路径建议也指向 U 盘。
            </div>
          )}
        </div>
      </Card>

      {/* PATH 备份 */}
      <Card className="mt-5">
        <CardHeader
          title={
            <span className="flex items-center gap-1.5">
              <History className="size-3.5 text-zinc-400" />
              PATH 备份
            </span>
          }
          desc="每次修改用户 PATH 前自动备份,保留最近 20 份"
        />
        {backups.length === 0 ? (
          <Empty title="暂无备份" desc="修改 PATH 后会自动在这里生成备份" />
        ) : (
          <div className="divide-y divide-zinc-100 dark:divide-zinc-800 max-h-72 overflow-y-auto">
            {backups.map((b) => (
              <div key={b.file} className="flex items-center gap-3 px-4 py-2.5">
                <History className="size-3.5 text-zinc-400 shrink-0" />
                <span className="flex-1 text-xs text-zinc-600 dark:text-zinc-300">{b.time}</span>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => restore(b)}
                  loading={busy === `restore:${b.file}`}
                >
                  <RotateCcw className="size-3.5" />
                  恢复
                </Button>
              </div>
            ))}
          </div>
        )}
      </Card>

      {/* 关于 */}
      <Card className="mt-5 p-4 flex items-center gap-3">
        <div className="size-9 rounded-lg bg-gradient-to-br from-emerald-500 to-teal-600 flex items-center justify-center">
          <Package className="size-4 text-white" />
        </div>
        <div className="flex-1">
          <div className="text-sm font-bold">EnvCon v{APP_VERSION}</div>
          <div className="text-xs text-zinc-400">Windows 开发环境一站式管理工具 · Tauri 2</div>
        </div>
      </Card>
    </PageShell>
  );
}
