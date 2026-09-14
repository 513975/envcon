import { useState } from "react";
import { FolderOpen, Trash2 } from "lucide-react";
import { api } from "../lib/api";
import type {
  OldPackageCleanupPreview,
  OldPackageCleanupResult,
} from "../lib/types";
import { Button } from "./ui/Button";

export function OldPackageCleanup({
  tool,
  destination,
  onBusyChange,
}: {
  tool: string;
  destination: string;
  onBusyChange: (busy: boolean) => void;
}) {
  const [preview, setPreview] = useState<OldPackageCleanupPreview | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [result, setResult] = useState<OldPackageCleanupResult | null>(null);
  const lock = (value: boolean) => {
    setBusy(value);
    onBusyChange(value);
  };
  const inspect = async () => {
    lock(true);
    setError("");
    setConfirmed(false);
    setPreview(null);
    try {
      const data = await api.previewOldPackageCleanup(tool, destination);
      setPreview(data);
      setSelected(data.packages.filter((p) => !p.reason).map((p) => p.name));
    } catch (e) {
      setError(String(e));
    } finally {
      lock(false);
    }
  };
  const remove = async () => {
    if (!preview || !confirmed || !selected.length || busy) return;
    lock(true);
    setError("");
    setResult(null);
    try {
      setResult(await api.cleanupOldPackages(tool, preview.token, selected));
    } catch (e) {
      setError(String(e));
    } finally {
      setPreview(null);
      setConfirmed(false);
      lock(false);
    }
  };
  return (
    <section
      aria-label="旧包清理"
      className="border-t border-zinc-200 dark:border-zinc-700 pt-3 space-y-3 text-xs"
    >
      <Button
        size="sm"
        onClick={inspect}
        loading={busy}
        title="核对新旧版本并预览卸载清单"
      >
        <Trash2 className="size-3.5" />
        清理已重装的旧包
      </Button>
      {busy && <p role="status">正在核对或卸载旧包，请保持应用运行</p>}
      {preview && (
        <>
          <p className="break-all selectable">旧来源：{preview.source}</p>
          {preview.warnings.map((warning, index) => (
            <p
              key={index}
              className="text-amber-700 dark:text-amber-300 break-all"
            >
              {warning}
            </p>
          ))}
          <div className="max-h-48 overflow-y-auto divide-y divide-zinc-200 dark:divide-zinc-800">
            {preview.packages.map((p) => (
              <label key={p.name} className="flex items-start gap-2 py-2">
                <input
                  type="checkbox"
                  disabled={busy || !!p.reason}
                  checked={selected.includes(p.name)}
                  onChange={(e) => {
                    setConfirmed(false);
                    setSelected((old) =>
                      e.target.checked
                        ? [...old, p.name]
                        : old.filter((n) => n !== p.name),
                    );
                  }}
                />
                <span className="min-w-0 break-all">
                  <span className="font-mono">
                    {p.name}@{p.version}
                  </span>
                  {p.path && (
                    <span className="block text-zinc-500 selectable">
                      {p.path}
                    </span>
                  )}
                  {p.reason && (
                    <span className="block text-amber-700 dark:text-amber-300">
                      {p.reason}
                    </span>
                  )}
                </span>
              </label>
            ))}
          </div>
          {selected.length > 0 && (
            <label className="flex items-start gap-2">
              <input
                type="checkbox"
                disabled={busy}
                checked={confirmed}
                onChange={(e) => setConfirmed(e.target.checked)}
              />
              已验证新环境可用并切换所需路径，确认卸载所选旧包；不会放入回收站
            </label>
          )}
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="danger"
              disabled={busy || !confirmed || !selected.length}
              onClick={remove}
            >
              <Trash2 className="size-3.5" />
              卸载 {selected.length} 个旧包
            </Button>
            <Button
              size="sm"
              disabled={busy}
              onClick={() => {
                setPreview(null);
                setConfirmed(false);
              }}
            >
              保留旧包
            </Button>
          </div>
        </>
      )}
      {result && (
        <div className="space-y-2">
          <p role="status">
            旧包清理：
            {result.items.filter((p) => p.status === "removed").length}{" "}
            个已卸载，
            {result.items.filter((p) => p.status !== "removed").length} 个未完成
          </p>
          {result.items.map((p) => (
            <div key={p.name} className="break-all">
              <span className="font-mono">{p.name}</span> ·{" "}
              {p.status === "removed" ? "已卸载" : "未完成"}
              {p.detail && (
                <p className="text-amber-700 dark:text-amber-300 whitespace-pre-wrap">
                  {p.detail}
                </p>
              )}
            </div>
          ))}
          <Button
            size="sm"
            onClick={() =>
              api.openPath(result.report).catch((e) => setError(String(e)))
            }
          >
            <FolderOpen className="size-3.5" />
            打开清理报告
          </Button>
        </div>
      )}
      {error && (
        <p
          role="alert"
          className="text-red-600 dark:text-red-400 whitespace-pre-wrap break-all"
        >
          {error}
        </p>
      )}
    </section>
  );
}
