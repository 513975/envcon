/** 环境类型(与后端 EnvType 对应) */
export type EnvType =
  | "jdk"
  | "python"
  | "node"
  | "go"
  | "rust"
  | "maven"
  | "gradle"
  | "php"
  | "llvm"
  | "zig"
  | "deno"
  | "bun"
  | "git"
  | "gh"
  | "mingw";

export const ENV_TYPE_META: Record<
  EnvType,
  { label: string; folder: string; junction: string; color: string }
> = {
  jdk: { label: "JDK", folder: "jdks", junction: "jdk", color: "text-orange-600 dark:text-orange-400 bg-orange-50 dark:bg-orange-950/50" },
  python: { label: "Python", folder: "pythons", junction: "python", color: "text-sky-600 dark:text-sky-400 bg-sky-50 dark:bg-sky-950/50" },
  node: { label: "Node.js", folder: "nodes", junction: "node", color: "text-emerald-600 dark:text-emerald-400 bg-emerald-50 dark:bg-emerald-950/50" },
  go: { label: "Go", folder: "gos", junction: "go", color: "text-cyan-600 dark:text-cyan-400 bg-cyan-50 dark:bg-cyan-950/50" },
  rust: { label: "Rust", folder: "rusts", junction: "rust", color: "text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-950/50" },
  maven: { label: "Maven", folder: "mavens", junction: "maven", color: "text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-950/50" },
  gradle: { label: "Gradle", folder: "gradles", junction: "gradle", color: "text-indigo-600 dark:text-indigo-400 bg-indigo-50 dark:bg-indigo-950/50" },
  php: { label: "PHP", folder: "phps", junction: "php", color: "text-violet-600 dark:text-violet-400 bg-violet-50 dark:bg-violet-950/50" },
  llvm: { label: "LLVM", folder: "llvms", junction: "llvm", color: "text-zinc-600 dark:text-zinc-300 bg-zinc-100 dark:bg-zinc-800" },
  zig: { label: "Zig", folder: "zigs", junction: "zig", color: "text-fuchsia-600 dark:text-fuchsia-400 bg-fuchsia-50 dark:bg-fuchsia-950/50" },
  deno: { label: "Deno", folder: "denos", junction: "deno", color: "text-teal-600 dark:text-teal-400 bg-teal-50 dark:bg-teal-950/50" },
  bun: { label: "Bun", folder: "buns", junction: "bun", color: "text-rose-600 dark:text-rose-400 bg-rose-50 dark:bg-rose-950/50" },
  git: { label: "Git", folder: "gits", junction: "git", color: "text-lime-700 dark:text-lime-300 bg-lime-100 dark:bg-lime-950/50" },
  gh: { label: "GitHub CLI", folder: "ghs", junction: "gh", color: "text-purple-600 dark:text-purple-400 bg-purple-100 dark:bg-purple-950/50" },
  mingw: { label: "C/C++", folder: "mingws", junction: "mingw", color: "text-blue-700 dark:text-blue-300 bg-blue-100 dark:bg-blue-950/50" },
};

export const ALL_ENV_TYPES: EnvType[] = [
  "jdk", "python", "node", "go", "rust", "maven", "gradle",
  "php", "llvm", "zig", "deno", "bun", "git", "gh", "mingw",
];

/** 管理器内的已装环境 */
export interface ManagedEnv {
  name: string;
  envType: EnvType;
  path: string;
  version: string | null;
  sizeBytes: number | null;
  isCurrent: boolean;
  status: "available" | "broken" | "inaccessible";
  statusDetail: string | null;
  sizeComplete: boolean;
  identityPath: string;
  isExternalLink: boolean;
}

export interface CategoryOverview {
  envType: EnvType;
  envs: ManagedEnv[];
  /** current junction 指向的环境名(未设置或损坏时为 null) */
  current: string | null;
  junctionPath: string | null;
  scanWarning: string | null;
  currentError: string | null;
}

export interface Overview {
  root: string;
  rootExists: boolean;
  portable: boolean;
  dataDir: string;
  categories: CategoryOverview[];
}

/** 系统散装环境 */
export interface ExternalEnv {
  tool: string;
  version: string | null;
  path: string | null;
  source: string;
  command: string;
  isPreferred: boolean;
  error: string | null;
  /** 可纳入管理的环境类型(结构不兼容时为 null) */
  envType: EnvType | null;
  installRoot: string | null;
  identityPath: string | null;
}

