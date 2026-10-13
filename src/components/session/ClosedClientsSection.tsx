import { PowerOff, RotateCcw, X } from "lucide-react";
import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { accountLabel } from "../../utils/accountName";
import { awayEventTime } from "../../utils/awayDigest";
import { closedReasonLabel, isPendingInQueue, rejoinLaunchTarget } from "../../utils/closedClients";
import { useGameIdentity } from "../../hooks/useGameIdentity";
import type { ClosedClient } from "../../types";

/**
 * Seção "Fechou" da página Session: as contas cujo cliente fechou no meio do
 * jogo e que ninguém está reabrindo (commands/closed_clients.rs), com o mesmo
 * "Entrar novamente" e o mesmo "×" da linha da lista de contas.
 *
 * Quem ainda aparece na seção "Reconnecting" (inclusive a que desistiu) fica
 * de fora: uma conta, uma linha — `hiddenIds` vem do painel.
 */
export function ClosedClientsSection({
  hiddenIds,
  contained,
}: {
  hiddenIds: ReadonlySet<number>;
  contained: boolean;
}) {
  const t = useTr();
  const store = useStore();
  const order = new Map(store.accounts.map((account, index) => [account.UserID, index] as const));
  const closed = [...(store.closedClients?.values() ?? [])]
    .filter((entry) => !hiddenIds.has(entry.userId))
    .sort((a, b) => (order.get(a.userId) ?? Infinity) - (order.get(b.userId) ?? Infinity) || a.userId - b.userId);
  if (closed.length === 0) return null;

  return (
    <section data-testid="closed-clients-panel" className="rounded-lg border theme-border bg-[var(--panel-soft)]">
      <header className="flex items-center gap-2 px-3 py-2 border-b theme-border min-w-0">
        <h2 className="text-[12px] font-semibold text-[var(--panel-fg)] flex items-center gap-1.5 shrink-0">
          <PowerOff size={13} strokeWidth={1.5} aria-hidden="true" />
          {t("Closed")}
        </h2>
        <span className="text-[12px] theme-muted truncate">
          {t("Their Roblox closed during a game. Rejoin puts them back in the same server.")}
        </span>
      </header>
      <ul className={`${contained ? "max-h-48 overflow-y-auto " : ""}py-1`}>
        {closed.map((entry) => (
          <ClosedLine key={entry.userId} entry={entry} />
        ))}
      </ul>
    </section>
  );
}

function ClosedLine({ entry }: { entry: ClosedClient }) {
  const t = useTr();
  const store = useStore();
  const identity = useGameIdentity(entry.placeId, entry.userId);
  const account = store.accounts.find((a) => a.UserID === entry.userId);
  const name = accountLabel(account, store, entry.userId);
  const reason = closedReasonLabel(entry, t);
  const game = entry.placeId ? (identity?.name ?? t("Place {{placeId}}", { placeId: entry.placeId })) : null;
  const when = t("Closed at {{time}}", { time: awayEventTime(entry.closedAtMs) });
  const queued = isPendingInQueue(store.launchQueue, entry.userId);
  const detail = [game, when].filter(Boolean).join(" · ");

  return (
    <li data-testid={`session-closed-${entry.userId}`} className="flex items-center gap-2 px-3 py-1.5 text-[12px]">
      <span aria-hidden="true" className="shrink-0 w-1.5 h-1.5 rounded-full bg-amber-400" />
      <span className="text-[var(--panel-fg)] truncate shrink-0 max-w-[40%]">{name}</span>
      <span className="min-w-0 flex flex-col">
        <span className="truncate text-amber-400" title={reason}>
          {reason}
        </span>
        <span className="truncate text-[11px] theme-muted" title={detail}>
          {detail}
        </span>
      </span>
      <span className="ml-auto flex items-center gap-1.5 shrink-0">
        {entry.rejoin && (
          <button
            type="button"
            onClick={() => {
              if (entry.rejoin) void store.joinServer(entry.userId, rejoinLaunchTarget(entry.rejoin));
            }}
            disabled={queued}
            className="sidebar-btn-sm flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
            title={queued ? t("Waiting in the launch queue") : t("Put this account back in the same game and server")}
          >
            <RotateCcw size={12} strokeWidth={1.5} aria-hidden="true" />
            {t("Rejoin")}
          </button>
        )}
        <button
          type="button"
          onClick={() => void store.dismissClosedClient(entry.userId)}
          aria-label={t("Dismiss {{name}}", { name })}
          title={t("Hide this notice")}
          className="inline-flex w-6 h-6 items-center justify-center rounded-md theme-muted hover:text-[var(--panel-fg)] hover:bg-[var(--row-hover)] transition-colors"
        >
          <X size={13} strokeWidth={2} aria-hidden="true" />
        </button>
      </span>
    </li>
  );
}
