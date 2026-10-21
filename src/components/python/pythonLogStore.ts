/**
 * Ring buffer behind the Python terminal. It lives outside React so lines keep
 * arriving while another tab (or the JavaScript mode) is showing, and it holds
 * the last 5000 lines like the backend does.
 *
 * This is NOT the app Console: nothing here is shared with `components/console`.
 */
import { listen } from "@tauri-apps/api/event";
import { PYTHON_EVENTS, pythonApi, type PythonLogLine } from "./pythonApi";

export const MAX_LOG_LINES = 5000;

/** A log line with a stable key for rendering. */
export interface LogEntry extends PythonLogLine {
  id: number;
}

/** How long a burst of lines is held before subscribers are told. */
const NOTIFY_DELAY_MS = 40;

function lineKey(line: PythonLogLine): string {
  return `${line.ts}|${line.runId ?? ""}|${line.userId ?? ""}|${line.level}|${line.message}`;
}

export interface PythonLogStore {
  append(line: PythonLogLine): void;
  appendMany(lines: PythonLogLine[]): void;
  /** Empties the terminal only; the backend keeps its own copy. */
  clear(): void;
  getLines(): LogEntry[];
  getVersion(): number;
  subscribe(listener: () => void): () => void;
  /** Loads the backlog and follows `python-log`. Reference counted. */
  connect(): () => void;
}

export function createPythonLogStore(): PythonLogStore {
  let lines: LogEntry[] = [];
  let nextId = 1;
  let version = 0;
  let clearedAt = 0;
  const listeners = new Set<() => void>();
  let notifyTimer: ReturnType<typeof setTimeout> | null = null;

  function notify() {
    version += 1;
    if (notifyTimer !== null) return;
    notifyTimer = setTimeout(() => {
      notifyTimer = null;
      for (const listener of [...listeners]) listener();
    }, NOTIFY_DELAY_MS);
  }

  function push(line: PythonLogLine) {
    lines.push({ ...line, id: nextId++ });
  }

  function trim() {
    if (lines.length > MAX_LOG_LINES) lines = lines.slice(lines.length - MAX_LOG_LINES);
  }

  const store: PythonLogStore = {
    append(line) {
      push(line);
      trim();
      notify();
    },
    appendMany(batch) {
      for (const line of batch) push(line);
      trim();
      notify();
    },
    clear() {
      clearedAt = Date.now();
      lines = [];
      notify();
    },
    getLines: () => lines,
    getVersion: () => version,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    connect: (() => {
      let refs = 0;
      let off: (() => void) | null = null;
      let cancelled = false;

      function start() {
        cancelled = false;
        const live: PythonLogLine[] = [];
        let backlogDone = false;
        listen<PythonLogLine>(PYTHON_EVENTS.log, (event) => {
          if (backlogDone) store.append(event.payload);
          else live.push(event.payload);
        })
          .then((fn) => {
            if (cancelled) fn();
            else off = fn;
          })
          .catch(() => {});

        void pythonApi
          .getLogs()
          .catch(() => [] as PythonLogLine[])
          .then((backlog) => {
            if (cancelled) return;
            const history = (Array.isArray(backlog) ? backlog : []).filter((line) => line.ts > clearedAt);
            // A line can be in both the backlog and the live buffer: keep one.
            const seen = new Set(history.slice(-300).map(lineKey));
            const fresh = live.filter((line) => !seen.has(lineKey(line)));
            lines = [];
            nextId = 1;
            for (const line of history) push(line);
            for (const line of fresh) push(line);
            trim();
            backlogDone = true;
            notify();
          });
      }

      return () => {
        refs += 1;
        if (refs === 1) start();
        return () => {
          refs -= 1;
          if (refs === 0) {
            cancelled = true;
            off?.();
            off = null;
          }
        };
      };
    })(),
  };
  return store;
}

/** The terminal's store. Tests build their own with `createPythonLogStore()`. */
export const pythonLogStore = createPythonLogStore();
