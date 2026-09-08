import { create } from "zustand";

export type Page =
  | "dashboard"
  | "environments"
  | "download"
  | "paths"
  | "settings";

interface AppState {
  page: Page;
  setPage: (p: Page) => void;
  /** 递增以触发 overview 重新加载 */
  overviewVersion: number;
  refreshOverview: () => void;
}

export const useAppStore = create<AppState>((set) => ({
  page: "dashboard",
  setPage: (page) => set({ page }),
  overviewVersion: 0,
  refreshOverview: () =>
    set((s) => ({ overviewVersion: s.overviewVersion + 1 })),
}));
