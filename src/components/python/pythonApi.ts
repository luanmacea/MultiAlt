/**
 * Contract with the Rust side of the Python automation feature
 * (`src-tauri/src/python/`). Types mirror `python/types.rs` (camelCase) and
 * the wrappers keep the command names and argument names of the Rust commands
 * (Tauri turns `user_id` into `userId` on the JS side).
 *
 * The frontend never reaches the filesystem or the network on its own: every
 * call here goes through `invoke`.
 */
import { invoke } from "@tauri-apps/api/core";

export interface PythonRuntimeStatus {
  installed: boolean;
  version: string | null;
  pythonExe: string | null;
  packages: string[];
  installing: boolean;
}

export type PythonInstallStage = "python" | "pip" | "packages" | "ready" | "error";

export interface PythonInstallProgress {
  stage: PythonInstallStage;
  message: string;
  downloaded: number;
  total: number;
}

export interface PythonWorkspaceInfo {
  root: string;
  scriptsDir: string;
  assetsDir: string;
  pythonExe: string | null;
  cliCommand: string;
}

/** One entry of the workspace tree (`python_fs_tree`). The root has `path: ""`. */
export interface FsNode {
  /** Relative to the workspace, `/` as separator. */
  path: string;
  name: string;
  kind: "file" | "dir";
  size: number;
  modifiedMs: number;
  /** False for the examples, the pinned blocks without Developer mode and the internal folders. */
  writable: boolean;
  /** One of the default blocks in `multialt/defaults/`. */
  pinned: boolean;
  /** A `.py` file directly in `scripts/`: the only files a run can start from. */
  runnable: boolean;
  children?: FsNode[] | null;
}

export interface GlobalFunction {
  name: string;
  /** `walk_forward(account, seconds=1.0)` */
  signature: string;
  /** First line of the docstring, empty when there is none. */
  doc: string;
}

export interface GlobalModule {
  path: string;
  /** Import path: `lib.helpers` or `multialt.defaults.vision`. */
  module: string;
  pinned: boolean;
  modified: boolean;
  newDefaultAvailable: boolean;
  functions: GlobalFunction[];
}

export type FsKind = "file" | "dir";

export type PythonTrigger =
  | { kind: "manual" }
  | { kind: "interval"; minutes: number }
  | { kind: "loop" };

export type PythonRunMode = "one_at_a_time" | "all_together";

export interface PythonRunConfig {
  script: string;
  accounts: number[];
  mode: PythonRunMode;
  trigger: PythonTrigger;
}

export type PythonRunSource = "app" | "terminal";
export type PythonRunState = "starting" | "running" | "waiting" | "stopping" | "finished" | "failed";

export interface PythonRunInfo {
  id: string;
  script: string;
  accounts: number[];
  mode: PythonRunMode;
  trigger: PythonTrigger;
  source: PythonRunSource;
  state: PythonRunState;
  startedMs: number;
  endedMs: number | null;
  nextRunMs: number | null;
  error: string | null;
}

export type PythonLogLevel = "log" | "warn" | "error" | "system" | "blocked";

export interface PythonLogLine {
  runId: string | null;
  userId: number | null;
  level: PythonLogLevel;
  message: string;
  code: string | null;
  hint: string | null;
  ts: number;
}

export type DashboardWidgetType =
  | "stat"
  | "progress"
  | "text"
  | "badge"
  | "table"
  | "accounts"
  | "line"
  | "log";

/** One widget of a script's dashboard layout, as `dashboard.define` sends it. */
export interface DashboardWidget {
  id: string;
  type: DashboardWidgetType | (string & {});
  title?: string;
  columns?: string[];
  max?: number;
}

export interface DashboardSection {
  script: string;
  /** Raw values from the script; the view validates each entry before drawing. */
  layout: unknown[];
  /** Keys are `"id"` or `"id@userId"`. */
  values: Record<string, unknown>;
  /** `id` -> `[[ts, value], ...]`, at most 500 points. */
  series: Record<string, unknown>;
  updatedMs: number;
}

export interface InspectorFrame {
  width: number;
  height: number;
  pngBase64: string;
  /** The frame on screen: `saveAsset` cuts from it, not from a new capture. */
  frameId: number;
}

/** A rectangle in fractions (0-1) of the client area. */
export interface FracRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export const PYTHON_EVENTS = {
  installProgress: "python-install-progress",
  log: "python-log",
  runUpdate: "python-run-update",
  dashboardUpdate: "python-dashboard-update",
} as const;

export const pythonApi = {
  runtimeStatus: () => invoke<PythonRuntimeStatus>("python_runtime_status"),
  installRuntime: () => invoke<void>("python_install_runtime"),
  reinstallRuntime: () => invoke<void>("python_reinstall_runtime"),
  workspaceInfo: () => invoke<PythonWorkspaceInfo>("python_workspace_info"),

  fsTree: () => invoke<FsNode>("python_fs_tree"),
  fsRead: (path: string) => invoke<string>("python_fs_read", { path }),
  /** A `data:image/png;base64,...` URL; only files in `assets/`. */
  fsReadImage: (path: string) => invoke<string>("python_fs_read_image", { path }),
  fsWrite: (path: string, content: string) => invoke<void>("python_fs_write", { path, content }),
  fsCreate: (path: string, kind: FsKind) => invoke<void>("python_fs_create", { path, kind }),
  fsRename: (from: string, to: string) => invoke<void>("python_fs_rename", { from, to }),
  /** Moves to the workspace `.trash/`; nothing is erased. */
  fsDelete: (path: string) => invoke<void>("python_fs_delete", { path }),
  listGlobalFunctions: () => invoke<GlobalModule[]>("python_list_global_functions"),

  restoreDefaultBlock: (name: string) => invoke<void>("python_restore_default_block", { name }),

  openWorkspaceFolder: () => invoke<void>("python_open_workspace_folder"),

  getRunConfigs: () => invoke<Record<string, PythonRunConfig>>("python_get_run_configs"),
  saveRunConfig: (config: PythonRunConfig) => invoke<void>("python_save_run_config", { config }),
  startRun: (config: PythonRunConfig) => invoke<string>("python_start_run", { config }),
  stopRun: (runId: string) => invoke<void>("python_stop_run", { runId }),
  stopAllRuns: () => invoke<void>("python_stop_all_runs"),
  listRuns: () => invoke<PythonRunInfo[]>("python_list_runs"),

  getDashboard: () => invoke<DashboardSection[]>("python_get_dashboard"),
  clearDashboard: (script: string) => invoke<void>("python_clear_dashboard", { script }),

  inspectorCapture: (userId: number) => invoke<InspectorFrame>("python_inspector_capture", { userId }),
  saveAsset: (userId: number, rect: FracRect, name: string, frameId: number) =>
    invoke<string>("python_save_asset", { userId, rect, name, frameId }),

  setDeveloperMode: (enabled: boolean) => invoke<void>("python_set_developer_mode", { enabled }),
  getLogs: () => invoke<PythonLogLine[]>("python_get_logs"),
};

/** Backend errors reach the UI as strings (or `Error`s in tests); show them as text. */
export function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}
