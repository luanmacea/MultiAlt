import { Lock, Undo2 } from "lucide-react";
import { useTr } from "../../i18n/text";
import type { ReconnectChoice } from "../../utils/autoReconnect";

/**
 * Reconexão automática de uma conta, compacta, na linha da lista "Em jogo"
 * (Painel de Sessão). Mostra o que vale de fato: a chave liga/desliga, "padrão"
 * quando a conta segue `General.AutoReconnect`, um botão para voltar ao padrão
 * quando a conta tem escolha própria, e um cadeado quando o AutoRelaunch do
 * Nexus a mantém ligada. O tooltip diz de onde vem o valor.
 */
export function ReconnectSwitch({
  name,
  choice,
  disabled = false,
  onChange,
}: {
  name: string;
  choice: ReconnectChoice;
  disabled?: boolean;
  /** `null` volta ao padrão de todas as contas. */
  onChange: (value: boolean | null) => void;
}) {
  const t = useTr();
  const { own, globalDefault, nexusForced, effective } = choice;
  const locked = nexusForced || disabled;
  const title = nexusForced
    ? t("Nexus AutoRelaunch is on for this account, so it reconnects even with this off.")
    : own === null
      ? globalDefault
        ? t("Following the default from Settings › General (on).")
        : t("Following the default from Settings › General (off).")
      : effective
        ? t("Reconnects if it drops (set for this account)")
        : t("Does not reconnect if it drops (set for this account)");
  const resetLabel = globalDefault
    ? t("Use the default from Settings › General (on)")
    : t("Use the default from Settings › General (off)");

  return (
    <span className="flex items-center gap-1 shrink-0" data-testid="reconnect-switch">
      <button
        type="button"
        role="switch"
        aria-checked={effective}
        aria-disabled={locked || undefined}
        aria-label={t("Auto-reconnect for {{name}}", { name })}
        title={title}
        onClick={() => {
          if (!locked) onChange(!effective);
        }}
        className={`relative w-7 h-4 rounded-full shrink-0 transition-colors ${
          effective ? "bg-[var(--toggle-on-bg)]" : "bg-[var(--toggle-off-bg)]"
        } ${locked ? "opacity-60 cursor-not-allowed" : "cursor-pointer"}`}
      >
        <span
          aria-hidden="true"
          className={`absolute top-[2px] w-3 h-3 rounded-full bg-[var(--toggle-knob-bg)] transition-all ${
            effective ? "left-[14px]" : "left-[2px]"
          }`}
        />
      </button>
      {nexusForced ? (
        <Lock size={11} strokeWidth={1.75} className="text-amber-400 shrink-0" aria-hidden="true" />
      ) : own === null ? (
        <span className="text-[11px] theme-muted" title={title}>
          {t("default")}
        </span>
      ) : (
        <button
          type="button"
          onClick={() => onChange(null)}
          disabled={disabled}
          aria-label={resetLabel}
          title={resetLabel}
          className="p-0.5 rounded theme-muted hover:text-[var(--panel-fg)] disabled:opacity-40"
        >
          <Undo2 size={11} strokeWidth={1.75} aria-hidden="true" />
        </button>
      )}
    </span>
  );
}
