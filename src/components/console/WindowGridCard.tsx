import { useEffect, useId, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, LayoutGrid } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { SessionCache } from "../../utils/sessionCache";
import { Toggle } from "../ui/Toggle";

type MonitorInfo = { index: number; width: number; height: number; primary: boolean };

/**
 * Últimos monitores detectados. A grade voltava com "No monitors detected"
 * piscando até a consulta responder; agora ela nasce com isto e a consulta só
 * confirma.
 */
const monitorsCache = new SessionCache<MonitorInfo[]>(1);

/** Para os testes: esquece os monitores da sessão. */
export function clearMonitorsCache(): void {
  monitorsCache.clear();
}

/** Monitores marcados: os salvos que ainda existem, ou todos. */
function monitorSelection(list: MonitorInfo[], saved: string | undefined): Set<number> {
  const savedIdx = (saved || "")
    .split(",")
    .map((s) => parseInt(s.trim()))
    .filter((n) => Number.isFinite(n));
  const kept = savedIdx.filter((i) => list.some((m) => m.index === i));
  return new Set(kept.length > 0 ? kept : list.map((m) => m.index));
}

function parseGap(raw: string | undefined): number {
  const n = parseInt(raw || "20");
  return Number.isFinite(n) ? Math.max(0, Math.min(200, n)) : 20;
}

/**
 * Grade de janelas, no topo da página Console (era a aba Windows da Choose
 * Game): os monitores da grade (`General.GridMonitors`, vazio = todos), o
 * espaço entre as janelas (`General.GridGap`), o botão que organiza as janelas
 * já abertas e a grade automática no launch (`General.AutoArrangeGrid`).
 *
 * Tudo é lido de `store.settings` e gravado por `store.updateSetting`, que
 * atualiza a store junto com o INI. Antes o componente guardava a seleção num
 * estado próprio, nascido da store, e gravava direto no backend — a store
 * ficava velha e, ao voltar à tela, o monitor desmarcado aparecia marcado.
 */
export function WindowGridCard() {
  const t = useTr();
  const store = useStore();
  const general = store.settings?.General;
  const [monitors, setMonitors] = useState<MonitorInfo[]>(() => monitorsCache.get("last") ?? []);
  const selected = monitorSelection(monitors, general?.GridMonitors);
  const gap = parseGap(general?.GridGap);
  // Texto cru enquanto digita, para o campo poder ficar vazio; vale no blur/Enter.
  const [gapText, setGapText] = useState(() => String(gap));
  const [busy, setBusy] = useState(false);
  const gapId = useId();
  // Ligada por padrão: só o "false" gravado a desliga.
  const autoArrange = general?.AutoArrangeGrid !== "false";

  useEffect(() => {
    setGapText(String(gap));
  }, [gap]);

  useEffect(() => {
    let cancelled = false;
    invoke<MonitorInfo[]>("list_display_monitors")
      .then((mons) => {
        if (cancelled || !Array.isArray(mons)) return;
        const list = mons.map((m) => ({ index: m.index, width: m.width, height: m.height, primary: !!m.primary }));
        monitorsCache.set("last", list);
        setMonitors(list);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  function toggleMonitor(index: number) {
    const next = new Set(selected);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    void store.updateSetting(
      "General",
      "GridMonitors",
      [...next].sort((a, b) => a - b).join(",")
    );
  }

  function commitGap() {
    const v = parseInt(gapText);
    if (!Number.isFinite(v)) {
      setGapText(String(gap));
      return;
    }
    const next = Math.max(0, Math.min(200, v));
    setGapText(String(next));
    if (next !== gap) void store.updateSetting("General", "GridGap", String(next));
  }

  async function arrange() {
    if (busy) return;
    setBusy(true);
    try {
      const res = await invoke<{ arranged: number; total: number }>("arrange_windows_grid", {
        monitorIndices: [...selected].sort((a, b) => a - b),
        gap,
      });
      const leftover = res.total - res.arranged;
      store.addToast(
        leftover > 0
          ? t("{{arranged}}/{{total}} windows arranged ({{leftover}} didn't fit)", {
              arranged: res.arranged,
              total: res.total,
              leftover,
            })
          : t("{{arranged}} window(s) arranged in grid", { arranged: res.arranged })
      );
    } catch (e) {
      store.addToast(t("Grid failed: {{error}}", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section
      data-testid="window-grid-card"
      data-tour="console-grid"
      aria-labelledby="window-grid-title"
      className="shrink-0 rounded-xl border theme-border bg-[var(--panel-soft)] px-4 py-3"
    >
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <div className="min-w-0 flex-1 basis-[220px]">
          <h2 id="window-grid-title" className="flex items-center gap-1.5 text-[13px] font-semibold text-[var(--panel-fg)]">
            <LayoutGrid size={14} strokeWidth={1.6} aria-hidden="true" />
            {t("Window grid")}
          </h2>
          <p className="text-[12px] theme-muted">{t("Tile all open Roblox windows across the selected monitors.")}</p>
        </div>
        <button
          onClick={() => void arrange()}
          disabled={busy || selected.size === 0}
          className="theme-btn shrink-0 inline-flex items-center gap-1.5 min-h-[28px] px-3 py-1.5 text-[12px] font-medium disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <LayoutGrid size={13} strokeWidth={1.6} aria-hidden="true" />
          {busy ? t("Arranging...") : t("Arrange in grid")}
        </button>
      </div>

      <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
        {monitors.length === 0 ? (
          <span className="text-[12px] theme-muted">{t("No monitors detected")}</span>
        ) : (
          monitors.map((m) => {
            const on = selected.has(m.index);
            return (
              <button
                key={m.index}
                type="button"
                aria-pressed={on}
                onClick={() => toggleMonitor(m.index)}
                className={`flex items-center gap-1.5 min-h-[28px] px-2 py-1 rounded-md border text-[12px] transition-colors ${
                  on
                    ? "border-[var(--accent-color)] text-[var(--panel-fg)] bg-[var(--accent-soft)]"
                    : "theme-border theme-muted hover:text-[var(--panel-fg)]"
                }`}
                title={`${m.width}×${m.height}`}
              >
                <span
                  aria-hidden="true"
                  className={`w-3 h-3 rounded-[3px] border flex items-center justify-center ${
                    on ? "border-[var(--accent-color)] bg-[var(--accent-color)]" : "theme-border"
                  }`}
                >
                  {on && <Check size={9} stroke="var(--forms-bg)" strokeWidth={3} />}
                </span>
                {t("Monitor {{n}}", { n: m.index })}
                {m.primary ? ` ${t("(main)")}` : ""}
                <span className="theme-muted tabular-nums">
                  {m.width}×{m.height}
                </span>
              </button>
            );
          })
        )}

        <div className="ml-auto flex items-center gap-1.5 text-[12px] theme-muted">
          <label htmlFor={gapId}>{t("Gap")}</label>
          <input
            id={gapId}
            type="number"
            min={0}
            max={200}
            value={gapText}
            onChange={(e) => setGapText(e.target.value)}
            onBlur={commitGap}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitGap();
            }}
            className="sidebar-input w-16 text-xs tabular-nums"
          />
          <span aria-hidden="true">px</span>
        </div>
      </div>

      <div className="mt-1 -mx-1">
        <Toggle
          checked={autoArrange}
          onChange={(next) => void store.updateSetting("General", "AutoArrangeGrid", String(next))}
          label="Arrange in grid on launch"
          description="Each new Roblox window takes the first free cell of the grid. Accounts with their own window size keep it."
        />
      </div>
    </section>
  );
}
