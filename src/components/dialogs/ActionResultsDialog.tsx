import { useEffect, useMemo, useRef, useState } from "react";
import {
  X,
  Check,
  CheckCheck,
  CircleSlash,
  KeyRound,
  Loader2,
  Clock,
  CircleAlert,
  type LucideIcon,
} from "lucide-react";
import { useStore } from "../../store";
import { useModalClose } from "../../hooks/useModalClose";
import { useBackdropClose } from "../../hooks/useBackdropClose";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { useConfirm } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";
import {
  ACTION_OUTCOME_ORDER,
  countOutcomes,
  rowsFromFollow,
  rowsFromFriendLink,
  sortRows,
  type ActionKind,
  type ActionOutcome,
} from "../../utils/actionReport";

const ICONS: Record<ActionOutcome, LucideIcon> = {
  login: KeyRound,
  failed: CircleAlert,
  running: Loader2,
  pending: Clock,
  skipped: CircleSlash,
  already: CheckCheck,
  ok: Check,
};

/** Cor de cada resultado. A pista que não depende dela é o ícone + o texto. */
const TONES: Record<ActionOutcome, string> = {
  login: "text-amber-300 bg-amber-500/15 border-amber-500/30",
  failed: "text-red-300 bg-red-500/15 border-red-500/30",
  running: "text-sky-300 bg-sky-500/15 border-sky-500/30",
  pending: "text-zinc-300 bg-zinc-500/15 border-zinc-500/30",
  skipped: "text-zinc-300 bg-zinc-500/15 border-zinc-500/30",
  already: "text-emerald-300 bg-emerald-500/10 border-emerald-500/25",
  ok: "text-emerald-300 bg-emerald-500/15 border-emerald-500/30",
};

/**
 * Resultado conta a conta das ações do menu "Actions" e do clique direito —
 * o mesmo desenho do "Check accounts": números que filtram, uma linha por
 * conta com o motivo, e o que dá para fazer com o que falhou. Sessão morta
 * vira "precisa de login" e oferece "Fazer login de novo". Ver
 * docs/features/accounts.md.
 */
