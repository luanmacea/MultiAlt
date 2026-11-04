import { useMemo, useRef, useState } from "react";
import { X, Check, CircleHelp, Loader2, FileWarning, type LucideIcon } from "lucide-react";
import { useStore } from "../../store";
import { useModalClose } from "../../hooks/useModalClose";
import { useBackdropClose } from "../../hooks/useBackdropClose";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { useConfirm } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";
import { canAcceptByApi, termsLabel } from "../../utils/terms";
import type { TermsState } from "../../types";

type Kind = TermsState["kind"] | "waiting";

const ICONS: Record<Kind, LucideIcon> = {
  pending: FileWarning,
  error: CircleHelp,
  none: Check,
  waiting: Loader2,
};

const TONES: Record<Kind, string> = {
  pending: "text-amber-300 bg-amber-500/15 border-amber-500/30",
  error: "text-zinc-300 bg-zinc-500/15 border-zinc-500/30",
  none: "text-emerald-300 bg-emerald-500/15 border-emerald-500/30",
  waiting: "text-sky-300 bg-sky-500/15 border-sky-500/30",
};

const ORDER: Kind[] = ["pending", "error", "waiting", "none"];

/**
 * Resultado do "Check terms" em lote: quais contas têm o aviso "Acordos
 * atualizados" pendente. Cada uma com "Accept terms" (pela API do Roblox) e
 * "Open in browser" (a pessoa clica "Concordo" no site); "Accept all" aceita
 * todas as pendentes depois de uma confirmação. Ver docs/features/accounts.md
 * ("Termos pendentes").
 */
export function TermsCheckDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useTr();
  const store = useStore();
  const accountLabel = useAccountLabel();
  const confirm = useConfirm();
  const { visible, closing, handleClose } = useModalClose(open, onClose);
  const backdropClose = useBackdropClose(handleClose);

  // O store zera a lista ao fechar; a animação de saída ainda precisa dela.
  const lastIds = useRef(store.termsDialogIds);
  if (store.termsDialogIds) lastIds.current = store.termsDialogIds;
  const ids = store.termsDialogIds ?? lastIds.current;

  const [accepting, setAccepting] = useState<Set<number>>(new Set());
  const accountsById = useMemo(() => new Map(store.accounts.map((a) => [a.UserID, a])), [store.accounts]);

  if (!visible || !ids) return null;

  const rows = ids
    .map((userId) => {
      const state = store.termsByUserId.get(userId);
      return { userId, state, kind: (state?.kind ?? "waiting") as Kind };
    })
    .sort((a, b) => ORDER.indexOf(a.kind) - ORDER.indexOf(b.kind));
  const acceptable = rows.filter((r) => canAcceptByApi(r.state)).map((r) => r.userId);
  const answered = rows.filter((r) => r.state).length;
  const busy = accepting.size > 0;

  const kindLabel = (kind: Kind): string => {
    switch (kind) {
      case "pending":
        return t("Pending");
      case "error":
        return t("Couldn't check");
      case "waiting":
        return t("Checking...");
      default:
        return t("No pending terms");
    }
  };

  async function accept(userIds: number[]) {
    if (userIds.length === 0 || busy) return;
    const ok = await confirm(t("You accept Roblox's updated Terms for {{count}} account(s).", { count: userIds.length }));
    if (!ok) return;
    setAccepting(new Set(userIds));
    try {
      // Uma conta por vez: cada resposta do Roblox aparece no aviso dela.
      for (const userId of userIds) {
        await store.acceptTerms(userId);
        setAccepting((prev) => {
          const next = new Set(prev);
          next.delete(userId);
          return next;
        });
      }
    } finally {
      setAccepting(new Set());
    }
  }

  return (
    <div
      data-testid="terms-check-backdrop"
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm ${closing ? "animate-fade-out" : "animate-fade-in"}`}
      {...backdropClose}
    >
      <div
        data-testid="terms-check-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="terms-check-title"
        className={`theme-modal-scope theme-panel theme-border bg-zinc-900 border border-zinc-800/80 rounded-2xl shadow-2xl w-[540px] max-w-[calc(100vw-24px)] max-h-[min(640px,calc(100vh-24px))] flex flex-col overflow-hidden ${closing ? "animate-scale-out" : "animate-scale-in"}`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 pt-4 pb-3 shrink-0">
          <h2 id="terms-check-title" className="text-sm font-semibold text-zinc-100">
            {t("Terms check results")}
          </h2>
          <button
            onClick={handleClose}
            aria-label={t("Close")}
            className="text-zinc-600 hover:text-zinc-400 transition-colors"
          >
            <X size={16} strokeWidth={2} />
          </button>
        </div>

        <ul className="flex-1 overflow-y-auto mx-5 mb-4 rounded-xl border theme-border divide-y theme-border">
          {rows.map((row) => {
            const Icon = ICONS[row.kind];
            const working = accepting.has(row.userId);
            return (
              <li
                key={row.userId}
                data-testid={`terms-row-${row.userId}`}
                className="flex items-start gap-3 px-3 py-2 text-[12px]"
              >
                <div className="flex-1 min-w-0">
                  <div className="text-zinc-200 truncate">{accountLabel(accountsById.get(row.userId), row.userId)}</div>
                  {row.state && row.kind !== "none" && (
                    <div className="text-[11px] theme-muted whitespace-normal break-words">
                      {termsLabel(row.state, t)}
                    </div>
                  )}
                  {row.kind === "pending" && (
                    <div className="mt-1 flex items-center gap-2">
                      {canAcceptByApi(row.state) && (
                        <button
                          onClick={() => void accept([row.userId])}
                          disabled={busy}
                          className="px-2 py-0.5 rounded-md text-[11px] theme-btn-ghost border theme-border disabled:opacity-40"
                        >
                          {working ? t("Accepting...") : t("Accept terms")}
                        </button>
                      )}
                      <button
                        onClick={() => void store.openAccountBrowser(row.userId)}
                        className="px-2 py-0.5 rounded-md text-[11px] theme-btn-ghost border theme-border"
                        title={t("Opens this account's browser so you can click \"I agree\" yourself.")}
                      >
                        {t("Open in browser")}
                      </button>
                    </div>
                  )}
                </div>
                <span
                  className={`shrink-0 inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium ${TONES[row.kind]}`}
                >
                  <Icon
                    size={11}
                    strokeWidth={2.5}
                    aria-hidden="true"
                    className={row.kind === "waiting" ? "animate-spin" : undefined}
                  />
                  {kindLabel(row.kind)}
                </span>
              </li>
            );
          })}
        </ul>

        <div className="shrink-0 flex items-center justify-between gap-3 px-5 py-3 border-t theme-border">
          <span className="text-[12px] theme-muted min-h-[1em] flex items-center gap-1.5">
            {store.termsChecking && (
              <>
                <Loader2 size={12} className="animate-spin" aria-hidden="true" />
                {t("Checking {{done}}/{{total}}...", { done: answered, total: rows.length })}
              </>
            )}
          </span>
          <div className="flex items-center gap-2">
            {acceptable.length > 0 && (
              <button
                onClick={() => void accept(acceptable)}
                disabled={busy || store.termsChecking}
                className="px-3 py-1.5 rounded-lg text-[12px] theme-btn-ghost border theme-border disabled:opacity-40 disabled:cursor-not-allowed"
              >
                {t("Accept all ({{count}})", { count: acceptable.length })}
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
