import { useCallback, useEffect, useState, type ReactElement } from "react";
import { LayoutDashboard, Trash2 } from "lucide-react";
import { useStore } from "../../store";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { useConfirm } from "../../hooks/usePrompt";
import { PYTHON_EVENTS, errorText, pythonApi, type DashboardSection } from "./pythonApi";
import { useTauriEvent } from "./useTauriEvent";
import { BadgeWidget, ProgressWidget, StatWidget, TextWidget } from "./widgets/SimpleWidgets";
import { LineWidget, LogWidget } from "./widgets/SeriesWidgets";
import { AccountsWidget, TableWidget } from "./widgets/TableWidgets";
import { WidgetFrame } from "./widgets/WidgetFrame";
import { parseWidget, toText, type WidgetProps } from "./widgets/widgetUtils";

const RENDERERS: Record<string, (props: WidgetProps) => ReactElement> = {
  stat: StatWidget,
  progress: ProgressWidget,
  text: TextWidget,
  badge: BadgeWidget,
  table: TableWidget,
  accounts: AccountsWidget,
  line: LineWidget,
  log: LogWidget,
};

/** One widget. A type this version does not know gets a small notice, never a crash. */
export function WidgetView(props: WidgetProps) {
  const Renderer = RENDERERS[props.widget.type];
  if (Renderer) return <Renderer {...props} />;
  return (
    <WidgetFrame title={props.widget.title ?? props.widget.id} testId={`widget-${props.widget.id}`}>
      <div className="text-[12px] italic text-zinc-500">Unsupported widget{props.widget.type ? ` (${toText(props.widget.type)})` : ""}</div>
    </WidgetFrame>
  );
}

function formatUpdated(ms: number): string {
  if (!ms) return "";
  const date = new Date(ms);
  return Number.isNaN(date.getTime()) ? "" : `Updated ${date.toLocaleTimeString([], { hour12: false })}`;
}

export function DashboardSectionView({
  section,
  accountName,
  onClear,
}: {
  section: DashboardSection;
  accountName: (userId: number) => string;
  onClear: (script: string) => void;
}) {
  const widgets = section.layout.map(parseWidget).filter((w): w is NonNullable<typeof w> => w !== null);
  return (
    <section aria-label={`Dashboard of ${section.script}`} className="rounded-lg border theme-border">
      <header className="flex flex-wrap items-center gap-2 border-b theme-border px-3 py-2">
        <h3 className="font-mono text-[12.5px] font-medium text-zinc-100">{section.script}</h3>
        <span className="text-[11px] text-zinc-500">{formatUpdated(section.updatedMs)}</span>
        <button
          type="button"
          onClick={() => onClear(section.script)}
          className="theme-btn-ghost ml-auto inline-flex items-center gap-1 px-2 py-0.5 text-[11.5px]"
        >
          <Trash2 size={11} strokeWidth={2} />
          Clear
        </button>
      </header>
      {widgets.length === 0 ? (
        <div className="px-3 py-4 text-[12px] text-zinc-500">This script has not defined any widgets yet.</div>
      ) : (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(210px,1fr))] gap-3 p-3">
          {widgets.map((widget) => (
            <WidgetView
              key={widget.id}
              widget={widget}
              values={section.values}
              series={section.series}
              accountName={accountName}
            />
          ))}
        </div>
      )}
    </section>
  );
}

/** Dashboard tab: one section per script, drawn from what the scripts report. */
export function DashboardView() {
  const store = useStore();
  const labelOf = useAccountLabel();
  const confirm = useConfirm();
  const [sections, setSections] = useState<DashboardSection[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    pythonApi
      .getDashboard()
      .then((list) => {
        if (!cancelled && Array.isArray(list)) setSections(list);
      })
      .catch((e) => {
        if (!cancelled) setError(errorText(e));
      })
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useTauriEvent<DashboardSection>(PYTHON_EVENTS.dashboardUpdate, (next) => {
    setSections((prev) => {
      const at = prev.findIndex((item) => item.script === next.script);
      if (at < 0) return [...prev, next].sort((a, b) => a.script.localeCompare(b.script));
      const copy = [...prev];
      copy[at] = next;
      return copy;
    });
  });

  const accountName = useCallback(
    (userId: number) => {
      const account = store.accounts.find((item) => item.UserID === userId);
      return labelOf(account, `User ${userId}`);
    },
    [store.accounts, labelOf]
  );

  async function clear(script: string) {
    const ok = await confirm(`Clear the dashboard of ${script}? Its widgets and values are removed until the script reports again.`, true, {
      confirmLabel: "Clear",
    });
    if (!ok) return;
    try {
      await pythonApi.clearDashboard(script);
      setSections((prev) => prev.filter((item) => item.script !== script));
    } catch (e) {
      store.addToast(errorText(e), "error");
    }
  }

  if (!loaded) return <div className="text-[12px] text-zinc-500">Loading the dashboard...</div>;

  if (sections.length === 0) {
    return (
      <div className="rounded-lg border border-dashed theme-border px-6 py-9 text-center">
        <LayoutDashboard size={22} strokeWidth={1.6} className="mx-auto text-zinc-600" />
        <div className="mt-2 text-[13px] font-medium text-zinc-200">Nothing reported yet</div>
        <p className="mx-auto mt-1 max-w-[62ch] text-[12px] leading-relaxed text-zinc-500">
          A script draws its own dashboard. Call <code className="font-mono text-zinc-300">dashboard.farm_report()</code> at the
          start of a script to get a ready-made report with a table per account, the total loops and a loops-per-hour
          chart. See <code className="font-mono text-zinc-300">examples/farm_dashboard.py</code> in the workspace for a
          complete script, or create one from the Dashboard template.
        </p>
        {error ? <p className="mt-2 text-[12px] text-red-400">{error}</p> : null}
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {sections.map((section) => (
        <DashboardSectionView key={section.script} section={section} accountName={accountName} onClear={(s) => void clear(s)} />
      ))}
    </div>
  );
}
