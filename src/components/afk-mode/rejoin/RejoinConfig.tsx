import { useState } from "react";
import { ChevronDown, ChevronRight, Columns2, Rows3 } from "lucide-react";
import { GameBadge } from "../../ui/GameBadge";
import { NumericInput } from "../../ui/NumericInput";
import { useTr } from "../../../i18n/text";
import type { RejoinController } from "./useRejoinController";

/**
 * A configuração do Auto Rejoin: contas, servidor e tempo. Cada bloco é um
 * componente só, usado nas duas visões (New View e Classic) — antes cada um
 * existia escrito duas vezes no diálogo.
 */

const CARD = "theme-surface rounded-xl border theme-border p-3";
const CARD_TITLE = "text-[13px] font-semibold text-[var(--panel-fg)] mb-2 flex items-center gap-2";

/**
 * O ciclo do backend (commands/botting.rs) fecha o cliente da conta com
 * `kill_for_user_graceful_async` e relança logo em seguida, e só age sobre as
 * contas bot da sessão. Quem tem várias contas abertas precisa ler isso ao
 * abrir a tela, não descobrir na prática.
 */
export function CycleExplainer({ ctl }: { ctl: RejoinController }) {
  const t = useTr();
  // Aberto com o modo parado (é antes do Start que a regra importa); com o
  // ciclo rodando, fechado — a lista ao vivo precisa da altura.
  const [open, setOpen] = useState(() => !ctl.running);
  return (
    <section className="shrink-0 rounded-xl border theme-border theme-soft px-3 py-2">
      <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
        <details
          open={open}
          onToggle={(e) => setOpen((e.currentTarget as HTMLDetailsElement).open)}
          className="group min-w-0 flex-1"
        >
          <summary className="flex cursor-pointer list-none items-center gap-1.5 py-1 text-[12px] font-medium text-[var(--panel-fg)] [&::-webkit-details-marker]:hidden">
            <ChevronRight
              size={14}
              strokeWidth={2}
              className="theme-muted transition-transform group-open:rotate-90"
              aria-hidden
            />
            {t("How each cycle works")}
          </summary>
          <ul className="mt-0.5 mb-1 space-y-0.5 text-[11px] theme-muted list-disc pl-[34px]">
            <li>
              {t(
                "Every rejoin closes that alt account's Roblox client and opens it again, so the account leaves the server and joins back."
              )}
            </li>
            <li>
              {t(
                "Only the alt accounts in this session are closed. Main accounts keep their client, and clients of accounts outside this session are left alone."
              )}
            </li>
            <li>
              {t(
                "Stop + Close Alt Accounts closes those same alt clients; Stop Auto Rejoin leaves every client open."
              )}
            </li>
          </ul>
        </details>
        <LayoutToggle ctl={ctl} />
      </div>
    </section>
  );
}

function LayoutToggle({ ctl }: { ctl: RejoinController }) {
  const t = useTr();
  const options = [
    { value: "split" as const, label: t("New View"), Icon: Columns2 },
    { value: "classic" as const, label: t("Classic"), Icon: Rows3 },
  ];
  return (
    <div className="flex items-center rounded-lg border theme-border p-0.5 bg-[var(--panel-bg)] shrink-0">
      {options.map(({ value, label, Icon }) => {
        const active = ctl.bottingLayout === value;
        return (
          <button
            key={value}
            type="button"
            aria-pressed={active}
            onClick={() => ctl.handleLayoutModeChange(value)}
            className={`flex items-center gap-1.5 px-2.5 py-1 text-[12px] rounded-md transition ${
              active
                ? "theme-accent-bg theme-accent"
                : "theme-muted hover:text-[var(--panel-fg)]"
            }`}
          >
            <Icon size={13} strokeWidth={1.75} aria-hidden />
            {label}
          </button>
        );
      })}
    </div>
  );
}

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

