import { Keyboard } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { SessionPanel } from "../session/SessionPanel";
import { PageShell } from "./PageShell";

/**
 * Página Session: o Painel de Sessão (o mesmo da aba Console da Choose Game)
 * na coluna principal, e ao lado um resumo do que está rodando e de quem
 * mantém as contas no jogo (AFK Mode / Auto Rejoin) — a página tem largura
 * para isso, o modal de 560 px não tinha.
 */
export function SessionPage({ active, onLeave }: { active: boolean; onLeave: () => void }) {
  const t = useTr();
  const store = useStore();
  if (!active) return null;

  const clientsOpen = store.launchedByProgram.size;
  const joining = (store.launchQueue?.entries ?? []).filter(
    (entry) => entry.state === "queued" || entry.state === "launching"
  ).length;
  const showPresence = store.settings?.General?.ShowPresence === "true";
  // Mesma regra do StatusBar: 1 = online, 3 = Studio, o resto (>= 2) é jogo.
  const inGame = showPresence
    ? store.accounts.filter((a) => {
        const presence = store.presenceByUserId.get(a.UserID) ?? 0;
        return presence >= 2 && presence !== 3;
      }).length
    : null;

  const bottingOn = store.bottingStatus?.active === true;
  const afkOn = store.afkStatus?.active === true;
  const keepAliveText = bottingOn
    ? t("Auto Rejoin is running for {{count}} accounts.", { count: store.bottingStatus?.userIds.length ?? 0 })
    : afkOn
      ? t("AFK Mode is on for {{count}} accounts.", { count: store.afkStatus?.accounts.length ?? 0 })
      : t("Nothing is keeping accounts in game right now.");

  const stats: { label: string; value: number | null }[] = [
    { label: t("Clients open"), value: clientsOpen },
    { label: t("Joining"), value: joining },
    { label: t("In game"), value: inGame },
  ];

  return (
    <PageShell
      title={t("Session")}
      description={t("Follow what is running now: accounts joining, Make Friends in progress, and clients already in game.")}
      onLeave={onLeave}
      dataTour="session-page"
    >
      <div className="flex flex-col-reverse gap-6 lg:flex-row lg:items-start">
        <div className="min-w-0 flex-1 max-w-[1040px]">
          <SessionPanel />
        </div>

        <aside aria-label={t("Summary")} className="w-full lg:w-[260px] xl:w-[300px] shrink-0 space-y-3 lg:sticky lg:top-0">
          <section className="rounded-xl border theme-border bg-[var(--panel-soft)] px-4 py-3">
            <h2 className="text-[12px] font-semibold text-[var(--panel-fg)]">{t("Right now")}</h2>
            <dl className="mt-2 divide-y divide-[var(--border-color)]">
              {stats.map((stat) => (
                <div key={stat.label} className="flex items-baseline justify-between py-1.5">
                  <dt className="text-[12px] text-[var(--panel-muted)]">{stat.label}</dt>
                  <dd className="text-[15px] font-semibold tabular-nums text-[var(--panel-fg)]">
                    {stat.value === null ? "–" : stat.value}
                  </dd>
                </div>
              ))}
            </dl>
            {inGame === null ? (
              <p className="mt-1.5 text-[11.5px] leading-snug text-[var(--panel-muted)]">
                {t("Turn on presence in Settings › General to count accounts in game.")}
              </p>
            ) : null}
          </section>

          <section className="rounded-xl border theme-border px-4 py-3">
            <h2 className="flex items-center gap-2 text-[12px] font-semibold text-[var(--panel-fg)]">
              <span
                aria-hidden="true"
                className={`w-1.5 h-1.5 rounded-full ${bottingOn || afkOn ? "bg-emerald-400" : "bg-[var(--border-color)]"}`}
              />
              {t("Keep accounts in game")}
            </h2>
            <p className="mt-1.5 text-[12px] leading-snug text-[var(--panel-muted)]">{keepAliveText}</p>
            <button
              type="button"
              onClick={() => store.setActivePage("afk")}
              className="theme-btn mt-3 inline-flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium"
            >
              <Keyboard size={13} strokeWidth={1.8} aria-hidden="true" />
              {t("Open AFK Mode")}
            </button>
          </section>

          <p className="px-1 text-[11.5px] leading-snug text-[var(--panel-muted)]">
            {t("Closing MultiAlt leaves every Roblox client running.")}
          </p>
        </aside>
      </div>
    </PageShell>
  );
}
