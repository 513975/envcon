import type { ExternalEnv, ScanStatus } from "./types";

export const SCAN_CATEGORIES = ["语言与运行时", "包管理器", "构建工具", "其他工具"] as const;
export type ScanCategory = typeof SCAN_CATEGORIES[number];
const managers = new Set(["npm", "pnpm", "yarn", "pip", "pipx", "uv", "poetry", "cargo", "composer", "conda"]);
const runtimes = new Set(["java", "python", "node.js", "go", "rust", "php", "zig", "llvm", "deno", "bun", "gcc", ".net", "powershell 7"]);

export function scanToolName(tool: string) {
  return tool === "pip (Python 模块)" ? "pip" : tool;
}

export function scanCategory(tool: string): ScanCategory {
  const name = scanToolName(tool).toLowerCase();
  return managers.has(name) ? "包管理器" : runtimes.has(name) ? "语言与运行时" : ["cmake", "maven", "gradle"].includes(name) ? "构建工具" : "其他工具";
}

export function scanHealthy(env: ExternalEnv) {
  return env.scanStatus ? env.scanStatus === "available" : !!env.version && !env.error;
}

export function scanStatusLabel(env: ExternalEnv) {
  return scanStatusText(env.scanStatus, scanHealthy(env) ? "可用" : "待检查");
}

export function scanStatusText(status: ScanStatus | undefined, fallback = "未扫描") {
  switch (status) {
    case "available": return "可用";
    case "notDiscovered": return "扫描范围内未发现";
    case "permissionDenied": return "权限不足";
    case "startFailed": return "无法启动";
    case "commandFailed": return "命令失败";
    case "parseFailed": return "输出无法解析";
    case "timedOut": return "探测超时";
    case "unscanned":
    default: return fallback;
  }
}

export function scanStatusColor(env: ExternalEnv): "gray" | "green" | "orange" | "red" {
  return scanStatusTone(env.scanStatus, scanHealthy(env));
}

export function scanStatusTone(status: ScanStatus | undefined, healthy = false): "gray" | "green" | "orange" | "red" {
  switch (status) {
    case "available": return "green";
    case "notDiscovered":
    case "unscanned": return "gray";
    case "permissionDenied":
    case "startFailed":
    case "commandFailed":
    case "parseFailed":
    case "timedOut": return "orange";
    default: return healthy ? "green" : "red";
  }
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
