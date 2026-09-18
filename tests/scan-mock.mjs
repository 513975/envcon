// Adapt existing behavior fixtures to the snapshot IPC without production fallback paths.
export async function installScanMock(page) {
  await page.addInitScript(() => {
    let internals;
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      get: () => internals,
      set: value => {
        const original = value.invoke;
        value.invoke = async (command, args) => {
          if (command === "query_scan") {
            if (args.cachedOnly) return null;
            const names = { overview: "get_overview", system: "scan_system", tools: "get_tool_configs", caches: "get_caches" };
            const oldArgs = args.kind === "system" ? { projectDirs: args.projectDirs } : undefined;
            const result = await original(names[args.kind], oldArgs);
            return { value: result, updatedAt: Date.now(), durationMs: 0, warning: null };
          }
          if (command === "invalidate_scans") return null;
          return original(command, args);
        };
        internals = value;
      },
    });
  });
}
