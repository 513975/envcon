import type { ReactNode } from "react";

/** 页面容器:统一标题区 + 内容区 */
export function PageShell({
  title,
  desc,
  actions,
  children,
}: {
  title: string;
  desc?: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="max-w-5xl mx-auto px-6 py-6">
      <div className="flex items-center justify-between gap-3 mb-5">
        <div>
          <h1 className="text-lg font-bold text-zinc-900 dark:text-white">{title}</h1>
          {desc && <p className="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">{desc}</p>}
        </div>
        {actions && <div className="flex items-center gap-2">{actions}</div>}
      </div>
      {children}
    </div>
  );
}
