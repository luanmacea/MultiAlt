import { useEffect, useState } from "react";
import { useStore } from "../../store";
import { AGED_COLOR_FROM, AGED_COLOR_TO } from "../../types";
import { Tooltip } from "../ui/Tooltip";
import { useTr } from "../../i18n/text";

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
  const bottingCountdown = nextRestartMs
    ? Math.max(0, Math.ceil((nextRestartMs - tickNow) / 1000))
    : null;
  const bottingLabel =
    bottingCountdown === null
      ? "-"
      : `${Math.floor(bottingCountdown / 60)}:${String(bottingCountdown % 60).padStart(2, "0")}`;

  useEffect(() => {
    if (!bottingActive) return;
    const timer = window.setInterval(() => setTickNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [bottingActive]);

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
              <span className="text-fuchsia-300/90">{t("botting")}</span>
              <span className="text-fuchsia-200/90">{t("next")} {bottingLabel}</span>
            </span>
          )}
        </div>
      </div>
      <div className="theme-muted flex items-center gap-3 shrink-0 text-[12px]">
        <span className="shrink-0 font-medium">{t("Legend:")}</span>
        <span className="inline-flex items-center gap-1 shrink-0">
          <span className="w-2.5 h-2.5 rounded-full bg-red-500" style={{ boxShadow: "0 0 0 1px var(--app-bg)" }} />
          {t("invalid")}
        </span>
        {/* O ponto e um degrade porque a cor real caminha de ambar a laranja com a idade. */}
        <Tooltip content={t("No use recorded for 20 days or more — the dot deepens toward orange as it ages.")}>
          <span className="inline-flex items-center gap-1 shrink-0">
            <span
              className="w-2.5 h-2.5 rounded-full"
              style={{
                background: `linear-gradient(135deg, ${AGED_COLOR_FROM}, ${AGED_COLOR_TO})`,
                boxShadow: "0 0 0 1px var(--app-bg)",
              }}
            />
            {t("idle 20d+")}
          </span>
        </Tooltip>
        <span className="inline-flex items-center gap-1 shrink-0">
          <span className="w-2.5 h-2.5 rounded-full bg-amber-500" style={{ boxShadow: "0 0 0 1px var(--app-bg)" }} />
          {t("launched")}
        </span>
        <span className="inline-flex items-center gap-1 shrink-0">
          <span className="w-2.5 h-2.5 rounded-full bg-sky-500" style={{ boxShadow: "0 0 0 1px var(--app-bg)" }} />
          {t("online")}
        </span>
        <span className="inline-flex items-center gap-1 shrink-0">
          <span className="w-2.5 h-2.5 rounded-full bg-emerald-500" style={{ boxShadow: "0 0 0 1px var(--app-bg)" }} />
          {t("in game")}
        </span>
        <span className="inline-flex items-center gap-1 shrink-0">
          <span className="w-2.5 h-2.5 rounded-full bg-violet-500" style={{ boxShadow: "0 0 0 1px var(--app-bg)" }} />
          {t("studio")}
        </span>
      </div>
    </div>
  );
}
