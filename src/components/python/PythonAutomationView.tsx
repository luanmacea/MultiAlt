import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Code2, LayoutDashboard, ScanSearch, ShieldAlert, Terminal, type LucideIcon } from "lucide-react";
import { useStore } from "../../store";
import { useConfirm } from "../../hooks/usePrompt";
import { DashboardView } from "./DashboardView";
import { DeveloperPanel } from "./DeveloperPanel";
import { InspectorView } from "./InspectorView";
import { InstallCard } from "./InstallCard";
import { PythonTerminal } from "./PythonTerminal";
import { RunPanel } from "./RunPanel";
import { RunningList } from "./RunningList";
import { ScriptEditor } from "./ScriptEditor";
import { ScriptList } from "./ScriptList";
import { usePythonRuns } from "./usePythonRuns";
import { usePythonRuntime } from "./usePythonRuntime";
import { usePythonScripts } from "./usePythonScripts";

export const PYTHON_PAGE_INTRO = (
  <>
    <span className="block text-[var(--panel-fg)] opacity-80">
      Python automation for the Roblox windows of your accounts
    </span>
    <span className="block mt-0.5">
      Scripts read pixels, find images and send keys and clicks to one window at a time. They do not inject code and do
      not read the game&apos;s memory.
    </span>
  </>
);

export type PythonTabId = "scripts" | "dashboard" | "inspector" | "developer";

const TABS: { id: PythonTabId; label: string; Icon: LucideIcon }[] = [
  { id: "scripts", label: "Scripts", Icon: Code2 },
  { id: "dashboard", label: "Dashboard", Icon: LayoutDashboard },
  { id: "inspector", label: "Inspector", Icon: ScanSearch },
  { id: "developer", label: "Developer", Icon: Terminal },
];

/** The Roblox Player automation (Python) mode of the Scripts page. */
export function PythonAutomationView() {
  const store = useStore();
  const [tab, setTab] = useState<PythonTabId>("scripts");
  const tabRefs = useRef<Partial<Record<PythonTabId, HTMLButtonElement | null>>>({});

  const runtime = usePythonRuntime();
  const { scripts, loaded, refresh: refreshScripts } = usePythonScripts();
  const { runs } = usePythonRuns();
  const [selected, setSelected] = useState<string | null>(null);
  // The editor has edits that are not saved yet: switching away asks first.
  const [editorDirty, setEditorDirty] = useState(false);
  const confirm = useConfirm();

  const addToast = store.addToast;
  const toast = (message: string, tone?: "info" | "success" | "warn" | "error") => addToast(message, tone);

  // Keep the selection valid: pick the first script on load, drop a deleted one.
  useEffect(() => {
    if (!loaded) return;
    if (selected && scripts.some((script) => script.name === selected)) return;
    setSelected(scripts[0]?.name ?? null);
  }, [loaded, scripts, selected]);

  const selectedFile = scripts.find((script) => script.name === selected) ?? null;

  /** True when there is nothing unsaved, or the user agreed to drop it. */
  async function mayLeaveEditor(next: string): Promise<boolean> {
    if (!editorDirty || !selected) return true;
    return confirm(`${selected} has unsaved changes. Discard them and open ${next}?`, true, {
      confirmLabel: "Discard changes",
    });
  }

  async function selectScript(name: string | null) {
    if (name === selected) return;
    // A deleted script has nothing left to keep.
    if (name !== null && !(await mayLeaveEditor(name))) return;
    setEditorDirty(false);
    setSelected(name);
  }

  async function selectTab(next: PythonTabId) {
    if (next === tab) return;
    if (tab === "scripts" && !(await mayLeaveEditor(`the ${next} tab`))) return;
    setTab(next);
  }

  function onTabKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const order = TABS.map((item) => item.id);
    const at = order.indexOf(tab);
    const next = order[(at + (event.key === "ArrowRight" ? 1 : order.length - 1)) % order.length];
    void selectTab(next).then(() => tabRefs.current[next]?.focus());
  }

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div role="tablist" aria-label="Python automation" className="mx-6 mt-2 flex shrink-0 gap-1 border-b theme-border">
        {TABS.map(({ id, label, Icon }) => {
          const on = id === tab;
          return (
            <button
              key={id}
              ref={(el) => {
                tabRefs.current[id] = el;
              }}
              type="button"
              role="tab"
              id={`python-tab-${id}`}
              aria-selected={on}
              aria-controls={`python-panel-${id}`}
              tabIndex={on ? 0 : -1}
              onClick={() => void selectTab(id)}
              onKeyDown={onTabKeyDown}
              className={`-mb-px inline-flex items-center gap-1.5 border-b-2 px-3 py-2 text-[12.5px] transition-colors ${
                on ? "border-sky-400 text-zinc-100" : "border-transparent text-zinc-500 hover:text-zinc-300"
              }`}
            >
              <Icon size={13} strokeWidth={1.9} className={on ? "text-sky-300" : ""} />
              {label}
            </button>
          );
        })}
      </div>

      <div
        role="tabpanel"
        id={`python-panel-${tab}`}
        aria-labelledby={`python-tab-${tab}`}
        className="min-h-0 flex-1 overflow-y-auto px-6 py-4"
      >
        <div
          role="note"
          className="mb-4 flex items-start gap-2 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-[12px] text-red-300"
        >
          <ShieldAlert size={14} strokeWidth={1.9} className="mt-0.5 shrink-0" />
          <span>Python scripts have full access to your PC. Only run scripts you trust.</span>
        </div>

        {tab === "scripts" && (
          <div className="space-y-4">
            <InstallCard
              status={runtime.status}
              progress={runtime.progress}
              error={runtime.error}
              installing={runtime.installing}
              onInstall={() => void runtime.install()}
              onReinstall={() => void runtime.reinstall()}
            />

            <div className="grid h-[400px] gap-4 md:grid-cols-[260px_minmax(0,1fr)]">
              <ScriptList
                scripts={scripts}
                loaded={loaded}
                selected={selected}
                onSelect={(name) => void selectScript(name)}
                onChanged={refreshScripts}
                onToast={toast}
              />
              {selectedFile ? (
                <ScriptEditor
                  key={selectedFile.name}
                  name={selectedFile.name}
                  modifiedMs={selectedFile.modifiedMs}
                  onDirtyChange={setEditorDirty}
                />
              ) : (
                <div className="flex items-center justify-center rounded-lg border border-dashed theme-border px-6 text-center text-[12.5px] text-zinc-500">
                  Select a script to edit it, or create one with New.
                </div>
              )}
            </div>

            {selectedFile ? (
              <RunPanel
                script={selectedFile.name}
                runtimeInstalled={runtime.status?.installed === true}
                runs={runs}
                onToast={toast}
              />
            ) : null}

            <RunningList runs={runs} onToast={toast} />

            <PythonTerminal runs={runs} className="h-[320px]" />
          </div>
        )}

        {tab === "dashboard" && <DashboardView />}

        {tab === "inspector" && <InspectorView />}

        {tab === "developer" && <DeveloperPanel />}
      </div>
    </div>
  );
}
