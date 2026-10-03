import { Check } from "lucide-react";
import { Tooltip } from "../../ui/Tooltip";
import { useTr } from "../../../i18n/text";
import type { BottingAccountStatus } from "../../../store";
import type { Account } from "../../../types";
import {
  canRunBottingActionOnRow,
  formatCountdown,
  formatDueTime,
  phaseTone,
  type BottingRowAction,
} from "./rejoinShared";
import type { RejoinController } from "./useRejoinController";

/**
 * A lista ao vivo do Auto Rejoin: cada conta da sessão com a fase, o relógio do
 * próximo rejoin, as tentativas e as ações por conta. A New View tem seleção
 * e ações em lote; a Classic, linhas compactas.
 */

function ThemedCheckbox({
  checked,
  disabled,
  onToggle,
  ariaLabel,
}: {
  checked: boolean;
  disabled?: boolean;
  onToggle: () => void;
  ariaLabel: string;
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={onToggle}
      className="theme-check"
    >
      <Check size={11} strokeWidth={3} className="theme-check-icon" />
    </button>
  );
}

type LiveRow = { userId: number; account: Account | null; row: BottingAccountStatus | null };

const ROW_ACTIONS: {
  action: BottingRowAction | "focus";
  label: string;
  tipTitle: string;
  tip: string;
  tone: string;
}[] = [
  {
    action: "disconnect",
    label: "Disconnect",
    tipTitle: "Disconnect from loop",
    tip: "Stops cycling this account without closing Roblox.",
    tone: "theme-border bg-[var(--buttons-bg)] text-[var(--buttons-fg)] hover:text-[var(--panel-fg)] hover:brightness-110",
  },
  {
    action: "close",
    label: "Close client",
    tipTitle: "Close client",
    tip: "Closes Roblox. If disconnected, it rejoins immediately; otherwise it waits for the next scheduled rejoin.",
    tone: "theme-border bg-[var(--buttons-bg)] text-[var(--buttons-fg)] hover:text-[var(--panel-fg)] hover:brightness-110",
  },
  {
    action: "restartClient",
    label: "Restart client",
    tipTitle: "Restart client",
    tip: "Closes and relaunches this client now while keeping the current loop timing. Works for main accounts too.",
    tone: "border-emerald-400/30 bg-[rgba(16,185,129,0.10)] text-emerald-100 hover:bg-[rgba(16,185,129,0.18)] hover:border-emerald-300/40",
  },
  {
    action: "focus",
    label: "Focus client",
    tipTitle: "Focus client",
    tip: "Brings this Roblox window to the front and restores it if minimized.",
    tone: "border-sky-400/25 bg-[rgba(56,189,248,0.10)] text-sky-100 hover:bg-[rgba(56,189,248,0.18)] hover:border-sky-300/35",
  },
  {
    action: "restartLoop",
    label: "Restart loop",
    tipTitle: "Restart loop",
    tip: "Closes the current client and relaunches now. Main accounts stay main accounts; alt accounts continue standard rejoin timing.",
    tone: "border-amber-400/30 bg-[rgba(245,158,11,0.10)] text-amber-100 hover:bg-[rgba(245,158,11,0.18)] hover:border-amber-300/40",
  },
  {
    action: "closeDisconnect",
    label: "Close + Disconnect",
    tipTitle: "Close + remove from loop",
    tip: "Closes Roblox and excludes the account from cycling.",
    tone: "border-red-400/25 bg-[rgba(239,68,68,0.10)] text-red-200 hover:bg-[rgba(239,68,68,0.18)] hover:border-red-400/35",
  },
];

function RowActions({ ctl, live }: { ctl: RejoinController; live: LiveRow }) {
  const t = useTr();
  const { userId, row } = live;
  const isRowBusy = ctl.rowBusy === userId;
  const canAct = ctl.running && !ctl.actionButtonsLocked && !!row;
  const canFocus = canAct && !row?.disconnected && ctl.store.launchedByProgram.has(userId);
  return (
    <div className="flex flex-wrap items-center gap-1.5 rounded-xl border theme-border p-1.5 bg-[linear-gradient(95deg,rgba(255,255,255,0.07),rgba(255,255,255,0.015))]">
      {ROW_ACTIONS.map(({ action, label, tipTitle, tip, tone }) => {
        const enabled =
          action === "focus"
            ? canFocus && !isRowBusy
            : canAct && !isRowBusy && canRunBottingActionOnRow(row, action);
        return (
          <Tooltip
            key={action}
            content={
              <div className="space-y-0.5">
                <div className="font-semibold">{t(tipTitle)}</div>
                <div className="theme-muted">{t(tip)}</div>
              </div>
            }
          >
            <span>
              <button
                type="button"
                disabled={!enabled}
                className={`px-3 py-1 text-[12px] font-medium rounded-lg border transition disabled:opacity-50 disabled:cursor-not-allowed ${tone}`}
                onClick={() => {
                  if (action === "focus") void ctl.runRowFocus(userId);
                  else void ctl.runRowAction(userId, action);
                }}
              >
                {t(label)}
              </button>
            </span>
          </Tooltip>
        );
      })}
    </div>
  );
}

