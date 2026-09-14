import type { ButtonHTMLAttributes } from "react";
import { cn } from "../../lib/format";

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  selected: boolean;
  count?: number;
  indicator?: "available" | "unavailable" | "unknown";
  statusLabel?: string;
}

export function SelectionButton({ selected, count, indicator, statusLabel, children, className, ...props }: Props) {
  return <button type="button" aria-pressed={selected} {...props} className={cn(
    "h-8 px-3 rounded-lg text-xs font-medium transition-colors inline-flex items-center gap-1.5 cursor-pointer shrink-0",
    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-emerald-500 disabled:opacity-50",
    selected ? "bg-zinc-900 dark:bg-zinc-100 text-white dark:text-zinc-900"
      : "bg-white dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-800 text-zinc-600 dark:text-zinc-300 hover:border-zinc-300",
    className,
  )}>
    {children}
    {count !== undefined && count > 0 && <span className={cn("rounded px-1 text-[10px] tabular-nums", selected ? "bg-white/20 dark:bg-black/10" : "bg-zinc-100 dark:bg-zinc-800")}>{count}</span>}
    {indicator && <span role="img" aria-label={statusLabel} title={statusLabel} className={cn("size-1.5 rounded-full shrink-0", indicator === "available" ? "bg-emerald-500" : indicator === "unavailable" ? "bg-zinc-400" : "bg-amber-500")} />}
  </button>;
}
