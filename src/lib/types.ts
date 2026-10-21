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
  | "mingw"
  | "adb";

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
  adb: { label: "ADB", folder: "adbs", junction: "adb", color: "text-green-700 dark:text-green-300 bg-green-100 dark:bg-green-950/50" },
};

export const ALL_ENV_TYPES: EnvType[] = [
  "jdk", "python", "node", "go", "rust", "maven", "gradle",
  "php", "llvm", "zig", "deno", "bun", "git", "gh", "mingw", "adb",
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
  isLinkEntry: boolean;
  isExternalLink: boolean;
}

export interface UninstallScopeItem {
  resource: string;
  path: string | null;
  details: string;
  count: number | null;
}

/** One command a new terminal should resolve after switching environments. */
export interface SwitchCommandDiagnosis {
  command: string;
  status: "managed" | "shadowed" | "missing";
  expected: string;
  resolved: string | null;
  detail: string;
}

/** Read-only diagnosis returned by switch_env; nothing is executed or written. */
export interface SwitchDiagnosis {
  envType: string;
  name: string;
  currentTarget: string | null;
  version: string | null;
  commands: SwitchCommandDiagnosis[];
  pathStatus: "active" | "requires-new-terminal" | "not-in-path" | "shadowed" | "unknown";
  pathDetail: string;
  retained: string[];
  notes: string[];
}

/** Read-only repair preview for a shadowed command after switching. */
export interface PathRepairPreview {
  envType: string;
  entry: string;
  shadowing: string[];
  /** `reorder` when only ordering changes, `manual` when the user must act, `none` when clean. */
  mode: "reorder" | "manual" | "none";
  expectedPath: string;
  resultingPath: string;
  summary: string;
  notes: string[];
  blocked: string[];
  plan: {
    expectedPath: string;
    entry: string;
    shadowing: string[];
    resultingPath: string;
  } | null;
  manualEntries: string[];
}

/** Outcome of a repair, recomputed from the live PATH after the write. */
export interface PathRepairResult {
  applied: NonNullable<PathRepairPreview["plan"]>;
  verified: boolean;
  userPath: string;
  commands: SwitchCommandDiagnosis[];
  problems: string[];
  requestedPath: string;
  requiresNewTerminal: boolean;
}

export interface UninstallPreview {  installerPlan?: {
    state: string;
    installPath: string;
    envType: EnvType;
    invocation: {
      registrationId: string;
      executable: string;
      arguments: string[];
      fileSha256: string;
    } | null;
    assessments: { registrationId: string; state: string; details: string }[];
    errors: string[];
    registryFingerprint: string;
  } | null;
  pathSnapshotHash?: string | null;
  currentLinkSnapshotHash?: string | null;
  legacyLinks?: UninstallScopeItem[];
  confirmedLegacyLinks?: string[];
  legacyPathEntries?: UninstallScopeItem[];
  confirmedLegacyPathEntries?: string[];
  name: string;
  envType: EnvType;
  isCurrent: boolean;
  isLinkEntry: boolean;
  isExternalLink: boolean;
  snapshotHash: string | null;
  removable: UninstallScopeItem[];
  retained: UninstallScopeItem[];
  unverified: UninstallScopeItem[];
  blocked: UninstallScopeItem[];
  scopeComplete: boolean;
  /** 卸载会删除内容；停用只解除管理、保留全部文件。 */
  mode?: "uninstall" | "detach";
}

export interface UninstallActionItem {
  resource: string;
  path: string | null;
  details: string;
}

export type UninstallResultStatus = "verifiedScopeRemoved" | "partial";

