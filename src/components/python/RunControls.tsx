import { useEffect, useRef, useState } from "react";
import { ChevronDown, Play, Square } from "lucide-react";
import { useStore } from "../../store";
import { RunPanel, defaultRunConfig } from "./RunPanel";
import { errorText, pythonApi, type PythonRunInfo } from "./pythonApi";
import { isRunActive } from "./usePythonRuns";

export const NOT_RUNNABLE_TEXT = "Only files directly in scripts/ can be run";

interface RunControlsProps {
  /** File name of the script (`farm.py`); runs are started by this simple name. */
  script: string;
  /** The file is a `.py` directly in `scripts/`. */
  runnable: boolean;
  runtimeInstalled: boolean;
  runs: PythonRunInfo[];
  onToast: (message: string, tone?: "info" | "success" | "warn" | "error") => void;
}

/**
 * Run and Stop for the open file, in the editor bar. Accounts, mode and trigger
 * live in a popover that reuses the RunPanel (it saves every change), so Run
 * starts the run with whatever was saved last.
 */
export function RunControls({ script, runnable, runtimeInstalled, runs, onToast }: RunControlsProps) {
  const store = useStore();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [starting, setStarting] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!settingsOpen) return;
    function onDown(event: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(event.target as Node)) setSettingsOpen(false);
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") setSettingsOpen(false);
    }
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [settingsOpen]);

  // The popover belongs to one script.
  useEffect(() => setSettingsOpen(false), [script]);

  const activeRuns = runs.filter((run) => run.script === script && isRunActive(run));
  const alreadyRunning = activeRuns.length > 0;
  const canRun = runnable && runtimeInstalled && !starting && !alreadyRunning;

  async function start() {
    if (!canRun) return;
    setStarting(true);
    try {
      const saved = await pythonApi.getRunConfigs();
      const config = { ...defaultRunConfig(script), ...(saved?.[script] ?? {}), script };
      const known = new Set(store.accounts.map((account) => account.UserID));
      const accounts = config.accounts.filter((id) => known.has(id));
      if (accounts.length === 0) {
        setSettingsOpen(true);
        onToast("Pick at least one account in the run settings.", "info");
        return;
      }
      const toRun = { ...config, accounts };
      await pythonApi.saveRunConfig(toRun);
      await pythonApi.startRun(toRun);
    } catch (e) {
      onToast(errorText(e), "error");
    } finally {
      setStarting(false);
    }
  }

  async function stop() {
    try {
      for (const run of activeRuns) await pythonApi.stopRun(run.id);
    } catch (e) {
      onToast(errorText(e), "error");
    }
  }

  const reason = !runnable
    ? NOT_RUNNABLE_TEXT
    : !runtimeInstalled
      ? "Install Python to run scripts"
      : alreadyRunning
        ? "This script is already running"
        : "";

  return (
    <div ref={wrapRef} className="relative flex items-center gap-1.5">
      {reason ? <span className="text-[11.5px] text-zinc-500">{reason}</span> : null}
      {runnable ? (
        <button
          type="button"
          aria-haspopup="dialog"
          aria-expanded={settingsOpen}
          title="Accounts, mode and trigger"
          onClick={() => setSettingsOpen((open) => !open)}
          className="theme-btn-ghost inline-flex items-center gap-1 px-2 py-1 text-[12px]"
        >
          Accounts
          <ChevronDown size={11} strokeWidth={2} className={`transition-transform ${settingsOpen ? "rotate-180" : ""}`} />
        </button>
      ) : null}
      {alreadyRunning ? (
        <button
          type="button"
          onClick={() => void stop()}
          className="inline-flex items-center gap-1.5 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-1 text-[12px] font-medium text-red-300 transition-colors hover:bg-red-500/20"
        >
          <Square size={11} strokeWidth={2.2} />
          {activeRuns.length > 1 ? `Stop (${activeRuns.length})` : "Stop"}
        </button>
      ) : null}
      <button
        type="button"
        onClick={() => void start()}
        disabled={!canRun}
        title={reason || "Run this script"}
        className="theme-btn inline-flex items-center gap-1.5 px-3 py-1 text-[12px] font-medium disabled:cursor-not-allowed disabled:opacity-40"
      >
        <Play size={12} strokeWidth={2.2} />
        Run
      </button>

      {settingsOpen ? (
        <div
          role="dialog"
          aria-label="Run settings"
          className="theme-modal-scope theme-panel theme-border absolute right-0 top-full z-40 mt-1.5 w-[min(620px,calc(100vw-80px))] rounded-xl border shadow-2xl"
        >
          <RunPanel script={script} runtimeInstalled={runtimeInstalled} runs={runs} onToast={onToast} />
        </div>
      ) : null}
    </div>
  );
}
