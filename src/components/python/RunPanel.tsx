import { useEffect, useMemo, useRef, useState } from "react";
import { Play, Square } from "lucide-react";
import { useStore } from "../../store";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { Select } from "../ui/Select";
import { errorText, pythonApi, type PythonRunConfig, type PythonRunInfo, type PythonRunMode } from "./pythonApi";
import { isRunActive } from "./usePythonRuns";

type TriggerKind = PythonRunConfig["trigger"]["kind"];

const MIN_INTERVAL_MINUTES = 1;
const MAX_INTERVAL_MINUTES = 1440;

export function defaultRunConfig(script: string): PythonRunConfig {
  return { script, accounts: [], mode: "one_at_a_time", trigger: { kind: "manual" } };
}

const MODES: { id: PythonRunMode; label: string; hint: string }[] = [
  { id: "one_at_a_time", label: "One at a time", hint: "Accounts take turns, in order." },
  { id: "all_together", label: "All together", hint: "Every account at once; input still goes through the focus queue." },
];

const TRIGGERS: { value: TriggerKind; label: string }[] = [
  { value: "manual", label: "Manual (run once)" },
  { value: "interval", label: "Every N minutes" },
  { value: "loop", label: "Loop (the script repeats itself)" },
];

interface RunPanelProps {
  script: string;
  /** False until Python is installed: the Run button stays off. */
  runtimeInstalled: boolean;
  runs: PythonRunInfo[];
  onToast: (message: string, tone?: "info" | "success" | "warn" | "error") => void;
}

