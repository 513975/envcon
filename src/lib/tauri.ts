import { invoke } from "@tauri-apps/api/core";
import { invalidateReads } from "./readCache";
import { invalidateScanReads } from "./scanClient";
import { pollSizes } from "./sizeStore";

/** 类型化 invoke 封装 */
export async function cmd<T>(
  name: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const mutation =
    /^(set_|save_|restore_|reload_settings|integrate_|switch_|uninstall_|update_|apply_|migrate_|clean|start_install|start_global_reinstall|start_pip_reinstall)/.test(
      name,
    );
  if (mutation) {
    invalidateReads();
    await invalidateScanReads(true);
  }
  try {
    return await invoke<T>(name, args);
  } finally {
    if (mutation) {
      invalidateReads();
      await invalidateScanReads(false);
      void pollSizes();
    }
  }
}
