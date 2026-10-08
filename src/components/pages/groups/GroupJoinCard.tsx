import { Check, ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { useTr } from "../../../i18n/text";
import { useAccountLabel } from "../../../hooks/useAccountLabel";
import type { Account } from "../../../types";
import { NEUTRAL_ACTION, PRIMARY_ACTION } from "../../afk-mode/ModeStatusBar";
import { GroupIcon, VerifiedBadge } from "./GroupResults";
import { STATUS_META, needsBrowser, type GroupJoinAccountResult, type GroupJoinSnapshot, type GroupSummary } from "./shared";

const CARD = "theme-surface rounded-xl border theme-border p-3";

/**
 * Quem entra no grupo escolhido, e o andamento de cada conta. A lista de
 * marcar é a mesma das abas do Modo AFK (Selecionar todas / Limpar); o selo de
 * cada linha vem do lote. Conta que esbarrou num captcha ganha "Resolver no
 * navegador" (abre o navegador dela na página do grupo) e "Conferir de novo".
 */
export function GroupJoinCard({
  group,
  icon,
  accounts,
  picked,
  onPickedChange,
  batch,
  starting,
  otherBatchRunning,
  cancelling,
  checking,
  onJoin,
  onCancel,
  onSolveInBrowser,
  onCheckAgain,
}: {
  group: GroupSummary | null;
  icon: string | null | undefined;
  accounts: Account[];
  picked: ReadonlySet<number>;
  onPickedChange: (next: Set<number>) => void;
  /** Só o lote deste grupo (o de outro grupo não pinta estas linhas). */
  batch: GroupJoinSnapshot | null;
  starting: boolean;
  /** Um lote de outro grupo está rodando: um por vez. */
  otherBatchRunning: boolean;
  cancelling: boolean;
  checking: ReadonlySet<number>;
  onJoin: () => void;
  onCancel: () => void;
  onSolveInBrowser: (userId: number) => void;
  onCheckAgain: (userId: number) => void;
}) {
  const t = useTr();
  const accountLabel = useAccountLabel();
  const running = batch?.running === true;
  const locked = running || starting || otherBatchRunning;
  const count = accounts.filter((a) => picked.has(a.UserID)).length;
  const results = new Map<number, GroupJoinAccountResult>((batch?.accounts ?? []).map((r) => [r.userId, r]));
  const total = Math.max(batch?.total ?? 0, batch?.done ?? 0);
  const percent = total > 0 ? Math.round(((batch?.done ?? 0) / total) * 100) : 0;
  const canJoin = group !== null && !group.isLocked && !locked && count > 0;

  function toggle(userId: number) {
    const next = new Set(picked);
    if (next.has(userId)) next.delete(userId);
    else next.add(userId);
    onPickedChange(next);
  }

  return (
    <section data-tour="groups-accounts" aria-label={t("Accounts to join")} className={`@container ${CARD}`}>
      <div className="mb-3 flex flex-wrap items-center gap-3">
        {group ? (
          <>
            <GroupIcon url={icon} size={32} />
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-1 min-w-0">
                <span className="truncate text-[13px] font-semibold text-[var(--panel-fg)]">
                  {t("Join {{name}}", { name: group.name })}
                </span>
                {group.hasVerifiedBadge ? <VerifiedBadge /> : null}
              </div>
              <div className="text-[11px] theme-muted">
                {group.isLocked
                  ? t("This group is locked and does not accept new members")
                  : group.publicEntryAllowed
                    ? t("Open group: accounts join right away.")
                    : t("This group approves each request: accounts end up pending until the owner accepts.")}
              </div>
            </div>
          </>
        ) : (
          <div className="flex-1 text-[13px] font-semibold text-[var(--panel-fg)]">
            {t("Pick a group above first")}
          </div>
        )}
      </div>

      <div className="mb-2 flex items-center justify-between gap-2">
        <div className="text-[12px] font-semibold text-[var(--panel-fg)]">
          {t("Accounts")}
          <span className="ml-1.5 text-[11px] font-normal theme-muted tabular-nums">
            {t("{{count}} of {{total}}", { count, total: accounts.length })}
          </span>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <button
            type="button"
            onClick={() => onPickedChange(new Set(accounts.map((a) => a.UserID)))}
            disabled={locked || accounts.length === 0}
            className="px-2 py-0.5 rounded-md text-[11px] theme-btn-ghost disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {t("Select all")}
          </button>
          <button
            type="button"
            onClick={() => onPickedChange(new Set())}
            disabled={locked || count === 0}
            className="px-2 py-0.5 rounded-md text-[11px] theme-btn-ghost disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {t("Clear")}
          </button>
        </div>
      </div>

      {accounts.length === 0 ? (
        <div className="text-[11px] theme-muted leading-4">{t("No accounts yet — add accounts first.")}</div>
      ) : (
        <ul className="grid grid-cols-1 gap-1.5 @2xl:grid-cols-2">
          {accounts.map((account) => {
            const on = picked.has(account.UserID);
            const name = accountLabel(account);
            const result = results.get(account.UserID);
            const meta = result ? STATUS_META[result.status] : null;
            const isChecking = checking.has(account.UserID);
            return (
              <li
                key={account.UserID}
                data-testid={`group-join-row-${account.UserID}`}
                className={`rounded-lg border transition-colors ${
                  on ? "border-[var(--accent-color)]/40 bg-[var(--accent-soft)]" : "theme-border"
                }`}
              >
                <div className="flex items-center gap-2 px-2 py-1.5">
                  <button
                    type="button"
                    role="checkbox"
                    aria-checked={on}
                    aria-label={name}
                    disabled={locked}
                    onClick={() => toggle(account.UserID)}
                    className="flex-1 min-w-0 flex items-center gap-2 text-left disabled:cursor-not-allowed disabled:opacity-70"
                  >
                    <span
                      className={`w-4 h-4 shrink-0 rounded border flex items-center justify-center ${
                        on
                          ? "bg-[var(--accent-color)] border-[var(--accent-color)] text-[var(--panel-bg)]"
                          : "theme-border"
                      }`}
                    >
                      {on ? <Check size={11} strokeWidth={3} /> : null}
                    </span>
                    <span className="flex-1 truncate text-[12px] text-[var(--panel-fg)]" title={name}>
                      {name}
                    </span>
                  </button>
                  {meta ? (
                    <span className={`shrink-0 inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full border text-[11px] ${meta.tone}`}>
                      {result?.status === "joining" ? <Loader2 size={10} className="animate-spin" aria-hidden="true" /> : null}
                      {t(meta.label)}
                    </span>
                  ) : null}
                </div>
                {result?.status === "failed" && result.reason ? (
                  <div className="px-2 pb-1.5 pl-8 text-[11px] text-red-300/90 break-words">{result.reason}</div>
                ) : null}
                {group && needsBrowser(result?.status) ? (
                  <div className="flex flex-wrap items-center gap-1.5 px-2 pb-1.5 pl-8">
                    <button
                      type="button"
                      onClick={() => onSolveInBrowser(account.UserID)}
                      aria-label={t("Solve in browser for {{name}}", { name })}
                      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md border text-[11px] border-amber-500/40 text-amber-200 hover:bg-amber-500/15"
                    >
                      <ExternalLink size={11} aria-hidden="true" />
                      {t("Solve in browser")}
                    </button>
                    <button
                      type="button"
                      onClick={() => onCheckAgain(account.UserID)}
                      disabled={isChecking}
                      aria-label={t("Check again for {{name}}", { name })}
                      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md border theme-border text-[11px] hover:bg-[var(--panel-soft)] disabled:opacity-50"
                    >
                      <RefreshCw size={11} aria-hidden="true" className={isChecking ? "animate-spin" : undefined} />
                      {t("Check again")}
                    </button>
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}

      <div data-tour="groups-join" className="mt-3 pt-3 border-t theme-border space-y-2">
        {batch && batch.accounts.length > 0 ? (
          <div className="space-y-1">
            <div className="flex items-center justify-between text-[11px] theme-muted tabular-nums">
              <span>{running ? t("Joining one account at a time...") : t("Finished")}</span>
              <span>{t("{{done}} of {{total}}", { done: batch.done, total })}</span>
            </div>
            <div
              role="progressbar"
              aria-label={t("Group join progress")}
              aria-valuemin={0}
              aria-valuemax={total}
              aria-valuenow={batch.done}
              className="h-2 rounded-full bg-[var(--panel-soft)] overflow-hidden"
            >
              <div
                className={`h-full rounded-full transition-[width] duration-500 ${
                  running ? "bg-[var(--accent-color)]" : "bg-emerald-500"
                }`}
                style={{ width: `${percent}%` }}
              />
            </div>
          </div>
        ) : null}
        <div className="flex flex-wrap items-center gap-2">
          {running ? (
            <button
              type="button"
              onClick={onCancel}
              disabled={cancelling}
              className={NEUTRAL_ACTION}
            >
              {cancelling ? t("Cancelling — finishing the current account...") : t("Cancel")}
            </button>
          ) : (
            <button
              type="button"
              onClick={onJoin}
              disabled={!canJoin}
              className={PRIMARY_ACTION}
            >
              {starting ? t("Starting...") : otherBatchRunning ? t("Another group join is running") : t("Join group")}
            </button>
          )}
          <span className="flex-1 min-w-[200px] text-[11px] theme-muted leading-4">
            {t(
              "Accounts join one at a time, with a short pause. If Roblox asks for a captcha, that account is skipped: solve it in its browser, then check again."
            )}
          </span>
        </div>
      </div>
    </section>
  );
}
