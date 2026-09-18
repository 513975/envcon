import { create } from "zustand";
import { api } from "./api";
import type { EnvType, SizeSnapshot } from "./types";

interface State {
  tasks: SizeSnapshot[];
  starting: boolean;
  error: string | null;
}
export const useSizeStore = create<State>(() => ({ tasks: [], starting: false, error: null }));
let timer: ReturnType<typeof setTimeout> | undefined;
let polling = false;
export async function pollSizes() {
  if (polling) return;
  polling = true;
  if (timer) clearTimeout(timer);
  try {
    const tasks = await api.sizeScanStatus();
    useSizeStore.setState({ tasks: tasks ?? [], error: null });
  } catch (error) { useSizeStore.setState({ error: String(error) }); }
  finally {
    polling = false;
    if (useSizeStore.getState().tasks.some(task => task.status === "running")) timer = setTimeout(pollSizes, 500);
  }
}
export async function startSize(envType: EnvType | null, name: string | null, cacheTool: string | null) {
  if (useSizeStore.getState().starting) return;
  useSizeStore.setState({ starting: true });
  try {
    await api.startSizeScan(envType, name, cacheTool);
    await pollSizes();
  } finally { useSizeStore.setState({ starting: false }); }
}
