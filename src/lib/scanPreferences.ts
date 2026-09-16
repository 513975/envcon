export function savedProjectDirs(): string[] {
  try {
    const saved = localStorage.getItem("envcon.scan.projectDirs");
    const parsed: unknown = saved ? JSON.parse(saved) : [];
    return Array.isArray(parsed)
      ? parsed.filter((p): p is string => typeof p === "string").slice(0, 8)
      : [];
  } catch {
    return [];
  }
}
