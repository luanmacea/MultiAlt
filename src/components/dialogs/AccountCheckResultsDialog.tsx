import { useEffect, useMemo, useRef, useState } from "react";
import { X, Ban, TriangleAlert, Check, CircleHelp, Loader2, type LucideIcon } from "lucide-react";
import { useStore } from "../../store";
import { useModalClose } from "../../hooks/useModalClose";
import { useBackdropClose } from "../../hooks/useBackdropClose";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { useConfirm } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";
import {
  buildCheckReport,
  mergeCheckResults,
  type AccountCheckOutcome,
} from "../../utils/accountCheck";
import { moderationLabel } from "../../utils/moderation";

const ICONS: Record<AccountCheckOutcome, LucideIcon> = {
  banned: Ban,
  warned: TriangleAlert,
  invalid: X,
  unknown: CircleHelp,
  ok: Check,
};

/** Cor de cada resultado. A pista que não depende dela é o ícone + o texto. */
const TONES: Record<AccountCheckOutcome, string> = {
  banned: "text-rose-300 bg-rose-500/15 border-rose-500/30",
  warned: "text-amber-300 bg-amber-500/15 border-amber-500/30",
  invalid: "text-red-300 bg-red-500/15 border-red-500/30",
  unknown: "text-zinc-300 bg-zinc-500/15 border-zinc-500/30",
  ok: "text-emerald-300 bg-emerald-500/15 border-emerald-500/30",
};

/** Ordem dos filtros: a mesma da lista (o que pede ação primeiro). */
const OUTCOMES: AccountCheckOutcome[] = ["banned", "warned", "invalid", "unknown", "ok"];

/**
 * Resultado do "Check accounts" em lote: QUAIS contas estão banidas, advertidas,
 * com a sessão inválida ou não deram para conferir (o toast de contagens só
 * dizia quantas). Os números do topo filtram a lista. "Select banned" deixa só
 * as banidas selecionadas; "Retry" confere de novo as que não deram para conferir
 * e junta a resposta ao resultado, sem perder o que já se sabia. Ver
 * docs/features/accounts.md.
 */
