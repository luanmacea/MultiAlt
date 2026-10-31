import { useCallback, useEffect, useState, type ReactNode } from "react";
import {
  BellRing,
  CircleHelp,
  Radio,
  Gamepad2,
  Keyboard,
  Layers,
  Palette,
  PanelLeftClose,
  PanelLeftOpen,
  ScrollText,
  Settings,
  Shirt,
  Sparkles,
  Star,
  TerminalSquare,
  Users,
  UsersRound,
  type LucideIcon,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useStore, type AppPage } from "../../store";
import { useTr } from "../../i18n/text";
import { ENABLE_GROUPS, ENABLE_HELP_BUTTON, ENABLE_NEXUS } from "../../featureFlags";
import { Tooltip } from "../ui/Tooltip";
import { StarPromptCard, openRepoForStar } from "./StarPromptCard";
import { platformFeatures, type PlatformFeatures } from "../../utils/platform";

/**
 * Barra lateral de navegação. Substitui os botões de ícone da Toolbar, que só
 * diziam o que faziam com o mouse parado em cima: aqui o nome fica sempre
 * visível, e cada item abre uma **página** na área principal (`activePage`).
 *
 * Recolhida, mostra só os ícones — e aí, só aí, o nome vem por tooltip. A
 * escolha fica no `localStorage` (preferência de quem está na máquina, como os
 * favoritos da Server List; ver docs/architecture.md, "Exceções à regra").
 * Janela estreita (menos de `NARROW_WIDTH`) recolhe sozinha: a lista de contas
 * precisa da largura mais do que os rótulos.
 */

export const NAV_COLLAPSED_KEY = "ram_nav_collapsed";
const NARROW_WIDTH = 900;

function readCollapsed(): boolean {
  try {
    return localStorage.getItem(NAV_COLLAPSED_KEY) === "1";
  } catch {
    return false;
  }
}

function writeCollapsed(collapsed: boolean) {
  try {
    localStorage.setItem(NAV_COLLAPSED_KEY, collapsed ? "1" : "0");
  } catch {
    // Armazenamento bloqueado: a escolha vale só até fechar o app.
  }
}

