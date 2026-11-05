import { useEffect, useState } from "react";
import {
  RefreshCw,
  Plus,
  Trash2,
  ArrowUp,
  ArrowDown,
  Zap,
  FolderOpen,
  Check,
  Variable,
  Wand2,
} from "lucide-react";
import { PageShell } from "../components/layout/PageShell";
import { Card, CardHeader } from "../components/ui/Card";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { Spinner, Empty } from "../components/ui/Empty";
import { SystemPathEditor } from "../components/SystemPathEditor";
import { api } from "../lib/api";
import { toast } from "../lib/toast";
import { cn } from "../lib/format";
import type { PathState, EnvVarInfo, EnvScope } from "../lib/types";

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
  // 用户级与系统级是两套独立的值：同名变量在两个 hive 里可以不同。
  const [envScope, setEnvScope] = useState<EnvScope>("user");
  // 当前进程是否已具备管理员权限；系统级写入需要它（或走提权子进程）。
  const [elevated, setElevated] = useState(false);
  // 不在常见清单里的变量（用户自行添加的），按当前作用域读取。
  const [customVars, setCustomVars] = useState<{ name: string; value: string }[] | null>(null);
  const [newVarName, setNewVarName] = useState("");
  const [newVarValue, setNewVarValue] = useState("");

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
      // 权限状态决定系统级写入是直接写还是走提权子进程，提示文案也不同。
      setElevated(await api.isElevated().catch(() => false));
      // 自定义变量按作用域读取；读失败降级为空表，不拖垮整页。
      setCustomVars(await api.getCustomEnvVars(envScope).catch(() => []));
    } catch (e) {
      toast.error("读取环境变量失败", String(e));
    }
  };

  useEffect(() => {
    loadPath();
    loadEnvVars();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [envScope]);

  /** 当前作用域下某个变量的值。用户级与系统级是两套独立的值。 */
  const valueOf = (v: EnvVarInfo) =>
    envScope === "system" ? v.systemValue : v.value;

  const saveEnvVar = async (v: EnvVarInfo) => {
    const value = envVarEdits[v.name] ?? "";
    setEnvVarSaving(v.name);
    try {
      await api.saveEnvVar(v.name, value, envScope);
      toast.success(
        value.trim() ? `${v.name} 已保存` : `${v.name} 已删除`,
        envScope === "system"
          ? "已写入系统环境变量；修改前已自动备份，新开终端即生效"
          : "修改前已自动备份，新开终端即生效",
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
      (v) => v.suggested && valueOf(v) !== v.suggested,
    );
    if (targets.length === 0) {
      toast.info("无需更新", `所有可推导变量在${envScope === "system" ? "系统" : "用户"}级均已指向当前环境`);
      return;
    }
    setEnvVarSaving("__all__");
    let ok = 0;
    const failures: string[] = [];
    for (const v of targets) {
      try {
        await api.saveEnvVar(v.name, v.suggested!, envScope);
        ok += 1;
      } catch (e) {
        failures.push(`${v.name}: ${String(e)}`);
      }
    }
    setEnvVarSaving(null);
    if (failures.length) toast.error(`${failures.length} 个变量未更新`, failures.join("\n"));
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
      desc="用户 / 系统 PATH 与开发环境变量"
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
          desc="保存前自动备份；新开的终端会读取更新后的 PATH"
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

      <SystemPathEditor entries={pathState?.system} onSaved={async () => {
        try {
          setPathState(await api.getPathState());
        } catch (e) {
          toast.error("读取 PATH 失败", String(e));
        }
      }} />

      {/* 环境变量管理 */}
      <Card className="mt-5">
        <CardHeader
          title={
            <span className="flex items-center gap-1.5">
              <Variable className="size-3.5 text-zinc-400" />
              开发环境变量
            </span>
          }
          desc={
            envScope === "system"
              ? "系统级变量（HKLM），对所有用户生效；写入需要管理员权限，会弹出一次授权"
              : "用户级变量（HKCU），仅对当前用户生效；修改前自动备份，新开终端即生效"
          }
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
        {/* 作用域切换：用户级与系统级是两套独立的值，必须让用户看清在改哪一套 */}
        <div
          role="group"
          aria-label="环境变量作用域"
          className="flex flex-wrap items-center gap-1.5 border-b border-zinc-100 px-4 py-2 dark:border-zinc-800"
        >
          <button
            type="button"
            aria-pressed={envScope === "user"}
            disabled={envVarSaving !== null}
            onClick={() => {
              setEnvScope("user");
              setEnvVarEdits({});
            }}
            className={cn(
              "rounded-md border px-2.5 py-1 text-xs",
              envScope === "user"
                ? "border-emerald-500 bg-emerald-50 font-medium text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300"
                : "border-zinc-200 text-zinc-600 dark:border-zinc-800 dark:text-zinc-400",
            )}
          >
            用户级（HKCU）
          </button>
          <button
            type="button"
            aria-pressed={envScope === "system"}
            disabled={envVarSaving !== null}
            onClick={() => {
              setEnvScope("system");
              setEnvVarEdits({});
            }}
            className={cn(
              "rounded-md border px-2.5 py-1 text-xs",
              envScope === "system"
                ? "border-amber-500 bg-amber-50 font-medium text-amber-700 dark:bg-amber-950 dark:text-amber-300"
                : "border-zinc-200 text-zinc-600 dark:border-zinc-800 dark:text-zinc-400",
            )}
          >
            系统级（HKLM）
          </button>
          {envScope === "system" && !elevated && (
            <span className="text-[11px] text-amber-600 dark:text-amber-400">
              写入时会弹出一次管理员授权
            </span>
          )}
        </div>
        {!envVars ? (
          <Spinner />
        ) : (
          <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
            {envVars.map((v) => {
              const editing = envVarEdits[v.name] !== undefined;
              // 必须按当前作用域取值：切到系统级后仍读 v.value 会让用户以为
              // 系统级变量等于用户级的值，进而覆盖掉真实的系统级配置。
              const value = envVarEdits[v.name] ?? valueOf(v) ?? "";
              const matchesSuggested =
                v.suggested != null && valueOf(v) === v.suggested;
              return (
                <div key={v.name} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2.5">
                  <div className="w-full min-w-0 shrink-0 sm:w-40">
                    <div className="truncate text-xs font-semibold font-mono">{v.name}</div>
                    <div className="truncate text-[10px] text-zinc-400">{v.label}</div>
                  </div>
                  <input
                    value={value}
                    placeholder={v.suggested ?? "未设置"}
                    aria-label={`${v.name} 的值`}
                    onChange={(e) =>
                      setEnvVarEdits((m) => ({ ...m, [v.name]: e.target.value }))
                    }
                    onKeyDown={(e) =>
                      e.key === "Enter" && editing && saveEnvVar(v)
                    }
                    className={cn(
                      "min-w-0 flex-1 h-7 rounded-md border bg-transparent px-2 text-xs font-mono outline-none transition-colors",
                      editing
                        ? "border-emerald-500"
                        : "border-transparent hover:border-zinc-200 dark:hover:border-zinc-700",
                      valueOf(v) == null && !editing && "text-zinc-400",
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
                    {!editing && v.suggested && valueOf(v) !== v.suggested && (
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
                    {valueOf(v) != null && !editing && (
                      <Button
                        size="sm"
                        variant="ghost"
                        className="text-red-500 hover:bg-red-50 dark:hover:bg-red-950"
                        title="删除此变量"
                        onClick={() => {
                          setEnvVarEdits((m) => ({ ...m, [v.name]: "" }));
                          setTimeout(() => {
                            api
                              .saveEnvVar(v.name, "", "user")
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

        {/* 自定义变量：常见清单之外的名字，用户自行添加 */}
        <div className="border-t border-zinc-100 px-4 py-3 dark:border-zinc-800">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-xs font-medium text-zinc-600 dark:text-zinc-300">
              自定义变量
            </span>
            <span className="text-[11px] text-zinc-400">
              {envScope === "system" ? "写入系统级（需授权）" : "写入用户级"}
            </span>
          </div>
          {customVars === null ? (
            <Spinner />
          ) : customVars.length === 0 ? (
            <p className="text-[11px] text-zinc-400">暂无自定义变量</p>
          ) : (
            <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
              {customVars.map((v) => (
                <div key={v.name} className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2">
                  <div className="w-full min-w-0 truncate text-xs font-semibold font-mono sm:w-40 sm:shrink-0" title={v.name}>
                    {v.name}
                  </div>
                  <span className="flex-1 truncate text-xs font-mono text-zinc-500" title={v.value}>
                    {v.value}
                  </span>
                  <Button
                    size="sm"
                    variant="ghost"
                    className="text-red-500 hover:bg-red-50 dark:hover:bg-red-950"
                    title={`删除 ${v.name}`}
                    aria-label={`删除变量 ${v.name}`}
                    loading={envVarSaving === `custom:${v.name}`}
                    onClick={async () => {
                      setEnvVarSaving(`custom:${v.name}`);
                      try {
                        await api.saveEnvVar(v.name, "", envScope);
                        toast.success(`${v.name} 已删除`);
                        await loadEnvVars();
                      } catch (e) {
                        toast.error("删除失败", String(e));
                      } finally {
                        setEnvVarSaving(null);
                      }
                    }}
                  >
                    <Trash2 className="size-3.5" />
                  </Button>
                </div>
              ))}
            </div>
          )}

          <div className="mt-2 flex flex-wrap items-center gap-2">
            <Plus className="size-3.5 shrink-0 text-zinc-400" />
            <input
              value={newVarName}
              onChange={(e) => setNewVarName(e.target.value)}
              placeholder="变量名，如 MY_TOOL_HOME"
              aria-label="新变量名"
              className="w-full min-w-0 h-7 rounded-md border border-transparent bg-transparent px-2 text-xs font-mono outline-none transition-colors hover:border-zinc-200 focus:border-emerald-500 dark:hover:border-zinc-700 sm:w-40 sm:shrink-0"
            />
            <input
              value={newVarValue}
              onChange={(e) => setNewVarValue(e.target.value)}
              placeholder="值"
              aria-label="新变量值"
              className="min-w-0 flex-1 h-7 rounded-md border border-transparent bg-transparent px-2 text-xs font-mono outline-none transition-colors hover:border-zinc-200 focus:border-emerald-500 dark:hover:border-zinc-700"
            />
            <Button
            size="sm"
            aria-label="添加环境变量"
            loading={envVarSaving === "__new__"}
            disabled={!newVarName.trim()}
              onClick={async () => {
                setEnvVarSaving("__new__");
                try {
                  await api.saveEnvVar(newVarName.trim(), newVarValue, envScope);
                  toast.success(
                    `${newVarName.trim()} 已保存`,
                    envScope === "system" ? "已写入系统环境变量" : "已写入用户环境变量",
                  );
                  setNewVarName("");
                  setNewVarValue("");
                  await loadEnvVars();
                } catch (e) {
                  toast.error("保存失败", String(e));
                } finally {
                  setEnvVarSaving(null);
                }
              }}
            >
              <Check className="size-3.5" />
              添加
            </Button>
          </div>
        </div>
      </Card>

    </PageShell>
  );
}
