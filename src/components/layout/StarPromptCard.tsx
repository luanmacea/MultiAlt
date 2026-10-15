import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Star, X } from "lucide-react";
import { useTr } from "../../i18n/text";
import {
  finishStarPrompt,
  loadStarPrompt,
  localDay,
  recordUseDay,
  saveStarPrompt,
  shouldShowStarPrompt,
  snoozeStarPrompt,
  type StarPromptState,
} from "../../utils/starPrompt";

/** Abre o repositório no navegador (o endereço mora no backend, `open_repo_url`). */
export function openRepoForStar(): void {
  void invoke("open_repo_url").catch(() => {});
}

/**
 * Cartão no rodapé da barra lateral pedindo uma estrela no GitHub. Regra de
 * quando aparece em `utils/starPrompt.ts`; só na barra aberta (recolhida não
 * cabe — lá fica só o botão "Star on GitHub").
 */
export function StarPromptCard() {
  const t = useTr();
  const [state, setState] = useState<StarPromptState | null>(null);

  useEffect(() => {
    const loaded = loadStarPrompt();
    if (!loaded) return;
    const counted = recordUseDay(loaded, localDay(new Date()));
    if (counted !== loaded) saveStarPrompt(counted);
    setState(counted);
  }, []);

  if (!state || !shouldShowStarPrompt(state, Date.now())) return null;

  const update = (next: StarPromptState) => {
    saveStarPrompt(next);
    setState(next);
  };

  return (
    <div
      data-testid="star-prompt"
      role="note"
      className="relative mb-2 rounded-lg border theme-border bg-[var(--panel-soft)] p-2.5 pr-6"
    >
      <button
        type="button"
        onClick={() => update(finishStarPrompt(state))}
        aria-label={t("Don't show again")}
        title={t("Don't show again")}
        className="absolute right-1 top-1 w-5 h-5 flex items-center justify-center rounded theme-muted hover:text-[var(--panel-fg)]"
      >
        <X size={12} strokeWidth={2} />
      </button>
      <div className="flex items-center gap-1.5 text-[12px] font-medium text-[var(--panel-fg)]">
        <Star size={13} strokeWidth={2} className="text-amber-400 shrink-0" fill="currentColor" />
        {t("Enjoying MultiAlt?")}
      </div>
      <p className="mt-1 text-[11px] leading-snug theme-muted">
        {t("It's free and open source. A star on GitHub helps the project keep getting updates.")}
      </p>
      <div className="mt-2 flex flex-col gap-1">
        <button
          type="button"
          onClick={() => {
            openRepoForStar();
            update(finishStarPrompt(state));
          }}
          className="w-full whitespace-nowrap rounded-md bg-amber-500/90 hover:bg-amber-400 px-2 py-1 text-[11px] font-semibold text-black transition-colors"
        >
          {t("Give a star")}
        </button>
        <button
          type="button"
          onClick={() => update(snoozeStarPrompt(state, Date.now()))}
          className="w-full rounded-md px-2 py-0.5 text-[11px] theme-muted hover:text-[var(--panel-fg)]"
        >
          {t("Not now")}
        </button>
      </div>
    </div>
  );
}