function RowStats({ ctl, live }: { ctl: RejoinController; live: LiveRow }) {
  const t = useTr();
  const { row } = live;
  const { nowMs } = ctl;
  const dueCountdownRaw = row?.disconnected
    ? "disconnected"
    : row?.isPlayer
      ? row.phase || "queued-player"
      : formatCountdown(row?.nextRestartAtMs ?? null, nowMs);
  const dueCountdown =
    dueCountdownRaw.includes(":") || dueCountdownRaw === "--" ? dueCountdownRaw : t(dueCountdownRaw);
  const dueClock = row && !row.disconnected && !row.isPlayer ? formatDueTime(row.nextRestartAtMs ?? null) : "--";
  const retryCount = row?.retryCount || 0;
  const retryTone =
    retryCount >= 4
      ? "border-red-500/30 bg-red-500/15 text-red-200"
      : retryCount >= 2
        ? "border-amber-400/35 bg-amber-500/15 text-amber-200"
        : "theme-border bg-[rgba(0,0,0,0.18)] text-[var(--panel-fg)]";
  const dueTone = !row
    ? "theme-border bg-[rgba(0,0,0,0.18)] text-[var(--panel-fg)]"
    : row.disconnected
      ? "border-zinc-500/35 bg-zinc-500/15 text-zinc-200"
      : row.isPlayer
        ? "border-sky-500/30 bg-sky-500/15 text-sky-200"
        : (row.nextRestartAtMs ?? 0) <= nowMs
          ? "border-amber-400/35 bg-amber-500/15 text-amber-100"
          : "border-emerald-500/30 bg-emerald-500/15 text-emerald-200";
  const cells = [
    { label: t("due"), value: dueCountdown, tone: dueTone },
    { label: t("next"), value: dueClock, tone: "theme-border bg-[rgba(0,0,0,0.18)] text-[var(--panel-fg)]" },
    { label: t("retries"), value: String(retryCount), tone: retryTone },
  ];
  return (
    <div className="grid grid-cols-3 gap-1.5 w-full @2xl:w-[300px] @2xl:shrink-0">
      {cells.map((cell) => (
        <div key={cell.label} className={`rounded-lg border px-2.5 py-1.5 ${cell.tone}`}>
          <div className="text-[11px] opacity-75">{cell.label}</div>
          <div className="text-[12px] font-mono leading-tight">{cell.value}</div>
        </div>
      ))}
    </div>
  );
}

function rowName(t: ReturnType<typeof useTr>, live: LiveRow): string {
  return live.account?.Alias || live.account?.Username || `${t("User ID")}: ${live.userId}`;
}