export interface ScanReport {
  tools: ExternalEnv[];
  warnings: string[];
}

export interface PathState {
  user: string[];
  system: string[];
}

export interface CacheInfo {
  tool: string;
  name: string;
  path: string;
  exists: boolean;
  sizeBytes: number | null;
  sizeComplete: boolean;
  sizeDetail: string | null;
}

export interface VersionInfo {
  version: string;
  date: string | null;
  sizeBytes: number | null;
  note: string | null;
  /** 直链下载地址(GitHub/Adoptium 等源使用) */
  url?: string | null;
}

/** 下载源 */
export interface SourceInfo {
  id: string;
  label: string;
}

/** 常见开发环境变量 */
export interface EnvVarInfo {
  name: string;
  label: string;
  value: string | null;
  suggested: string | null;
}

/** 包管理器路径配置(npm/pnpm/yarn/pip) */
export interface ToolConfig {
  tool: string;
  name: string;
  available: boolean;
  executablePath: string | null;
  error: string | null;
  globalPath: string | null;
  cachePath: string | null;
  defaultGlobal: string | null;
  defaultCache: string | null;
}

export interface PackageList {
  tool: string; executable: string; scope: string; source: string | null;
  packages: { name: string; version: string | null; path: string | null; detail: string | null }[];
  warnings: string[];
}

export interface PackageScanStatus {
  status: "running" | "done" | "partial" | "canceled" | "error";
  visited: number;
  currentPath: string;
  sources: { tool: string; path: string; packages: number; reinstallable: number; current: boolean; error: string | null }[];
  warnings: string[];
  warningCount: number;
  skippedProjects: number;
  roots: string[];
}

export interface MigrationPlan {
  tool: string;
  kind: "global" | "cache";
  source: string;
  target: string;
  files: number;
  bytes: number;
  links: number;
}

export interface MigrationResult {
  target: string;
  backup: string;
  files: number;
  bytes: number;
  journal: string;
}

export interface PipReinstallPlan {
  source: string;
  sourceKind: "python" | "directory";
  python: string;
  destination: string;
  sourceVersion: string;
  targetVersion: string;
  packages: { name: string; version: string; reason: string | null }[];
}

export interface PipReinstallStatus {
  status: "running" | "done" | "partial" | "error" | "canceled";
  message: string;
  destination: string;
  report: string;
  items: { name: string; version: string; status: string; detail: string | null }[];
  dependencyCheck: string | null;
}
export interface GlobalReinstallPlan {
  tool: string; executable: string; toolVersion: string;
  source: string; destination: string; binPath: string; cachePath: string; packages: { name: string; version: string; reason: string | null }[];
}
export interface GlobalReinstallStatus { tool: string; status: string; message: string; destination: string; binPath: string; report: string; items: { name: string; version: string; status: string; detail: string | null }[] }

export interface OldPackageCleanupPreview {
  token: string; tool: string; source: string; destination: string;
  packages: { name: string; version: string; path: string | null; reason: string | null }[];
  warnings: string[];
}
export interface OldPackageCleanupResult {
  report: string;
  items: { name: string; version: string; status: string; detail: string | null }[];
}

export interface Settings {
  root: string | null;
  downloadsDir: string | null;
  mirrors: Partial<Record<EnvType, string>>;
}

export type DownloadStatus =
  | "downloading"
  | "installing"
  | "done"
  | "error"
  | "canceled";

export interface DownloadTask {
  id: number;
  envType: EnvType;
  version: string;
  targetName: string;
  status: DownloadStatus;
  downloaded: number;
  total: number | null;
  speed: number;
  message: string | null;
}

export interface PathBackup {
  file: string;
  time: string;
}

/** 安装任务快照(与后端 TaskSnapshot 对应) */
export type InstallTask =
  & {
      id: number;
      envType: EnvType;
      version: string;
      targetName: string;
    }
  & (
      | { status: "downloading"; downloaded: number; total: number | null; speed: number }
      | { status: "installing"; message: string; cancelable: boolean }
      | { status: "done" }
      | { status: "error"; message: string }
      | { status: "canceled" }
    );
