import { useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { PythonTerminal } from "./PythonTerminal";
import { RunningList } from "./RunningList";
import type { PythonRunInfo } from "./pythonApi";
import { isRunActive } from "./usePythonRuns";

export const PANEL_STATE_KEY = "ram_python_bottom_panel";
export const PANEL_MIN_HEIGHT = 160;
export const PANEL_DEFAULT_HEIGHT = 260;
const PANEL_MAX_HEIGHT = 640;
const KEY_STEP = 24;

interface PanelState {
  height: number;
  collapsed: boolean;
}

function clampHeight(height: number): number {
  return Math.min(PANEL_MAX_HEIGHT, Math.max(PANEL_MIN_HEIGHT, Math.round(height)));
}

function loadState(): PanelState {
  try {
    const raw = window.localStorage.getItem(PANEL_STATE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<PanelState>;
      return {
        height: typeof parsed.height === "number" ? clampHeight(parsed.height) : PANEL_DEFAULT_HEIGHT,
        collapsed: parsed.collapsed === true,
      };
    }
  } catch {
    // No storage: the defaults are fine.
  }
  return { height: PANEL_DEFAULT_HEIGHT, collapsed: false };
}

function saveState(state: PanelState): void {
  try {
    window.localStorage.setItem(PANEL_STATE_KEY, JSON.stringify(state));
  } catch {
    // Not remembered; the panel still works.
  }
}

type PanelTab = "terminal" | "running";

interface BottomPanelProps {
  runs: PythonRunInfo[];
  onToast: (message: string, tone?: "info" | "success" | "warn" | "error") => void;
}

/** The bottom panel of the Code tab (like the VS Code terminal): output and running scripts, collapsible and resizable. */
export function BottomPanel({ runs, onToast }: BottomPanelProps) {
  const [state, setState] = useState<PanelState>(loadState);
  const [tab, setTab] = useState<PanelTab>("terminal");
  const drag = useRef<{ startY: number; startHeight: number } | null>(null);
  const activeCount = runs.filter(isRunActive).length;

  function update(next: PanelState) {
    setState(next);
    saveState(next);
  }

  function onPointerDown(event: PointerEvent<HTMLDivElement>) {
    if (state.collapsed) return;
    drag.current = { startY: event.clientY, startHeight: state.height };
    event.currentTarget.setPointerCapture?.(event.pointerId);
  }

  function onPointerMove(event: PointerEvent<HTMLDivElement>) {
    if (!drag.current) return;
    // Dragging up makes the panel taller.
    setState({ ...state, height: clampHeight(drag.current.startHeight + (drag.current.startY - event.clientY)) });
  }

  function onPointerUp() {
    if (!drag.current) return;
    drag.current = null;
    saveState(state);
  }

  function onHandleKey(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "ArrowUp") {
      event.preventDefault();
      update({ ...state, height: clampHeight(state.height + KEY_STEP) });
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      update({ ...state, height: clampHeight(state.height - KEY_STEP) });
    }
  }

  function selectTab(next: PanelTab) {
    setTab(next);
    if (state.collapsed) update({ ...state, collapsed: false });
  }

  const tabClass = (on: boolean) =>
    `-mb-px border-b-2 px-3 py-1.5 text-[12px] transition-colors ${
      on ? "border-sky-400 text-zinc-100" : "border-transparent text-zinc-500 hover:text-zinc-300"
    }`;

  return (
    <section aria-label="Panel" className="flex shrink-0 flex-col border-t theme-border bg-[var(--panel-soft)]">
      {!state.collapsed ? (
        <div
          role="separator"
          aria-orientation="horizontal"
          aria-label="Resize the panel"
          aria-valuemin={PANEL_MIN_HEIGHT}
          aria-valuemax={PANEL_MAX_HEIGHT}
          aria-valuenow={state.height}
          tabIndex={0}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onKeyDown={onHandleKey}
          className="-mt-0.5 h-1 shrink-0 cursor-row-resize touch-none bg-transparent hover:bg-sky-400/50 focus-visible:bg-sky-400/60 focus-visible:outline-none"
        />
      ) : null}
      <div role="tablist" aria-label="Panel" className="flex shrink-0 items-center gap-1 border-b theme-border px-2">
        <button type="button" role="tab" aria-selected={tab === "terminal"} onClick={() => selectTab("terminal")} className={tabClass(tab === "terminal")}>
          Terminal
        </button>
        <button type="button" role="tab" aria-selected={tab === "running"} onClick={() => selectTab("running")} className={tabClass(tab === "running")}>
          Running
          {activeCount > 0 ? (
            <span className="ml-1.5 rounded-full bg-emerald-500/20 px-1.5 text-[11px] text-emerald-400">{activeCount}</span>
          ) : null}
        </button>
        <button
          type="button"
          aria-label={state.collapsed ? "Expand the panel" : "Collapse the panel"}
          title={state.collapsed ? "Expand the panel" : "Collapse the panel"}
          onClick={() => update({ ...state, collapsed: !state.collapsed })}
          className="ml-auto rounded p-1 text-zinc-400 hover:bg-zinc-700/60 hover:text-zinc-100"
        >
          {state.collapsed ? <ChevronUp size={13} strokeWidth={2} /> : <ChevronDown size={13} strokeWidth={2} />}
        </button>
      </div>
      {/* Always mounted: closing the panel or looking at Running must not drop the terminal. */}
      <div className="min-h-0 overflow-hidden" style={{ height: state.height }} hidden={state.collapsed} data-testid="panel-body">
        <div className="h-full" hidden={tab !== "terminal"}>
          <PythonTerminal runs={runs} className="h-full rounded-none border-0 bg-transparent" />
        </div>
        <div className="h-full overflow-y-auto p-2" hidden={tab !== "running"}>
          <RunningList runs={runs} onToast={onToast} />
        </div>
      </div>
    </section>
  );
}
