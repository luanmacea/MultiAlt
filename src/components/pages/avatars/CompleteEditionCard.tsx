import { useState } from "react";
import { Download, Loader2, Sparkles } from "lucide-react";
import { useStore } from "../../../store";
import { useTr } from "../../../i18n/text";

/**
 * Aba Distribuir na edição padrão: a distribuição em lote só vem na edição
 * completa (`ENABLE_AVATAR_BATCH` / feature Rust `avatar-batch`). O botão troca
 * o canal do updater para a completa e baixa o instalador dela pelo próprio
 * updater do app — progresso no diálogo de atualização, nada no navegador.
 */
export function CompleteEditionCard() {
  const t = useTr();
  const store = useStore();
  const [busy, setBusy] = useState(false);

  async function handleGet() {
    if (busy) return;
    setBusy(true);
    try {
      await store.switchToCompleteEdition();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="h-full flex items-center justify-center p-2">
      <section
        data-tour="avatars-edition"
        aria-labelledby="avatars-edition-title"
        className="theme-surface rounded-xl border theme-border w-full max-w-md p-5 space-y-3"
      >
        <div className="flex items-center gap-2">
          <Sparkles size={16} strokeWidth={1.75} className="theme-accent shrink-0" />
          <h3 id="avatars-edition-title" className="text-[13.5px] font-semibold text-[var(--panel-fg)]">
            {t("Distributing avatars is an extra")}
          </h3>
        </div>
        <p className="text-[12.5px] theme-muted leading-5">
          {t(
            "Giving saved avatars to many accounts at once picks up the free catalog items for each account automatically. Some security tools treat automatic purchases — even free ones — as risky, so this part is not included by default. You can add it by switching to the complete edition (it also includes Nexus and the Web Server)."
          )}
        </p>
        <button
          onClick={() => void handleGet()}
          disabled={busy}
          className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-[var(--button-radius)] border theme-accent-border theme-accent-bg text-[12.5px] font-semibold text-[var(--panel-fg)] transition-all hover:brightness-110 active:scale-[0.98] disabled:opacity-60 disabled:cursor-wait"
        >
          {busy ? (
            <Loader2 size={15} className="animate-spin" />
          ) : (
            <Download size={15} strokeWidth={1.75} className="theme-accent" />
          )}
          {t("Get the complete edition")}
        </button>
        <p className="text-[11px] theme-muted leading-4">
          {t(
            "The update downloads the complete installer from this project's GitHub releases. Your accounts and settings are kept."
          )}
        </p>
      </section>
    </div>
  );
}
