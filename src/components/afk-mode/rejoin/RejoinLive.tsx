import { Check, Focus, PowerOff, RefreshCcw, RotateCw, Unplug, X, type LucideIcon } from "lucide-react";
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
 * A lista ao vivo do Auto Rejoin: uma linha por conta da sessão, com a fase,
 * quando é o próximo rejoin e as ações (ícones, com o nome no tooltip). Marcar
 * linhas libera as mesmas ações em lote.
 */

type LiveRow = { userId: number; account: Account | null; row: BottingAccountStatus | null };

const ROW_ACTIONS: {
  action: BottingRowAction | "focus";
  label: string;
  tip: string;
  Icon: LucideIcon;
  tone: string;
}[] = [
  {
    action: "focus",
    label: "Focus client",
    tip: "Brings this Roblox window to the front.",
    Icon: Focus,
    tone: "hover:text-sky-200 hover:border-sky-400/40",
  },
  {
    action: "restartClient",
    label: "Restart client",
    tip: "Closes and reopens this client now. The rejoin timer keeps going.",
    Icon: RotateCw,
    tone: "hover:text-emerald-200 hover:border-emerald-400/40",
  },
  {
    action: "restartLoop",
    label: "Restart loop",
    tip: "Closes and reopens this client now, and starts its rejoin timer over.",
    Icon: RefreshCcw,
    tone: "hover:text-amber-200 hover:border-amber-400/40",
  },
  {
    action: "disconnect",
    label: "Disconnect",
    tip: "Stops rejoining this account. Its client stays open.",
    Icon: Unplug,
    tone: "hover:text-[var(--panel-fg)]",
  },
  {
    action: "close",
    label: "Close client",
    tip: "Closes this client. Auto Rejoin opens it again at its next rejoin.",
    Icon: X,
    tone: "hover:text-red-200 hover:border-red-400/40",
  },
  {
    action: "closeDisconnect",
    label: "Close + Disconnect",
    tip: "Closes this client and stops rejoining this account.",
    Icon: PowerOff,
    tone: "hover:text-red-200 hover:border-red-400/40",
  },
];

const BULK_ACTIONS: { action: BottingRowAction; label: string; tone: string }[] = [
  { action: "restartClient", label: "Restart client", tone: "" },
  { action: "restartLoop", label: "Restart loop", tone: "" },
  { action: "disconnect", label: "Disconnect", tone: "" },
  { action: "close", label: "Close client", tone: "text-red-200 border-red-400/30" },
  { action: "closeDisconnect", label: "Close + Disconnect", tone: "text-red-200 border-red-400/30" },
];

const SMALL_BUTTON =
  "px-2 py-0.5 text-[12px] rounded-md border theme-border bg-[var(--buttons-bg)] text-[var(--buttons-fg)] hover:text-[var(--panel-fg)] hover:brightness-110 transition disabled:opacity-50 disabled:cursor-not-allowed";

function RowActions({ ctl, live }: { ctl: RejoinController; live: LiveRow }) {
  const t = useTr();
  const { userId, row } = live;
  const isRowBusy = ctl.rowBusy === userId;
  const canAct = ctl.running && !ctl.actionButtonsLocked && !!row;
  const canFocus = canAct && !row?.disconnected && ctl.store.launchedByProgram.has(userId);
  return (
    <div className="flex shrink-0 items-center gap-1">
      {ROW_ACTIONS.map(({ action, label, tip, Icon, tone }) => {
        const enabled =
          action === "focus"
            ? canFocus && !isRowBusy
            : canAct && !isRowBusy && canRunBottingActionOnRow(row, action);
        return (
          <Tooltip
            key={action}
            content={
              <div className="space-y-0.5">
                <div className="font-semibold">{t(label)}</div>
                <div className="theme-muted">{t(tip)}</div>
              </div>
            }
          >
            <span>
              <button
                type="button"
                aria-label={t(label)}
                disabled={!enabled}
                className={`flex h-7 w-7 items-center justify-center rounded-md border theme-border theme-muted transition disabled:opacity-40 disabled:cursor-not-allowed ${tone}`}
                onClick={() => {
                  if (action === "focus") void ctl.runRowFocus(userId);
                  else void ctl.runRowAction(userId, action);
                }}
              >
                <Icon size={14} strokeWidth={1.75} aria-hidden />
              </button>
            </span>
          </Tooltip>
        );
      })}
    </div>
  );
}

/** "em 4:12 · 14:05", ou a fase quando não há rejoin agendado. */
function NextRejoin({ ctl, row }: { ctl: RejoinController; row: BottingAccountStatus | null }) {
  const t = useTr();
  if (!row || row.disconnected || row.isPlayer || typeof row.nextRestartAtMs !== "number") {
    return <span className="text-[12px] theme-muted">--</span>;
  }
  const countdown = formatCountdown(row.nextRestartAtMs, ctl.nowMs);
  const due = countdown === "due";
  return (
    <span className="text-right text-[12px] leading-tight">
      <span className={`block font-mono ${due ? "text-amber-200" : "text-[var(--panel-fg)]"}`}>
        {due ? t("due") : countdown}
      </span>
      <span className="block font-mono text-[11px] theme-muted">{formatDueTime(row.nextRestartAtMs)}</span>
    </span>
  );
}

