import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowRight, FolderOpen, MoveRight } from "lucide-react";
import { Modal } from "./ui/Modal";
import { Button } from "./ui/Button";
import { api } from "../lib/api";
import { formatBytes } from "../lib/format";
import type { MigrationPlan, MigrationResult, ToolConfig } from "../lib/types";
import { managerDefinition } from "../lib/managers";

export function ToolMigration({
  tool,
  onClose,
  onComplete,
}: {
  tool: ToolConfig;
  onClose: () => void;
  onComplete: () => void;
}) {
  const definition = managerDefinition(tool.tool);
  const [kind, setKind] = useState<"global" | "cache">(
    definition.migrateGlobal ? "global" : "cache",
  );
  const [source, setSource] = useState(
    (definition.migrateGlobal ? tool.globalPath : tool.cachePath) ?? "",
  );
  const [plan, setPlan] = useState<MigrationPlan | null>(null);
  const [result, setResult] = useState<MigrationResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const target = kind === "global" ? tool.defaultGlobal : tool.defaultCache;

  const preview = async () => {
    setBusy(true);
    setConfirmed(false);
    setError("");
    setPlan(null);
    try {
      setPlan(await api.previewToolMigration(tool.tool, kind, source.trim()));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const migrate = async () => {
    if (!plan || !confirmed) return;
    setBusy(true);
    setError("");
    try {
      setResult(await api.migrateToolData(plan));
      onComplete();
    } catch (e) {
      setError(String(e));
      setPlan(null);
    } finally {
      setBusy(false);
    }
  };
  const browse = async () => {
    try {
      const path = await open({
        directory: true,
        title: "选择原全局包或缓存目录",
      });
      if (typeof path === "string") {
        setSource(path);
        setPlan(null);
      }
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Modal
      open
      onClose={busy ? () => {} : onClose}
      title={`${tool.name} 数据迁移`}
      footer={
        result ? (
          <Button onClick={onClose}>完成</Button>
        ) : (
          <>
            <Button disabled={busy} onClick={onClose}>
              取消
            </Button>
            {plan ? (
              <Button
                variant="primary"
                loading={busy}
                disabled={!confirmed}
                onClick={migrate}
              >
                <MoveRight className="size-3.5" />
                {busy ? "正在复制和校验" : "迁移并保留备份"}
              </Button>
            ) : (
              <Button
                variant="primary"
                loading={busy}
                disabled={!source.trim()}
                onClick={preview}
              >
                预览迁移
              </Button>
            )}
          </>
        )
      }
    >
      <div className="max-h-[65vh] overflow-y-auto space-y-4">
        {result ? (
          <>
            <p>
              已迁移 {result.files} 个文件，共 {formatBytes(result.bytes)}
              。原路径现已链接到目标目录。
            </p>
            <div className="text-xs">
              目标目录
              <div className="font-mono break-all selectable mt-1">
                {result.target}
              </div>
            </div>
            <div className="text-xs">
              原数据备份
              <div className="font-mono break-all selectable mt-1">
                {result.backup}
              </div>
            </div>
            <div className="text-xs">
              迁移记录
              <div className="font-mono break-all selectable mt-1">
                {result.journal}
              </div>
            </div>
            <p className="text-xs text-amber-700 dark:text-amber-300">
              备份仍占用原磁盘空间。确认全局命令可用后，再清理该备份；保留原路径兼容链接。
            </p>
          </>
        ) : (
          <>
            <div className="flex gap-4 text-xs">
              {definition.migrateGlobal && (
                <label className="flex items-center gap-2">
                  <input
                    type="radio"
                    name="migration-kind"
                    checked={kind === "global"}
                    disabled={busy}
                    onChange={() => {
                      setKind("global");
                      setSource(tool.globalPath ?? "");
                      setPlan(null);
                    }}
                  />
                  全局包目录
                </label>
              )}
              <label className="flex items-center gap-2">
                <input
                  type="radio"
                  name="migration-kind"
                  checked={kind === "cache"}
                  disabled={busy}
                  onChange={() => {
                    setKind("cache");
                    setSource(tool.cachePath ?? "");
                    setPlan(null);
                  }}
                />
                缓存 / 存储目录
              </label>
            </div>
            <label className="block text-xs">
              原目录（可选择之前使用的目录）
              <div className="flex gap-2 mt-1">
                <input
                  aria-label="迁移源目录"
                  value={source}
                  disabled={busy}
                  onChange={(e) => {
                    setSource(e.target.value);
                    setPlan(null);
                  }}
                  className="min-w-0 flex-1 rounded border border-zinc-300 dark:border-zinc-700 bg-transparent px-2 py-2 font-mono text-xs"
                />
                <Button disabled={busy} title="选择原目录" onClick={browse}>
                  <FolderOpen className="size-4" />
                </Button>
              </div>
            </label>
            <div className="text-xs">
              <span className="flex items-center gap-1">
                <ArrowRight className="size-3.5" />
                目标目录
              </span>
              <div className="font-mono break-all mt-1">
                {plan?.target ?? target ?? "待预览"}
              </div>
            </div>
            {plan && (
              <p className="text-xs">
                {plan.files} 个文件 · {formatBytes(plan.bytes)} · {plan.links}{" "}
                个目录链接
              </p>
            )}
            <p className="text-xs">
              复制校验后保留原目录备份，原路径改为兼容链接；包管理器配置不变。目标必须为空，pip
              的已安装包不在迁移范围内。
            </p>
            {plan && (
              <label className="flex items-start gap-2 text-xs">
                <input
                  className="mt-0.5"
                  type="checkbox"
                  checked={confirmed}
                  disabled={busy}
                  onChange={(e) => setConfirmed(e.target.checked)}
                />
                已停止包安装、更新及缓存清理，迁移期间保持应用打开
              </label>
            )}
          </>
        )}
        {error && (
          <p
            role="alert"
            className="text-xs text-red-600 dark:text-red-400 break-all whitespace-pre-wrap"
          >
            {error}
          </p>
        )}
      </div>
    </Modal>
  );
}
