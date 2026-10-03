import { Check, ChevronRight, Gamepad2, KeyRound } from "lucide-react";
import { GameBadge } from "../../ui/GameBadge";
import { NumericInput } from "../../ui/NumericInput";
import { useTr } from "../../../i18n/text";
import type { Account } from "../../../types";
import type { RejoinController, ServerMode } from "./useRejoinController";

/**
 * A configuração do Auto Rejoin: quais contas, para onde rejogam e de quanto
 * em quanto tempo. Três cartões, pouco texto (pedido do dono, 03/10/2026: a
 * tela tinha conteúdo demais).
 */

const CARD = "theme-surface rounded-xl border theme-border p-3";
const CARD_TITLE = "text-[13px] font-semibold text-[var(--panel-fg)]";
const SMALL_BUTTON =
  "px-2 py-0.5 text-[12px] rounded-md border theme-border bg-[var(--buttons-bg)] text-[var(--buttons-fg)] hover:text-[var(--panel-fg)] hover:brightness-110 transition disabled:opacity-50 disabled:cursor-not-allowed";

/** Faixa de "feche o Roblox" quando o Multi Roblox não consegue o mutex. */
export function CloseRobloxAlert({ ctl }: { ctl: RejoinController }) {
  const t = useTr();
  if (!ctl.showCloseRobloxAction || !ctl.closeRobloxAlertMessage) return null;
  return (
    <div className="shrink-0 rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-xs text-red-300 flex items-center justify-between animate-fade-in">
      <span className="truncate pr-2">{ctl.closeRobloxAlertMessage}</span>
      <div className="ml-2 flex items-center gap-2 shrink-0">
        <button
          onClick={() => {
            void ctl.handleCloseRobloxBannerAction();
          }}
          disabled={ctl.closingRoblox}
          className="px-2 py-1 rounded-md bg-red-500/20 border border-red-500/30 text-red-200 hover:bg-red-500/30 active:bg-red-500/40 active:scale-[0.98] transition disabled:opacity-60 disabled:cursor-not-allowed"
        >
          {t("Close Roblox")}
        </button>
        <button
          onClick={ctl.dismissError}
          className="text-red-500/60 hover:text-red-300 transition-colors"
          aria-label={t("Close")}
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>
  );
}

/**
 * As contas para marcar — o mesmo gesto dos cliques AFK: marca quem vai e dá
 * Start. Com o ciclo rodando, a mesma lista mostra só quem está fora dele.
 */
export function AccountsCard({
  ctl,
  accounts,
  title,
}: {
  ctl: RejoinController;
  accounts: Pick<Account, "UserID" | "Username" | "Alias">[];
  title: string;
}) {
  const t = useTr();
  const pickedHere = accounts.filter((a) => ctl.picked.includes(a.UserID)).length;
  return (
    <section className={`@container ${CARD}`}>
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <div className={CARD_TITLE}>{title}</div>
        {accounts.length > 0 ? (
          <span className="text-[12px] theme-muted">
            {pickedHere} / {accounts.length}
          </span>
        ) : null}
        {accounts.length > 0 ? (
          <div className="ml-auto flex items-center gap-1.5">
            <button type="button" onClick={ctl.selectAllAccounts} className={SMALL_BUTTON}>
              {t("Select all")}
            </button>
            <button
              type="button"
              onClick={ctl.clearAccounts}
              disabled={pickedHere === 0}
              className={SMALL_BUTTON}
            >
              {t("Clear")}
            </button>
          </div>
        ) : null}
      </div>
      {accounts.length === 0 ? (
        <div className="text-[12px] theme-muted leading-4">
          {t("Open the accounts first: Auto Rejoin works with the Roblox clients that are open.")}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-1.5 @md:grid-cols-2 @3xl:grid-cols-3">
          {accounts.map((account) => {
            const on = ctl.picked.includes(account.UserID);
            const name = ctl.accountLabel(account);
            return (
              <button
                key={account.UserID}
                type="button"
                onClick={() => ctl.toggleAccount(account.UserID)}
                aria-pressed={on}
                aria-label={name}
                className={`flex items-center gap-2 rounded-lg border px-2 py-1.5 text-left transition-colors ${
                  on ? "theme-accent-border bg-[var(--accent-soft)]" : "theme-border hover:bg-[var(--panel-soft)]"
                }`}
              >
                <span
                  className={`w-4 h-4 shrink-0 rounded border flex items-center justify-center ${
                    on ? "theme-accent-border theme-accent" : "theme-border"
                  }`}
                  aria-hidden
                >
                  {on ? <Check size={11} strokeWidth={3} /> : null}
                </span>
                <span className="flex-1 truncate text-[12px] text-[var(--panel-fg)]" title={name}>
                  {name}
                </span>
              </button>
            );
          })}
        </div>
      )}
    </section>
  );
}

