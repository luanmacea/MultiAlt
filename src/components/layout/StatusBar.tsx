import { useEffect, useState } from "react";
import { useStore } from "../../store";
import { AGED_COLOR_FROM, AGED_COLOR_TO } from "../../types";
import { Tooltip } from "../ui/Tooltip";
import { useTr } from "../../i18n/text";
import { TONE_STYLES } from "../../utils/toastTone";
import { StatusBadge } from "../accounts/StatusBadge";

/**
 * O chip do rodapé cobre as duas formas de criar conta, e o destino do clique
 * já depende disso (`provider` vazio = criação gratuita no formulário do
 * Roblox). O texto era único e dizia que estava gastando saldo mesmo quando o
 * que rodava era a gratuita.
 */
export function generatorChipTooltip(provider: string, t: (key: string) => string): string {
  return provider
    ? t("The account generator is still running and spending your provider balance. Click to open it and stop.")
    : t("Accounts are still being created in the browser, at no cost. Click to open it and stop.");
}

export function StatusBar() {
  const t = useTr();
  const store = useStore();
  const [tickNow, setTickNow] = useState(Date.now());
  const selected = store.selectedIds.size;
  const total = store.accounts.length;
  const filtered = store.searchQuery ? store.groups.reduce((n, g) => n + g.accounts.length, 0) : total;
  const showPresence = store.settings?.General?.ShowPresence === "true";
  /**
   * Contadores mutuamente exclusivos: cada conta entra em um estado só, o mesmo
   * que `AccountRow` pinta na lista (1 = online, 3 = Studio, resto = em jogo).
   * Antes "online" contava >= 1 e "in game" contava >= 2, então quem estava
   * jogando aparecia nos dois e quem estava no Studio virava "in game".
   */
  const countPresence = (match: (presence: number) => boolean) =>
    showPresence
      ? store.accounts.filter((a) => match(store.presenceByUserId.get(a.UserID) ?? 0)).length
      : 0;
  const onlineCount = countPresence((p) => p === 1);
  const inGameCount = countPresence((p) => p >= 2 && p !== 3);
  const studioCount = countPresence((p) => p === 3);
  const launchedCount = store.launchedByProgram.size;
  const bottingActive = store.bottingStatus?.active === true;
  const nextRestartMs = bottingActive
    ? store.bottingStatus?.accounts
        ?.map((a) => a.nextRestartAtMs)
        .filter((v): v is number => typeof v === "number")
        .sort((a, b) => a - b)[0] ?? null
    : null;
  /** "m:ss" que falta até `atMs`, ou "-" quando nada está agendado. */
  const countdownLabel = (atMs: number | null) => {
    if (!atMs) return "-";
    const seconds = Math.max(0, Math.ceil((atMs - tickNow) / 1000));
    return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
  };
  const bottingLabel = countdownLabel(nextRestartMs);
  const actionStatus = store.actionStatus;

  /**
   * O gerador compra contas num serviço pago e continua rodando depois que o
   * diálogo fecha — sem este indicador ele gastava saldo sem aparecer em lugar
   * nenhum. Clicar traz o diálogo de volta (é onde se para o laço).
   */
  const generator = store.generatorStatus;
  const generatorActive = generator?.active === true;
  const generatedLabel = generator
    ? generator.maxAccounts > 0
      ? `${generator.totalGenerated}/${generator.maxAccounts}`
      : `${generator.totalGenerated}`
    : "";

  useEffect(() => {
    if (!bottingActive && !generatorActive) return;
    const timer = window.setInterval(() => setTickNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [bottingActive, generatorActive]);

  return (
    <div data-tour="status-bar" className="theme-surface theme-border flex items-center justify-between gap-3 px-4 py-2 border-t text-[12px] shrink-0">
      <div className="flex items-center min-w-0 overflow-hidden pr-1">
        <div
          className={`overflow-hidden transition-[max-width,opacity,margin] duration-150 ease-out ${
            selected > 0 ? "max-w-[120px] opacity-100 mr-4" : "max-w-0 opacity-0 mr-0"
          }`}
        >
            <span className="theme-accent flex items-center gap-1.5 shrink-0 whitespace-nowrap">
              <span className="w-2 h-2 rounded-full bg-[var(--accent-color)]" />
              <span>{selected}</span> {t("selected")}
            </span>
          </div>

        <div
          className={`flex items-center gap-4 min-w-0 transition-transform duration-150 ease-out ${
            selected > 0 ? "translate-x-0.5" : "translate-x-0"
          }`}
        >
          <span className="theme-muted shrink-0">
          {store.searchQuery ? (
            <>
              <span className="text-[var(--panel-fg)]">{filtered}</span> / {total} {t("accounts")}
            </>
          ) : (
            <>
              <span className="text-[var(--panel-fg)]">{total}</span> {t(total !== 1 ? "accounts" : "account")}
            </>
          )}
          </span>
          {showPresence && (
            <span className="theme-muted inline-flex items-center gap-3 shrink-0">
              <span className="inline-flex items-center gap-1">
                <span className="w-2 h-2 rounded-full bg-sky-500/80 animate-pulse" />
                <span className="text-sky-400/90">{onlineCount}</span> {t("online")}
              </span>
              <span className="inline-flex items-center gap-1">
                <span className="w-2 h-2 rounded-full bg-emerald-500/80 animate-pulse" />
                <span className="text-emerald-400/90">{inGameCount}</span> {t("in game")}
              </span>
              <span className="inline-flex items-center gap-1">
                <span className="w-2 h-2 rounded-full bg-violet-500/80 animate-pulse" />
                <span className="text-violet-300/90">{studioCount}</span> {t("studio")}
              </span>
            </span>
          )}
          {launchedCount > 0 && (
            <span className="theme-muted inline-flex items-center gap-1 shrink-0">
              <span className="w-2 h-2 rounded-full bg-amber-500/90" />
              <span className="text-amber-300/90">{launchedCount}</span> {t("launched")}
            </span>
          )}
          {bottingActive && (
            <span className="theme-muted inline-flex items-center gap-1 shrink-0">
              <span className="w-2 h-2 rounded-full bg-fuchsia-400 animate-pulse" />
              <span className="text-fuchsia-300/90">{t("auto rejoin")}</span>
              <span className="text-fuchsia-200/90">{t("next")} {bottingLabel}</span>
            </span>
          )}
          {generatorActive && generator && (
            <Tooltip content={generatorChipTooltip(generator.provider, t)}>
              <button
                type="button"
                onClick={() => store.openGeneratorDialog(generator.provider ? "provider" : "signup")}
                className="theme-muted inline-flex items-center gap-1 shrink-0 hover:text-[var(--panel-fg)] transition-colors"
              >
                <span className="w-2 h-2 rounded-full bg-lime-400 animate-pulse" />
                <span className="text-lime-300/90">{t("generating")}</span>
                <span className="text-lime-200/90">{generatedLabel}</span>
                <span className="text-lime-200/90">{t("next")} {countdownLabel(generator.nextAttemptAtMs)}</span>
              </button>
            </Tooltip>
          )}
        </div>

        {/* `actionStatus` é o "está acontecendo agora" da store: progresso de
            download, conta N de M, Settings saved. Dezesseis chamadas escreviam
            nele e nenhum componente lia — a mensagem nunca chegava à tela.
            A bolinha usa a cor do tom (a mesma paleta do Console de launch), e
            o texto trunca porque uma frase de erro longa não pode empurrar os
            contadores para fora da barra. */}
        <div
          className={`min-w-0 overflow-hidden transition-[max-width,opacity,margin] duration-150 ease-out ${
            actionStatus ? "max-w-[420px] opacity-100 ml-4" : "max-w-0 opacity-0 ml-0"
          }`}
        >
          {actionStatus && (
            <span
              data-testid="action-status"
              title={actionStatus.message}
              className={`flex items-center gap-1.5 min-w-0 ${TONE_STYLES[actionStatus.tone].text}`}
            >
              <span className={`w-2 h-2 rounded-full shrink-0 ${TONE_STYLES[actionStatus.tone].dot}`} />
              <span className="truncate">{actionStatus.message}</span>
            </span>
          )}
        </div>
      </div>
      {/* Cor e ícone em cada estado: quem não distingue as cores reconhece pela
          forma (StatusBadge, o mesmo selo das linhas de conta). */}
      <div data-testid="status-legend" className="theme-muted flex items-center gap-3 shrink-0 text-[12px]">
        <span className="shrink-0 font-medium">{t("Legend:")}</span>
        <span className="inline-flex items-center gap-1 shrink-0">
          <StatusBadge kind="invalid" label={t("invalid")} decorative />
          {t("invalid")}
        </span>
        {/* O fundo e um degrade porque a cor real caminha de ambar a laranja com a idade. */}
        <Tooltip content={t("No use recorded for 20 days or more — the dot deepens toward orange as it ages.")}>
          <span className="inline-flex items-center gap-1 shrink-0">
            <StatusBadge
              kind="aged"
              label={t("idle 20d+")}
              decorative
              background={`linear-gradient(135deg, ${AGED_COLOR_FROM}, ${AGED_COLOR_TO})`}
            />
            {t("idle 20d+")}
          </span>
        </Tooltip>
        <span className="inline-flex items-center gap-1 shrink-0">
          <StatusBadge kind="launched" label={t("launched")} decorative />
          {t("launched")}
        </span>
        <span className="inline-flex items-center gap-1 shrink-0">
          <StatusBadge kind="online" label={t("online")} decorative />
          {t("online")}
        </span>
        <span className="inline-flex items-center gap-1 shrink-0">
          <StatusBadge kind="ingame" label={t("in game")} decorative />
          {t("in game")}
        </span>
        <span className="inline-flex items-center gap-1 shrink-0">
          <StatusBadge kind="studio" label={t("studio")} decorative />
          {t("studio")}
        </span>
      </div>
    </div>
  );
}
