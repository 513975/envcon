import type { ExternalEnv } from "./types";

export const SCAN_CATEGORIES = ["语言与运行时", "包管理器", "构建工具", "其他工具"] as const;
export type ScanCategory = typeof SCAN_CATEGORIES[number];
const managers = new Set(["npm", "pnpm", "yarn", "pip", "pipx", "uv", "poetry", "cargo", "composer", "conda"]);
const runtimes = new Set(["java", "python", "node.js", "go", "rust", "php", "zig", "llvm", "deno", "bun", "gcc", ".net"]);

export function scanToolName(tool: string) {
  return tool === "pip (Python 模块)" ? "pip" : tool;
}

export function scanCategory(tool: string): ScanCategory {
  const name = scanToolName(tool).toLowerCase();
  return managers.has(name) ? "包管理器" : runtimes.has(name) ? "语言与运行时" : ["cmake", "maven", "gradle"].includes(name) ? "构建工具" : "其他工具";
}

export function scanHealthy(env: ExternalEnv) {
  return !!env.version && !env.error;
}

export function groupScanResults(rows: ExternalEnv[]) {
  const groups = new Map<string, { tool: string; category: ScanCategory; entries: ExternalEnv[] }>();
  for (const row of rows) {
    const tool = scanToolName(row.tool);
    const key = tool.toLowerCase();
    if (!groups.has(key)) groups.set(key, { tool, category: scanCategory(tool), entries: [] });
    groups.get(key)!.entries.push(row);
  }
  // Keep separate launchers and installations, even when versions match.
  return [...groups.values()].map(group => ({ ...group, entries: [...group.entries].sort((a, b) =>
    Number(b.isPreferred) - Number(a.isPreferred) || Number(scanHealthy(b)) - Number(scanHealthy(a)) || (a.path ?? "").localeCompare(b.path ?? ""),
  ) })).sort((a, b) => SCAN_CATEGORIES.indexOf(a.category) - SCAN_CATEGORIES.indexOf(b.category) || a.tool.localeCompare(b.tool));
}

export function scanMatches(env: ExternalEnv, query: string) {
  return [env.tool, env.version, env.path, env.source, env.command, env.error, env.installRoot, env.identityPath].join(" ").toLowerCase().includes(query.trim().toLowerCase());
}
