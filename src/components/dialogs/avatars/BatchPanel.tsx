import { Loader2 } from "lucide-react";
import { useTr } from "../../../i18n/text";
import { Headshot, type AvatarAccountResult, type AvatarBatchSnapshot } from "./shared";

/** Progresso do lote e, no fim, o resultado de cada conta. */
export function BatchPanel({
  batch,
  cancelling,
  onCancel,
  onDismiss,
  accountName,
  avatarName,
  headshot,
}: {
  batch: AvatarBatchSnapshot;
  cancelling: boolean;
  onCancel: () => void;
  onDismiss: () => void;
  accountName: (userId: number) => string;
  avatarName: (avatarId: string) => string | null;
  headshot: (userId: number) => string | undefined;
}) {
  const t = useTr();
  const total = Math.max(batch.total, batch.done);
  const percent = total > 0 ? Math.round((batch.done / total) * 100) : 0;

  function chip(result: AvatarAccountResult): { text: string; tone: string } {
    if (result.status === "ok") {
      return { text: t("Applied"), tone: "border-emerald-500/30 bg-emerald-500/15 text-emerald-300" };
    }
    if (result.status === "skipped") {
      if (result.reason === "challenge") {
        return {
          text: t("Verification required — skipped"),
          tone: "border-amber-500/30 bg-amber-500/15 text-amber-300",
        };
      }
      if (result.reason === "cancelled") {
        return { text: t("Cancelled"), tone: "theme-border bg-[var(--panel-soft)] theme-muted" };
      }
      return { text: t("Skipped"), tone: "border-amber-500/30 bg-amber-500/15 text-amber-300" };
    }
    return { text: t("Failed"), tone: "border-red-500/30 bg-red-500/15 text-red-300" };
  }

  function reasonText(result: AvatarAccountResult): string | null {
    if (!result.reason || result.reason === "challenge" || result.reason === "cancelled") return null;
    if (result.reason === "nothing to wear") return t("None of the items could be claimed or worn");
    return result.reason;
  }

  const ok = batch.accounts.filter((a) => a.status === "ok").length;

  return (
    <>
      <div className="px-3 py-2.5 border-b theme-border shrink-0 space-y-2">
        <div className="flex items-center justify-between gap-2">
          <div className="text-[12.5px] font-semibold text-[var(--panel-fg)]">
            {batch.running ? t("Applying avatars") : t("Finished")}
          </div>
          <div className="text-[11px] theme-muted tabular-nums">
            {t("{{done}} of {{total}}", { done: batch.done, total })}
          </div>
        </div>
        <div
          role="progressbar"
          aria-label={t("Applying avatars")}
          aria-valuemin={0}
          aria-valuemax={total}
          aria-valuenow={batch.done}
          className="h-2 rounded-full bg-[var(--panel-soft)] overflow-hidden"
        >
          <div
            className={`h-full rounded-full transition-[width] duration-500 ${
              batch.running ? "bg-[var(--accent-color)]" : "bg-emerald-500"
            }`}
            style={{ width: `${percent}%` }}
          />
        </div>
        {batch.running ? (
          <div className="flex items-center gap-2 min-h-[24px]">
            {batch.currentUserId !== null ? (
              <>
                <Headshot url={headshot(batch.currentUserId)} size={22} />
                <span className="flex-1 min-w-0 text-[11.5px] text-[var(--panel-fg)] truncate">
                  {t("Now: {{name}}", { name: accountName(batch.currentUserId) })}
                </span>
              </>
            ) : (
              <span className="flex-1 text-[11.5px] theme-muted">{t("Starting...")}</span>
            )}
            <Loader2 size={14} className="animate-spin theme-accent shrink-0" />
          </div>
        ) : (
          <div className="text-[11.5px] theme-muted">
            {t("{{ok}} of {{total}} accounts got their avatar", { ok, total })}
          </div>
        )}
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto p-2 space-y-1">
        {batch.accounts.length === 0 ? (
          <div className="text-[11px] theme-muted px-1.5 py-2">{t("Results appear here as each account finishes.")}</div>
        ) : (
          batch.accounts.map((result, index) => {
            const status = chip(result);
            const reason = reasonText(result);
            const avatar = avatarName(result.avatarId);
            return (
              <div
                key={`${result.userId}-${index}`}
                className="rounded-lg border theme-border px-2 py-1.5 animate-fade-in-up"
              >
                <div className="flex items-center gap-2">
                  <Headshot url={headshot(result.userId)} size={24} />
                  <div className="flex-1 min-w-0">
                    <div className="text-[12px] text-[var(--panel-fg)] truncate">{accountName(result.userId)}</div>
                    {avatar ? <div className="text-[11px] theme-muted truncate">{avatar}</div> : null}
                  </div>
                  <span className={`shrink-0 px-1.5 py-0.5 rounded-full border text-[11px] ${status.tone}`}>
                    {status.text}
                  </span>
                </div>
                {result.claimed > 0 || result.missing > 0 || reason ? (
                  <div className="mt-1 pl-8 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px]">
                    {result.claimed > 0 ? (
                      <span className="text-emerald-400/90">{t("{{count}} claimed", { count: result.claimed })}</span>
                    ) : null}
                    {result.missing > 0 ? (
                      <span className="text-amber-400/90">{t("{{count}} missing", { count: result.missing })}</span>
                    ) : null}
                    {reason ? <span className="theme-muted break-words">{reason}</span> : null}
                  </div>
                ) : null}
              </div>
            );
          })
        )}
      </div>

      <div className="p-3 border-t theme-border shrink-0">
        {batch.running ? (
          <button
            onClick={onCancel}
            disabled={cancelling}
            aria-label={t("Cancel")}
            className="sidebar-btn-sm w-full text-red-300 border-red-400/40 hover:bg-red-500/15 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {cancelling ? t("Cancelling — finishing the current item...") : t("Cancel")}
          </button>
        ) : (
          <button onClick={onDismiss} className="sidebar-btn-sm w-full">
            {t("Clear results")}
          </button>
        )}
      </div>
    </>
  );
}
