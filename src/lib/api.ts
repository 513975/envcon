import { cmd } from "./tauri";
import type {
  EnvType,
  ExternalEnv,
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
  scanSystem: () => cmd<ExternalEnv[]>("scan_system"),
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
  cleanCache: (tool: string) => cmd<number>("clean_cache", { tool }),
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
  applyToolConfig: (tool: string, globalPath?: string | null, cachePath?: string | null) =>
    cmd<string[]>("apply_tool_config", {
      tool,
      globalPath: globalPath ?? null,
      cachePath: cachePath ?? null,
    }),
};
