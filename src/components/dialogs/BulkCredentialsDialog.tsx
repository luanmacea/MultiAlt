import { useMemo, useRef, useState } from "react";
import { X, AlertTriangle, Check, SkipForward, Loader2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import { useModalClose } from "../../hooks/useModalClose";
import { useBackdropClose } from "../../hooks/useBackdropClose";
import { useConfirm } from "../../hooks/usePrompt";
import { useAccountLabel } from "../../hooks/useAccountLabel";
import { useTr } from "../../i18n/text";
import { credentialErrorText, isRetryLater } from "../../utils/credentialErrors";
import {
  MIN_ROBLOX_PASSWORD,
  planBulkCredentials,
  runnableCount,
  validateBulkRequest,
  type BulkCredentialsRequest,
} from "../../utils/bulkCredentials";

type RowState =
  | { kind: "pending" }
  | { kind: "running" }
  | { kind: "done" }
  | { kind: "skipped" }
  /** `passwordDone`: a senha já trocou e só o e-mail falhou — tentar de novo
   *  não pode trocar a senha outra vez. */
  | { kind: "error"; message: string; passwordDone: boolean };

/**
 * Pausa entre uma conta e a próxima. Trocas seguidas demais fazem o Roblox
 * recusar por um tempo (relato do dono, 15/10/2026: umas davam certo e outras
 * não). Exportada para o teste encurtar.
 */
export const BULK_CREDENTIALS_GAP_MS = { value: 4_000 };

/**
 * Trocar senha e/ou e-mail de VÁRIAS contas de uma vez (menu Ações, issue #30).
 * Uma conta por vez, pelos comandos `change_password`/`change_email` que já
 * existem. A senha atual de cada conta vem da senha guardada ou de uma senha
 * compartilhada digitada aqui; conta sem nenhuma é pulada. Ver
 * utils/bulkCredentials.ts e docs/features/accounts.md.
 */
export function BulkCredentialsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useTr();
  const store = useStore();
  const confirm = useConfirm();
  const accountLabel = useAccountLabel();
  const { visible, closing, handleClose } = useModalClose(open, onClose);
  const backdropClose = useBackdropClose(handleClose);

  const accounts = store.selectedAccounts;

  const [changePassword, setChangePassword] = useState(false);
  const [newPassword, setNewPassword] = useState("");
  const [randomPassword, setRandomPassword] = useState(false);
  // A senha aleatória que cada conta já recebeu nesta janela: o e-mail pede a
  // senha da conta, e o "Retry failed" de um e-mail não troca a senha de novo.
  const randomGiven = useRef(new Map<number, string>());
  const [changeEmail, setChangeEmail] = useState(false);
  const [newEmail, setNewEmail] = useState("");
  const [sharedCurrentPassword, setSharedCurrentPassword] = useState("");
  const [running, setRunning] = useState(false);
  const [rows, setRows] = useState<Map<number, RowState>>(new Map());

  const req: BulkCredentialsRequest = {
    changePassword,
    newPassword,
    changeEmail,
    newEmail,
    sharedCurrentPassword,
    randomPassword,
  };

  const plan = useMemo(
    () => planBulkCredentials(accounts, req),
    // os campos que mudam o plano:
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [accounts, sharedCurrentPassword]
  );
  const willRun = runnableCount(plan);
  const skipped = plan.length - willRun;
  const validationError = validateBulkRequest(req);

  const errorText = (e: ReturnType<typeof validateBulkRequest>): string => {
    if (!e) return "";
    if (e.kind === "nothing-selected") return t("Choose the password, the email, or both.");
    if (e.kind === "weak-password") return t("The new password needs at least {{min}} characters.", { min: e.min });
    return t("That does not look like an email address.");
  };

  async function run(onlyRetry = false) {
    if (validationError || willRun === 0 || running) return;
    const retryIds = onlyRetry
      ? new Set([...rows.entries()].filter(([, r]) => r.kind === "error").map(([id]) => id))
      : null;
    const count = retryIds ? retryIds.size : willRun;
    if (count === 0) return;
    const summary = [changePassword && t("password"), changeEmail && t("email")].filter(Boolean).join(` ${t("and")} `);
    const confirmed = await confirm(
      t(
        "Change the {{what}} on {{count}} real Roblox account(s)? This changes the live accounts, not just this app, and cannot be undone from here. Roblox may ask some accounts to verify — those are reported as failed.",
        { what: summary, count }
      ),
      true
    );
    if (!confirmed) return;

    setRunning(true);
    const previous = rows;
    const next = new Map<number, RowState>(retryIds ? previous : []);
    if (!retryIds) {
      for (const step of plan) next.set(step.userId, { kind: step.skip ? "skipped" : "pending" });
    }
    setRows(new Map(next));

    let first = true;
    for (const step of plan) {
      if (step.skip || step.currentPassword === null) continue;
      if (retryIds && !retryIds.has(step.userId)) continue;
      // Pausa entre contas: trocas seguidas demais o Roblox recusa por um tempo.
      if (!first) await new Promise((r) => setTimeout(r, BULK_CREDENTIALS_GAP_MS.value));
      first = false;

      const before = previous.get(step.userId);
      let passwordDone = before?.kind === "error" && before.passwordDone;
      next.set(step.userId, { kind: "running" });
      setRows(new Map(next));
      try {
        if (changePassword && !passwordDone) {
          if (randomPassword) {
            // Quem gera e grava é o backend (antes de ir ao Roblox, para a
            // senha nunca se perder); volta só para o e-mail usar.
            const given = await invoke<string>("change_password_random", {
              userId: step.userId,
              currentPassword: step.currentPassword,
            });
            randomGiven.current.set(step.userId, given);
          } else {
            await invoke("change_password", {
              userId: step.userId,
              currentPassword: step.currentPassword,
              newPassword,
            });
          }
          passwordDone = true;
        }
        if (changeEmail) {
          // O e-mail pede a senha da conta: a nova, se ela já trocou.
          const newForThisAccount = randomPassword ? randomGiven.current.get(step.userId) : newPassword;
          const passForEmail = changePassword && newForThisAccount ? newForThisAccount : step.currentPassword;
          await invoke("change_email", { userId: step.userId, password: passForEmail, newEmail });
        }
        next.set(step.userId, { kind: "done" });
      } catch (e) {
        next.set(step.userId, { kind: "error", message: String(e), passwordDone });
      }
      setRows(new Map(next));
    }
    setRunning(false);
    await store.loadAccounts();
    const ok = [...next.values()].filter((r) => r.kind === "done").length;
    store.addToast(t("Changed {{ok}} of {{total}} account(s)", { ok, total: willRun }));
  }

  const failedCount = [...rows.values()].filter((r) => r.kind === "error").length;
  const laterCount = [...rows.values()].filter((r) => r.kind === "error" && isRetryLater(r.message)).length;

  if (!visible) return null;

  const rowIcon = (state: RowState | undefined) => {
    switch (state?.kind) {
      case "running":
        return <Loader2 size={13} className="animate-spin text-[var(--accent-color)]" />;
      case "done":
        return <Check size={13} className="text-emerald-400" />;
      case "skipped":
        return <SkipForward size={13} className="text-zinc-500" />;
      case "error":
        return <AlertTriangle size={13} className="text-red-400" />;
      default:
        return <span className="w-[13px]" />;
    }
  };

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm ${closing ? "animate-fade-out" : "animate-fade-in"}`}
      {...backdropClose}
    >
      <div
        data-testid="bulk-credentials-dialog"
        className={`theme-modal-scope theme-panel theme-border bg-zinc-900 border border-zinc-800/80 rounded-2xl shadow-2xl w-[540px] max-w-[calc(100vw-24px)] max-h-[min(640px,calc(100vh-24px))] flex flex-col overflow-hidden ${closing ? "animate-scale-out" : "animate-scale-in"}`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 pt-4 pb-3 shrink-0">
          <h2 className="text-sm font-semibold text-zinc-100">
            {t("Change password / email for {{count}} accounts", { count: accounts.length })}
          </h2>
          <button onClick={handleClose} className="text-zinc-600 hover:text-zinc-400 transition-colors">
            <X size={16} strokeWidth={2} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 pb-5 space-y-4">
          <div className="flex items-start gap-2 rounded-xl border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-[12px] text-amber-200/90">
            <AlertTriangle size={14} strokeWidth={1.8} className="shrink-0 mt-[1px]" />
            <p>
              {t(
                "This changes the real Roblox accounts. Using the same password on every account is less safe, and Roblox may ask some accounts to verify."
              )}
            </p>
          </div>

          {/* Senha */}
          <label className="flex items-center gap-2 text-[13px] text-zinc-200">
            <input type="checkbox" checked={changePassword} onChange={(e) => setChangePassword(e.target.checked)} />
            {t("Change the password")}
          </label>
          {changePassword && (
            <label className="flex items-start gap-2 pl-5 text-[13px] text-zinc-200">
              <input
                type="checkbox"
                className="mt-[3px]"
                aria-label={t("A different random password for each account")}
                checked={randomPassword}
                onChange={(e) => setRandomPassword(e.target.checked)}
              />
              <span>
                {t("A different random password for each account")}
                <span className="block text-[11px] theme-muted">
                  {t(
                    "Safer: 16 random letters and digits per account, saved in MultiAlt before Roblox is asked. Copy them from each account later."
                  )}
                </span>
              </span>
            </label>
          )}
          {changePassword && !randomPassword && (
            <input
              type="text"
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
              placeholder={t("New password (at least {{min}} characters)", { min: MIN_ROBLOX_PASSWORD })}
              className="sidebar-input w-full text-[13px]"
            />
          )}

          {/* E-mail */}
          <label className="flex items-center gap-2 text-[13px] text-zinc-200">
            <input type="checkbox" checked={changeEmail} onChange={(e) => setChangeEmail(e.target.checked)} />
            {t("Change the email")}
          </label>
          {changeEmail && (
            <input
              type="email"
              value={newEmail}
              onChange={(e) => setNewEmail(e.target.value)}
              placeholder={t("New email for every account")}
              className="sidebar-input w-full text-[13px]"
            />
          )}

          {/* Senha atual compartilhada */}
          <div>
            <div className="text-[12px] text-zinc-400 mb-1">
              {t("Current password (used for accounts that don't have one saved)")}
            </div>
            <input
              type="text"
              value={sharedCurrentPassword}
              onChange={(e) => setSharedCurrentPassword(e.target.value)}
              placeholder={t("Leave empty to change only accounts with a saved password")}
              className="sidebar-input w-full text-[13px]"
            />
            <p className="mt-1 text-[11px] theme-muted">
              {skipped > 0
                ? t("{{willRun}} will change, {{skipped}} will be skipped (no current password).", {
                    willRun,
                    skipped,
                  })
                : t("All {{willRun}} selected accounts can change.", { willRun })}
            </p>
          </div>

          {/* Resultado por conta */}
          {rows.size > 0 && (
            <div className="rounded-xl border theme-border divide-y theme-border">
              {plan.map((step) => {
                const state = rows.get(step.userId);
                const account = accounts.find((a) => a.UserID === step.userId);
                return (
                  <div
                    key={step.userId}
                    data-testid={`bulk-cred-row-${step.userId}`}
                    className="flex items-center gap-2 px-3 py-1.5 text-[12px]"
                  >
                    {rowIcon(state)}
                    <span className="text-zinc-300 truncate flex-1">{accountLabel(account, step.userId)}</span>
                    <span
                      className={`text-[11px] max-w-[60%] text-right ${state?.kind === "error" ? "text-red-300/90 whitespace-normal leading-snug" : "theme-muted truncate"}`}
                      title={state?.kind === "error" ? state.message : undefined}
                    >
                      {state?.kind === "error"
                        ? credentialErrorText(state.message, t)
                        : state?.kind === "skipped"
                          ? t("skipped — no current password")
                          : state?.kind === "done"
                            ? t("done")
                            : state?.kind === "running"
                              ? t("changing…")
                              : ""}
                    </span>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        <div className="shrink-0 flex items-center justify-between gap-3 px-5 py-3 border-t theme-border">
          <span className="text-[12px] text-red-400 min-h-[1em]">
            {errorText(validationError) ||
              (laterCount > 0 && !running
                ? t("{{count}} were blocked for now by Roblox. Wait a few minutes, then use Retry failed.", {
                    count: laterCount,
                  })
                : "")}
          </span>
          <div className="flex items-center gap-2">
            <button
              onClick={handleClose}
              className="px-3 py-1.5 rounded-lg text-[12px] theme-btn-ghost border theme-border"
            >
              {t("Close")}
            </button>
            {failedCount > 0 && !running && (
              <button
                onClick={() => void run(true)}
                data-testid="bulk-cred-retry"
                className="px-3 py-1.5 rounded-lg text-[12px] theme-btn-ghost border theme-border"
              >
                {t("Retry failed ({{count}})", { count: failedCount })}
              </button>
            )}
            <button
              onClick={() => void run()}
              disabled={!!validationError || willRun === 0 || running}
              className="px-3 py-1.5 rounded-lg text-[12px] font-medium bg-[var(--accent-color)] text-white disabled:opacity-40 disabled:cursor-not-allowed"
            >
              {running ? t("Changing…") : t("Change {{count}} account(s)", { count: willRun })}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
