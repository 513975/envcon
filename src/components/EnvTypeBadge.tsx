import { ALL_ENV_TYPES, ENV_TYPE_META, type EnvType } from "../lib/types";
import { cn } from "../lib/format";

/** 环境类型彩色徽标:首字母方块 + 标签 */
export function EnvTypeBadge({
  type,
  size = "md",
}: {
  type: EnvType;
  size?: "sm" | "md" | "lg";
}) {
  const meta = ENV_TYPE_META[type];
  const initial = {
    jdk: "J",
    python: "P",
    node: "N",
    go: "G",
    rust: "R",
    maven: "M",
    gradle: "G",
    php: "P",
    llvm: "L",
    zig: "Z",
    deno: "D",
    bun: "B",
    git: "G",
    gh: "G",
    mingw: "C",
  }[type];
  const box = size === "lg" ? "size-10 text-lg" : size === "sm" ? "size-6 text-[11px]" : "size-8 text-sm";
  return (
    <span className="inline-flex items-center gap-2">
      <span
        className={cn(
          "inline-flex items-center justify-center rounded-lg font-bold shrink-0",
          meta.color,
          box,
        )}
      >
        {initial}
      </span>
      {size !== "sm" && (
        <span className="text-sm font-semibold text-zinc-700 dark:text-zinc-200">
          {meta.label}
        </span>
      )}
    </span>
  );
}

export { ALL_ENV_TYPES };