export function AccountCheckResultsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useTr();
  const store = useStore();
  const accountLabel = useAccountLabel();
  const confirm = useConfirm();
  const { visible, closing, handleClose } = useModalClose(open, onClose);
  const backdropClose = useBackdropClose(handleClose);

  // O store zera o relatório ao fechar; a animação de saída ainda precisa dele.
  const lastReport = useRef(store.accountCheckReport);
  if (store.accountCheckReport) lastReport.current = store.accountCheckReport;
  const report = store.accountCheckReport ?? lastReport.current;

  const [filter, setFilter] = useState<AccountCheckOutcome | null>(null);
  useEffect(() => {
    if (!open) setFilter(null);
  }, [open]);

  const built = useMemo(
    () => buildCheckReport(report?.results ?? [], store.moderationByUserId),
    [report, store.moderationByUserId]
  );
  const accountsById = useMemo(() => new Map(store.accounts.map((a) => [a.UserID, a])), [store.accounts]);

  if (!visible || !report) return null;

  const rows = filter ? built.rows.filter((r) => r.outcome === filter) : built.rows;
  const bannedIds = built.rows.filter((r) => r.outcome === "banned").map((r) => r.userId);
  const unknownIds = built.rows.filter((r) => r.outcome === "unknown").map((r) => r.userId);
  const invalidIds = built.rows.filter((r) => r.outcome === "invalid").map((r) => r.userId);
  const checking = store.accountCheckProgress !== null;

  const outcomeLabel = (outcome: AccountCheckOutcome): string => {
    switch (outcome) {
      case "banned":
        return t("Banned");
      case "warned":
        return t("Warned");
      case "invalid":
        return t("Invalid session");
      case "unknown":
        return t("Couldn't check");
      default:
        return t("OK");
    }
  };

  function selectBanned() {
    store.setSelectedIds(new Set(bannedIds));
    handleClose();
  }

  /** Sessão morta não se conserta com refresh: o caminho é logar de novo. */
  async function signInAgain() {
    if (invalidIds.length === 0 || checking) return;
    const ok = await confirm(
      t(
        "{{count}} account(s) need to sign in again. A browser window opens for each one, one at a time, with the username and password filled in when they are saved. You solve the CAPTCHA, and the new session goes into the same account.",
        { count: invalidIds.length }
      )
    );
    if (!ok) return;
    handleClose();
    await store.reloginAccounts(invalidIds);
  }

  async function retryUnchecked() {
    if (!report || unknownIds.length === 0 || checking) return;
    const summary = await store.checkAccounts(unknownIds);
    if (summary) {
      store.setAccountCheckReport({ results: mergeCheckResults(report.results, summary.results), at: Date.now() });
    }
  }

  return (
    <div
      data-testid="account-check-results-backdrop"
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm ${closing ? "animate-fade-out" : "animate-fade-in"}`}
      {...backdropClose}
    >
      <div
        data-testid="account-check-results-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="account-check-results-title"
        className={`theme-modal-scope theme-panel theme-border bg-zinc-900 border border-zinc-800/80 rounded-2xl shadow-2xl w-[540px] max-w-[calc(100vw-24px)] max-h-[min(640px,calc(100vh-24px))] flex flex-col overflow-hidden ${closing ? "animate-scale-out" : "animate-scale-in"}`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 pt-4 pb-3 shrink-0">
          <h2 id="account-check-results-title" className="text-sm font-semibold text-zinc-100">
            {t("Account check results")}
          </h2>
          <button
            onClick={handleClose}
            aria-label={t("Close")}
            className="text-zinc-600 hover:text-zinc-400 transition-colors"
          >
            <X size={16} strokeWidth={2} />
          </button>
        </div>

        <div role="group" aria-label={t("Filter by result")} className="flex flex-wrap gap-1.5 px-5 pb-3 shrink-0">
          <FilterChip
            active={filter === null}
            onClick={() => setFilter(null)}
            label={t("All")}
            count={built.rows.length}
          />
          {OUTCOMES.filter((o) => built.counts[o] > 0).map((o) => (
            <FilterChip
              key={o}
              active={filter === o}
              onClick={() => setFilter(filter === o ? null : o)}
              label={outcomeLabel(o)}
              count={built.counts[o]}
              Icon={ICONS[o]}
              tone={TONES[o]}
            />
          ))}
        </div>

        <ul className="flex-1 overflow-y-auto mx-5 mb-4 rounded-xl border theme-border divide-y theme-border">
          {rows.map((row) => {
            const Icon = ICONS[row.outcome];
            // "Warned" / "Banned" já está na pílula: a linha de detalhe só entra quando acrescenta algo.
            const label = row.moderation ? moderationLabel(row.moderation, t) : "";
            const detail = label === outcomeLabel(row.outcome) ? "" : label;
            const note = row.moderation?.note ? t("Moderator note: {{note}}", { note: row.moderation.note }) : "";
            return (
              <li
                key={row.userId}
                data-testid={`check-row-${row.userId}`}
                className="flex items-start gap-3 px-3 py-2 text-[12px]"
              >
                <div className="flex-1 min-w-0">
                  <div className="text-zinc-200 truncate">
                    {accountLabel(accountsById.get(row.userId), row.userId)}
                  </div>
                  {detail && <div className="text-[11px] theme-muted">{detail}</div>}
                  {note && <div className="text-[11px] theme-muted whitespace-normal break-words">{note}</div>}
                </div>
                <span
                  className={`shrink-0 inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium ${TONES[row.outcome]}`}
                >
                  <Icon size={11} strokeWidth={2.5} aria-hidden="true" />
                  {outcomeLabel(row.outcome)}
                </span>
              </li>
            );
          })}
        </ul>

        <div className="shrink-0 flex items-center justify-between gap-3 px-5 py-3 border-t theme-border">
          <span className="text-[12px] theme-muted min-h-[1em] flex items-center gap-1.5">
            {checking && store.accountCheckProgress && (
              <>
                <Loader2 size={12} className="animate-spin" aria-hidden="true" />
                {t("Checking {{done}}/{{total}}...", store.accountCheckProgress)}
              </>
            )}
          </span>
          <div className="flex items-center gap-2">
            {unknownIds.length > 0 && (
              <button
                onClick={() => void retryUnchecked()}
                disabled={checking}
                className="px-3 py-1.5 rounded-lg text-[12px] theme-btn-ghost border theme-border disabled:opacity-40 disabled:cursor-not-allowed"
              >
                {t("Retry {{count}} that couldn't be checked", { count: unknownIds.length })}
              </button>
            )}
            {invalidIds.length > 0 && (
              <button
                onClick={() => void signInAgain()}
                disabled={checking}
                className="px-3 py-1.5 rounded-lg text-[12px] theme-btn-ghost border theme-border disabled:opacity-40 disabled:cursor-not-allowed"
              >
                {t("Sign in again ({{count}})", { count: invalidIds.length })}
              </button>
            )}
            {bannedIds.length > 0 && (
              <button
                onClick={selectBanned}
                className="px-3 py-1.5 rounded-lg text-[12px] theme-btn-ghost border theme-border"
              >
                {t("Select banned")}
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