function ServerOption({
  ctl,
  mode,
  label,
  children,
}: {
  ctl: RejoinController;
  mode: ServerMode;
  label: string;
  children?: React.ReactNode;
}) {
  const on = ctl.serverMode === mode;
  return (
    <div
      className={`rounded-lg border transition-colors ${
        on ? "theme-accent-border bg-[var(--accent-soft)]" : "theme-border"
      }`}
    >
      <label className="flex cursor-pointer items-center gap-2.5 px-2.5 py-2">
        <input
          type="radio"
          name="rejoin-server"
          value={mode}
          checked={on}
          onChange={() => ctl.chooseServerMode(mode)}
          className="peer sr-only"
        />
        <span
          aria-hidden
          className={`flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full border peer-focus-visible:ring-2 peer-focus-visible:ring-[var(--input-focus)] ${
            on ? "theme-accent-border" : "theme-border"
          }`}
        >
          {on ? <span className="h-1.5 w-1.5 rounded-full bg-[var(--accent-color)]" /> : null}
        </span>
        <span className="text-[12px] font-medium text-[var(--panel-fg)]">{label}</span>
      </label>
      {on && children ? <div className="px-2.5 pb-2.5 pl-8">{children}</div> : null}
    </div>
  );
}

/** O que a presença disse sobre o jogo das contas marcadas. */
function CurrentGameStatus({ ctl }: { ctl: RejoinController }) {
  const t = useTr();
  const { detection, game } = ctl;
  if (detection === "found") {
    return (
      <div className="space-y-1">
        {game?.name || game?.iconUrl ? (
          <GameBadge name={game?.name ?? null} iconUrl={game?.iconUrl ?? null} placeId={game?.placeId} className="text-[12px] text-[var(--panel-fg)]" />
        ) : (
          <div className="text-[12px] font-mono text-[var(--panel-fg)]">
            {t("Place {{id}}", { id: ctl.detectedPlaceId ?? "" })}
          </div>
        )}
        <div className="text-[11px] leading-4 theme-muted">
          {t("Nobody is closed now. Each rejoin goes back to this game.")}
        </div>
      </div>
    );
  }
  const tone = detection === "mixed" || detection === "missing" ? "text-amber-300" : "theme-muted";
  const text =
    detection === "detecting"
      ? t("Finding the game these accounts are in...")
      : detection === "mixed"
        ? t("The selected accounts are in different games. Tick accounts from one game, or pick a game.")
        : detection === "missing"
          ? t("Could not tell which game these accounts are in. Pick a game.")
          : t("Uses the game of the accounts you tick.");
  return <div className={`text-[11px] leading-4 ${tone}`}>{text}</div>;
}