export function TargetsCard({ ctl }: { ctl: RejoinController }) {
  const t = useTr();
  const { playerUserIds, statusMap, targetAccounts, playerMenuOpen } = ctl;
  return (
    <section className={`${CARD} relative ${playerMenuOpen ? "z-30" : "z-10"}`}>
      <div className={CARD_TITLE}>
        {t("Targets")}
        <span className="font-normal text-[12px] theme-muted">{targetAccounts.length}</span>
      </div>
      {ctl.adopt ? (
        <p className="mb-2 text-[11px] leading-4 text-emerald-300/90">
          {t(
            "These accounts are already in game. Starting adopts their open clients: nothing is closed or reopened now, and each account stays in its server until its first rejoin."
          )}
        </p>
      ) : null}
      <div className="flex flex-wrap gap-1.5 mb-2">
        {targetAccounts.length === 0 ? (
          <span className="text-[12px] theme-muted">{t("No selected accounts")}</span>
        ) : null}
        {targetAccounts.map((a) => {
          const isPlayer = playerUserIds.includes(a.UserID) || !!statusMap.get(a.UserID)?.isPlayer;
          return (
            <span
              key={a.UserID}
              title={a.Alias || a.Username}
              className={[
                "px-2 py-1 rounded-md text-[12px] border theme-soft max-w-[160px] truncate",
                isPlayer ? "theme-accent-bg theme-accent-border theme-accent" : "theme-border text-[var(--panel-fg)]",
              ].join(" ")}
            >
              {a.Alias || a.Username}
            </span>
          );
        })}
      </div>
      <div className="flex items-center gap-2">
        <label className="text-[12px] theme-muted w-24 shrink-0">{t("Main Accounts")}</label>
        <div ref={ctl.playerMenuRef} className="relative w-full min-w-0">
          <button
            type="button"
            onClick={() => ctl.setPlayerMenuOpen((v) => !v)}
            className="sidebar-input text-xs flex items-center justify-between gap-2 hover:brightness-110 transition-all"
            aria-haspopup="listbox"
            aria-expanded={playerMenuOpen}
          >
            <span className="truncate" title={ctl.playerAccountTitle}>
              {ctl.playerAccountLabel}
            </span>
            <ChevronDown
              size={14}
              strokeWidth={2}
              className={`theme-muted transition-transform duration-150 ${playerMenuOpen ? "rotate-180" : ""}`}
            />
          </button>
          <div
            className={`absolute left-0 right-0 top-[calc(100%+6px)] z-20 max-h-64 overflow-y-auto rounded-lg border theme-border theme-panel shadow-2xl transition-all duration-150 ${
              playerMenuOpen
                ? "opacity-100 translate-y-0 pointer-events-auto"
                : "opacity-0 -translate-y-1 pointer-events-none"
            }`}
          >
            <button
              type="button"
              onClick={ctl.handleClearPlayers}
              className={`w-full text-left px-3 py-2 text-[12px] transition-colors ${
                playerUserIds.length === 0
                  ? "theme-accent-bg theme-accent"
                  : "text-[var(--panel-fg)] hover:bg-[var(--panel-soft)]"
              }`}
            >
              {t("None")}
            </button>
            <div className="h-px theme-border border-t" />
            {targetAccounts.map((a) => {
              const active = playerUserIds.includes(a.UserID);
              return (
                <button
                  key={a.UserID}
                  type="button"
                  onClick={() => {
                    void ctl.handleTogglePlayer(a.UserID);
                  }}
                  className={`w-full text-left px-3 py-2 text-[12px] transition-colors ${
                    active
                      ? "theme-accent-bg theme-accent"
                      : "text-[var(--panel-fg)] hover:bg-[var(--panel-soft)]"
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate" title={a.Alias || a.Username}>
                      {a.Alias || a.Username}
                    </span>
                    {active ? <span className="text-[12px] opacity-80">{t("Selected")}</span> : null}
                  </div>
                </button>
              );
            })}
          </div>
        </div>
      </div>
    </section>
  );
}

/**
 * Onde o ciclo rejoga. Adotando contas em jogo, só o place: ele vem do jogo
 * em que elas estão (presença), e Job/JoinData ficam de fora — fixar o servidor
 * atual mandaria todo reinício para um servidor que pode não existir mais.
 */
export function ServerCard({ ctl, columns }: { ctl: RejoinController; columns: 1 | 3 }) {
  const t = useTr();
  const { game } = ctl;
  const fieldsGrid = columns === 3 && !ctl.adopt ? "grid grid-cols-1 @2xl:grid-cols-3 gap-2" : "grid grid-cols-1 gap-2";
  return (
    <section className={CARD}>
      <div className={CARD_TITLE}>
        {t("Server")}
        {/* Qual jogo o ciclo vai rejogar, sem precisar sair da tela. */}
        <GameBadge
          name={game?.name ?? null}
          iconUrl={game?.iconUrl ?? null}
          placeId={game?.placeId}
          className="font-normal"
        />
      </div>
      <div className={fieldsGrid}>
        <input
          value={ctl.placeId}
          onChange={(e) => ctl.updatePlaceId(e.target.value)}
          onBlur={() => void ctl.saveCurrentDraft()}
          placeholder={t("Place ID")}
          aria-label={t("Place ID")}
          className="sidebar-input text-xs font-mono"
        />
        {ctl.adopt ? null : (
          <>
            <input
              value={ctl.jobId}
              onChange={(e) => ctl.updateJobId(e.target.value)}
              onBlur={() => void ctl.saveCurrentDraft()}
              placeholder={t("Job ID (optional)")}
              className="sidebar-input text-xs font-mono"
            />
            <input
              value={ctl.launchData}
              onChange={(e) => ctl.updateLaunchData(e.target.value)}
              onBlur={() => void ctl.saveCurrentDraft()}
              placeholder={t("JoinData (optional)")}
              className="sidebar-input text-xs"
            />
          </>
        )}
      </div>
      {ctl.adopt ? (
        <div
          className={`mt-2 text-[11px] leading-4 ${
            ctl.detection === "missing" ? "text-amber-300" : "theme-muted"
          }`}
        >
          {ctl.detection === "detecting"
            ? t("Finding the game these accounts are in...")
            : ctl.detection === "found"
              ? t("Found from the game these accounts are in now. Each rejoin goes back to this place.")
              : ctl.detection === "missing"
                ? t("Could not tell which game these accounts are in. Type the Place ID of the game they are playing.")
                : t("Each rejoin goes back to this place.")}
        </div>
      ) : ctl.shareLaunchFields ? (
        <div className="mt-2 text-[11px] theme-muted">
          {t("Launch fields are currently synced with Sidebar")}
        </div>
      ) : (
        <div className="mt-2 flex gap-2">
          <button onClick={ctl.applyCurrentLaunchFields} className="sidebar-btn-sm">
            {t("Use Current Launch Fields")}
          </button>
        </div>
      )}
    </section>
  );
}

/**
 * Unidade, significado e faixa de cada campo de Timing. Os limites sao os do
 * backend (`src-tauri/src/commands/botting.rs`): `clamp_botting_interval_minutes`
 * 10..480, `clamp_botting_launch_delay_seconds` 5..120 e
 * `resolve_player_grace_minutes` 1..90.
 */
function TimingFieldHints() {
  const t = useTr();
  return (
    <ul className="mt-2 space-y-0.5 text-[11px] theme-muted list-disc pl-4">
      <li>
        {t(
          "Rejoin Interval: minutes an alt account stays in the server before its client is closed and reopened (10-480)."
        )}
      </li>
      <li>
        {t(
          "Launch Delay: seconds between two launches, so the accounts do not all start at once (5-120)."
        )}
      </li>
      <li>
        {t(
          "Main Grace: minutes a main account keeps its client after you remove it from Main Accounts, before it joins the cycle (1-90)."
        )}
      </li>
    </ul>
  );
}

export function TimingCard({ ctl, columns }: { ctl: RejoinController; columns: 1 | 3 }) {
  const t = useTr();
  const fields = [
    {
      label: t("Rejoin Interval (minutes)"),
      value: ctl.intervalMinutes,
      min: 10,
      max: 480,
      onChange: ctl.setIntervalMinutes,
      onCommit: (v: number) => void ctl.saveCurrentDraft({ interval: v }),
    },
    {
      label: t("Launch Delay (seconds)"),
      value: ctl.launchDelaySeconds,
      min: 5,
      max: 120,
      onChange: ctl.setLaunchDelaySeconds,
      onCommit: (v: number) => void ctl.saveCurrentDraft({ delay: v }),
    },
    {
      label: t("Main Grace (minutes)"),
      value: ctl.playerGraceMinutes,
      min: 1,
      max: 90,
      onChange: ctl.setPlayerGraceMinutes,
      onCommit: (v: number) => void ctl.saveCurrentDraft({ grace: v }),
    },
  ];
  return (
    <section className={CARD}>
      <div className={CARD_TITLE}>{t("Timing")}</div>
      <div className={columns === 3 ? "grid grid-cols-1 @2xl:grid-cols-3 gap-2" : "grid grid-cols-1 gap-2"}>
        {fields.map((field) => (
          <div key={field.label} className="flex items-center gap-2">
            <label className={`text-[12px] theme-muted shrink-0 ${columns === 3 ? "w-36" : "w-32"}`}>
              {field.label}
            </label>
            <NumericInput
              ariaLabel={field.label}
              value={field.value}
              min={field.min}
              max={field.max}
              step={1}
              integer
              showStepper
              onChange={field.onChange}
              onCommit={field.onCommit}
              className="sidebar-input text-xs pr-10"
            />
          </div>
        ))}
      </div>
      <TimingFieldHints />
      <div className="text-[11px] theme-muted mt-2">
        {t("Main account demotion grace is {{minutes}} minutes before it enters normal restart cycle.", {
          minutes: ctl.playerGraceMinutes,
        })}
      </div>
    </section>
  );
}
