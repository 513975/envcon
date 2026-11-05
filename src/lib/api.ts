import { cmd } from "./tauri";
import { scanRead } from "./scanClient";
import type {
  EnvType,
  ScanReport,
  Overview,
  PathState,
  InstallTask,
  VersionInfo,
  SourceInfo,
  EnvVarInfo,
  EnvScope,
  UninstallPreview,
  UninstallResult,
} from "./types";

/** 后端 API 封装 */
export const api = {
  startSizeScan: (envType: EnvType | null, name: string | null, cacheTool: string | null) =>
    cmd<number>("start_size_scan", { envType, name, cacheTool }),
  sizeScanStatus: () => cmd<import("./types").SizeSnapshot[]>("size_scan_status"),
  cancelSizeScan: (id: number) => cmd<void>("cancel_size_scan", { id }),
  abandonSizeScan: (id: number) => cmd<void>("abandon_size_scan", { id }),
  getOverview: (force = false) =>
    scanRead<Overview>("overview", [], force),
  exportEnvironmentManifest: (path: string) =>
    cmd<string>("export_environment_manifest", { path }),
  redetectEnvironment: (envType: import("./types").EnvType, name: string) =>
    cmd<import("./types").EnvironmentRedetect>("redetect_environment", {
      envType,
      name,
    }),
  importEnvironmentManifest: (path: string) =>
    cmd<import("./types").ManifestComparison>("import_environment_manifest", {
      path,
    }),
  updatePackageManager: (tool: string, expectedPreview: import("./types").PackageManagerUpdatePreview) =>
    cmd<import("./types").PackageManagerUpdateResult>(
      "update_package_manager",
      { tool, expectedPreview },
    ),
  previewPackageManagerUpdate: (tool: string) =>
    cmd<import("./types").PackageManagerUpdatePreview>(
      "preview_package_manager_update",
      { tool },
    ),
  previewPackageManagerUpdates: () =>
    cmd<import("./types").PackageManagerUpdateBatchPreview>(
      "preview_package_manager_updates",
    ),
  updatePackageManagers: (tools: string[], skip: string[] = []) =>
    cmd<import("./types").PackageManagerBatchResult>(
      "update_package_managers",
      { tools, skip },
    ),
  packageManagerUpdateRecord: (tool: string) =>
    cmd<import("./types").PackageManagerUpdateRecord | null>(
      "package_manager_update_record",
      { tool },
    ),
  setRoot: (path: string) => cmd<string>("set_root", { path }),
  scanSystem: (projectDirs: string[] = [], force = false) =>
    scanRead<ScanReport>("system", projectDirs, force),
  integrateExternalEnv: (
    envType: EnvType,
    path: string,
    version: string | null,
  ) => cmd<string>("integrate_external_env", { envType, path, version }),
  previewMoveEnvironment: (envType: EnvType, name: string) =>
    cmd<{ envType: EnvType; name: string; source: string; target: string; files: number; bytes: number; links: number }>(
      "preview_move_environment",
      { envType, name },
    ),
  executeMoveEnvironment: (plan: {
    envType: EnvType; name: string; source: string; target: string; files: number; bytes: number; links: number;
  }) => cmd<{ target: string; source: string; files: number; bytes: number }>("execute_move_environment", { plan }),
  previewDiscardMovedSource: (envType: EnvType, name: string, source: string) =>
    cmd<{ envType: EnvType; name: string; source: string; files: number; bytes: number }>(
      "preview_discard_moved_source",
      { envType, name, source },
    ),
  executeDiscardMovedSource: (plan: {
    envType: EnvType; name: string; source: string; files: number; bytes: number;
  }) => cmd<void>("execute_discard_moved_source", { plan }),
  switchEnv: (envType: EnvType, name: string) =>
    cmd<import("./types").SwitchDiagnosis>("switch_env", { envType, name }),
  previewPathRepair: (envType: EnvType) =>
    cmd<import("./types").PathRepairPreview>("preview_path_repair", { envType }),
  applyPathRepair: (plan: NonNullable<import("./types").PathRepairPreview["plan"]>) =>
    cmd<import("./types").PathRepairResult>("apply_path_repair", { plan }),
  previewUninstallEnv: (
    envType: EnvType,
    name: string,
    mode?: "uninstall" | "detach",
  ) => cmd<UninstallPreview>("preview_uninstall_env", { envType, name, mode }),
  uninstallEnv: (
    envType: EnvType,
    name: string,
    expectedPreview: UninstallPreview,
  ) =>
    cmd<UninstallResult>("uninstall_env", {
      envType,
      name,
      expectedPreview,
    }),
  openPath: (path: string) => cmd<void>("open_path", { path }),
  getPathState: () => cmd<PathState>("get_path_state"),
  saveUserPath: (entries: string[]) => cmd<void>("save_user_path", { entries }),
  previewSystemPathEdit: (entries?: string[], expectedPath?: string) =>
    cmd<import("./types").PathEditPlan>("preview_system_path_edit", { entries: entries ?? null, expectedPath: expectedPath ?? null }),
  saveSystemPath: (plan: import("./types").PathEditPlan) =>
    cmd<void>("save_system_path", { plan }),
  integrateCurrentToPath: () => cmd<string[]>("integrate_current_to_path"),
  getSources: (envType: EnvType) =>
    cmd<SourceInfo[]>("get_sources", { envType }),
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
  getCaches: (force = false) =>
    scanRead<import("./types").CacheInfo[]>("caches", [], force),
  cleanCache: (tool: string, expectedPath: string) =>
    cmd<number>("clean_cache", { tool, expectedPath }),
  getSettings: () =>
    cmd<{
      root: string | null;
      downloadsDir: string | null;
      portable: boolean;
      dataDir: string;
      configWarning: string | null;
      writeBlocked: boolean;
      hasConfigBackup: boolean;
    }>("get_settings"),
  checkAppUpdate: () =>
    cmd<import("./types").AppUpdateStatus>("check_app_update"),
  reloadSettings: () => cmd<void>("reload_settings"),
  restoreSettingsBackup: () => cmd<void>("restore_settings_backup"),
  listConfigHistory: () =>
    cmd<{ file: string; stamp: number }[]>("list_config_history"),
  restoreConfigHistory: (file: string) =>
    cmd<void>("restore_config_history", { file }),
  setDownloadsDir: (dir: string) => cmd<void>("set_downloads_dir", { dir }),
  listPathBackups: () =>
    cmd<{ file: string; time: string }[]>("list_path_backups"),
  restorePathBackup: (file: string) =>
    cmd<void>("restore_path_backup", { file }),
  getEnvVars: () => cmd<EnvVarInfo[]>("get_env_vars"),
  getCustomEnvVars: (scope: EnvScope) =>
    cmd<{ name: string; value: string }[]>("get_custom_env_vars", { scope }),
  isElevated: () => cmd<boolean>("is_elevated"),
  saveEnvVar: (name: string, value: string, scope: EnvScope) =>
    cmd<void>("save_env_var", { name, value, scope }),
  getToolConfigs: (force = false) =>
    scanRead<import("./types").ToolConfig[]>("tools", [], force),
  inspectUvLayout: (path: string) =>
    cmd<import("./types").UvLegacyLayout>("inspect_uv_layout", { path }),
  recoverUvLayout: (
    expected: import("./types").UvLegacyLayout,
    executable: string,
  ) =>
    cmd<import("./types").UvRecoveryReport>("recover_uv_layout", {
      expected,
      executable,
    }),
  listInstalledPackages: (
    tool: string,
    directory: string | null = null,
    historical = false,
  ) =>
    cmd<import("./types").PackageList>("list_installed_packages", {
      tool,
      directory,
      historical,
    }),
  globalPackageSources: (tool: string) =>
    cmd<string[]>("global_package_sources", { tool }),
  startPackageScan: (
    mode: "common" | "drives" | "directory",
    directory: string | null,
  ) => cmd<void>("start_package_scan", { mode, directory }),
  packageScanStatus: () =>
    cmd<import("./types").PackageScanStatus | null>("package_scan_status"),
  cancelPackageScan: () => cmd<void>("cancel_package_scan"),
  previewToolMigration: (
    tool: string,
    kind: "global" | "cache",
    source: string,
  ) =>
    cmd<import("./types").MigrationPlan>("preview_tool_migration", {
      tool,
      kind,
      source,
    }),
  migrateToolData: (plan: import("./types").MigrationPlan) =>
    cmd<import("./types").MigrationResult>("migrate_tool_data", { plan }),
  previewPipReinstall: (
    source: string,
    sourceKind: "python" | "directory",
    python: string,
    destination: string,
  ) =>
    cmd<import("./types").PipReinstallPlan>("preview_pip_reinstall", {
      source,
      sourceKind,
      python,
      destination,
    }),
  startPipReinstall: (
    plan: import("./types").PipReinstallPlan,
    names: string[],
  ) => cmd<void>("start_pip_reinstall", { plan, names }),
  pipReinstallStatus: () =>
    cmd<import("./types").PipReinstallStatus | null>("pip_reinstall_status"),
  cancelPipReinstall: () => cmd<void>("cancel_pip_reinstall"),
  previewGlobalReinstall: (
    tool: string,
    executable: string,
    source: string,
    destination: string,
    allowUnknownRegistry = false,
  ) =>
    cmd<import("./types").GlobalReinstallPlan>("preview_global_reinstall", {
      tool,
      executable,
      source,
      destination,
      allowUnknownRegistry,
    }),
  startGlobalReinstall: (
    plan: import("./types").GlobalReinstallPlan,
    names: string[],
  ) => cmd<void>("start_global_reinstall", { plan, names }),
  globalReinstallStatus: (tool: string) =>
    cmd<import("./types").GlobalReinstallStatus | null>(
      "global_reinstall_status",
      { tool },
    ),
  cancelGlobalReinstall: (tool: string) =>
    cmd<void>("cancel_global_reinstall", { tool }),
  previewOldPackageCleanup: (tool: string, destination: string) =>
    cmd<import("./types").OldPackageCleanupPreview>(
      "preview_old_package_cleanup",
      { tool, destination },
    ),
  previewScannedPackageCleanup: (tool: string, source: string) =>
    cmd<import("./types").OldPackageCleanupPreview>(
      "preview_scanned_package_cleanup",
      { tool, source },
    ),
  cleanupOldPackages: (tool: string, token: string, names: string[]) =>
    cmd<import("./types").OldPackageCleanupResult>("cleanup_old_packages", {
      tool,
      token,
      names,
    }),
  previewToolConfig: (
    tool: string,
    globalPath?: string | null,
    cachePath?: string | null,
  ) =>
    cmd<import("./types").ToolConfigPreview>("preview_tool_config", {
      tool,
      globalPath: globalPath ?? null,
      cachePath: cachePath ?? null,
    }),
  previewToolConfigs: (
    tools: { tool: string; globalPath: string | null; cachePath: string | null }[],
  ) =>
    cmd<import("./types").ToolConfigBatchPreview>("preview_tool_configs", {
      tools,
    }),
  applyToolConfig: (
    tool: string,
    globalPath?: string | null,
    cachePath?: string | null,
  ) =>
    cmd<string[]>("apply_tool_config", {
      tool,
      globalPath: globalPath ?? null,
      cachePath: cachePath ?? null,
    }),
};
