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

export interface PythonScriptFile {
  name: string;
  size: number;
  modifiedMs: number;
}

export interface DefaultBlockInfo {
  name: string;
  modified: boolean;
  newDefaultAvailable: boolean;
}

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
}

/** A rectangle in fractions (0-1) of the client area. */
export interface FracRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type PythonScriptTemplate = "blank" | "per_account" | "controller" | "dashboard";

export const PYTHON_EVENTS = {
  installProgress: "python-install-progress",
  log: "python-log",
  runUpdate: "python-run-update",
  dashboardUpdate: "python-dashboard-update",
} as const;

const SCRIPT_NAME_PATTERN = /^[A-Za-z0-9_-]{1,64}\.py$/;

/** Same rule as the backend: a plain `.py` name inside `scripts/`, no separators. */
export function isValidScriptName(name: string): boolean {
  return SCRIPT_NAME_PATTERN.test(name);
}

export const pythonApi = {
  runtimeStatus: () => invoke<PythonRuntimeStatus>("python_runtime_status"),
  installRuntime: () => invoke<void>("python_install_runtime"),
  reinstallRuntime: () => invoke<void>("python_reinstall_runtime"),
  workspaceInfo: () => invoke<PythonWorkspaceInfo>("python_workspace_info"),

  listScripts: () => invoke<PythonScriptFile[]>("python_list_scripts"),
  readScript: (name: string) => invoke<string>("python_read_script", { name }),
  writeScript: (name: string, source: string) => invoke<void>("python_write_script", { name, source }),
  createScript: (name: string, template: PythonScriptTemplate) =>
    invoke<void>("python_create_script", { name, template }),
  deleteScript: (name: string) => invoke<void>("python_delete_script", { name }),

  listDefaultBlocks: () => invoke<DefaultBlockInfo[]>("python_list_default_blocks"),
  readDefaultBlock: (name: string) => invoke<string>("python_read_default_block", { name }),
  writeDefaultBlock: (name: string, source: string) =>
    invoke<void>("python_write_default_block", { name, source }),
  restoreDefaultBlock: (name: string) => invoke<void>("python_restore_default_block", { name }),

  openWorkspaceFolder: () => invoke<void>("python_open_workspace_folder"),
  openInEditor: (name: string) => invoke<void>("python_open_in_editor", { name }),

  getRunConfigs: () => invoke<Record<string, PythonRunConfig>>("python_get_run_configs"),
  saveRunConfig: (config: PythonRunConfig) => invoke<void>("python_save_run_config", { config }),
  startRun: (config: PythonRunConfig) => invoke<string>("python_start_run", { config }),
  stopRun: (runId: string) => invoke<void>("python_stop_run", { runId }),
  stopAllRuns: () => invoke<void>("python_stop_all_runs"),
  listRuns: () => invoke<PythonRunInfo[]>("python_list_runs"),

  getDashboard: () => invoke<DashboardSection[]>("python_get_dashboard"),
  clearDashboard: (script: string) => invoke<void>("python_clear_dashboard", { script }),

  inspectorCapture: (userId: number) => invoke<InspectorFrame>("python_inspector_capture", { userId }),
  saveAsset: (userId: number, rect: FracRect, name: string) =>
    invoke<string>("python_save_asset", { userId, rect, name }),

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