export function LiveList({ ctl }: { ctl: RejoinController }) {
  const t = useTr();
  const { liveRows, actionButtonsLocked, bulkSelectedUserIds, bulkEligibleCounts } = ctl;
  const counts = [
    ctl.disconnectedCount > 0 ? t("Disconnected: {{count}}", { count: ctl.disconnectedCount }) : null,
    ctl.retryingCount > 0 ? t("Retrying: {{count}}", { count: ctl.retryingCount }) : null,
  ].filter(Boolean);
  return (
    <section data-testid="rejoin-live" className="@container theme-surface rounded-xl border theme-border p-3">
      <div className="flex flex-wrap items-center gap-2">
        <div className="text-[13px] font-semibold text-[var(--panel-fg)]">{t("In the cycle")}</div>
        <span className="text-[12px] theme-muted">{liveRows.length}</span>
        {counts.map((c) => (
          <span key={c} className="rounded-md border border-amber-400/25 bg-amber-500/10 px-1.5 py-px text-[11px] text-amber-100">
            {c}
          </span>
        ))}
        <div className="ml-auto flex items-center gap-1.5">
          {bulkSelectedUserIds.length > 0 ? (
            <span className="text-[12px] theme-muted">
              {t("{{count}} selected", { count: bulkSelectedUserIds.length })}
            </span>
          ) : null}
          <button
            type="button"
            onClick={() => ctl.setBulkSelected(liveRows.map(({ userId }) => userId))}
            disabled={actionButtonsLocked || liveRows.length === 0}
            className={SMALL_BUTTON}
          >
            {t("Select all")}
          </button>
          <button
            type="button"
            onClick={() => ctl.setBulkSelected([])}
            disabled={actionButtonsLocked || bulkSelectedUserIds.length === 0}
            className={SMALL_BUTTON}
          >
            {t("Clear")}
          </button>
        </div>
      </div>

      {bulkSelectedUserIds.length > 0 ? (
        <div className="mt-2 flex flex-wrap gap-1.5 animate-fade-in">
          {BULK_ACTIONS.map(({ action, label, tone }) => (
            <button
              key={action}
              type="button"
              onClick={() => {
                void ctl.runBulkAction(action);
              }}
              disabled={actionButtonsLocked || bulkEligibleCounts[action] === 0}
              className={`${SMALL_BUTTON} ${tone}`}
            >
              {t(label)} ({bulkEligibleCounts[action]})
            </button>
          ))}
        </div>
      ) : null}

      <ul className="mt-2 divide-y divide-[var(--border-color)]">
        {liveRows.map((live) => {
          const { userId, account, row } = live;
          const selected = ctl.bulkSelectedSet.has(userId);
          const name = ctl.accountLabel(account, `${t("User ID")}: ${userId}`);
          // Com os nomes ocultos a foto também some (a menos que a opção de
          // manter avatares esteja ligada) — igual à lista de contas.
          const avatarUrl = ctl.hideAvatar ? undefined : ctl.store.avatarUrls.get(userId);
          return (
            <li
              key={userId}
              className={`flex flex-wrap items-center gap-x-3 gap-y-2 py-2 ${selected ? "bg-[var(--accent-soft)] -mx-1.5 px-1.5 rounded-lg" : ""}`}
            >
              <button
                type="button"
                role="checkbox"
                aria-checked={selected}
                aria-label={t("Select account {{id}}", { id: userId })}
                disabled={actionButtonsLocked}
                onClick={() => ctl.toggleBulkSelected(userId)}
                className="theme-check"
              >
                <Check size={11} strokeWidth={3} className="theme-check-icon" />
              </button>
              {avatarUrl ? (
                <img
                  src={avatarUrl}
                  alt=""
                  className="theme-avatar w-7 h-7 rounded-full bg-[var(--panel-soft)] shrink-0"
                  loading="lazy"
                />
              ) : (
                <div className="theme-avatar w-7 h-7 rounded-full bg-[var(--panel-soft)] flex items-center justify-center theme-muted text-[11px] font-medium shrink-0">
                  {ctl.accountInitial(account)}
                </div>
              )}
              <div className="min-w-0 flex-1 basis-40">
                <div className="truncate text-[13px] text-[var(--panel-fg)]" title={name}>
                  {name}
                </div>
                <div className="flex min-w-0 items-center gap-2 text-[12px]">
                  <span className={phaseTone(row?.phase || "idle")}>{t(row?.phase || "idle")}</span>
                  {(row?.retryCount || 0) > 0 ? (
                    <span className={(row?.retryCount || 0) >= 4 ? "text-red-300" : "text-amber-300"}>
                      {t("{{count}} retries", { count: row?.retryCount || 0 })}
                    </span>
                  ) : null}
                </div>
                {row?.lastError ? (
                  <div className="truncate text-[12px] text-red-300/90" title={row.lastError}>
                    {row.lastError}
                  </div>
                ) : null}
              </div>
              <NextRejoin ctl={ctl} row={row} />
              <RowActions ctl={ctl} live={live} />
            </li>
          );
        })}
      </ul>
    </section>
  );
}
