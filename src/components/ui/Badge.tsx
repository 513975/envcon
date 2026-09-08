import type { ReactNode } from "react";
import { cn } from "../../lib/format";

export function Badge({
  children,
  color = "gray",
  className,
}: {
  children: ReactNode;
  color?: "gray" | "green" | "blue" | "orange" | "red";
  className?: string;
}) {
  const colors = {
    gray: "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300",
    green: "bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-400",
    blue: "bg-sky-100 text-sky-700 dark:bg-sky-950 dark:text-sky-400",
    orange: "bg-orange-100 text-orange-700 dark:bg-orange-950 dark:text-orange-400",
    red: "bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-400",
  };
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] font-medium whitespace-nowrap",
        colors[color],
        className,
      )}
    >
      {children}
    </span>
  );
}