export function ActionResultsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useTr();
  const store = useStore();
  const confirm = useConfirm();
  const accountLabel = useAccountLabel();
  const { visible, closing, handleClose } = useModalClose(open, onClose);
  const backdropClose = useBackdropClose(handleClose);

  // O store zera o relatório ao fechar; a animação de saída ainda precisa dele.
  const lastReport = useRef(store.actionReport);
  if (store.actionReport) lastReport.current = store.actionReport;
  const report = store.actionReport ?? lastReport.current;

  const [filter, setFilter] = useState<ActionOutcome | null>(null);
  useEffect(() => {
    if (!open) setFilter(null);
  }, [open]);

  const loginDetail = t("Roblox ended this session. Sign in again to use the account.");
  // Make Friends e Follow: as linhas vêm ao vivo do backend enquanto rodam.
  const liveRows = useMemo(() => {
    if (!report?.running) return null;
    if (report.kind === "make-friends" && store.friendLinkState?.accounts.length) {
      return rowsFromFriendLink(store.friendLinkState, loginDetail);
    }
    if (report.kind === "follow" && store.followUserState?.accounts.length) {
      return rowsFromFollow(store.followUserState, loginDetail, t("Waiting for you in the browser window"));
    }
    return null;
  }, [report, store.friendLinkState, store.followUserState, loginDetail, t]);

  const allRows = useMemo(() => sortRows(liveRows ?? report?.rows ?? []), [liveRows, report]);
  const counts = useMemo(() => countOutcomes(allRows), [allRows]);
  const accountsById = useMemo(() => new Map(store.accounts.map((a) => [a.UserID, a])), [store.accounts]);

  if (!visible || !report) return null;

  const rows = filter ? allRows.filter((r) => r.outcome === filter) : allRows;
  const loginIds = allRows.filter((r) => r.outcome === "login").map((r) => r.userId);
  const failedIds = allRows.filter((r) => r.outcome === "failed" || r.outcome === "login").map((r) => r.userId);
  const done = allRows.filter((r) => r.outcome !== "pending" && r.outcome !== "running").length;

  const outcomeLabel = (outcome: ActionOutcome): string => okLabel(report.kind, outcome, t);

  async function signInAgain() {
    if (loginIds.length === 0 || report?.running) return;
    const ok = await confirm(
      t(
        "{{count}} account(s) need to sign in again. A browser window opens for each one, one at a time, with the username and password filled in when they are saved. You solve the CAPTCHA, and the new session goes into the same account.",
        { count: loginIds.length }
      )
    );
    if (!ok) return;
    await store.reloginAccounts(loginIds);
  }

  function selectFailed() {
    store.setSelectedIds(new Set(failedIds));
    handleClose();
  }

  return (
    <div
      data-testid="action-results-backdrop"
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm ${closing ? "animate-fade-out" : "animate-fade-in"}`}
      {...backdropClose}
    >
      <div
        data-testid="action-results-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="action-results-title"
        className={`theme-modal-scope theme-panel theme-border bg-zinc-900 border border-zinc-800/80 rounded-2xl shadow-2xl w-[540px] max-w-[calc(100vw-24px)] max-h-[min(640px,calc(100vh-24px))] flex flex-col overflow-hidden ${closing ? "animate-scale-out" : "animate-scale-in"}`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 pt-4 pb-3 shrink-0">
          <h2 id="action-results-title" className="text-sm font-semibold text-zinc-100">
            {report.title}
          </h2>
          <button
            onClick={handleClose}
            aria-label={t("Close")}
            className="text-zinc-600 hover:text-zinc-400 transition-colors"
          >
            <X size={16} strokeWidth={2} />
          </button>
        </div>

        {allRows.length > 0 && (
          <div role="group" aria-label={t("Filter by result")} className="flex flex-wrap gap-1.5 px-5 pb-3 shrink-0">
            <FilterChip active={filter === null} onClick={() => setFilter(null)} label={t("All")} count={allRows.length} />
            {ACTION_OUTCOME_ORDER.filter((o) => counts[o] > 0).map((o) => (
              <FilterChip
                key={o}
                active={filter === o}
                onClick={() => setFilter(filter === o ? null : o)}
                label={outcomeLabel(o)}
                count={counts[o]}
                Icon={ICONS[o]}
                tone={TONES[o]}
              />
            ))}
          </div>
        )}

        {report.summary && (
          <p data-testid="action-results-summary" className="px-5 pb-3 text-[12px] theme-muted shrink-0">
            {report.summary}
          </p>
        )}

        {allRows.length > 0 && (
          <ul className="flex-1 overflow-y-auto mx-5 mb-4 rounded-xl border theme-border divide-y theme-border">
            {rows.map((row) => {
              const Icon = ICONS[row.outcome];
              return (
                <li
                  key={row.userId}
                  data-testid={`action-row-${row.userId}`}
                  className="flex items-start gap-3 px-3 py-2 text-[12px]"
                >
                  <div className="flex-1 min-w-0">
                    <div className="text-zinc-200 truncate">{accountLabel(accountsById.get(row.userId), row.userId)}</div>
                    {row.detail && <div className="text-[11px] theme-muted whitespace-normal break-words">{row.detail}</div>}
                  </div>
                  <span
                    className={`shrink-0 inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium ${TONES[row.outcome]}`}
                  >
                    <Icon
                      size={11}
                      strokeWidth={2.5}
                      aria-hidden="true"
                      className={row.outcome === "running" ? "animate-spin" : undefined}
                    />
                    {outcomeLabel(row.outcome)}
                  </span>
                </li>
              );
            })}
          </ul>
        )}

        <div className="shrink-0 flex items-center justify-between gap-3 px-5 py-3 border-t theme-border">
          <span className="text-[12px] theme-muted min-h-[1em] flex items-center gap-1.5">
            {report.running && (
              <>
                <Loader2 size={12} className="animate-spin" aria-hidden="true" />
                {allRows.length > 0
                  ? t("Working {{done}}/{{total}}...", { done, total: allRows.length })
                  : t("Working...")}
              </>
            )}
          </span>
          <div className="flex items-center gap-2">
            {!report.running && loginIds.length > 0 && (
              <button
                onClick={() => void signInAgain()}
                className="px-3 py-1.5 rounded-lg text-[12px] theme-btn-ghost border theme-border"
              >
                {t("Sign in again ({{count}})", { count: loginIds.length })}
              </button>
            )}
            {!report.running && failedIds.length > 0 && (
              <button
                onClick={selectFailed}
                className="px-3 py-1.5 rounded-lg text-[12px] theme-btn-ghost border theme-border"
              >
                {t("Select the ones that failed")}
              </button>
            )}
            <button
              onClick={handleClose}
              className="px-3 py-1.5 rounded-lg text-[12px] font-medium bg-[var(--accent-color)] text-white"
            >
              {t("Close")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

/** O texto de cada resultado; "OK" e "já estava" dizem o que a ação fez. */
function okLabel(kind: ActionKind, outcome: ActionOutcome, t: ReturnType<typeof useTr>): string {
  switch (outcome) {
    case "login":
      return t("Needs sign-in");
    case "failed":
      return t("Failed");
    case "running":
      return t("Working");
    case "pending":
      return t("Waiting");
    case "skipped":
      return t("Skipped");
    case "already":
      return kind === "follow" ? t("Already following") : t("Already done");
    default:
      switch (kind) {
        case "refresh-cookies":
          return t("Refreshed");
        case "copy-cookies":
          return t("Copied");
        case "make-friends":
          return t("Linked");
        case "follow":
          return t("Followed");
        case "move-group":
          return t("Moved");
        case "restart-clients":
          return t("Restarting");
        case "add-auto-rejoin":
          return t("Added");
        case "close-all":
          return t("Closed");
        case "remove-accounts":
          return t("Removed");
        case "relogin":
          return t("Signed in");
        default:
          return t("OK");
      }
  }
}

function FilterChip({
  active,
  onClick,
  label,
  count,
  Icon,
  tone,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
  count: number;
  Icon?: LucideIcon;
  tone?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={`inline-flex items-center gap-1 rounded-full border px-2.5 py-1 text-[11px] font-medium transition-colors ${tone ?? "theme-border text-zinc-300"} ${active ? "ring-1 ring-[var(--accent-color)]" : "opacity-80 hover:opacity-100"}`}
    >
      {Icon && <Icon size={11} strokeWidth={2.5} aria-hidden="true" />}
      {label}
      <span className="tabular-nums">{count}</span>
    </button>
  );
}