export interface UninstallResult {
  name: string;
  status: UninstallResultStatus;
  removed: UninstallActionItem[];
  retained: UninstallScopeItem[];
  unverified: UninstallScopeItem[];
  failed: UninstallActionItem[];
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

/** 只读清单中的一条环境记录；路径是本机绝对路径，不是可移植备份。 */
export interface ManifestEntry {
  envType: EnvType;
  name: string;
  path: string;
  identityPath: string;
  version: string | null;
  status: string;
  isCurrent: boolean;
  isLinkEntry: boolean;
  isExternalLink: boolean;
}

export interface EnvironmentManifest {
  schemaVersion: number;
  generatedAt: string;
  root: string | null;
  portable: boolean;
  dataDir: string;
  entries: ManifestEntry[];
  /** 读取者必须先了解的跨机器限制，不是免责声明。 */
  compatibility: string[];
  notes: string[];
}

/** 清单条目与本机现状的比对结论，每条记录只落入其中一类。 */
export type ManifestComparisonKind =
  | "unchanged"
  | "added"
  | "missing"
  | "changed";

export interface ManifestComparisonRow {
  envType: EnvType;
  name: string;
  kind: ManifestComparisonKind;
  /** 清单一侧；`added` 时为 null。 */
  recordedVersion: string | null;
  recordedPath: string | null;
  recordedStatus: string | null;
  /** 本机一侧；`missing` 时为 null。 */
  currentVersion: string | null;
  currentPath: string | null;
  currentStatus: string | null;
  /** 差异字段名，仅在 `changed` 时非空。 */
  differences: string[];
}

export interface ManifestComparisonSummary {
  unchanged: number;
  added: number;
  missing: number;
  changed: number;
}

/** 只读比对结果：不含任何恢复指令，不能据此创建环境。 */
export interface ManifestComparison {
  schemaVersion: number;
  recordedGeneratedAt: string;
  recordedPortable: boolean;
  recordedRoot: string | null;
  currentRoot: string | null;
  sameRoot: boolean;
  rows: ManifestComparisonRow[];
  summary: ManifestComparisonSummary;
  limitations: string[];
}

/** 单个环境的按需重新读取结果；只读，不修改任何目录。 */
export interface EnvironmentRedetect {
  envType: EnvType;
  name: string;
  path: string;
  version: string | null;
  status: string;
  statusDetail: string | null;
  isCurrent: boolean;
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
  scanStatus?: ScanStatus;
  /** 可纳入管理的环境类型(结构不兼容时为 null) */
  envType: EnvType | null;
  installRoot: string | null;
  identityPath: string | null;
  /** Same-installation entrypoints that reported a different version. */
  versionConflicts?: VersionConflict[];
}

export interface VersionConflict {
  path: string;
  version: string | null;
  error: string | null;
}

export type ScanStatus =
  | "unscanned"
  | "available"
  | "notDiscovered"
  | "permissionDenied"
  | "startFailed"
  | "commandFailed"
  | "parseFailed"
  | "timedOut";

export interface ScanReport {
  tools: ExternalEnv[];
  coreTools: ToolScanSummary[];
  status: "complete" | "partial" | "failed";
  warnings: string[];
}

export interface ToolScanSummary {
  tool: string;
  status: ScanStatus;
  entryCount: number;
  detail: string | null;
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

export interface SizeSnapshot {
  id: number;
  key: string;
  path: string;
  status: "running" | "done" | "partial" | "canceled";
  bytes: number;
  files: number;
  directories: number;
  skippedLinks: number;
  issues: number;
  detail: string | null;
  currentPath: string;
  complete: boolean;
  elapsedMs: number;
}

export interface VersionInfo {
  version: string;
  date: string | null;
  sizeBytes: number | null;
  note: string | null;
  /** 直链下载地址(GitHub/Adoptium 等源使用) */
  url?: string | null;
  verification?: VerificationInfo;
}

export type VerificationInfo =
  | { status: "trustedSha256"; expected: string; source: string }
  | { status: "checksumFile"; url: string; filename: string; source: string }
  | { status: "invalid"; reason: string }
  | { status: "unverified"; reason: string };

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
  /** 当前选中可执行文件返回的版本号 */
  version: string | null;
  executablePath: string | null;
  error: string | null;
  globalPath: string | null;
  cachePath: string | null;
  defaultGlobal: string | null;
  defaultCache: string | null;
  /** Manager-specific command entry for the selected global root. */
  commandPath: string | null;
  /** Whether commandPath came from the manager or was derived from the declared layout. */
  commandPathSource: "queried" | "layout" | null;
  /** Read-only warning about a legacy layout that needs recovery first. */
  layoutWarning: string | null;
  /** Runtime reason this installation cannot use its own self-update. */
  selfUpdateBlocker: string | null;
}

/** One `key → value` change with its previous value, shown before writing. */
export interface ToolConfigChange {
  key: string;
  role: "global" | "command" | "cache" | "configuration";
  current: string | null;
  next: string;
}

/** Read-only configuration diff; applying it is a separate call. */
export interface ToolConfigPreview {
  tool: string;
  available: boolean;
  blocked: string[];
  warnings: string[];
  changes: ToolConfigChange[];
  commandEntry: string;
  commandEntryChanged: boolean;
  entryInPath: boolean;
  commandStatus: "managed" | "shadowed" | "missing" | "unknown";
  commandDetail: string;
  movesExistingPackages: boolean;
  notes: string[];
  /** Rows whose value actually differs from the current value. */
  changedCount: number;
  /** Existing global packages read from the current root before writing. */
  affectedPackages: AffectedPackages;
}

export interface AffectedPackage {
  name: string;
  version: string | null;
  status: "kept" | "unresolved";
  detail: string | null;
}

export interface AffectedPackages {
  packages: AffectedPackage[];
  unresolvedCount: number;
  note: string;
}

/** Batch result for "configure everything"; blocked tools are skipped. */
export interface ToolConfigBatchPreview {
  items: ToolConfigPreview[];
  ready: string[];
  blocked: string[];
  totalChanges: number;
  movesExistingPackages: boolean;
}

export interface UvToolLayout {
  name: string;
  version: string | null;
  directory: string;
  receiptStatus: string;
  receiptSha256: string | null;
  requirement: string | null;
  entrypoints: string[];
  entrypointSnapshots: UvEntrypointSnapshot[];
  needsRecovery: boolean;
  recoverable: boolean;
  issues: string[];
}

export interface UvEntrypointSnapshot {
  path: string;
  sha256: string | null;
  legacy: boolean;
}

export interface UvLegacyLayout {
  toolRoot: string;
  legacyBin: string;
  legacyCache: string;
  nestedBinExists: boolean;
  nestedCacheExists: boolean;
  proposedBin: string;
  proposedCache: string;
  status: "clean" | "legacy" | "legacy-partial" | "legacy-empty";
  tools: UvToolLayout[];
  issues: string[];
}

export interface UvRecoveryItem {
  name: string;
  version: string;
  // 只有这三种：`retained` 不是条目状态，而是 `UvRecoveryReport.retained`
  // 里的保留项——那些工具在预览阶段就被判定为不可自动恢复，从未被尝试。
  status: "recovered" | "recovered-with-residue" | "failed";
  detail: string | null;
  removedEntrypoints: string[];
  newEntrypoints: string[];
}

export interface UvRecoveryReport {
  status: "complete" | "partial" | "failed";
  items: UvRecoveryItem[];
  retained: string[];
  layout: UvLegacyLayout;
}

export interface UpdateCapabilities {
  /** 此入口只安装 latest，不接受版本参数 */
  latestOnly: boolean;
  /** 管理器确实支持的指定版本命令；null 表示没有可背书的单行命令 */
  manualPinnedCommand: string | null;
}

export interface PackageManagerUpdatePreview {
  tool: string;
  blocked: string | null;
  executablePath: string | null;
  currentVersion: string | null;
  updateProgram: string | null;
  updateArguments: string[];
  runsSameFile: boolean | null;
  capabilities: UpdateCapabilities;
  notes: string[];
}

export interface PackageManagerUpdateBatchPreview {
  items: PackageManagerUpdatePreview[];
  ready: string[];
  blocked: string[];
}

export type PackageManagerBatchOutcome =
  | "updated"
  | "unchanged"
  | "unverified"
  | "failed"
  | "skipped";

export interface PackageManagerBatchItem {
  tool: string;
  outcome: PackageManagerBatchOutcome;
  result: PackageManagerUpdateResult | null;
  skippedReason: string | null;
}

export interface PackageManagerBatchResult {
  items: PackageManagerBatchItem[];
  retryable: string[];
}

export interface PackageManagerUpdateRecord {
  status: "planned" | "completed" | "unverified" | "failed";
  tool: string;
  executablePath: string;
  beforeVersion: string | null;
  afterVersion: string | null;
  program: string;
  arguments: string[];
  attempt: string;
}

export interface PackageManagerUpdateResult {
  status:
    | "commandFailed"
    | "verificationFailed"
    | "versionChanged"
    | "versionUnchanged"
    | "versionUnknown";
  tool: string;
  executablePath: string;
  beforeVersion: string | null;
  afterVersion: string | null;
  versionChanged: boolean | null;
  output: string;
  error: string | null;
}

export interface PackageList {
  tool: string; executable: string; scope: string; source: string | null;
  packages: { name: string; version: string | null; path: string | null; detail: string | null }[];
  warnings: string[];
}

export interface PackageScanProblem {
  kind:
    | "permission-denied"
    | "not-found"
    | "unreadable"
    | "entry-unreadable"
    | "package-list-failed"
    | "link-skipped"
    | "depth-limit"
    | "limit-reached";
  path: string | null;
  detail: string;
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
  problems: PackageScanProblem[];
  permissionDenied: number;
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
  allowUnknownRegistry?: boolean;
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

export interface AppUpdateStatus {
  status: "upToDate" | "updateAvailable" | "noRelease" | "unknownVersion" | "error";
  currentVersion: string;
  latestVersion: string | null;
  releaseName: string | null;
  releaseUrl: string | null;
  notes: string | null;
  publishedAt: string | null;
  assets: { name: string; size: number; digest: string | null }[];
  error: string | null;
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
