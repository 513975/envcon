import { useEffect, useState } from "react";
import {
  RefreshCw,
  Plus,
  Trash2,
  ArrowUp,
  ArrowDown,
  Zap,
  FolderOpen,
  Eraser,
  Lock,
  Check,
  Variable,
  Wand2,
  Package,
} from "lucide-react";
import { PageShell } from "../components/layout/PageShell";
import { Card, CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { Spinner, Empty } from "../components/ui/Empty";
import { ConfirmDialog } from "../components/ui/Modal";
import { api } from "../lib/api";
import { toast } from "../lib/toast";
import { formatBytes, cn } from "../lib/format";
import type { PathState, CacheInfo, EnvVarInfo, ToolConfig } from "../lib/types";

export function Paths() {
  // PATH 状态
  const [pathState, setPathState] = useState<PathState | null>(null);
  const [userPath, setUserPath] = useState<string[]>([]);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [integrating, setIntegrating] = useState(false);
  const [newEntry, setNewEntry] = useState("");

  // 环境变量
  const [envVars, setEnvVars] = useState<EnvVarInfo[] | null>(null);
  const [envVarEdits, setEnvVarEdits] = useState<Record<string, string>>({});
  const [envVarSaving, setEnvVarSaving] = useState<string | null>(null);

  // 包管理器路径配置
  const [toolConfigs, setToolConfigs] = useState<ToolConfig[] | null>(null);
  const [toolBusy, setToolBusy] = useState<string | null>(null);

  // 缓存
  const [caches, setCaches] = useState<CacheInfo[] | null>(null);
  const [cacheLoading, setCacheLoading] = useState(false);
  const [confirmClean, setConfirmClean] = useState<CacheInfo | null>(null);
  const [cleaning, setCleaning] = useState(false);

  const loadPath = async () => {
    try {
      const s = await api.getPathState();
      setPathState(s);
      setUserPath(s.user);
      setDirty(false);
    } catch (e) {
      toast.error("读取 PATH 失败", String(e));
    }
  };

  const loadEnvVars = async () => {
    try {
      const vars = await api.getEnvVars();
      setEnvVars(vars);
      setEnvVarEdits({});
    } catch (e) {
      toast.error("读取环境变量失败", String(e));
    }
  };

  const loadToolConfigs = async () => {
    try {
      setToolConfigs(await api.getToolConfigs());
    } catch (e) {
      toast.error("读取包管理器配置失败", String(e));
    }
  };

  const loadCaches = async () => {
    setCacheLoading(true);
    try {
      setCaches(await api.getCaches());
    } catch (e) {
      toast.error("缓存检测失败", String(e));
    } finally {
      setCacheLoading(false);
    }
  };

  useEffect(() => {
    loadPath();
    loadCaches();
    loadEnvVars();
    loadToolConfigs();
  }, []);

  /** 应用单个工具的默认路径 */
  const applyTool = async (t: ToolConfig) => {
    setToolBusy(t.tool);
    try {
      const applied = await api.applyToolConfig(
        t.tool,
        t.defaultGlobal ?? null,
        t.defaultCache ?? null,
      );
      toast.success(`${t.name} 已配置`, applied.join("\n") || undefined);
      await loadToolConfigs();
    } catch (e) {
      toast.error(`${t.name} 配置失败`, String(e));
    } finally {
      setToolBusy(null);
    }
  };

  /** 一键:全部可用工具的路径指向 根目录\globals */
  const applyAllTools = async () => {
    if (!toolConfigs) return;
    const targets = toolConfigs.filter(
      (t) => t.available && (t.defaultGlobal || t.defaultCache),
    );
    if (targets.length === 0) {
      toast.info("没有可配置的工具", "未检测到 npm/pnpm/yarn/pip");
      return;
    }
    setToolBusy("__all__");
    let ok = 0;
    for (const t of targets) {
      try {
        await api.applyToolConfig(t.tool, t.defaultGlobal ?? null, t.defaultCache ?? null);
        ok += 1;
      } catch {
        /* 单个失败继续 */
      }
    }
    setToolBusy(null);
    if (ok > 0) {
      toast.success(`已配置 ${ok} 个工具`, "全局与缓存路径已指向 根目录\\globals");
    }
    await loadToolConfigs();
  };

  const saveEnvVar = async (v: EnvVarInfo) => {
    const value = envVarEdits[v.name] ?? "";
    setEnvVarSaving(v.name);
    try {
      await api.saveEnvVar(v.name, value);
      toast.success(
        value.trim() ? `${v.name} 已保存` : `${v.name} 已删除`,
        "修改前已自动备份,新开终端即生效",
      );
      await loadEnvVars();
    } catch (e) {
      toast.error("保存失败", String(e));
    } finally {
      setEnvVarSaving(null);
    }
  };

  /** 一键将所有"当前值 ≠ 建议值且有建议"的变量设为建议值 */
  const applyAllSuggested = async () => {
    if (!envVars) return;
    const targets = envVars.filter(
      (v) => v.suggested && v.value !== v.suggested,
    );
    if (targets.length === 0) {
      toast.info("无需更新", "所有可推导变量均已指向当前环境");
      return;
    }
    setEnvVarSaving("__all__");
    let ok = 0;
    for (const v of targets) {
      try {
        await api.saveEnvVar(v.name, v.suggested!);
        ok += 1;
      } catch {
        /* 单个失败继续 */
      }
    }
    setEnvVarSaving(null);
    if (ok > 0) {
      toast.success(`已更新 ${ok} 个变量`, "全部指向当前激活环境,新开终端即生效");
    }
    await loadEnvVars();
  };

  const save = async () => {
    setSaving(true);
    try {
      await api.saveUserPath(userPath);
      toast.success("用户 PATH 已保存", "修改前已自动备份,新开终端即生效");
      setDirty(false);
      await loadPath();
    } catch (e) {
      toast.error("保存失败", String(e));
    } finally {
      setSaving(false);
    }
  };

  const integrate = async () => {
    setIntegrating(true);
    try {
      const added = await api.integrateCurrentToPath();
      if (added.length > 0) {
        toast.success(`已集成 ${added.length} 条路径`, added.join("\n"));
      } else {
        toast.info("PATH 已是最新", "所有已激活环境的路径都已在 PATH 中");
      }
      await loadPath();
    } catch (e) {
      toast.error("集成失败", String(e));
    } finally {
      setIntegrating(false);
    }
  };

  const doClean = async () => {
    if (!confirmClean) return;
    setCleaning(true);
    try {
      const freed = await api.cleanCache(confirmClean.tool);
      toast.success("缓存已清理", `释放 ${formatBytes(freed)}`);
      setConfirmClean(null);
      await loadCaches();
    } catch (e) {
      toast.error("清理失败", String(e));
    } finally {
      setCleaning(false);
    }
  };

  const move = (i: number, dir: -1 | 1) => {
    const j = i + dir;
    if (j < 0 || j >= userPath.length) return;
    const next = [...userPath];
    [next[i], next[j]] = [next[j], next[i]];
    setUserPath(next);
    setDirty(true);
  };

  const update = (i: number, v: string) => {
    const next = [...userPath];
    next[i] = v;
    setUserPath(next);
    setDirty(true);
  };

  const remove = (i: number) => {
    setUserPath(userPath.filter((_, idx) => idx !== i));
    setDirty(true);
  };

  const add = () => {
    const v = newEntry.trim();
    if (!v) return;
    setUserPath([...userPath, v]);
    setNewEntry("");
    setDirty(true);
  };

  return (
    <PageShell
      title="路径管理"
      desc="用户 PATH 编辑 / 环境一键集成 / 缓存清理"
      actions={
        <Button size="sm" onClick={loadPath}>
          <RefreshCw className="size-3.5" />
          刷新
        </Button>
      }
    >
      {/* 一键集成 */}
      <Card className="mb-4 p-4 flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="size-9 rounded-lg bg-emerald-50 dark:bg-emerald-950/60 flex items-center justify-center">
            <Zap className="size-4.5 text-emerald-600 dark:text-emerald-400" />
          </div>
          <div>
            <div className="text-sm font-semibold">一键集成到 PATH</div>
            <div className="text-xs text-zinc-500">
              把已激活环境的可执行目录(current\*)自动补进用户 PATH
            </div>
          </div>
        </div>
        <Button variant="primary" size="sm" onClick={integrate} loading={integrating}>
          <Zap className="size-3.5" />
          立即集成
        </Button>
      </Card>

      {/* 用户 PATH 编辑器 */}
      <Card>
        <CardHeader
          title="用户 PATH"
          desc="保存前自动备份;修改对新开的终端立即生效"
          actions={
            <>
              {dirty && (
                <Button size="sm" variant="primary" onClick={save} loading={saving}>
                  <Check className="size-3.5" />
                  保存
                </Button>
              )}
            </>
          }
        />
        {!pathState ? (
          <Spinner />
        ) : userPath.length === 0 ? (
          <Empty title="用户 PATH 为空" />
        ) : (
          <div className="divide-y divide-zinc-50 dark:divide-zinc-800/50">
            {userPath.map((p, i) => (
              <div key={i} className="flex items-center gap-1.5 px-4 py-2 group">
                <span className="w-6 text-[10px] text-zinc-400 text-right">{i + 1}</span>
                <input
                  value={p}
                  onChange={(e) => update(i, e.target.value)}
                  className="flex-1 h-7 rounded-md border border-transparent hover:border-zinc-200 dark:hover:border-zinc-700 focus:border-emerald-500 bg-transparent px-2 text-xs font-mono outline-none transition-colors"
                />
                <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
                  <button
                    onClick={() => move(i, -1)}
                    disabled={i === 0}
                    className="p-1 rounded text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-200 disabled:opacity-30 cursor-pointer"
                  >
                    <ArrowUp className="size-3" />
                  </button>
                  <button
                    onClick={() => move(i, 1)}
                    disabled={i === userPath.length - 1}
                    className="p-1 rounded text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-200 disabled:opacity-30 cursor-pointer"
                  >
                    <ArrowDown className="size-3" />
                  </button>
                  <button
                    onClick={() => api.openPath(p).catch(() => {})}
                    className="p-1 rounded text-zinc-400 hover:text-emerald-600 cursor-pointer"
                    title="打开目录(如存在)"
                  >
                    <FolderOpen className="size-3" />
                  </button>
                  <button
                    onClick={() => remove(i)}
                    className="p-1 rounded text-zinc-400 hover:text-red-500 cursor-pointer"
                  >
                    <Trash2 className="size-3" />
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
        <div className="flex items-center gap-2 px-4 py-3 border-t border-zinc-100 dark:border-zinc-800">
          <Plus className="size-3.5 text-zinc-400" />
          <input
            value={newEntry}
            onChange={(e) => setNewEntry(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && add()}
            placeholder="添加新路径,如 D:\tools\bin"
            className="flex-1 h-7 bg-transparent text-xs font-mono outline-none placeholder:text-zinc-300 dark:placeholder:text-zinc-600"
          />
          <Button size="sm" onClick={add} disabled={!newEntry.trim()}>
            添加
          </Button>
        </div>
      </Card>

      {/* 系统 PATH(只读) */}
      <Card className="mt-5">
        <CardHeader
          title={
            <span className="flex items-center gap-1.5">
              <Lock className="size-3.5 text-zinc-400" />
              系统 PATH(只读)
            </span>
          }
          desc="修改系统 PATH 需要管理员权限,EnvCon 出于安全仅作展示"
        />
        <div className="px-4 py-2 divide-y divide-zinc-50 dark:divide-zinc-800/50">
          {pathState?.system.map((p, i) => (
            <div key={i} className="py-1.5 text-xs font-mono text-zinc-500 dark:text-zinc-400 selectable">
              {p}
            </div>
          ))}
        </div>
      </Card>

      {/* 环境变量管理 */}
      <Card className="mt-5">
        <CardHeader
          title={
            <span className="flex items-center gap-1.5">
              <Variable className="size-3.5 text-zinc-400" />
              开发环境变量
            </span>
          }
          desc="JAVA_HOME / GOPATH 等用户级变量;修改前自动备份,新开终端即生效"
          actions={
            <>
              <Button
                size="sm"
                onClick={applyAllSuggested}
                loading={envVarSaving === "__all__"}
              >
                <Wand2 className="size-3.5" />
                指向当前环境
              </Button>
              <Button size="sm" onClick={loadEnvVars}>
                <RefreshCw className="size-3.5" />
              </Button>
            </>
          }
        />
        {!envVars ? (
          <Spinner />
        ) : (
          <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
            {envVars.map((v) => {
              const editing = envVarEdits[v.name] !== undefined;
              const value = envVarEdits[v.name] ?? v.value ?? "";
              const matchesSuggested =
                v.suggested != null && v.value === v.suggested;
              return (
                <div key={v.name} className="flex items-center gap-3 px-4 py-2.5">
                  <div className="w-40 shrink-0">
                    <div className="text-xs font-semibold font-mono">{v.name}</div>
                    <div className="text-[10px] text-zinc-400">{v.label}</div>
                  </div>
                  <input
                    value={value}
                    placeholder={v.suggested ?? "未设置"}
                    onChange={(e) =>
                      setEnvVarEdits((m) => ({ ...m, [v.name]: e.target.value }))
                    }
                    onKeyDown={(e) =>
                      e.key === "Enter" && editing && saveEnvVar(v)
                    }
                    className={cn(
                      "flex-1 h-7 rounded-md border bg-transparent px-2 text-xs font-mono outline-none transition-colors",
                      editing
                        ? "border-emerald-500"
                        : "border-transparent hover:border-zinc-200 dark:hover:border-zinc-700",
                      v.value == null && !editing && "text-zinc-400",
                    )}
                  />
                  {matchesSuggested && !editing && <Badge color="green">已指向当前</Badge>}
                  <div className="flex items-center gap-1.5 shrink-0">
                    {editing && (
                      <Button
                        size="sm"
                        variant="primary"
                        onClick={() => saveEnvVar(v)}
                        loading={envVarSaving === v.name}
                      >
                        <Check className="size-3.5" />
                        保存
                      </Button>
                    )}
                    {!editing && v.suggested && v.value !== v.suggested && (
                      <Button
                        size="sm"
                        variant="ghost"
                        title={`设为 ${v.suggested}`}
                        onClick={() =>
                          setEnvVarEdits((m) => ({
                            ...m,
                            [v.name]: v.suggested!,
                          }))
                        }
                      >
                        <Wand2 className="size-3.5" />
                      </Button>
                    )}
                    {v.value != null && !editing && (
                      <Button
                        size="sm"
                        variant="ghost"
                        className="text-red-500 hover:bg-red-50 dark:hover:bg-red-950"
                        title="删除此变量"
                        onClick={() => {
                          setEnvVarEdits((m) => ({ ...m, [v.name]: "" }));
                          setTimeout(() => {
                            api
                              .saveEnvVar(v.name, "")
                              .then(() => {
                                toast.success(`${v.name} 已删除`);
                                loadEnvVars();
                              })
                              .catch((e) => toast.error("删除失败", String(e)));
                          }, 0);
                        }}
                      >
                        <Trash2 className="size-3.5" />
                      </Button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </Card>

      {/* 包管理器路径配置 */}
      <Card className="mt-5">
        <CardHeader
          title={
            <span className="flex items-center gap-1.5">
              <Package className="size-3.5 text-zinc-400" />
              包管理器路径
            </span>
          }
          desc="npm / pnpm / yarn / pip 的全局安装与缓存路径,一键收纳到 根目录\\globals"
          actions={
            <>
              <Button
                size="sm"
                variant="primary"
                onClick={applyAllTools}
                loading={toolBusy === "__all__"}
              >
                <Wand2 className="size-3.5" />
                一键配置全部
              </Button>
              <Button size="sm" onClick={loadToolConfigs} loading={toolBusy === null && !toolConfigs}>
                <RefreshCw className="size-3.5" />
              </Button>
            </>
          }
        />
        {!toolConfigs ? (
          <Spinner />
        ) : (
          <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
            {toolConfigs.map((t) => (
              <div key={t.tool} className="flex items-center gap-3 px-4 py-3">
                <div className="w-16 shrink-0">
                  <div className="text-sm font-semibold">{t.name}</div>
                  {!t.available && (
                    <div className="text-[10px] text-zinc-400">未安装</div>
                  )}
                </div>
                <div className="min-w-0 flex-1 grid grid-cols-2 gap-x-4 gap-y-1">
                  <div>
                    <div className="text-[10px] text-zinc-400">全局安装</div>
                    <div
                      className="text-xs font-mono truncate selectable"
                      title={t.globalPath ?? "默认(用户目录)"}
                    >
                      {t.defaultGlobal
                        ? t.globalPath ?? <span className="text-zinc-400">未自定义</span>
                        : "—"}
                    </div>
                  </div>
                  <div>
                    <div className="text-[10px] text-zinc-400">缓存</div>
                    <div
                      className="text-xs font-mono truncate selectable"
                      title={t.cachePath ?? "默认(用户目录)"}
                    >
                      {t.cachePath ?? <span className="text-zinc-400">未自定义</span>}
                    </div>
                  </div>
                </div>
                <Button
                  size="sm"
                  disabled={!t.available || !t.defaultCache}
                  loading={toolBusy === t.tool}
                  title={t.defaultGlobal ? `设为 ${t.defaultGlobal} / ${t.defaultCache}` : undefined}
                  onClick={() => applyTool(t)}
                >
                  <Wand2 className="size-3.5" />
                  收纳到根目录
                </Button>
              </div>
            ))}
          </div>
        )}
      </Card>

      {/* 缓存管理 */}
      <Card className="mt-5">
        <CardHeader
          title="开发缓存"
          desc="包管理器与构建工具的本地缓存,可安全清理"
          actions={
            <Button size="sm" onClick={loadCaches} loading={cacheLoading}>
              <RefreshCw className="size-3.5" />
              重新统计
            </Button>
          }
        />
        {cacheLoading && !caches ? (
          <Spinner />
        ) : (
          <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
            {(caches ?? []).map((c) => (
              <div key={c.tool} className="flex items-center gap-3 px-4 py-3">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium">{c.name}</span>
                    {!c.exists && <Badge>未检测到</Badge>}
                  </div>
                  <div className="mt-0.5 text-xs text-zinc-400 truncate selectable" title={c.path}>
                    {c.path}
                  </div>
                </div>
                <span className={cn("text-sm font-semibold tabular-nums", c.exists ? "text-zinc-700 dark:text-zinc-200" : "text-zinc-400")}>
                  {c.exists ? formatBytes(c.sizeBytes) : "—"}
                </span>
                <div className="flex items-center gap-1.5">
                  <Button
                    size="sm"
                    onClick={() => api.openPath(c.path).catch(() => toast.error("打开失败", "目录不存在"))}
                  >
                    <FolderOpen className="size-3.5" />
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    className="text-red-500 hover:bg-red-50 dark:hover:bg-red-950 disabled:hidden"
                    disabled={!c.exists || (c.sizeBytes ?? 0) === 0}
                    onClick={() => setConfirmClean(c)}
                  >
                    <Eraser className="size-3.5" />
                    清理
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </Card>

      <ConfirmDialog
        open={confirmClean !== null}
        onClose={() => setConfirmClean(null)}
        onConfirm={doClean}
        danger
        confirmText="清理"
        loading={cleaning}
        title={`清理 ${confirmClean?.name ?? ""}`}
        message={
          <>
            <p>
              将删除 <b className="selectable">{confirmClean?.path}</b> 下的全部内容
              (释放约 {formatBytes(confirmClean?.sizeBytes ?? null)})。
            </p>
            <p className="mt-1.5 text-xs text-zinc-400">
              缓存清理是安全的,下次安装依赖时会自动重新下载。
            </p>
          </>
        }
      />
    </PageShell>
  );
}
