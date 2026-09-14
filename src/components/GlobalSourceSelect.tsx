import { useEffect, useState } from "react";
import { api } from "../lib/api";

export function GlobalSourceSelect({
  tool,
  disabled,
  onSelect,
}: {
  tool: string;
  disabled?: boolean;
  onSelect: (path: string) => void;
}) {
  const [paths, setPaths] = useState<string[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    let alive = true;
    setPaths([]);
    setError("");
    api
      .globalPackageSources(tool)
      .then((value) => {
        if (alive) setPaths(value);
      })
      .catch((e) => {
        if (alive) setError(String(e));
      });
    return () => {
      alive = false;
    };
  }, [tool]);
  return (
    <>
      {paths.length > 0 && (
        <select
          aria-label="已发现的全局目录"
          value=""
          disabled={disabled}
          onChange={(e) => onSelect(e.target.value)}
          className="w-full min-w-0 rounded border border-zinc-300 dark:border-zinc-700 bg-white dark:bg-zinc-900 p-2 text-xs"
        >
          <option value="" disabled>
            已发现的全局目录
          </option>
          {paths.map((path) => (
            <option key={path} value={path}>
              {path}
            </option>
          ))}
        </select>
      )}
      {error && (
        <p
          role="alert"
          className="text-xs text-amber-700 dark:text-amber-300 break-all"
        >
          目录发现失败：{error}
        </p>
      )}
    </>
  );
}