/** New View: contadores, seleção em lote e um cartão por conta. */
export function LiveList({ ctl }: { ctl: RejoinController }) {
  const t = useTr();
  const { liveRows, actionButtonsLocked, bulkSelectedUserIds, bulkEligibleCounts, running } = ctl;
  const bulkButton =
    "px-2.5 py-1 text-[12px] rounded-md border theme-border bg-[var(--buttons-bg)] text-[var(--buttons-fg)] hover:text-[var(--panel-fg)] hover:brightness-110 transition disabled:opacity-50 disabled:cursor-not-allowed";
  const bulkActions: { action: BottingRowAction; label: string; tone: string }[] = [
    { action: "disconnect", label: "Disconnect", tone: "theme-border bg-[var(--buttons-bg)] text-[var(--buttons-fg)] hover:text-[var(--panel-fg)] hover:brightness-110" },
    { action: "close", label: "Close client", tone: "theme-border bg-[var(--buttons-bg)] text-[var(--buttons-fg)] hover:text-[var(--panel-fg)] hover:brightness-110" },
    { action: "restartClient", label: "Restart client", tone: "border-emerald-400/30 bg-[rgba(16,185,129,0.10)] text-emerald-100 hover:bg-[rgba(16,185,129,0.18)]" },
    { action: "restartLoop", label: "Restart loop", tone: "border-amber-400/30 bg-[rgba(245,158,11,0.10)] text-amber-100 hover:bg-[rgba(245,158,11,0.18)]" },
    { action: "closeDisconnect", label: "Close + Disconnect", tone: "border-red-400/25 bg-[rgba(239,68,68,0.10)] text-red-200 hover:bg-[rgba(239,68,68,0.18)]" },
  ];
  return (
    <section className="@container theme-surface rounded-2xl border theme-border p-3 flex flex-col @3xl/rejoin:h-full @3xl/rejoin:min-h-0">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <div className="text-[13px] font-semibold text-[var(--panel-fg)]">{t("Live Auto Rejoin List")}</div>
          <div className="text-[11px] theme-muted mt-0.5">
            {t("Track each account cycle, quick actions, and retry pressure in one place")}
          </div>
        </div>
        <div className="flex flex-wrap gap-1.5 text-[12px]">
          <div className="rounded-md border theme-border px-2 py-1 bg-[rgba(16,185,129,0.14)] text-emerald-200">
            {t("Accounts")}: {liveRows.length}
          </div>
          <div className="rounded-md border border-sky-500/25 px-2 py-1 bg-sky-500/12 text-sky-200">
            {t("Mains")}: {ctl.splitPlayersCount}
          </div>
          <div className="rounded-md border border-zinc-500/25 px-2 py-1 bg-zinc-500/12 text-zinc-200">
            {t("Disconnected")}: {ctl.splitDisconnectedCount}
          </div>
          <div className="rounded-md border border-amber-400/25 px-2 py-1 bg-amber-500/12 text-amber-100">
            {t("Retrying")}: {ctl.splitRetryingCount}
          </div>
        </div>
      </div>

      <div className="mt-3 rounded-xl border theme-border theme-soft p-2.5">
        <div className="flex flex-wrap items-center gap-1.5">
          <label className="inline-flex items-center gap-1.5 rounded-md border theme-border px-2 py-1 text-[12px]">
            <ThemedCheckbox
              checked={ctl.allVisibleBulkSelected}
              disabled={actionButtonsLocked || liveRows.length === 0}
              onToggle={() =>
                ctl.setBulkSelected(ctl.allVisibleBulkSelected ? [] : liveRows.map(({ userId }) => userId))
              }
              ariaLabel={t("Toggle all visible")}
            />
            <span className="text-[var(--panel-fg)]">{t("All visible")}</span>
          </label>
          <button
            type="button"
            onClick={() => ctl.setBulkSelected(ctl.visibleBotRowIds)}
            disabled={actionButtonsLocked || ctl.visibleBotRowIds.length === 0}
            className={bulkButton}
          >
            {t("Select alts")}
          </button>
          <button
            type="button"
            onClick={() => ctl.setBulkSelected(liveRows.map(({ userId }) => userId))}
            disabled={actionButtonsLocked || liveRows.length === 0}
            className={bulkButton}
          >
            {t("Select all")}
          </button>
          <button
            type="button"
            onClick={() => ctl.setBulkSelected([])}
            disabled={actionButtonsLocked || bulkSelectedUserIds.length === 0}
            className={bulkButton}
          >
            {t("Clear")}
          </button>
          <div className="ml-auto rounded-md border border-sky-500/30 bg-sky-500/12 text-sky-200 text-[12px] px-2 py-1">
            {t("Selected")}: {bulkSelectedUserIds.length}
          </div>
        </div>

        <div
          className={[
            "mt-2 overflow-hidden transition-[max-height,opacity,transform] duration-200 ease-out",
            bulkSelectedUserIds.length > 0
              ? "max-h-28 opacity-100 translate-y-0"
              : "max-h-0 opacity-0 -translate-y-1 pointer-events-none",
          ].join(" ")}
        >
          <div className="flex flex-wrap gap-1.5">
            {bulkActions.map(({ action, label, tone }) => (
              <button
                key={action}
                type="button"
                onClick={() => {
                  void ctl.runBulkAction(action);
                }}
                disabled={actionButtonsLocked || !running || bulkEligibleCounts[action] === 0}
                className={`px-3 py-1 text-[12px] font-medium rounded-lg border transition disabled:opacity-50 disabled:cursor-not-allowed ${tone}`}
              >
                {t(label)} ({bulkEligibleCounts[action]})
              </button>
            ))}
          </div>
        </div>

        <div className="mt-2 text-[11px] theme-muted">
          {t("Select rows to run bulk actions. Launch delays and retry rules remain unchanged")}
        </div>
      </div>

      <div className="mt-3 space-y-2 pr-1 @3xl/rejoin:flex-1 @3xl/rejoin:min-h-0 @3xl/rejoin:overflow-y-auto">
        {liveRows.length === 0 ? (
          <div className="min-h-[96px] rounded-xl border theme-border theme-soft flex items-center justify-center text-[12px] theme-muted">
            {t("No selected accounts")}
          </div>
        ) : (
          liveRows.map((live) => {
            const { userId, account, row } = live;
            const isBulkSelected = ctl.bulkSelectedSet.has(userId);
            const name = rowName(t, live);
            const avatarUrl = ctl.store.avatarUrls.get(userId);
            return (
              <div
                key={userId}
                className={[
                  "rounded-2xl border p-3 theme-soft",
                  isBulkSelected
                    ? "border-sky-500/45 ring-1 ring-sky-400/30"
                    : row?.disconnected
                      ? "border-zinc-500/30"
                      : row?.isPlayer
                        ? "theme-accent-border"
                        : "theme-border",
                ].join(" ")}
              >
                <div className="flex flex-col gap-3 @2xl:flex-row @2xl:items-start @2xl:justify-between">
                  <div className="min-w-0 flex items-start gap-2.5">
                    <div className="mt-1 inline-flex items-center">
                      <ThemedCheckbox
                        checked={isBulkSelected}
                        disabled={actionButtonsLocked}
                        onToggle={() => ctl.toggleBulkSelected(userId)}
                        ariaLabel={t("Select account {{id}}", { id: userId })}
                      />
                    </div>
                    {avatarUrl ? (
                      <img
                        src={avatarUrl}
                        alt=""
                        className="theme-avatar w-7 h-7 rounded-full bg-[var(--panel-soft)] shrink-0"
                        loading="lazy"
                      />
                    ) : (
                      <div className="theme-avatar w-7 h-7 rounded-full bg-[var(--panel-soft)] flex items-center justify-center theme-muted text-[11px] font-medium shrink-0">
                        {(account?.Username || "?").charAt(0).toUpperCase()}
                      </div>
                    )}
                    <div className="min-w-0">
                      <div className="text-[13px] text-[var(--panel-fg)] truncate" title={name}>
                        {name}
                      </div>
                      <div className={`text-[12px] ${phaseTone(row?.phase || "idle")}`}>
                        {t(row?.phase || "idle")}
                      </div>
                      {row?.lastError ? (
                        <div className="text-[12px] text-red-300/90 truncate mt-0.5" title={row.lastError}>
                          {row.lastError}
                        </div>
                      ) : null}
                    </div>
                  </div>
                  <RowStats ctl={ctl} live={live} />
                </div>
                <div className="mt-3">
                  <RowActions ctl={ctl} live={live} />
                </div>
              </div>
            );
          })
        )}
      </div>
    </section>
  );
}

