import { GraduationCap } from "lucide-react";
import { useTr } from "../../i18n/text";
import { startTour, useTourSeen } from "./tourState";
import type { TourId } from "./tours";

/**
 * Botão "Tutorial" do cabeçalho de cada tela. Discreto de propósito: o
 * tutorial é opcional e nunca abre sozinho. O pontinho de destaque só aparece
 * enquanto a pessoa nunca abriu o tutorial daquela tela.
 */
export function TourButton({ tour, className = "" }: { tour: TourId; className?: string }) {
  const t = useTr();
  const seen = useTourSeen(tour);
  const label = t("Tutorial");
  const hint = t("A short guide to this screen");

  return (
    <button
      type="button"
      onClick={() => startTour(tour)}
      title={hint}
      aria-label={seen ? label : `${label} (${t("new")})`}
      data-tour="tour-button"
      className={`theme-btn-ghost relative inline-flex items-center gap-1.5 shrink-0 rounded-lg border theme-border px-2.5 py-1.5 text-[12px] ${className}`}
    >
      <GraduationCap size={14} strokeWidth={1.75} aria-hidden="true" />
      <span>{label}</span>
      {!seen ? (
        <span
          data-testid="tour-new-dot"
          aria-hidden="true"
          className="absolute -top-0.5 -right-0.5 h-2 w-2 rounded-full bg-[var(--accent-color)] ring-2 ring-[var(--panel-bg)]"
        />
      ) : null}
    </button>
  );
}
