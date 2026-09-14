import { cmd } from "./tauri";
import type {
  EnvType,
  ScanReport,
  Overview,
  PathState,
  InstallTask,
  VersionInfo,
  SourceInfo,
  EnvVarInfo,
} from "./types";

/** 后端 API 封装 */
export const api = {
  getOverview: () => cmd<Overview>("get_overview"),
  setRoot: (path: string) => cmd<string>("set_root", { path }),
  scanSystem: (projectDirs: string[] = []) => cmd<ScanReport>("scan_system", { projectDirs }),
  integrateExternalEnv: (envType: EnvType, path: string, version: string | null) =>
    cmd<string>("integrate_external_env", { envType, path, version }),
  switchEnv: (envType: EnvType, name: string) =>
    cmd<void>("switch_env", { envType, name }),
  uninstallEnv: (envType: EnvType, name: string) =>
    cmd<void>("uninstall_env", { envType, name }),
  openPath: (path: string) => cmd<void>("open_path", { path }),
  getPathState: () => cmd<PathState>("get_path_state"),
  saveUserPath: (entries: string[]) =>
    cmd<void>("save_user_path", { entries }),
  integrateCurrentToPath: () => cmd<string[]>("integrate_current_to_path"),
  getSources: (envType: EnvType) => cmd<SourceInfo[]>("get_sources", { envType }),
  listVersions: (envType: EnvType, source?: string) =>
    cmd<VersionInfo[]>("list_versions", { envType, source }),
  startInstall: (
    envType: EnvType,
    version: string,
    targetName: string,
    source?: string,
    url?: string | null,
  ) =>
    cmd<number>("start_install", {
      envType,
      version,
      targetName,
      source,
      url: url ?? null,
    }),
  cancelInstall: (id: number) => cmd<void>("cancel_install", { id }),
  installTasks: () => cmd<InstallTask[]>("install_tasks"),
  getCaches: () => cmd<import("./types").CacheInfo[]>("get_caches"),
  cleanCache: (tool: string, expectedPath: string) => cmd<number>("clean_cache", { tool, expectedPath }),
  getSettings: () =>
    cmd<{ root: string | null; downloadsDir: string | null; portable: boolean; dataDir: string }>(
      "get_settings",
    ),
  setDownloadsDir: (dir: string) => cmd<void>("set_downloads_dir", { dir }),
  listPathBackups: () => cmd<{ file: string; time: string }[]>("list_path_backups"),
  restorePathBackup: (file: string) => cmd<void>("restore_path_backup", { file }),
  getEnvVars: () => cmd<EnvVarInfo[]>("get_env_vars"),
  saveEnvVar: (name: string, value: string) =>
    cmd<void>("save_env_var", { name, value }),
  getToolConfigs: () =>
    cmd<import("./types").ToolConfig[]>("get_tool_configs"),
  listInstalledPackages: (tool: string, directory: string | null = null, historical = false) => cmd<import("./types").PackageList>("list_installed_packages", { tool, directory, historical }),
  globalPackageSources: (tool: string) => cmd<string[]>("global_package_sources", { tool }),
  startPackageScan: (mode: "common" | "drives" | "directory", directory: string | null) => cmd<void>("start_package_scan", { mode, directory }),
  packageScanStatus: () => cmd<import("./types").PackageScanStatus | null>("package_scan_status"),
  cancelPackageScan: () => cmd<void>("cancel_package_scan"),
  previewToolMigration: (tool: string, kind: "global" | "cache", source: string) =>
    cmd<import("./types").MigrationPlan>("preview_tool_migration", { tool, kind, source }),
  migrateToolData: (plan: import("./types").MigrationPlan) =>
    cmd<import("./types").MigrationResult>("migrate_tool_data", { plan }),
  previewPipReinstall: (source: string, sourceKind: "python" | "directory", python: string, destination: string) =>
    cmd<import("./types").PipReinstallPlan>("preview_pip_reinstall", { source, sourceKind, python, destination }),
  startPipReinstall: (plan: import("./types").PipReinstallPlan, names: string[]) =>
    cmd<void>("start_pip_reinstall", { plan, names }),
  pipReinstallStatus: () => cmd<import("./types").PipReinstallStatus | null>("pip_reinstall_status"),
  cancelPipReinstall: () => cmd<void>("cancel_pip_reinstall"),
  previewGlobalReinstall: (tool: string, executable: string, source: string, destination: string) => cmd<import("./types").GlobalReinstallPlan>("preview_global_reinstall", { tool, executable, source, destination }),
  startGlobalReinstall: (plan: import("./types").GlobalReinstallPlan, names: string[]) => cmd<void>("start_global_reinstall", { plan, names }),
  globalReinstallStatus: (tool: string) => cmd<import("./types").GlobalReinstallStatus | null>("global_reinstall_status", { tool }),
  cancelGlobalReinstall: (tool: string) => cmd<void>("cancel_global_reinstall", { tool }),
  previewOldPackageCleanup: (tool: string, destination: string) => cmd<import("./types").OldPackageCleanupPreview>("preview_old_package_cleanup", { tool, destination }),
  cleanupOldPackages: (tool: string, token: string, names: string[]) => cmd<import("./types").OldPackageCleanupResult>("cleanup_old_packages", { tool, token, names }),
  applyToolConfig: (tool: string, globalPath?: string | null, cachePath?: string | null) =>
    cmd<string[]>("apply_tool_config", {
      tool,
      globalPath: globalPath ?? null,
      cachePath: cachePath ?? null,
    }),
};
