import { useEffect, useState, type ComponentType } from "react";
import { Sidebar } from "./components/layout/Sidebar";
import { ToastContainer } from "./components/ui/Toast";
import { useAppStore, type Page } from "./lib/store";
import { startInstallListener } from "./lib/installStore";
import { Dashboard } from "./pages/Dashboard";
import { Environments } from "./pages/Environments";
import { Download } from "./pages/Download";
import { Paths } from "./pages/Paths";
import { Settings } from "./pages/Settings";
import { PackageManagers } from "./pages/PackageManagers";
import { api } from "./lib/api";
import { savedProjectDirs } from "./lib/scanPreferences";
import { startScanRevalidation } from "./lib/scanClient";
import { pollSizes } from "./lib/sizeStore";

const PAGES: { id: Page; Component: ComponentType }[] = [
  { id: "dashboard", Component: Dashboard },
  { id: "environments", Component: Environments },
  { id: "download", Component: Download },
  { id: "paths", Component: Paths },
  { id: "packages", Component: PackageManagers },
  { id: "settings", Component: Settings },
];

export default function App() {
  const page = useAppStore((s) => s.page);
  const setPage = useAppStore((s) => s.setPage);
  const overviewVersion = useAppStore((s) => s.overviewVersion);
  const [configWarning, setConfigWarning] = useState<string | null>(null);

  useEffect(() => {
    let canceled = false;
    void api.getSettings().then((settings) => {
      if (!canceled) setConfigWarning(settings?.configWarning ?? null);
    }).catch(() => {
      if (!canceled) setConfigWarning("读取应用配置失败，请进入设置检查并重试。");
    });
    return () => { canceled = true; };
  }, [overviewVersion]);

  // keep-alive:页面首次访问后保持挂载,切换仅显隐不卸载(避免重复加载)
  const [mounted, setMounted] = useState<Set<Page>>(() => new Set([page]));
  useEffect(() => {
    setMounted((prev) => {
      if (prev.has(page)) return prev;
      const next = new Set(prev);
      next.add(page);
      return next;
    });
  }, [page]);

  useEffect(() => {
    startInstallListener();
    void pollSizes();
    const stopRevalidation = startScanRevalidation({ overview: api.getOverview, tools: api.getToolConfigs, caches: api.getCaches, system: (force) => api.scanSystem(savedProjectDirs(), force) });
    const timer = window.setTimeout(() => {
      void Promise.allSettled([
        api.getOverview(),
        api.getToolConfigs(),
        api.scanSystem(savedProjectDirs()),
      ]);
    }, 100);
    const cacheTimer = window.setTimeout(() => {
      void api.getCaches().catch(() => {});
    }, 1500);
    return () => {
      window.clearTimeout(timer);
      window.clearTimeout(cacheTimer);
      stopRevalidation();
    };
  }, []);

  // Preserve existing shortcuts; the new page uses Ctrl+6.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey || e.shiftKey || e.metaKey) return;
      const pages: Page[] = [
        "dashboard",
        "environments",
        "download",
        "paths",
        "settings",
        "packages",
      ];
      const idx = parseInt(e.key, 10) - 1;
      if (idx >= 0 && idx < pages.length) {
        e.preventDefault();
        setPage(pages[idx]);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [setPage]);

  return (
    <div className="h-full flex">
      <Sidebar />
      <main className="flex-1 min-w-0 overflow-y-auto bg-zinc-50 dark:bg-zinc-950">
        {configWarning && (
          <div role="alert" className="m-4 rounded-lg border border-amber-300 bg-amber-50 p-3 text-xs text-amber-800 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200">
            <p className="break-words">{configWarning}</p>
            <button className="mt-2 underline cursor-pointer" onClick={() => setPage("settings")}>前往设置修复</button>
          </div>
        )}
        {PAGES.map(({ id, Component }) =>
          mounted.has(id) ? (
            <div key={id} className={page === id ? "contents" : "hidden"}>
              <Component />
            </div>
          ) : null,
        )}
      </main>
      <ToastContainer />
    </div>
  );
}
