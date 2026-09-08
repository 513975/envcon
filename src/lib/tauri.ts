import { invoke } from "@tauri-apps/api/core";

/** 类型化 invoke 封装 */
export async function cmd<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(name, args);
}
