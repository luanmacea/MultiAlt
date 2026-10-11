import { Keyboard } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { SessionPanel } from "../session/SessionPanel";
import { PageShell } from "./PageShell";
import { Toggle } from "../ui/Toggle";
import { isWindowsPlatform } from "../../utils/platform";
import { MemoryDefaultSelect } from "../session/MemoryLimitControl";
import { MEMORY_LIMIT_SETTING, memoryLimitChoice } from "../../utils/memoryLimit";
import { RecordingsSummaryCard } from "../afk-mode/recordings/RecordingsSummaryCard";

/**
 * Página Session, o centro de quem joga com muitas contas: primeiro, em
 * qualquer largura, a faixa de números e as listas da sessão (Entrando, Em
 * jogo, agrupado por jogo e servidor); depois os cartões de ajuste — quem
 * mantém as contas no jogo, gravações e teto de memória. Abaixo de 1280 px os
 * cartões vão para baixo das listas (em duas colunas), para a lista ter a
 * largura; acima, ficam numa coluna ao lado. A página rola inteira: as listas
 * não têm rolagem própria aqui (`contained={false}`).
 *
 * Janela grande (monitor 4K, pedido de 11/10/2026): as listas não têm teto de
 * largura — ocupam o que sobra, e as linhas do "Em jogo" viram colunas pela
 * largura da lista (SessionPanel, `@container/ingame`). A coluna de ajustes
 * alarga com a janela e, a partir de 2000 px (125rem), vira duas colunas de cartões.
 * Breakpoint em **rem**, não px: o Tailwind 4 não ordena `min-[2000px]` junto
 * dos `xl:`/`2xl:` (em rem) — a regra saía antes no CSS e o `xl:` vencia.
 */
