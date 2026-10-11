import { ArrowRightToLine, Hourglass, RotateCw, WifiOff, type LucideIcon } from "lucide-react";
import { useTr } from "../../i18n/text";

/**
 * Faixa de números no topo da página Session (era o cartão "Right now" ao
 * lado): jogando, reconectando, não responde, caiu (só quando há) e entrando.
 * Sai dos mesmos dados das linhas do "Em jogo" — o cartão antigo contava "em
 * jogo" pela presença, e com a presença desligada dizia "–" ao lado de "8
 * rodando". Cada número é um filtro da lista; "entrando" leva até a fila.
 */

export type SessionFilter = "playing" | "reconnecting" | "notResponding" | "dropped";

export interface SessionCounts {
  playing: number;
  reconnecting: number;
  notResponding: number;
  dropped: number;
  joining: number;
}

const STATS: { key: SessionFilter | "joining"; label: string; icon: LucideIcon | null; tone: string }[] = [
  { key: "playing", label: "Playing", icon: null, tone: "text-emerald-400" },
  { key: "reconnecting", label: "Reconnecting", icon: RotateCw, tone: "text-amber-400" },
  { key: "notResponding", label: "Not responding", icon: Hourglass, tone: "text-amber-400" },
  { key: "dropped", label: "Dropped", icon: WifiOff, tone: "text-red-400" },
  { key: "joining", label: "Joining", icon: ArrowRightToLine, tone: "text-[var(--accent-color)]" },
];

export function SessionStats({
  counts,
  filter,
  onFilter,
  onJoining,
}: {
  counts: SessionCounts;
  filter: SessionFilter | null;
  onFilter: (filter: SessionFilter | null) => void;
  onJoining: () => void;
}) {
  const t = useTr();
  return (
    <div
      role="group"
      aria-label={t("Right now")}
      data-tour="session-stats"
      className="flex flex-wrap items-stretch gap-1.5"
    >
      {STATS.map(({ key, label, icon: Icon, tone }) => {
        const count = counts[key];
        // "Caiu" só aparece quando há: é o caso raro, e zero não diz nada.
        if (key === "dropped" && count === 0 && filter !== "dropped") return null;
        const pressed = key !== "joining" && filter === key;
        const disabled = count === 0 && !pressed;
        return (
          <button
            key={key}
            type="button"
            aria-pressed={key === "joining" ? undefined : pressed}
            disabled={disabled}
            onClick={() => (key === "joining" ? onJoining() : onFilter(pressed ? null : key))}
            className={`inline-flex items-center gap-1.5 min-h-[32px] rounded-lg border px-3 py-1 text-[12px] transition-colors disabled:cursor-default ${
              pressed
                ? "border-[var(--accent-color)] bg-[var(--accent-soft)] text-[var(--panel-fg)]"
                : "theme-border bg-[var(--panel-soft)] text-[var(--panel-fg)] hover:bg-[var(--row-hover)] disabled:hover:bg-[var(--panel-soft)]"
            }`}
          >
            <span className={`text-[15px] font-semibold tabular-nums ${count > 0 ? tone : "theme-muted"}`}>{count}</span>
            {Icon ? (
              <Icon size={12} strokeWidth={1.75} aria-hidden="true" className={count > 0 ? tone : "theme-muted"} />
            ) : (
              <span aria-hidden="true" className={`w-1.5 h-1.5 rounded-full ${count > 0 ? "bg-emerald-400" : "bg-[var(--border-color)]"}`} />
            )}
            <span className={count > 0 ? undefined : "theme-muted"}>{t(label)}</span>
          </button>
        );
      })}
    </div>
  );
}
