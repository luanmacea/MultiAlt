import { useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { ArrowDownToLine, Copy, Eraser, Search, SquareTerminal } from "lucide-react";
import { useStore } from "../../store";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { Select } from "../ui/Select";
import { copyText } from "./copyText";
import { pythonLogStore, type LogEntry, type PythonLogStore } from "./pythonLogStore";
import type { PythonLogLevel, PythonRunInfo } from "./pythonApi";

const LEVELS: { id: PythonLogLevel; label: string; tone: string }[] = [
  { id: "log", label: "log", tone: "text-zinc-300" },
  { id: "warn", label: "warn", tone: "text-amber-300" },
  { id: "error", label: "error", tone: "text-red-400" },
  { id: "system", label: "system", tone: "text-sky-300" },
  { id: "blocked", label: "blocked", tone: "text-amber-300" },
];

const LINE_TONE: Record<PythonLogLevel, string> = {
  log: "text-zinc-300",
  warn: "text-amber-300",
  error: "text-red-400",
  system: "text-sky-300/90",
  blocked: "text-amber-200",
};

/** How close to the bottom still counts as "following" the output. */
const FOLLOW_SLACK_PX = 24;

function formatTime(ts: number): string {
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return "--:--:--";
  return date.toLocaleTimeString([], { hour12: false });
}

interface PythonTerminalProps {
  /** Defaults to the app-wide store; tests pass their own. */
  store?: PythonLogStore;
  /** Known runs, to show the script name instead of the run id. */
  runs?: PythonRunInfo[];
  className?: string;
}

/**
 * The log terminal of the Python screen. Own component on purpose (it does not
 * reuse the app Console): monospace, follows the end of the output until the
 * user scrolls up, filters by run, account and level, search, copy and clear.
 * Lock errors (`blocked`) are highlighted with their code and hint.
 */
export function PythonTerminal({ store: logStore = pythonLogStore, runs = [], className = "" }: PythonTerminalProps) {
  const appStore = useStore();
  const labelOf = useAccountLabel();

  useEffect(() => logStore.connect(), [logStore]);
  const version = useSyncExternalStore(logStore.subscribe, logStore.getVersion);
  const lines = logStore.getLines();

  const [levels, setLevels] = useState<Set<PythonLogLevel>>(() => new Set(LEVELS.map((l) => l.id)));
  const [runFilter, setRunFilter] = useState("all");
  const [accountFilter, setAccountFilter] = useState("all");
  const [query, setQuery] = useState("");
  const [following, setFollowing] = useState(true);
  const [copied, setCopied] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  const runLabel = (runId: string | null): string => {
    if (!runId) return "app";
    const run = runs.find((item) => item.id === runId);
    return run ? `${run.script} (${runId})` : runId;
  };

  const accountLabelOf = (userId: number | null): string => {
    if (userId === null) return "";
    const account = appStore.accounts.find((item) => item.UserID === userId);
    return labelOf(account, `User ${userId}`);
  };

  const runOptions = useMemo(() => {
    const ids = new Set<string>();
    for (const line of lines) if (line.runId) ids.add(line.runId);
    for (const run of runs) ids.add(run.id);
    return [{ value: "all", label: "All runs" }, ...[...ids].map((id) => ({ value: id, label: runLabel(id) }))];
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lines, version, runs]);

  const accountOptions = useMemo(() => {
    const ids = new Set<number>();
    for (const line of lines) if (line.userId !== null) ids.add(line.userId);
    return [
      { value: "all", label: "All accounts" },
      ...[...ids].map((id) => ({ value: String(id), label: accountLabelOf(id) })),
    ];
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lines, version, appStore.accounts, labelOf]);

  const needle = query.trim().toLowerCase();
  const visible = useMemo(
    () =>
      lines.filter((line) => {
        if (!levels.has(line.level)) return false;
        if (runFilter !== "all" && line.runId !== runFilter) return false;
        if (accountFilter !== "all" && String(line.userId) !== accountFilter) return false;
        if (needle) {
          const haystack = `${line.message} ${line.code ?? ""} ${line.hint ?? ""}`.toLowerCase();
          if (!haystack.includes(needle)) return false;
        }
        return true;
      }),
    [lines, version, levels, runFilter, accountFilter, needle]
  );

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (el && following) el.scrollTop = el.scrollHeight;
  }, [visible, following]);

  function onScroll() {
    const el = scrollRef.current;
    if (!el) return;
    setFollowing(el.scrollHeight - el.scrollTop - el.clientHeight <= FOLLOW_SLACK_PX);
  }

  function jumpToLatest() {
    setFollowing(true);
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }

  function toggleLevel(level: PythonLogLevel) {
    setLevels((prev) => {
      const next = new Set(prev);
      if (next.has(level)) next.delete(level);
      else next.add(level);
      return next;
    });
  }

  function plainLine(line: LogEntry): string {
    const who = accountLabelOf(line.userId);
    const head = `[${formatTime(line.ts)}] [${runLabel(line.runId)}]${who ? ` [${who}]` : ""} ${line.level.toUpperCase()}`;
    const code = line.code ? ` (${line.code})` : "";
    const hint = line.hint ? `\n    Hint: ${line.hint}` : "";
    return `${head} ${line.message}${code}${hint}`;
  }

  async function copyVisible() {
    const ok = await copyText(visible.map(plainLine).join("\n"));
    setCopied(ok);
    if (ok) window.setTimeout(() => setCopied(false), 1500);
  }

  return (
    <section
      aria-label="Python terminal"
      className={`flex min-h-0 flex-col rounded-lg border theme-border bg-[var(--panel-soft)] ${className}`}
    >
      <div className="flex flex-wrap items-center gap-2 border-b theme-border px-3 py-2">
        <span className="inline-flex items-center gap-1.5 text-[12px] font-medium text-zinc-200">
          <SquareTerminal size={13} strokeWidth={1.9} className="text-sky-300" />
          Terminal
        </span>

        <div className="flex items-center gap-1" role="group" aria-label="Levels">
          {LEVELS.map((level) => {
            const on = levels.has(level.id);
            return (
              <button
                key={level.id}
                type="button"
                aria-pressed={on}
                onClick={() => toggleLevel(level.id)}
                className={`rounded-md border px-1.5 py-0.5 text-[11px] transition-colors ${
                  on
                    ? `border-zinc-600 bg-zinc-800/80 ${level.tone}`
                    : "border-zinc-800 bg-transparent text-zinc-600 line-through"
                }`}
              >
                {level.label}
              </button>
            );
          })}
        </div>

        <Select className="w-[170px]" ariaLabel="Filter by run" value={runFilter} options={runOptions} onChange={setRunFilter} />
        <Select
          className="w-[150px]"
          ariaLabel="Filter by account"
          value={accountFilter}
          options={accountOptions}
          onChange={setAccountFilter}
        />

        <div className="relative min-w-[120px] flex-1">
          <Search
            size={12}
            strokeWidth={1.9}
            className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-zinc-600"
          />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search the log"
            aria-label="Search the log"
            spellCheck={false}
            className="w-full rounded-lg border border-zinc-700/60 bg-zinc-800/50 py-1 pl-7 pr-2 text-[12px] text-zinc-200 placeholder:text-zinc-600 focus:border-zinc-500 focus:outline-none"
          />
        </div>

        <button
          type="button"
          onClick={() => void copyVisible()}
          className="theme-btn-ghost inline-flex items-center gap-1 px-2 py-1 text-[11.5px]"
        >
          <Copy size={11} strokeWidth={2} />
          {copied ? "Copied" : "Copy visible"}
        </button>
        <button
          type="button"
          onClick={() => logStore.clear()}
          className="theme-btn-ghost inline-flex items-center gap-1 px-2 py-1 text-[11.5px]"
        >
          <Eraser size={11} strokeWidth={2} />
          Clear
        </button>
      </div>

      <div className="relative min-h-0 flex-1">
        <div
          ref={scrollRef}
          onScroll={onScroll}
          role="log"
          aria-label="Python log lines"
          className="h-full min-h-[140px] overflow-y-auto px-3 py-2 font-mono text-[11.5px] leading-[1.55]"
        >
          {visible.length === 0 ? (
            <div className="py-6 text-center font-sans text-[12px] text-zinc-500">
              {lines.length === 0
                ? "Nothing logged yet. Output of your scripts, and what MultiAlt does for them, shows up here."
                : "No lines match the filters."}
            </div>
          ) : (
            visible.map((line) => (
              <TerminalLine key={line.id} line={line} runLabel={runLabel(line.runId)} who={accountLabelOf(line.userId)} />
            ))
          )}
        </div>
        {!following && (
          <button
            type="button"
            onClick={jumpToLatest}
            className="theme-btn absolute bottom-3 right-4 inline-flex items-center gap-1 px-2.5 py-1 text-[11.5px] shadow-lg"
          >
            <ArrowDownToLine size={12} strokeWidth={2} />
            Jump to latest
          </button>
        )}
      </div>

      <div className="border-t theme-border px-3 py-1 text-[11px] text-zinc-500">
        {visible.length === lines.length ? `${lines.length} lines` : `${visible.length} of ${lines.length} lines`}
      </div>
    </section>
  );
}

function TerminalLine({ line, runLabel, who }: { line: LogEntry; runLabel: string; who: string }) {
  const blocked = line.level === "blocked";
  return (
    <div
      data-level={line.level}
      className={`whitespace-pre-wrap break-words ${LINE_TONE[line.level]} ${
        blocked ? "my-0.5 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 py-1" : ""
      }`}
    >
      <span className="text-zinc-600">{formatTime(line.ts)}</span> <span className="text-zinc-500">{runLabel}</span>
      {who ? <span className="text-zinc-400"> {who}</span> : null}{" "}
      {blocked ? (
        <span className="rounded bg-amber-500/20 px-1 text-[11px] font-semibold uppercase tracking-wide text-amber-200">
          {line.code ?? "blocked"}
        </span>
      ) : null}{" "}
      <span>{line.message}</span>
      {blocked && line.hint ? <div className="mt-0.5 text-amber-100/80">Hint: {line.hint}</div> : null}
    </div>
  );
}
