import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Package, Square } from "lucide-react";
import { api } from "../lib/api";
import type {
  GlobalReinstallPlan,
  GlobalReinstallStatus,
  ToolConfig,
} from "../lib/types";
import { Button } from "./ui/Button";
import { Modal } from "./ui/Modal";
import { OldPackageCleanup } from "./OldPackageCleanup";
import { GlobalSourceSelect } from "./GlobalSourceSelect";
import { useTaskStatus } from "../lib/useTaskStatus";

const states: Record<string, string> = {
  running: "正在重装",
  done: "已完成",
  partial: "部分失败",
  error: "重装失败",
  canceled: "已停止",
  pending: "等待",
  installing: "安装中",
  installed: "已安装",
  failed: "失败",
};

export function GlobalPackageReinstall({
  tool,
  initialSource,
  onClose,
}: {
  tool: ToolConfig;
  initialSource?: string;
  onClose: () => void;
}) {
  const [executable, setExecutable] = useState(tool.executablePath ?? "");
  const [source, setSource] = useState(initialSource ?? "");
  const [destination, setDestination] = useState(
    tool.globalPath ?? tool.defaultGlobal ?? "",
  );
  const [plan, setPlan] = useState<GlobalReinstallPlan | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const {
    task,
    setTask,
    initializing,
    statusError,
    beginMutation,
    endMutation,
  } = useTaskStatus<GlobalReinstallStatus>(tool.tool, () =>
    api.globalReinstallStatus(tool.tool),
  );
  const [busy, setBusy] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const [confirmed, setConfirmed] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState("");
  const running = task?.status === "running";
  const locked = busy || cleaning || running || initializing;

  const invalidate = () => {
    setPlan(null);
    setConfirmed(false);
    setError("");
  };
  const browse = async (which: "executable" | "source" | "destination") => {
    setBusy(true);
    try {
      const value = await open({
        directory: which !== "executable",
        title:
          which === "destination" ? "选择目标全局目录" : "选择启动器或旧目录",
        ...(which === "executable"
          ? {
              filters: [
                { name: "包管理器", extensions: ["cmd", "exe", "bat"] },
              ],
            }
          : {}),
      });
      if (typeof value === "string") {
        if (which === "executable") setExecutable(value);
        else if (which === "source") setSource(value);
        else setDestination(value);
        invalidate();
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const preview = async () => {
    if (locked) return;
    setBusy(true);
    invalidate();
    try {
      const p = await api.previewGlobalReinstall(
        tool.tool as GlobalReinstallPlan["tool"],
        executable.trim(),
        source.trim(),
        destination.trim(),
      );
      setPlan(p);
      setSelected(p.packages.filter((x) => !x.reason).map((x) => x.name));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const start = async () => {
    if (!plan || !selected.length || !confirmed || locked) return;
    beginMutation();
    setBusy(true);
    setError("");
    setStopping(false);
    try {
      await api.startGlobalReinstall(plan, selected);
      setPlan(null);
      setConfirmed(false);
      setTask(await api.globalReinstallStatus(tool.tool));
    } catch (e) {
      setError(String(e));
    } finally {
      endMutation();
      setBusy(false);
    }
  };
  const stop = async () => {
    setStopping(true);
    try {
      await api.cancelGlobalReinstall(tool.tool);
    } catch (e) {
      setError(String(e));
      setStopping(false);
    }
  };
  const showPath = async (path: string) => {
    try {
      await api.openPath(path);
    } catch (e) {
      setError(String(e));
    }
  };
  const input = (
    label: string,
    value: string,
    setter: (value: string) => void,
    which: "executable" | "source" | "destination",
  ) => (
    <label className="block text-xs">
      {label}
      <div className="mt-1 flex gap-2">
        <input
          aria-label={label}
          value={value}
          disabled={locked}
          onChange={(e) => {
            setter(e.target.value);
            invalidate();
          }}
          className="min-w-0 flex-1 rounded border border-zinc-300 dark:border-zinc-700 bg-transparent p-2 text-xs font-mono"
        />
        <Button
          disabled={locked}
          title={`选择${label}`}
          aria-label={`选择${label}`}
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
      title={`${tool.name} 重装旧包`}
      onClose={busy || cleaning ? () => {} : onClose}
      footer={
        <>
          <Button disabled={busy || cleaning} onClick={onClose}>
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
                initializing || cleaning || !selected.length || !confirmed
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
                !destination.trim() ||
                !executable.trim()
              }
              onClick={preview}
            >
              读取包清单
            </Button>
          )}
        </>
      }
    >
      <div className="space-y-3">
        {initializing && (
          <p role="status" className="text-xs">
            正在读取任务状态
          </p>
        )}
        {!running && (
          <>
            {input("包管理器启动器", executable, setExecutable, "executable")}
            <GlobalSourceSelect
              tool={tool.tool}
              disabled={locked}
              onSelect={(path) => {
                setSource(path);
                invalidate();
              }}
            />
            {input("旧全局包目录", source, setSource, "source")}
            {input("目标全局目录", destination, setDestination, "destination")}
            {plan && (
              <>
                <p className="text-xs">
                  {tool.name} {plan.toolVersion} · 已选 {selected.length} /{" "}
                  {plan.packages.length} 个包
                </p>
                <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-xs">
                  <dt>全局目录</dt>
                  <dd className="font-mono break-all selectable">
                    {plan.destination}
                  </dd>
                  <dt>命令目录</dt>
                  <dd className="font-mono break-all selectable">
                    {plan.binPath}
                  </dd>
                  <dt>{tool.tool === "pnpm" ? "包存储" : "安装缓存"}</dt>
                  <dd className="font-mono break-all selectable">
                    {plan.cachePath}
                  </dd>
                </dl>
                {plan.packages.length === 0 && (
                  <p className="text-xs">未发现已安装的全局包</p>
                )}
                <div className="max-h-48 overflow-auto border-y border-zinc-200 dark:border-zinc-800 divide-y divide-zinc-100 dark:divide-zinc-800">
                  {plan.packages.map((p) => (
                    <label
                      key={p.name}
                      className="flex items-start gap-2 py-2 text-xs"
                    >
                      <input
                        type="checkbox"
                        disabled={!!p.reason || busy}
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
                          {p.name}@{p.version || "?"}
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
                <label className="flex items-start gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={confirmed}
                    disabled={busy}
                    onChange={(e) => setConfirmed(e.target.checked)}
                  />
                  下载并安装所选版本及其依赖，允许包的安装脚本运行；保留旧目录及现有
                  PATH
                </label>
              </>
            )}
          </>
        )}
        {task && (
          <div className="space-y-2 text-xs">
            <p role="status">
              {states[task.status] ?? task.status}：{task.message}
            </p>
            <div className="font-mono break-all selectable">
              目标：{task.destination}
            </div>
            <div className="font-mono break-all selectable">
              命令目录：{task.binPath}
            </div>
            {task.items.map((p) => (
              <div key={p.name} className="break-all">
                <span className="font-mono">
                  {p.name}@{p.version}
                </span>{" "}
                · {states[p.status] ?? p.status}
                {p.detail && (
                  <p className="text-red-600 dark:text-red-400 whitespace-pre-wrap">
                    {p.detail}
                  </p>
                )}
              </div>
            ))}
            <div className="break-all selectable">报告：{task.report}</div>
            <div className="flex flex-wrap gap-2">
              <Button size="sm" onClick={() => showPath(task.destination)}>
                <FolderOpen className="size-3.5" />
                打开目标
              </Button>
              <Button size="sm" onClick={() => showPath(task.report)}>
                <FolderOpen className="size-3.5" />
                打开报告
              </Button>
            </div>
            {task.status === "done" && (
              <OldPackageCleanup
                key={task.report}
                tool={tool.tool}
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
