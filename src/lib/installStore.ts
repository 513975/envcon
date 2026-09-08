import { create } from "zustand";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { InstallTask } from "./types";
import { api } from "./api";

interface InstallState {
  tasks: Record<number, InstallTask>;
  apply: (t: InstallTask) => void;
  remove: (id: number) => void;
  init: () => Promise<void>;
}

export const useInstallStore = create<InstallState>((set) => ({
  tasks: {},
  apply: (t) =>
    set((s) => ({ tasks: { ...s.tasks, [t.id]: t } })),
  remove: (id) =>
    set((s) => {
      const tasks = { ...s.tasks };
      delete tasks[id];
      return { tasks };
    }),
  init: async () => {
    try {
      const list = await api.installTasks();
      set({
        tasks: Object.fromEntries(list.map((t) => [t.id, t])),
      });
    } catch {
      /* ignore */
    }
  },
}));

let started = false;
let unlisteners: UnlistenFn[] = [];

/** 全局启动安装事件监听(App 挂载时调用一次) */
export async function startInstallListener() {
  if (started) return;
  started = true;
  await useInstallStore.getState().init();
  unlisteners.push(
    await listen<InstallTask>("install://update", (e) => {
      useInstallStore.getState().apply(e.payload);
    }),
  );
}

export function stopInstallListener() {
  unlisteners.forEach((u) => u());
  unlisteners = [];
  started = false;
}