function useNarrowWindow(): boolean {
  const [narrow, setNarrow] = useState(() => typeof window !== "undefined" && window.innerWidth < NARROW_WIDTH);
  useEffect(() => {
    const onResize = () => setNarrow(window.innerWidth < NARROW_WIDTH);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return narrow;
}

interface NavItemDef {
  page: AppPage;
  label: string;
  icon: LucideIcon;
  tour?: string;
}

/**
 * Ordem combinada com o dono. Os grupos só separam visualmente: a lista, o que
 * roda agora, e o que se monta/ajusta. Função (e não constante) para as flags
 * serem lidas a cada render — os testes as trocam.
 */
const navGroups = (features: PlatformFeatures): NavItemDef[][] => [
  [{ page: "accounts", label: "Accounts", icon: Users, tour: "nav-accounts" }],
  [
    { page: "session", label: "Session", icon: Gamepad2, tour: "nav-session" },
    // O que o app fez (launch, Auto Rejoin, Watcher), ao lado do que roda agora.
    { page: "console", label: "Console", icon: ScrollText },
    // Auto Rejoin + tecla/clique AFK: no Linux (sem SendInput) a página some.
    ...(features.afkPage ? [{ page: "afk" as const, label: "AFK Mode", icon: Keyboard, tour: "nav-afk" }] : []),
    // Avisos, status ao vivo e resumo no webhook do usuário (docs/features/webhook.md).
    { page: "discord", label: "Discord alerts", icon: BellRing },
    // A janela "MultiAlt Live" para transmitir as contas (docs/features/live.md).
    ...(features.liveView ? [{ page: "live" as const, label: "Live", icon: Radio }] : []),
  ],
  [
    { page: "avatars", label: "Avatars", icon: Shirt },
    ...(ENABLE_GROUPS ? [{ page: "groups" as const, label: "Groups", icon: UsersRound }] : []),
    { page: "scripts", label: "Scripts", icon: TerminalSquare },
    { page: "theme", label: "Theme", icon: Palette },
    ...(ENABLE_NEXUS ? [{ page: "nexus" as const, label: "Nexus", icon: Layers }] : []),
    { page: "settings", label: "Settings", icon: Settings, tour: "nav-settings" },
  ],
];

/**
 * "What's new" (Novidades) fica no rodapé, ao lado do recolher: fala do
 * próprio app, não do trabalho com as contas, e quem procura um deles olha ali.
 * É página como as de cima (marca `aria-current`), não ação como o Help. (O
 * Help está escondido por `ENABLE_HELP_BUTTON` desde 08/10/2026.)
 */
const FOOTER_ITEMS: NavItemDef[] = [{ page: "changelog", label: "What's new", icon: Sparkles }];

export function NavSidebar() {
  const t = useTr();
  const store = useStore();
  const narrow = useNarrowWindow();
  const [userCollapsed, setUserCollapsed] = useState(readCollapsed);
  const collapsed = userCollapsed || narrow;

  const toggleCollapsed = useCallback(() => {
    setUserCollapsed((prev) => {
      const next = !prev;
      writeCollapsed(next);
      return next;
    });
  }, []);

  const running = store.launchedByProgram.size;
  const afkOn = store.afkStatus?.active === true || store.bottingStatus?.active === true;

  function trailing(page: AppPage): { visual: ReactNode; spoken: string | null } {
    if (page === "accounts" && store.accounts.length > 0) {
      return {
        visual: collapsed ? null : (
          <span className="ml-auto text-[11px] tabular-nums text-[var(--panel-muted)]">
            {store.accounts.length}
          </span>
        ),
        spoken: null,
      };
    }
    if (page === "session" && running > 0) {
      return {
        visual: (
          <span
            data-testid="nav-session-count"
            aria-hidden="true"
            className={
              collapsed
                ? "absolute top-1 right-1 min-w-[15px] h-[15px] px-1 rounded-full bg-[var(--accent-color)] text-[11px] font-semibold leading-[15px] text-center text-black"
                : "ml-auto min-w-[18px] h-[18px] px-1.5 rounded-full bg-[var(--accent-soft)] text-[var(--accent-color)] text-[11px] font-semibold leading-[18px] text-center tabular-nums"
            }
          >
            {running}
          </span>
        ),
        spoken: t("{{count}} running", { count: running }),
      };
    }
    if (page === "afk" && afkOn) {
      return {
        visual: (
          <span
            data-testid="nav-afk-active"
            aria-hidden="true"
            className={collapsed ? "absolute top-1.5 right-1.5 flex" : "ml-auto flex items-center pr-1"}
          >
            <span className="relative flex w-1.5 h-1.5">
              <span className="absolute inset-0 rounded-full bg-emerald-400 opacity-60 animate-ping motion-reduce:animate-none" />
              <span className="relative w-1.5 h-1.5 rounded-full bg-emerald-400" />
            </span>
          </span>
        ),
        spoken: t("On"),
      };
    }
    return { visual: null, spoken: null };
  }

  function renderItem(item: NavItemDef) {
    const active = store.activePage === item.page;
    const Icon = item.icon;
    const label = t(item.label);
    const extra = trailing(item.page);
    const button = (
      <button
        type="button"
        data-nav={item.page}
        data-tour={item.tour}
        aria-current={active ? "page" : undefined}
        onClick={() => store.setActivePage(item.page)}
        className={`nav-item group relative flex items-center rounded-lg text-[13px] transition-colors outline-none focus-visible:shadow-[0_0_0_2px_var(--input-focus)] ${
          collapsed ? "w-10 h-10 justify-center" : "w-full h-9 gap-3 px-2.5"
        } ${
          active
            ? "nav-item-active bg-[var(--panel-soft)] text-[var(--panel-fg)] font-medium"
            : "text-[var(--panel-muted)] hover:text-[var(--panel-fg)] hover:bg-[var(--row-hover)]"
        }`}
      >
        <Icon
          size={17}
          strokeWidth={active ? 2 : 1.6}
          aria-hidden="true"
          className={`shrink-0 ${active ? "text-[var(--accent-color)]" : ""}`}
        />
        <span className={collapsed ? "sr-only" : "truncate"}>{label}</span>
        {extra.spoken ? <span className="sr-only">{`, ${extra.spoken}`}</span> : null}
        {extra.visual}
      </button>
    );
    return (
      <li key={item.page} className={collapsed ? "flex justify-center" : undefined}>
        {collapsed ? (
          <Tooltip content={label} side="right" delayMs={200}>
            {button}
          </Tooltip>
        ) : (
          button
        )}
      </li>
    );
  }

  const helpLabel = t("Help");
  const helpButton = (
    <button
      type="button"
      onClick={store.openFirstRunWalkthroughFromSettings}
      aria-label={t("Help — replay the walkthrough")}
      className={`flex items-center rounded-lg text-[13px] text-[var(--panel-muted)] hover:text-[var(--panel-fg)] hover:bg-[var(--row-hover)] transition-colors outline-none focus-visible:shadow-[0_0_0_2px_var(--input-focus)] ${
        collapsed ? "w-10 h-10 justify-center" : "w-full h-9 gap-3 px-2.5"
      }`}
    >
      <CircleHelp size={17} strokeWidth={1.6} aria-hidden="true" className="shrink-0" />
      <span className={collapsed ? "sr-only" : "truncate"}>{helpLabel}</span>
    </button>
  );

  // O servidor da comunidade no Discord: abre o convite no navegador. O
  // endereço mora no backend (`open_discord_invite`).
  const discordLabel = t("Discord");
  const discordButton = (
    <button
      type="button"
      onClick={() => void invoke("open_discord_invite").catch(() => {})}
      aria-label={discordLabel}
      className={`flex items-center rounded-lg text-[13px] text-[var(--panel-muted)] hover:text-[var(--panel-fg)] hover:bg-[var(--row-hover)] transition-colors outline-none focus-visible:shadow-[0_0_0_2px_var(--input-focus)] ${
        collapsed ? "w-10 h-10 justify-center" : "w-full h-9 gap-3 px-2.5"
      }`}
    >
      <DiscordGlyph />
      <span className={collapsed ? "sr-only" : "truncate"}>{discordLabel}</span>
    </button>
  );

  // Estrela no GitHub (pedido do dono, 15/10/2026): o projeto é open source e
  // a estrela ajuda a manter os updates. Fixo no rodapé; o cartão que pede
  // (StarPromptCard) só aparece depois de uns dias de uso.
  const starLabel = t("Star on GitHub");
  const starButton = (
    <button
      type="button"
      onClick={openRepoForStar}
      aria-label={starLabel}
      className={`flex items-center rounded-lg text-[13px] text-[var(--panel-muted)] hover:text-[var(--panel-fg)] hover:bg-[var(--row-hover)] transition-colors outline-none focus-visible:shadow-[0_0_0_2px_var(--input-focus)] ${
        collapsed ? "w-10 h-10 justify-center" : "w-full h-9 gap-3 px-2.5"
      }`}
    >
      <Star size={17} strokeWidth={1.6} aria-hidden="true" className="shrink-0 text-amber-400" />
      <span className={collapsed ? "sr-only" : "truncate"}>{starLabel}</span>
    </button>
  );

  const collapseLabel = collapsed ? t("Expand sidebar") : t("Collapse sidebar");
  const CollapseIcon = collapsed ? PanelLeftOpen : PanelLeftClose;
  const collapseButton = (
    <button
      type="button"
      onClick={toggleCollapsed}
      aria-label={collapseLabel}
      aria-expanded={!collapsed}
      className={`flex items-center justify-center rounded-lg text-[var(--panel-muted)] hover:text-[var(--panel-fg)] hover:bg-[var(--row-hover)] transition-colors outline-none focus-visible:shadow-[0_0_0_2px_var(--input-focus)] ${
        collapsed ? "w-10 h-9" : "w-8 h-8"
      }`}
    >
      <CollapseIcon size={16} strokeWidth={1.6} aria-hidden="true" />
    </button>
  );

  return (
    <nav
      aria-label={t("Main navigation")}
      data-collapsed={collapsed ? "true" : "false"}
      className={`theme-panel theme-border border-r shrink-0 flex flex-col min-h-0 transition-[width] duration-200 ease-out motion-reduce:transition-none ${
        collapsed ? "w-[60px]" : "w-[188px]"
      }`}
    >
      <div className={`flex-1 min-h-0 overflow-y-auto overflow-x-hidden py-3 ${collapsed ? "px-2.5" : "px-2.5"}`}>
        {navGroups(platformFeatures(store.platformCapabilities)).map((group, index) => (
          <ul key={index} className={`space-y-0.5 ${index > 0 ? "mt-2 pt-2 border-t theme-border" : ""}`}>
            {group.map(renderItem)}
          </ul>
        ))}
      </div>

      <div className="shrink-0 border-t theme-border py-2 px-2.5">
        {collapsed ? null : <StarPromptCard />}
        <ul className="mb-0.5 space-y-0.5">
          {/* Pedido do dono (26/10/2026): o "Send feedback" saiu e o Discord
              ficou no lugar dele, logo abaixo da estrela. */}
          <li className={collapsed ? "flex justify-center" : undefined}>
            {collapsed ? (
              <Tooltip content={starLabel} side="right" delayMs={200}>
                {starButton}
              </Tooltip>
            ) : (
              starButton
            )}
          </li>
          <li className={collapsed ? "flex justify-center" : undefined}>
            {collapsed ? (
              <Tooltip content={discordLabel} side="right" delayMs={200}>
                {discordButton}
              </Tooltip>
            ) : (
              discordButton
            )}
          </li>
          {ENABLE_HELP_BUTTON ? (
            <li className={collapsed ? "flex justify-center" : undefined}>
              {collapsed ? (
                <Tooltip content={helpLabel} side="right" delayMs={200}>
                  {helpButton}
                </Tooltip>
              ) : (
                helpButton
              )}
            </li>
          ) : null}
        </ul>
        {/* Última linha: "What's new" ao lado do recolher (pedido do dono,
            08/10/2026). Recolhida, os dois empilham no centro; janela estreita
            (que já recolhe sozinha) não tem o botão de recolher. */}
        <div
          data-testid="nav-footer-actions"
          className={collapsed ? "flex flex-col items-center gap-1" : "flex items-center gap-1"}
        >
          <ul className={collapsed ? undefined : "flex-1 min-w-0"}>{FOOTER_ITEMS.map(renderItem)}</ul>
          {narrow ? null : collapsed ? (
            <Tooltip content={collapseLabel} side="right" delayMs={200}>
              {collapseButton}
            </Tooltip>
          ) : (
            collapseButton
          )}
        </div>
      </div>
    </nav>
  );
}

/** O logo do Discord (traço único, na cor do texto), do tamanho dos ícones da barra. */
function DiscordGlyph() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" className="shrink-0">
      <path d="M20.32 4.37a19.8 19.8 0 0 0-4.89-1.52.07.07 0 0 0-.08.04c-.21.38-.44.87-.61 1.25a18.3 18.3 0 0 0-5.49 0 12.6 12.6 0 0 0-.62-1.25.08.08 0 0 0-.08-.04 19.7 19.7 0 0 0-4.89 1.52.07.07 0 0 0-.03.03C.53 9.05-.32 13.58.1 18.06a.08.08 0 0 0 .03.06 19.9 19.9 0 0 0 5.99 3.03.08.08 0 0 0 .08-.03c.46-.63.87-1.3 1.23-1.99a.08.08 0 0 0-.04-.11 13.1 13.1 0 0 1-1.87-.89.08.08 0 0 1-.01-.13l.37-.29a.07.07 0 0 1 .08-.01c3.93 1.79 8.18 1.79 12.06 0a.07.07 0 0 1 .08.01l.37.29a.08.08 0 0 1-.01.13c-.6.35-1.22.65-1.87.89a.08.08 0 0 0-.04.11c.36.7.78 1.36 1.22 1.99a.08.08 0 0 0 .08.03 19.8 19.8 0 0 0 6-3.03.08.08 0 0 0 .03-.05c.5-5.18-.84-9.67-3.55-13.66a.06.06 0 0 0-.03-.03ZM8.02 15.33c-1.18 0-2.16-1.09-2.16-2.42s.96-2.42 2.16-2.42c1.21 0 2.18 1.1 2.16 2.42 0 1.33-.96 2.42-2.16 2.42Zm7.97 0c-1.18 0-2.15-1.09-2.15-2.42s.95-2.42 2.15-2.42c1.21 0 2.18 1.1 2.16 2.42 0 1.33-.95 2.42-2.16 2.42Z" />
    </svg>
  );
}
