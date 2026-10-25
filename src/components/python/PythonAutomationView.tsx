import { useCallback, useRef, useState, type KeyboardEvent } from "react";
import { Braces, Code2, LayoutDashboard, ScanSearch, ShieldAlert, Terminal, X, type LucideIcon } from "lucide-react";
import { useStore } from "../../store";
import { CodeTab, type OpenRequest } from "./CodeTab";
import { DashboardView } from "./DashboardView";
import { DeveloperPanel } from "./DeveloperPanel";
import { GlobalFunctionsTab } from "./GlobalFunctionsTab";
import { InspectorView } from "./InspectorView";
import { InstallCard } from "./InstallCard";
import { usePythonRuns } from "./usePythonRuns";
import { usePythonRuntime } from "./usePythonRuntime";

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

export const WARNING_DISMISSED_KEY = "ram_python_warning_dismissed";

export type PythonTabId = "code" | "globals" | "dashboard" | "inspector" | "developer";

const TABS: { id: PythonTabId; label: string; Icon: LucideIcon }[] = [
  { id: "code", label: "Code", Icon: Code2 },
  { id: "globals", label: "Global functions", Icon: Braces },
  { id: "dashboard", label: "Dashboard", Icon: LayoutDashboard },
  { id: "inspector", label: "Inspector", Icon: ScanSearch },
  { id: "developer", label: "Developer", Icon: Terminal },
];

function readWarningDismissed(): boolean {
  try {
    return window.localStorage.getItem(WARNING_DISMISSED_KEY) === "1";
  } catch {
    return false;
  }
}

/** The Roblox Player automation (Python) mode of the Scripts page. */
export function PythonAutomationView() {
  const store = useStore();
  const [tab, setTab] = useState<PythonTabId>("code");
  const [warningDismissed, setWarningDismissed] = useState(readWarningDismissed);
  const [openRequest, setOpenRequest] = useState<OpenRequest | null>(null);
  const tabRefs = useRef<Partial<Record<PythonTabId, HTMLButtonElement | null>>>({});

  const runtime = usePythonRuntime();
  const { runs } = usePythonRuns();

  const addToast = store.addToast;
  const toast = useCallback(
    (message: string, tone?: "info" | "success" | "warn" | "error") => addToast(message, tone),
    [addToast]
  );
  const developerMode = store.settings?.Python?.DeveloperMode === "true";

  function dismissWarning() {
    setWarningDismissed(true);
    try {
      window.localStorage.setItem(WARNING_DISMISSED_KEY, "1");
    } catch {
      // Not remembered: it comes back next time, which is the safe side.
    }
  }

  const openInCode = useCallback((path: string) => {
    setOpenRequest((prev) => ({ path, seq: (prev?.seq ?? 0) + 1 }));
    setTab("code");
  }, []);

  function onTabKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const order = TABS.map((item) => item.id);
    const at = order.indexOf(tab);
    const next = order[(at + (event.key === "ArrowRight" ? 1 : order.length - 1)) % order.length];
    setTab(next);
    tabRefs.current[next]?.focus();
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
              onClick={() => setTab(id)}
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

      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-6 py-4">
        {!warningDismissed ? (
          <div
            role="note"
            data-testid="python-warning"
            className="mb-3 flex shrink-0 items-start gap-2 rounded-lg border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-[12px] text-amber-200"
          >
            <ShieldAlert size={14} strokeWidth={1.9} className="mt-0.5 shrink-0" />
            <span className="min-w-0 flex-1">Python scripts have full access to your PC. Only run scripts you trust.</span>
            <button
              type="button"
              aria-label="Dismiss the warning"
              title="Dismiss"
              onClick={dismissWarning}
              className="-mr-1 shrink-0 rounded p-0.5 text-amber-300/80 hover:bg-amber-500/20 hover:text-amber-100"
            >
              <X size={13} strokeWidth={2} />
            </button>
          </div>
        ) : null}

        {/* The Code tab stays mounted while another tab shows, so open files keep their unsaved edits. */}
        <div
          role="tabpanel"
          id="python-panel-code"
          aria-labelledby="python-tab-code"
          hidden={tab !== "code"}
          className="flex flex-1 flex-col gap-3"
        >
          <InstallCard
            status={runtime.status}
            progress={runtime.progress}
            error={runtime.error}
            installing={runtime.installing}
            onInstall={() => void runtime.install()}
            onReinstall={() => void runtime.reinstall()}
          />
          <div className="min-h-[560px] flex-1">
            <CodeTab
              visible={tab === "code"}
              openRequest={openRequest}
              runtimeInstalled={runtime.status?.installed === true}
              runs={runs}
              developerMode={developerMode}
              onOpenDeveloper={() => setTab("developer")}
              onToast={toast}
            />
          </div>
        </div>

        {tab !== "code" ? (
          <div
            role="tabpanel"
            id={`python-panel-${tab}`}
            aria-labelledby={`python-tab-${tab}`}
            className="min-h-0 flex-1 overflow-y-auto"
          >
            {tab === "globals" && (
              <GlobalFunctionsTab
                visible
                developerMode={developerMode}
                onOpenFile={openInCode}
                onOpenDeveloper={() => setTab("developer")}
                onToast={toast}
              />
            )}
            {tab === "dashboard" && <DashboardView />}
            {tab === "inspector" && <InspectorView />}
            {tab === "developer" && <DeveloperPanel />}
          </div>
        ) : null}
      </div>
    </div>
  );
}
