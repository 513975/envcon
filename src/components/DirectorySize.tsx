import { HardDrive, X } from "lucide-react";
import { api } from "../lib/api";
import { formatBytes } from "../lib/format";
import { toast } from "../lib/toast";
import { pollSizes, startSize, useSizeStore } from "../lib/sizeStore";
import type { EnvType } from "../lib/types";
import { Button } from "./ui/Button";

export function DirectorySize({ path, envType = null, name = null, cacheTool = null }: { path: string; envType?: EnvType | null; name?: string | null; cacheTool?: string | null }) {
  const { tasks, starting, error } = useSizeStore();
  const key = cacheTool ? `cache:${cacheTool}` : path;
  const matches = tasks.filter(t => t.key.toLowerCase() === key.toLowerCase() && t.path.replace(/\\/g, "/").toLowerCase() === path.replace(/\\/g, "/").toLowerCase());
  const task = matches[matches.length - 1];
  const running = task?.status === "running";
  const anyRunning = tasks.some(t => t.status === "running");
  return <div className="flex flex-wrap items-center gap-1.5 text-xs min-w-0">
    <span>{task ? `${task.complete ? "" : "已读取 "}${formatBytes(task.bytes)}` : "尚未统计大小"}</span>
    {task && <span className="text-zinc-500">{task.files} 文件 / {task.directories} 目录{task.status === "canceled" ? " · 已取消" : ""}{task.status === "partial" ? " · 部分结果" : ""}</span>}
    {running ? <Button size="sm" variant="ghost" title="取消大小统计" aria-label="取消大小统计" onClick={() => api.cancelSizeScan(task.id).then(pollSizes).catch(e => toast.error("取消失败", String(e)))}><X className="size-3.5" /></Button> :
      <Button size="sm" variant="ghost" title="统计目录大小" aria-label={`统计 ${name ?? cacheTool} 大小`} disabled={starting || anyRunning} onClick={() => startSize(envType, name, cacheTool).catch(e => toast.error("统计失败", String(e)))}><HardDrive className="size-3.5" /></Button>}
    {running && <span className="truncate max-w-64" title={task.currentPath}>统计中：{task.currentPath || path}</span>}
    {(task?.detail || error) && <span className="text-amber-700 dark:text-amber-400 break-all">{task?.detail ?? error}</span>}
  </div>;
}