export function SessionPage({ active, onLeave }: { active: boolean; onLeave: () => void }) {
  const t = useTr();
  const store = useStore();
  if (!active) return null;

  const isWindows = isWindowsPlatform(store.platformCapabilities);
  const reconnectDefault = store.settings?.General?.AutoReconnect === "true";
  // Teto de memória: só com a feature `memory-trim` (nas duas edições).
  const memoryTrim = isWindows && store.platformCapabilities?.supportsMemoryTrim === true;
  const memoryDefaultMb = memoryLimitChoice(
    undefined,
    store.settings?.[MEMORY_LIMIT_SETTING.section]?.[MEMORY_LIMIT_SETTING.key]
  ).defaultMb;
  const bottingOn = store.bottingStatus?.active === true;
  const afkOn = store.afkStatus?.active === true;
  // Reconexão automática em andamento (commands/reconnect.rs).
  const reconnecting = (store.autoReconnect ?? []).filter(
    (entry) => entry.phase !== "gaveUp" && entry.phase !== "stopped"
  ).length;
  // Singular à parte, como o resto do app ("1 account" / "{{count}} accounts"):
  // o catálogo não usa o plural do i18next (chave = frase em inglês).
  const rejoinCount = store.bottingStatus?.userIds.length ?? 0;
  const afkCount = store.afkStatus?.accounts.length ?? 0;
  const keepAliveText = bottingOn
    ? rejoinCount === 1
      ? t("Auto Rejoin is running for 1 account.")
      : t("Auto Rejoin is running for {{count}} accounts.", { count: rejoinCount })
    : afkOn
      ? afkCount === 1
        ? t("AFK Mode is on for 1 account.")
        : t("AFK Mode is on for {{count}} accounts.", { count: afkCount })
      : reconnecting === 1
        ? t("Auto-reconnect is bringing back 1 account.")
        : reconnecting > 1
          ? t("Auto-reconnect is bringing back {{count}} accounts.", { count: reconnecting })
          : t("Nothing is keeping accounts in game right now.");

  return (
    <PageShell
      title={t("Session")}
      description={t("What is running now: accounts joining and Roblox clients in game, grouped by game and server, with the ones that need you on top.")}
      onLeave={onLeave}
      dataTour="session-page"
      tour="session"
    >
      <div data-testid="session-layout" className="flex flex-col gap-6 xl:flex-row xl:items-start">
        <div data-testid="session-lists" className="min-w-0 flex-1">
          <SessionPanel showStats contained={false} />
        </div>

        <aside
          aria-label={t("Session settings")}
          data-tour="session-summary"
          className="w-full xl:w-[300px] 2xl:w-[340px] min-[125rem]:w-[640px] min-[175rem]:w-[760px] shrink-0 grid gap-3 sm:grid-cols-2 xl:grid-cols-1 min-[125rem]:grid-cols-2 items-start xl:sticky xl:top-0"
        >
          <section className="rounded-xl border theme-border px-4 py-3">
            <h2 className="flex items-center gap-2 text-[12px] font-semibold text-[var(--panel-fg)]">
              <span
                aria-hidden="true"
                className={`w-1.5 h-1.5 rounded-full ${bottingOn || afkOn || reconnecting > 0 ? "bg-emerald-400" : "bg-[var(--border-color)]"}`}
              />
              {t("Keep accounts in game")}
            </h2>
            <p className="mt-1.5 text-[12px] leading-snug text-[var(--panel-muted)]">{keepAliveText}</p>
            {/* `General.AutoReconnect`: o único interruptor dele (Settings › Keep in game só leva até aqui).
                Cada conta muda o seu na lista "Em jogo" (SessionPanel). */}
            {isWindows && (
              <div className="mt-2 -mx-1">
                <Toggle
                  checked={reconnectDefault}
                  onChange={(v) => void store.updateSetting("General", "AutoReconnect", v ? "true" : "false")}
                  label="Reconnect accounts that drop"
                  description="Reopens an account in the same game after a lost connection, a kick or a crash. Only windows MultiAlt opened, never ones opened from the website. Each account can change this in its row of the In game list."
                />
              </div>
            )}
            <button
              type="button"
              onClick={() => store.setActivePage("afk")}
              className="theme-btn mt-3 inline-flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium"
            >
              <Keyboard size={13} strokeWidth={1.8} aria-hidden="true" />
              {t("Open AFK Mode")}
            </button>
          </section>

          {/* Gravações: quando tocam (Modo AFK, depois da reconexão), à vista
              sem ir até a aba Recordings. Gravação é só Windows. */}
          {isWindows && <RecordingsSummaryCard />}

          {/* O mesmo `Optimization.MemoryLimit` de Settings › Optimization.
              Cada conta muda o seu na lista "Em jogo" (SessionPanel). */}
          {memoryTrim && (
            <section className="rounded-xl border theme-border px-4 py-3">
              <h2 className="text-[12px] font-semibold text-[var(--panel-fg)]">{t("Memory limit")}</h2>
              <p className="mt-1.5 text-[12px] leading-snug text-[var(--panel-muted)]">
                {t(
                  "A client over this limit: MultiAlt asks Windows to free its memory first. Only windows MultiAlt opened; each account can change it in the In game list."
                )}
              </p>
              <div className="mt-2">
                <MemoryDefaultSelect
                  valueMb={memoryDefaultMb}
                  onChange={(mb) =>
                    void store.updateSetting(MEMORY_LIMIT_SETTING.section, MEMORY_LIMIT_SETTING.key, String(mb))
                  }
                />
              </div>
              <div className="mt-1">
                <Toggle
                  checked={store.settings?.Optimization?.CloseOverMemoryLimit === "true"}
                  onChange={(v) =>
                    void store.updateSetting("Optimization", "CloseOverMemoryLimit", v ? "true" : "false")
                  }
                  label="Close a client that stays over its limit"
                  description="If a client is still over its limit a minute after MultiAlt freed its memory, it is closed. Only that client, only windows MultiAlt opened. Off: MultiAlt only frees memory and never closes."
                />
              </div>
            </section>
          )}

        </aside>
      </div>
    </PageShell>
  );
}
