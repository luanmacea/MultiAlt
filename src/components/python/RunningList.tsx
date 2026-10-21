import { useEffect, useState } from "react";
import { Square } from "lucide-react";
import { errorText, pythonApi, type PythonRunInfo, type PythonRunState, type PythonTrigger } from "./pythonApi";
import { isRunActive } from "./usePythonRuns";

const STATE_STYLE: Record<PythonRunState, string> = {
  starting: "bg-sky-500/15 text-sky-300",
  running: "bg-emerald-500/15 text-emerald-400",
  waiting: "bg-amber-500/15 text-amber-300",
  stopping: "bg-amber-500/15 text-amber-300",
  finished: "bg-zinc-700/40 text-zinc-400",
  failed: "bg-red-500/15 text-red-400",
};

export function describeTrigger(trigger: PythonTrigger): string {
  if (trigger.kind === "interval") return `every ${trigger.minutes} min`;
  if (trigger.kind === "loop") return "loop";
  return "manual";
}

export function describeMode(mode: PythonRunInfo["mode"]): string {
  return mode === "all_together" ? "all together" : "one at a time";
}

/** "in 4m 12s" for a future moment, empty when there is none. */
export function formatCountdown(targetMs: number | null, now: number): string {
  if (targetMs === null) return "";
  const seconds = Math.max(0, Math.round((targetMs - now) / 1000));
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return minutes > 0 ? `in ${minutes}m ${rest.toString().padStart(2, "0")}s` : `in ${rest}s`;
}

interface RunningListProps {
  runs: PythonRunInfo[];
  onToast: (message: string, tone?: "info" | "success" | "warn" | "error") => void;
}

/** Every run the backend knows, started from this screen or from the terminal. */
export function RunningList({ runs, onToast }: RunningListProps) {
  const [now, setNow] = useState(() => Date.now());
  const active = runs.filter(isRunActive);

  useEffect(() => {
    if (!runs.some((run) => run.nextRunMs !== null)) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [runs]);

  async function stopOne(id: string) {
    try {
      await pythonApi.stopRun(id);
    } catch (e) {
      onToast(errorText(e), "error");
    }
  }

  async function stopAll() {
    try {
      await pythonApi.stopAllRuns();
    } catch (e) {
      onToast(errorText(e), "error");
    }
  }

  return (
    <section aria-label="Running" className="rounded-lg border theme-border bg-[var(--panel-soft)]">
      <div className="flex items-center gap-2 border-b theme-border px-3 py-1.5">
        <h3 className="text-[12px] font-medium text-zinc-200">Running</h3>
        <span className="text-[11px] text-zinc-500">{active.length} active</span>
        <button
          type="button"
          onClick={() => void stopAll()}
          disabled={active.length === 0}
          className="ml-auto inline-flex items-center gap-1 rounded-md border border-red-500/30 bg-red-500/10 px-2 py-0.5 text-[11.5px] text-red-300 transition-colors hover:bg-red-500/20 disabled:cursor-not-allowed disabled:opacity-40"
        >
          <Square size={10} strokeWidth={2.2} />
          Stop all
        </button>
      </div>
      {runs.length === 0 ? (
        <div className="px-3 py-3 text-[12px] text-zinc-500">No runs yet. Pick accounts and press Run.</div>
      ) : (
        <ul className="max-h-[210px] divide-y divide-zinc-800/60 overflow-y-auto">
          {runs.map((run) => (
            <li key={run.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2">
              <span className="min-w-0 max-w-[220px] truncate font-mono text-[12px] text-zinc-200">{run.script}</span>
              <span
                className={`rounded px-1.5 py-0.5 text-[11px] font-medium ${
                  run.source === "terminal" ? "bg-violet-500/15 text-violet-300" : "bg-zinc-700/40 text-zinc-400"
                }`}
                title={run.source === "terminal" ? "Started from a terminal (developer mode)" : "Started from this screen"}
              >
                {run.source}
              </span>
              <span className={`rounded px-1.5 py-0.5 text-[11px] font-medium ${STATE_STYLE[run.state]}`}>{run.state}</span>
              <span className="text-[11.5px] text-zinc-500">
                {run.accounts.length} {run.accounts.length === 1 ? "account" : "accounts"}, {describeMode(run.mode)},{" "}
                {describeTrigger(run.trigger)}
              </span>
              {run.nextRunMs !== null && isRunActive(run) ? (
                <span className="text-[11.5px] text-sky-300">next run {formatCountdown(run.nextRunMs, now)}</span>
              ) : null}
              {run.error ? <span className="min-w-0 basis-full truncate text-[11.5px] text-red-400">{run.error}</span> : null}
              {isRunActive(run) ? (
                <button
                  type="button"
                  onClick={() => void stopOne(run.id)}
                  aria-label={`Stop ${run.script} (${run.id})`}
                  className="theme-btn-ghost ml-auto inline-flex items-center gap-1 px-2 py-0.5 text-[11.5px]"
                >
                  <Square size={10} strokeWidth={2.2} />
                  Stop
                </button>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
