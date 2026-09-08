import { cn } from "../../lib/format";

export function ProgressBar({
  value,
  indeterminate = false,
  className,
}: {
  /** 0-1 */
  value: number;
  indeterminate?: boolean;
  className?: string;
}) {
  const pct = Math.min(100, Math.max(0, value * 100));
  return (
    <div
      className={cn(
        "h-1.5 rounded-full bg-zinc-200 dark:bg-zinc-700 overflow-hidden",
        className,
      )}
    >
      {indeterminate ? (
        <div className="h-full w-1/3 rounded-full bg-emerald-500 animate-[progress-slide_1s_ease-in-out_infinite]" />
      ) : (
        <div
          className="h-full rounded-full bg-emerald-500 transition-[width] duration-150"
          style={{ width: `${pct}%` }}
        />
      )}
      <style>{`@keyframes progress-slide { 0% { transform: translateX(-100%) } 100% { transform: translateX(400%) } }`}</style>
    </div>
  );
}
