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
  }, []);

  // Preserve existing shortcuts; the new page uses Ctrl+6.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey || e.shiftKey || e.metaKey) return;
      const pages: Page[] = ["dashboard", "environments", "download", "paths", "settings", "packages"];
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
