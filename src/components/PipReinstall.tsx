import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Package, Square } from "lucide-react";
import { Modal } from "./ui/Modal";
import { Button } from "./ui/Button";
import { api } from "../lib/api";
import type { PipReinstallPlan, PipReinstallStatus } from "../lib/types";
import { OldPackageCleanup } from "./OldPackageCleanup";
import { useTaskStatus } from "../lib/useTaskStatus";

export function PipReinstall({
  initialSource,
  onClose,
}: {
  initialSource?: string;
  onClose: () => void;
}) {
  const [sourceKind, setSourceKind] = useState<"python" | "directory">(
    initialSource ? "directory" : "python",
  );
  const [source, setSource] = useState(initialSource ?? "");
  const [python, setPython] = useState("");
  const [destination, setDestination] = useState("");
  const [plan, setPlan] = useState<PipReinstallPlan | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const {
    task,
    setTask,
    initializing,
    statusError,
    beginMutation,
    endMutation,
  } = useTaskStatus<PipReinstallStatus>("pip", api.pipReinstallStatus);
  const [busy, setBusy] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const [error, setError] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [stopping, setStopping] = useState(false);
  const running = task?.status === "running";
  const invalidate = () => {
    setPlan(null);
    setConfirmed(false);
    setError("");
  };
  const browse = async (which: "source" | "python" | "destination") => {
    try {
      const directory =
        which === "destination" ||
        (which === "source" && sourceKind === "directory");
      const value = await open({
        directory,
        title:
          which === "destination"
            ? "选择新环境的父目录"
            : "选择 Python 或旧包目录",
        ...(!directory
          ? { filters: [{ name: "Python", extensions: ["exe"] }] }
          : {}),
      });
      if (typeof value === "string") {
        if (which === "source") setSource(value);
        else if (which === "python") setPython(value);
        else setDestination(`${value.replace(/[\\/]$/, "")}\\python-restored`);
        invalidate();
      }
    } catch (e) {
      setError(String(e));
    }
  };
  const preview = async () => {
    if (initializing || busy || cleaning || running) return;
    setBusy(true);
    invalidate();
    try {
      const value = await api.previewPipReinstall(
        source.trim(),
        sourceKind,
        python.trim(),
        destination.trim(),
      );
      setPlan(value);
      setSelected(value.packages.filter((p) => !p.reason).map((p) => p.name));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const start = async () => {
    if (!plan || !confirmed || initializing || busy || cleaning || running)
      return;
    beginMutation();
    setBusy(true);
    setError("");
    setStopping(false);
    try {
      await api.startPipReinstall(plan, selected);
      setPlan(null);
      setConfirmed(false);
      setTask(await api.pipReinstallStatus());
    } catch (e) {
      setError(String(e));
    } finally {
      endMutation();
      setBusy(false);
    }
  };
  const stop = async () => {
    try {
      await api.cancelPipReinstall();
      setStopping(true);
    } catch (e) {
      setError(String(e));
    }
  };
  const input = (
    label: string,
    value: string,
    setter: (v: string) => void,
    which: "source" | "python" | "destination",
  ) => (
    <label className="block text-xs">
      {label}
      <div className="flex gap-2 mt-1">
        <input
          aria-label={label}
          value={value}
          disabled={busy || cleaning || running}
          onChange={(e) => {
            setter(e.target.value);
            invalidate();
          }}
          className="min-w-0 flex-1 rounded border border-zinc-300 dark:border-zinc-700 bg-transparent p-2 text-xs font-mono"
        />
        <Button
          disabled={busy || cleaning || running}
          title={`选择${label}`}
          onClick={() => browse(which)}
        >
          <FolderOpen className="size-4" />
        </Button>
      </div>
    </label>
  );
  return (
    <Modal
      open
      title="Python 包重装"
      onClose={busy || cleaning ? () => {} : onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={busy || cleaning}>
            {running ? "后台运行" : "关闭"}
          </Button>
          {running ? (
            <Button onClick={stop} disabled={stopping}>
              <Square className="size-3.5" />
              {stopping ? "当前包结束后停止" : "停止后续安装"}
            </Button>
          ) : plan ? (
            <Button
              variant="primary"
              loading={busy}
              disabled={
                initializing || cleaning || !confirmed || selected.length === 0
              }
              onClick={start}
            >
              <Package className="size-3.5" />
              开始重装
            </Button>
          ) : (
            <Button
              variant="primary"
              loading={busy}
              disabled={
                initializing ||
                cleaning ||
                !source.trim() ||
                !python.trim() ||
                !destination.trim()
              }
              onClick={preview}
            >
              读取包清单
            </Button>
          )}
        </>
      }
    >
      <div className="max-h-[58vh] overflow-y-auto space-y-3 pr-1">
        {initializing && (
          <p role="status" className="text-xs">
            正在读取任务状态
          </p>
        )}
        {!running && (
          <>
            <div className="flex gap-3 text-xs">
              <label>
                <input
                  type="radio"
                  name="pip-source"
                  checked={sourceKind === "python"}
                  disabled={busy}
                  onChange={() => {
                    setSourceKind("python");
                    setSource("");
                    invalidate();
                  }}
                />{" "}
                旧解释器
              </label>
              <label>
                <input
                  type="radio"
                  name="pip-source"
                  checked={sourceKind === "directory"}
                  disabled={busy}
                  onChange={() => {
                    setSourceKind("directory");
                    setSource("");
                    invalidate();
                  }}
                />{" "}
                旧 site-packages
              </label>
            </div>
            {input(
              sourceKind === "python"
                ? "旧 python.exe"
                : "旧 site-packages 目录",
              source,
              setSource,
              "source",
            )}
            {input("目标 python.exe", python, setPython, "python")}
            {input(
              "新虚拟环境目录",
              destination,
              setDestination,
              "destination",
            )}
            {plan && (
              <>
                <p className="text-xs">
                  Python {plan.sourceVersion} → {plan.targetVersion}
                </p>
                <div className="max-h-48 overflow-auto border-y border-zinc-200 dark:border-zinc-800 divide-y divide-zinc-100 dark:divide-zinc-800">
                  {plan.packages.map((p) => (
                    <label
                      key={p.name}
                      className="flex gap-2 py-2 text-xs items-start"
                    >
                      <input
                        type="checkbox"
                        disabled={!!p.reason || busy}
                        checked={selected.includes(p.name)}
                        onChange={(e) =>
                          setSelected((old) =>
                            e.target.checked
                              ? [...old, p.name]
                              : old.filter((n) => n !== p.name),
                          )
                        }
                      />
                      <span className="min-w-0 break-all">
                        <span className="font-mono">
                          {p.name}=={p.version}
                        </span>
                        {p.reason && (
                          <span className="block text-amber-700 dark:text-amber-300">
                            {p.reason}
                          </span>
                        )}
                      </span>
                    </label>
                  ))}
                </div>
                <p className="text-xs">
                  已选 {selected.length} / {plan.packages.length} 个包
                </p>
                <label className="flex gap-2 text-xs items-start">
                  <input
                    type="checkbox"
                    checked={confirmed}
                    disabled={busy}
                    onChange={(e) => setConfirmed(e.target.checked)}
                  />
                  在新虚拟环境中下载并安装所选版本；失败保留结果，不改变旧环境
                </label>
              </>
            )}
          </>
        )}
        {task && (
          <div className="space-y-2 text-xs">
            <p role="status">
              {task.status === "done"
                ? "已完成"
                : task.status === "partial"
                  ? "部分失败"
                  : task.status === "error"
                    ? "重装失败"
                    : task.status === "canceled"
                      ? "已停止"
                      : "正在重装"}
              ：{task.message}
            </p>
            <div className="font-mono break-all selectable">
              {task.destination}
            </div>
            {task.items.map((p) => (
              <div key={p.name} className="break-all">
                <span className="font-mono">
                  {p.name}=={p.version}
                </span>{" "}
                ·{" "}
                {(
                  {
                    pending: "等待",
                    installing: "安装中",
                    installed: "已安装",
                    failed: "失败",
                    canceled: "已停止",
                  } as Record<string, string>
                )[p.status] ?? p.status}
                {p.detail && (
                  <p className="text-red-600 dark:text-red-400 whitespace-pre-wrap">
                    {p.detail}
                  </p>
                )}
              </div>
            ))}
            {task.dependencyCheck && (
              <p className="whitespace-pre-wrap break-all">
                依赖检查：{task.dependencyCheck}
              </p>
            )}
            <div className="break-all selectable">报告：{task.report}</div>
            {task.status === "done" && (
              <OldPackageCleanup
                key={task.report}
                tool="pip"
                destination={task.destination}
                onBusyChange={setCleaning}
              />
            )}
          </div>
        )}
        {(error || statusError) && (
          <p
            role="alert"
            className="text-xs text-red-600 dark:text-red-400 whitespace-pre-wrap break-all"
          >
            {error || statusError}
          </p>
        )}
      </div>
    </Modal>
  );
}