/** Accounts, mode and trigger for one script, plus Run and Stop. */
export function RunPanel({ script, runtimeInstalled, runs, onToast }: RunPanelProps) {
  const store = useStore();
  const labelOf = useAccountLabel();
  const [config, setConfig] = useState<PythonRunConfig>(() => defaultRunConfig(script));
  const [loaded, setLoaded] = useState(false);
  const [starting, setStarting] = useState(false);
  // What is typed in the minutes box. It can be empty while typing, which the saved number cannot.
  const [minutesDraft, setMinutesDraft] = useState<string | null>(null);
  const scriptRef = useRef(script);

  // Load the saved configuration of the selected script.
  useEffect(() => {
    scriptRef.current = script;
    let cancelled = false;
    setLoaded(false);
    setConfig(defaultRunConfig(script));
    pythonApi
      .getRunConfigs()
      .then((all) => {
        if (cancelled) return;
        const saved = all?.[script];
        if (saved) setConfig({ ...defaultRunConfig(script), ...saved, script });
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, [script]);

  const knownIds = useMemo(() => new Set(store.accounts.map((account) => account.UserID)), [store.accounts]);
  const selected = useMemo(() => new Set(config.accounts.filter((id) => knownIds.has(id))), [config.accounts, knownIds]);
  const openClients = useMemo(
    () => new Set<number>([...store.launchedByProgram, ...store.adoptedClients]),
    [store.launchedByProgram, store.adoptedClients]
  );

  const activeRuns = runs.filter((run) => run.script === script && isRunActive(run));

  function update(patch: Partial<PythonRunConfig>) {
    const next = { ...config, ...patch, script };
    setConfig(next);
    void pythonApi.saveRunConfig(next).catch((e) => onToast(errorText(e), "error"));
  }

  function setAccounts(ids: Iterable<number>) {
    update({ accounts: [...ids] });
  }

  function toggleAccount(id: number) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setAccounts(next);
  }

  function setGroup(ids: number[], on: boolean) {
    const next = new Set(selected);
    for (const id of ids) {
      if (on) next.add(id);
      else next.delete(id);
    }
    setAccounts(next);
  }

  function setTriggerKind(kind: TriggerKind) {
    if (kind === "interval") {
      const minutes = config.trigger.kind === "interval" ? config.trigger.minutes : 30;
      update({ trigger: { kind, minutes } });
    } else {
      update({ trigger: { kind } });
    }
  }

  function setMinutes(raw: string) {
    setMinutesDraft(raw);
    const parsed = Math.round(Number(raw));
    if (raw.trim() === "" || !Number.isFinite(parsed)) return;
    const minutes = Math.min(MAX_INTERVAL_MINUTES, Math.max(MIN_INTERVAL_MINUTES, parsed));
    update({ trigger: { kind: "interval", minutes } });
  }

  async function start() {
    if (starting) return;
    setStarting(true);
    const toRun: PythonRunConfig = { ...config, script, accounts: [...selected] };
    try {
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

  const canRun = runtimeInstalled && selected.size > 0 && !starting && loaded;
  const modeHint = MODES.find((mode) => mode.id === config.mode)?.hint ?? "";
  const triggerHint =
    config.trigger.kind === "loop"
      ? "Use this when the script has its own while loop."
      : config.trigger.kind === "interval"
        ? "A new run starts that many minutes after the previous one ends."
        : "Runs once when you press Run.";

  return (
    <section aria-label="Run" className="rounded-lg border theme-border bg-[var(--panel-soft)]">
      <div className="grid gap-x-5 gap-y-3 p-3 md:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]">
        <div className="min-w-0">
          <div className="mb-1.5 flex items-center gap-2">
            <h3 className="text-[12px] font-medium text-zinc-200">Accounts</h3>
            <span className="text-[11px] text-zinc-500">{selected.size} selected</span>
            <span className="ml-auto flex gap-1">
              <button type="button" className="theme-btn-ghost px-1.5 py-0.5 text-[11px]" onClick={() => setAccounts(store.accounts.map((a) => a.UserID))}>
                All
              </button>
              <button
                type="button"
                className="theme-btn-ghost px-1.5 py-0.5 text-[11px]"
                onClick={() => setAccounts(store.accounts.filter((a) => openClients.has(a.UserID)).map((a) => a.UserID))}
              >
                Open clients
              </button>
              <button type="button" className="theme-btn-ghost px-1.5 py-0.5 text-[11px]" onClick={() => setAccounts([])}>
                None
              </button>
            </span>
          </div>
          <div className="max-h-[190px] overflow-y-auto rounded-lg border theme-border">
            {store.groups.length === 0 ? (
              <div className="px-3 py-3 text-[12px] text-zinc-500">No accounts yet.</div>
            ) : (
              store.groups.map((group) => {
                const ids = group.accounts.map((account) => account.UserID);
                const allOn = ids.every((id) => selected.has(id));
                return (
                  <div key={group.key}>
                    <label className="flex cursor-pointer items-center gap-2 border-b theme-border bg-zinc-800/30 px-2.5 py-1 text-[11px] font-medium uppercase tracking-wide text-zinc-400">
                      <input
                        type="checkbox"
                        className="accent-sky-400"
                        checked={allOn}
                        onChange={(e) => setGroup(ids, e.target.checked)}
                        aria-label={`Select the group ${group.displayName}`}
                      />
                      {group.displayName}
                    </label>
                    {group.accounts.map((account) => (
                      <label
                        key={account.UserID}
                        className="flex cursor-pointer items-center gap-2 px-2.5 py-1 text-[12px] text-zinc-300 hover:bg-zinc-800/40"
                      >
                        <input
                          type="checkbox"
                          className="accent-sky-400"
                          checked={selected.has(account.UserID)}
                          onChange={() => toggleAccount(account.UserID)}
                        />
                        <span className="min-w-0 flex-1 truncate">{labelOf(account, account.UserID)}</span>
                        {openClients.has(account.UserID) ? (
                          <span className="shrink-0 rounded bg-emerald-500/15 px-1.5 text-[10.5px] text-emerald-400">
                            client open
                          </span>
                        ) : (
                          <span className="shrink-0 text-[10.5px] text-zinc-600">no client</span>
                        )}
                      </label>
                    ))}
                  </div>
                );
              })
            )}
          </div>
        </div>

        <div className="min-w-0 space-y-3">
          <fieldset>
            <legend className="mb-1 text-[12px] font-medium text-zinc-200">Mode</legend>
            <div className="space-y-1">
              {MODES.map((mode) => (
                <label key={mode.id} className="flex cursor-pointer items-center gap-2 text-[12px] text-zinc-300">
                  <input
                    type="radio"
                    name={`python-mode-${script}`}
                    className="accent-sky-400"
                    checked={config.mode === mode.id}
                    onChange={() => update({ mode: mode.id })}
                  />
                  {mode.label}
                </label>
              ))}
            </div>
            <p className="mt-1 text-[11px] text-zinc-500">{modeHint}</p>
          </fieldset>

          <div>
            <div className="mb-1 text-[12px] font-medium text-zinc-200">Trigger</div>
            <div className="flex items-center gap-2">
              <Select
                className="min-w-0 flex-1"
                ariaLabel="Trigger"
                value={config.trigger.kind}
                options={TRIGGERS}
                onChange={(value) => setTriggerKind(value as TriggerKind)}
              />
              {config.trigger.kind === "interval" ? (
                <label className="flex shrink-0 items-center gap-1.5 text-[12px] text-zinc-400">
                  <input
                    type="number"
                    min={MIN_INTERVAL_MINUTES}
                    max={MAX_INTERVAL_MINUTES}
                    aria-label="Minutes between runs"
                    value={minutesDraft ?? config.trigger.minutes}
                    onChange={(e) => setMinutes(e.target.value)}
                    onBlur={() => setMinutesDraft(null)}
                    className="w-[64px] rounded-lg border border-zinc-700/60 bg-zinc-800/50 px-2 py-1 text-[12px] text-zinc-200 focus:border-zinc-500 focus:outline-none"
                  />
                  min
                </label>
              ) : null}
            </div>
            <p className="mt-1 text-[11px] text-zinc-500">{triggerHint}</p>
          </div>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2 border-t theme-border px-3 py-2">
        <button
          type="button"
          onClick={() => void start()}
          disabled={!canRun}
          className="theme-btn inline-flex items-center gap-1.5 px-3.5 py-1.5 text-[12.5px] font-medium disabled:cursor-not-allowed disabled:opacity-40"
        >
          <Play size={12} strokeWidth={2.2} />
          Run
        </button>
        {activeRuns.length > 0 ? (
          <button
            type="button"
            onClick={() => void stop()}
            className="inline-flex items-center gap-1.5 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-1.5 text-[12.5px] font-medium text-red-300 transition-colors hover:bg-red-500/20"
          >
            <Square size={11} strokeWidth={2.2} />
            {activeRuns.length > 1 ? `Stop (${activeRuns.length})` : "Stop"}
          </button>
        ) : null}
        <span className="text-[11.5px] text-zinc-500">
          {!runtimeInstalled
            ? "Install Python to run scripts."
            : selected.size === 0
              ? "Pick at least one account."
              : "Only accounts with a client opened by MultiAlt receive input."}
        </span>
      </div>
    </section>
  );
}
