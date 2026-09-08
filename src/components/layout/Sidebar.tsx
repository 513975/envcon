import {
  LayoutDashboard,
  Boxes,
  Download,
  Route,
  Settings,
  Package,
} from "lucide-react";
import { useAppStore, type Page } from "../../lib/store";
import { cn } from "../../lib/format";

const NAV: { id: Page; label: string; icon: typeof LayoutDashboard }[] = [
  { id: "dashboard", label: "仪表盘", icon: LayoutDashboard },
  { id: "environments", label: "环境管理", icon: Boxes },
  { id: "download", label: "下载中心", icon: Download },
  { id: "paths", label: "路径管理", icon: Route },
  { id: "settings", label: "设置", icon: Settings },
];

export function Sidebar() {
  const page = useAppStore((s) => s.page);
  const setPage = useAppStore((s) => s.setPage);

  return (
    <aside className="w-52 shrink-0 flex flex-col border-r border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900">
      {/* Logo */}
      <div className="flex items-center gap-2.5 px-4 h-14 border-b border-zinc-100 dark:border-zinc-800">
        <div className="size-8 rounded-lg bg-gradient-to-br from-emerald-500 to-teal-600 flex items-center justify-center shadow-sm">
          <Package className="size-4 text-white" />
        </div>
        <div>
          <div className="text-sm font-bold tracking-tight text-zinc-900 dark:text-white">
            EnvCon
          </div>
          <div className="text-[10px] text-zinc-400 dark:text-zinc-500 -mt-0.5">
            开发环境管理
          </div>
        </div>
      </div>

      {/* 导航 */}
      <nav className="flex-1 px-2.5 py-3 space-y-1">
        {NAV.map((item) => {
          const Icon = item.icon;
          const active = page === item.id;
          return (
            <button
              key={item.id}
              onClick={() => setPage(item.id)}
              className={cn(
                "w-full flex items-center gap-2.5 px-3 h-9 rounded-lg text-sm font-medium transition-colors cursor-pointer relative",
                active
                  ? "bg-emerald-50 dark:bg-emerald-950/60 text-emerald-700 dark:text-emerald-400"
                  : "text-zinc-600 dark:text-zinc-400 hover:bg-zinc-100 dark:hover:bg-zinc-800",
              )}
            >
              {active && (
                <span className="absolute left-0 top-1/2 -translate-y-1/2 h-4 w-0.5 rounded-full bg-emerald-500" />
              )}
              <Icon className="size-4" />
              {item.label}
            </button>
          );
        })}
      </nav>

      <div className="px-4 py-3 text-[10px] text-zinc-400 dark:text-zinc-600 border-t border-zinc-100 dark:border-zinc-800">
        v0.1.0 · 便携模式就绪
      </div>
    </aside>
  );
}
