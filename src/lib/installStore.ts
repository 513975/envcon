import { create } from "zustand";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { InstallTask } from "./types";
import { api } from "./api";
import { toast } from "./toast";

interface InstallState {
  tasks: Record<number, InstallTask>;
  apply: (t: InstallTask) => void;
  remove: (id: number) => void;
  /** Hide a finished task locally without discarding the backend record. */
  dismiss: (id: number) => void;
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
  dismiss: (id) =>
    set((s) => {
      const tasks = { ...s.tasks };
      const task = tasks[id];
      // A running task must never be hidden by a stale timer.
      if (task && (task.status === "downloading" || task.status === "installing")) {
        return { tasks };
      }
      delete tasks[id];
      return { tasks };
    }),
  init: async () => {
    try {
      const list = await api.installTasks();
      // Merge instead of replacing: a task finished while this request was in
      // flight must not be dropped from the store.
      set((s) => {
        const tasks = { ...s.tasks };
        for (const task of list) {
          const existing = tasks[task.id];
          if (!existing || existing.status !== "done" || task.status !== "done") {
            tasks[task.id] = task;
          }
        }
        return { tasks };
      });
    } catch (e) {
      toast.error("读取安装任务失败", String(e));
    }
  },
}));

let started = false;
let unlisteners: UnlistenFn[] = [];

/** 全局启动安装事件监听(App 挂载时调用一次) */
export async function startInstallListener() {
  if (started) return;
  started = true;
  const pending: InstallTask[] = [];
  let initialized = false;
  try {
    unlisteners.push(await listen<InstallTask>("install://update", (e) => {
      if (initialized) useInstallStore.getState().apply(e.payload);
      else pending.push(e.payload);
    }));
    await useInstallStore.getState().init();
    pending.forEach(t => useInstallStore.getState().apply(t));
    initialized = true;
  } catch (e) {
    started = false;
    toast.error("安装状态监听失败", String(e));
  }
}

export function stopInstallListener() {
  unlisteners.forEach((u) => u());
  unlisteners = [];
  started = false;
}
