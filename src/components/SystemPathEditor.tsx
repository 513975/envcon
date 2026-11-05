import { useState } from "react";
import { ArrowDown, ArrowUp, Lock, Plus, Trash2 } from "lucide-react";
import { Card, CardHeader } from "./ui/Card";
import { Button } from "./ui/Button";
import { ConfirmDialog } from "./ui/Modal";
import { api } from "../lib/api";
import { toast } from "../lib/toast";
import type { PathEditPlan } from "../lib/types";

export function SystemPathEditor({ entries, onSaved }: { entries: string[] | undefined; onSaved: () => Promise<void> }) {
  const [base, setBase] = useState<PathEditPlan | null>(null);
  const [draft, setDraft] = useState<string[]>([]);
  const [newEntry, setNewEntry] = useState("");
  const [preview, setPreview] = useState<PathEditPlan | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState(false);
  const changed = base !== null && draft.join(";") !== base.expectedPath;

  const begin = async () => {
    setBusy(true);
    try {
      const plan = await api.previewSystemPathEdit();
      setBase(plan);
      setDraft(plan.entries);
      setNewEntry("");
      setPreview(null);
    } catch (e) {
      toast.error("读取系统 PATH 失败", String(e));
    } finally {
      setBusy(false);
    }
  };

  const prepare = async () => {
    if (!base) return;
    setBusy(true);
    try {
      setPreview(await api.previewSystemPathEdit(draft, base.expectedPath));
      setConfirmed(false);
    } catch (e) {
      toast.error("系统 PATH 预览失败", String(e));
    } finally {
      setBusy(false);
    }
  };

  const save = async () => {
    if (!preview || !confirmed) return;
    setSaving(true);
    try {
      await api.saveSystemPath(preview);
      setPreview(null);
      setBase(null);
      toast.success("系统 PATH 已保存", "已备份并复查；对所有用户生效，请新开终端核对命令解析");
      await onSaved();
    } catch (e) {
      setPreview(null);
      toast.error("系统 PATH 未能确认保存", `${String(e)}；请重新读取核对，不要假定原值未改变`);
    } finally {
      setSaving(false);
    }
  };

  const move = (index: number, direction: -1 | 1) => {
    const next = [...draft];
    const to = index + direction;
    if (to < 0 || to >= next.length) return;
    [next[index], next[to]] = [next[to], next[index]];
    setDraft(next);
  };

  return (
    <Card className="mt-5">
      <CardHeader
        title={<span className="flex items-center gap-1.5"><Lock className="size-3.5 text-zinc-400" />系统 PATH</span>}
        desc="所有用户共享；逐项编辑后预览确认，写入时申请管理员授权。不会自动认领共享条目。"
        actions={
          base ? <>
            <Button size="sm" disabled={busy || saving} onClick={begin}>重新读取</Button>
            <Button size="sm" disabled={busy || saving} onClick={() => setBase(null)}>取消编辑</Button>
            <Button size="sm" variant="primary" loading={busy} disabled={!changed || saving} onClick={prepare}>预览系统 PATH</Button>
          </> : <Button size="sm" loading={busy} disabled={!entries} onClick={begin}>编辑系统 PATH</Button>
        }
      />
      {base ? (
        <div role="group" aria-label="系统 PATH 编辑器">
          <p className="px-4 py-2 text-xs text-amber-700 dark:text-amber-400">保留不认识的条目；删除或调整顺序可能影响其他软件。未修改的原始条目（含空项）保持原样。</p>
          {draft.map((entry, index) => (
            <div key={index} className="flex items-center gap-1 px-4 py-2">
              <input
                aria-label={`系统 PATH 第 ${index + 1} 项`}
                value={entry}
                disabled={busy || saving}
                onChange={event => setDraft(draft.map((value, i) => i === index ? event.target.value : value))}
                className="min-w-0 flex-1 rounded border border-zinc-200 bg-transparent px-2 py-1 text-xs font-mono dark:border-zinc-700"
              />
              <button aria-label={`上移系统 PATH 第 ${index + 1} 项`} disabled={index === 0 || busy || saving} onClick={() => move(index, -1)} className="p-1 disabled:opacity-30"><ArrowUp className="size-3.5" /></button>
              <button aria-label={`下移系统 PATH 第 ${index + 1} 项`} disabled={index === draft.length - 1 || busy || saving} onClick={() => move(index, 1)} className="p-1 disabled:opacity-30"><ArrowDown className="size-3.5" /></button>
              <button aria-label={`删除系统 PATH 第 ${index + 1} 项`} disabled={busy || saving} onClick={() => setDraft(draft.filter((_, i) => i !== index))} className="p-1 text-red-500"><Trash2 className="size-3.5" /></button>
            </div>
          ))}
          <div className="flex items-center gap-2 border-t border-zinc-100 px-4 py-3 dark:border-zinc-800">
            <Plus className="size-3.5 shrink-0 text-zinc-400" />
            <input aria-label="新增系统 PATH" value={newEntry} disabled={busy || saving} onChange={event => setNewEntry(event.target.value)} className="min-w-0 flex-1 bg-transparent text-xs font-mono outline-none" placeholder="如 D:\tools\bin 或 %TOOLS%\bin" />
            <Button size="sm" disabled={!newEntry.trim() || busy || saving} onClick={() => { setDraft([...draft, newEntry.trim()]); setNewEntry(""); }}>添加系统路径</Button>
          </div>
        </div>
      ) : (
        <div className="divide-y divide-zinc-50 px-4 py-2 dark:divide-zinc-800/50">
          {entries?.map((entry, index) => <div key={index} className="break-all py-1.5 text-xs font-mono text-zinc-500 selectable">{entry}</div>)}
        </div>
      )}
      <ConfirmDialog
        open={preview !== null}
        onClose={() => setPreview(null)}
        onConfirm={save}
        title="确认修改系统 PATH"
        confirmText="授权并保存系统 PATH"
        confirmDisabled={!confirmed}
        loading={saving}
        danger
        message={preview && <div className="space-y-3">
          <p>这会影响所有用户及其他软件。来源未确认不表示可以安全删除；手工保存会清空此作用域的自动归属。</p>
          <p className="text-xs">写入前备份，提权后再次核对原值与来源记录；其他安装器已改动时停止，不覆盖。</p>
          <div><strong>修改前（原始顺序）</strong><ol className="mt-1 list-inside list-decimal text-xs font-mono">{preview.expectedPath.split(";").map((entry, i) => <li key={i} className="break-all">{entry || "（空项）"}</li>)}</ol></div>
          <div><strong>修改后（完整顺序与来源）</strong><ol className="mt-1 list-inside list-decimal text-xs">{preview.entries.map((entry, i) => <li key={i} className="break-all"><span className="font-mono">{entry || "（空项）"}</span><span className="block text-zinc-500">{preview.sources[i]}</span></li>)}</ol>{preview.entries.length === 0 && <p role="alert">将清空系统 PATH。</p>}</div>
          <label className="flex items-start gap-2 text-xs"><input type="checkbox" checked={confirmed} disabled={saving} onChange={e => setConfirmed(e.target.checked)} />我已核对所有增删与顺序变化，确认影响所有用户</label>
        </div>}
      />
    </Card>
  );
}
