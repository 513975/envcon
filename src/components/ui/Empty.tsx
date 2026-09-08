import type { ReactNode } from "react";
import { PackageOpen } from "lucide-react";

export function Empty({
  icon,
  title,
  desc,
  action,
}: {
  icon?: ReactNode;
  title: string;
  desc?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center py-14 text-center">
      <div className="size-11 rounded-xl bg-zinc-100 dark:bg-zinc-800 flex items-center justify-center text-zinc-400">
        {icon ?? <PackageOpen className="size-5" />}
      </div>
      <p className="mt-3 text-sm font-medium text-zinc-700 dark:text-zinc-300">{title}</p>
      {desc && (
        <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400 max-w-xs">{desc}</p>
      )}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

export function Spinner({ className }: { className?: string }) {
  return (
    <div className={className ?? "py-14 flex justify-center"}>
      <div className="size-6 rounded-full border-2 border-zinc-300 dark:border-zinc-600 border-t-emerald-500 animate-spin" />
    </div>
  );
}