/** Classic: uma linha compacta por conta, sem seleção em lote. */
export function LiveCycleList({ ctl }: { ctl: RejoinController }) {
  const t = useTr();
  return (
    <section className="@container theme-surface rounded-xl border theme-border p-3">
      <div className="text-[13px] font-semibold text-[var(--panel-fg)] mb-2">{t("Live Cycle")}</div>
      <div className="space-y-1.5">
        {ctl.liveRows.map((live) => {
          const { userId, row } = live;
          const name = rowName(t, live);
          return (
            <div
              key={userId}
              className={[
                "rounded-xl border px-3 py-2.5 theme-soft",
                row?.disconnected ? "border-zinc-500/30" : row?.isPlayer ? "theme-accent-border" : "theme-border",
              ].join(" ")}
            >
              <div className="flex flex-col gap-2 @3xl:flex-row @3xl:items-center @3xl:justify-between">
                <div className="flex-1 min-w-0">
                  {/* A caixa do nome chega a ficar estreita (a barra de ações
                      ocupa o resto da linha): o `title` lê o nome inteiro. */}
                  <div className="text-[13px] text-[var(--panel-fg)] truncate" title={name}>
                    {name}
                  </div>
                  <div className={`text-[12px] ${phaseTone(row?.phase || "idle")}`}>
                    {t(row?.phase || "idle")}
                    {row?.lastError ? ` - ${row.lastError}` : ""}
                  </div>
                </div>
                <div className="flex w-full flex-col gap-1.5 @3xl:w-auto @3xl:items-end">
                  <RowActions ctl={ctl} live={live} />
                  <RowStats ctl={ctl} live={live} />
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
