import catalog from "./managerCatalog.json";

export type ManagerDefinition = {
  id: string;
  name: string;
  family: "node" | "python" | "native";
  adapter: "nodeCli" | "pythonPip" | "isolated";
  reinstall: "global" | "venv";
  globalConfig: boolean;
  cacheConfig: boolean;
  migrateGlobal: boolean;
  projectQuery: boolean;
};

// The same catalog is embedded by Rust; UI capabilities do not maintain a second tool list.
export const managers = catalog as ManagerDefinition[];
export function managerDefinition(id: string): ManagerDefinition {
  const manager = managers.find((item) => item.id === id);
  if (!manager) throw new Error(`未知包管理器: ${id}`);
  return manager;
}