/** Favoritos (com os VIPs salvos neles) e, fechado, o Place ID digitado. */
function GamePicker({ ctl }: { ctl: RejoinController }) {
  const t = useTr();
  const { favorites, chosenPlaceId, jobId } = ctl;
  const chosenJob = jobId.trim();
  const isFavorite = favorites.some((f) => f.placeId === chosenPlaceId);
  return (
    <div className="space-y-2">
      {chosenPlaceId !== null && !isFavorite ? (
        <div className="text-[12px] text-[var(--panel-fg)]">
          {ctl.game?.name || ctl.game?.iconUrl ? (
            <GameBadge name={ctl.game?.name ?? null} iconUrl={ctl.game?.iconUrl ?? null} placeId={chosenPlaceId} />
          ) : (
            <span className="font-mono">{t("Place {{id}}", { id: chosenPlaceId })}</span>
          )}
        </div>
      ) : null}
      {favorites.length === 0 ? (
        <div className="text-[11px] leading-4 theme-muted">
          {t("No favorite games yet. Star a game in Choose Game and it shows up here.")}
        </div>
      ) : (
        <ul className="max-h-56 space-y-1 overflow-y-auto pr-0.5" aria-label={t("Favorites")}>
          {favorites.map((fav) => {
            const gameOn = chosenPlaceId === fav.placeId && chosenJob === "";
            const vips = fav.vipServers ?? [];
            return (
              <li key={fav.placeId}>
                <button
                  type="button"
                  aria-pressed={gameOn}
                  onClick={() => ctl.pickGame(fav.placeId)}
                  className={`flex w-full items-center gap-2 rounded-md border px-2 py-1 text-left transition-colors ${
                    gameOn ? "theme-accent-border bg-[var(--accent-soft)]" : "border-transparent hover:bg-[var(--panel-soft)]"
                  }`}
                >
                  {fav.iconUrl ? (
                    <img src={fav.iconUrl} alt="" className="h-5 w-5 shrink-0 rounded" loading="lazy" />
                  ) : (
                    <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded bg-[var(--panel-soft)] theme-muted" aria-hidden>
                      <Gamepad2 size={12} strokeWidth={1.75} />
                    </span>
                  )}
                  <span className="truncate text-[12px] text-[var(--panel-fg)]" title={fav.name}>
                    {fav.name}
                  </span>
                </button>
                {vips.length > 0 ? (
                  <div className="mt-1 flex flex-wrap gap-1 pl-7">
                    {vips.map((vip) => {
                      const vipOn = chosenPlaceId === fav.placeId && chosenJob === vip.link;
                      return (
                        <button
                          key={vip.id}
                          type="button"
                          aria-pressed={vipOn}
                          onClick={() => ctl.pickGame(fav.placeId, vip.link)}
                          title={t("Private server")}
                          className={`inline-flex max-w-full items-center gap-1 rounded-md border px-1.5 py-0.5 text-[11px] transition-colors ${
                            vipOn
                              ? "border-amber-400/50 bg-amber-500/15 text-amber-100"
                              : "theme-border text-amber-200/90 hover:bg-[var(--panel-soft)]"
                          }`}
                        >
                          <KeyRound size={11} strokeWidth={1.75} aria-hidden />
                          <span className="truncate">{vip.name}</span>
                        </button>
                      );
                    })}
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
      <details className="group">
        <summary className="flex cursor-pointer list-none items-center gap-1 text-[11px] theme-muted hover:text-[var(--panel-fg)] [&::-webkit-details-marker]:hidden">
          <ChevronRight size={12} strokeWidth={2} className="transition-transform group-open:rotate-90" aria-hidden />
          {t("Type a Place ID")}
        </summary>
        <div className="mt-1.5 grid grid-cols-1 gap-1.5">
          <input
            value={ctl.placeId}
            onChange={(e) => ctl.updatePlaceId(e.target.value)}
            placeholder={t("Place ID")}
            aria-label={t("Place ID")}
            inputMode="numeric"
            className="sidebar-input text-xs font-mono"
          />
          <input
            value={ctl.jobId}
            onChange={(e) => ctl.updateJobId(e.target.value)}
            placeholder={t("Job ID or private server link (optional)")}
            aria-label={t("Job ID or private server link (optional)")}
            className="sidebar-input text-xs font-mono"
          />
        </div>
      </details>
    </div>
  );
}

/** Para onde o ciclo rejoga: onde as contas já estão (padrão) ou um jogo escolhido. */
export function ServerCard({ ctl }: { ctl: RejoinController }) {
  const t = useTr();
  return (
    <section className={`${CARD} space-y-2`}>
      <div className={CARD_TITLE} id="rejoin-server-title">
        {t("Server")}
      </div>
      <div role="radiogroup" aria-labelledby="rejoin-server-title" className="space-y-1.5">
        <ServerOption ctl={ctl} mode="current" label={t("The game they are playing now")}>
          <CurrentGameStatus ctl={ctl} />
        </ServerOption>
        <ServerOption ctl={ctl} mode="game" label={t("A game I pick")}>
          <GamePicker ctl={ctl} />
        </ServerOption>
      </div>
    </section>
  );
}

/**
 * Os limites são os do backend (`src-tauri/src/commands/botting.rs`):
 * `clamp_botting_interval_minutes` 10..480 e
 * `clamp_botting_launch_delay_seconds` 5..120.
 */
export function TimingCard({ ctl }: { ctl: RejoinController }) {
  const t = useTr();
  const fields = [
    {
      label: t("Rejoin every"),
      ariaLabel: t("Rejoin every (minutes)"),
      unit: t("min"),
      value: ctl.intervalMinutes,
      min: 10,
      max: 480,
      onChange: ctl.setIntervalMinutes,
      onCommit: (v: number) => void ctl.saveDraft({ interval: v }),
    },
    {
      label: t("Time between launches"),
      ariaLabel: t("Time between launches (seconds)"),
      unit: t("s"),
      value: ctl.launchDelaySeconds,
      min: 5,
      max: 120,
      onChange: ctl.setLaunchDelaySeconds,
      onCommit: (v: number) => void ctl.saveDraft({ delay: v }),
    },
  ];
  return (
    <section className={`${CARD} space-y-2`}>
      <div className={CARD_TITLE}>{t("Timing")}</div>
      {fields.map((field) => (
        <div key={field.ariaLabel} className="flex items-center gap-2">
          <span className="w-36 shrink-0 text-[12px] theme-muted">{field.label}</span>
          <NumericInput
            ariaLabel={field.ariaLabel}
            value={field.value}
            min={field.min}
            max={field.max}
            step={1}
            integer
            showStepper
            disabled={ctl.running}
            onChange={field.onChange}
            onCommit={field.onCommit}
            containerClassName="relative flex-1"
            className="sidebar-input text-xs w-full pr-10 disabled:opacity-60"
          />
          <span className="w-6 text-[12px] theme-muted">{field.unit}</span>
        </div>
      ))}
      <p className="text-[11px] leading-4 theme-muted">
        {t(
          "Each rejoin closes that account's Roblox client and opens it again. Clients of other accounts are never touched."
        )}
      </p>
    </section>
  );
}
