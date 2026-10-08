import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Bug, Lightbulb, X } from "lucide-react";
import { useBackdropClose } from "../../hooks/useBackdropClose";
import { useTr } from "../../i18n/text";

export type FeedbackKind = "bug" | "idea";

interface FeedbackDialogProps {
  open: boolean;
  onClose: () => void;
}

/**
 * Reportar problema ou sugerir ideia. O app **não envia nada**: abre o
 * formulário do GitHub no navegador (`open_feedback_form`, com os modelos de
 * `.github/ISSUE_TEMPLATE/`), e quem escreve e envia é a pessoa. Por isso o
 * texto diz que precisa de conta no GitHub e que nada sai do app sozinho.
 */
export function FeedbackDialog({ open, onClose }: FeedbackDialogProps) {
  const t = useTr();
  const backdropClose = useBackdropClose(onClose);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  function openForm(kind: FeedbackKind) {
    onClose();
    void invoke("open_feedback_form", { kind }).catch(() => {});
  }

  const option =
    "flex items-start gap-3 w-full px-3 py-3 text-left rounded-lg border theme-border hover:bg-[var(--panel-soft)] transition-colors outline-none focus-visible:shadow-[0_0_0_2px_var(--input-focus)]";

  return (
    <div
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in"
      {...backdropClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("Send feedback")}
        className="theme-modal-scope theme-panel theme-border border rounded-2xl shadow-2xl w-[440px] max-w-[calc(100vw-24px)] max-h-[calc(100vh-24px)] overflow-y-auto animate-scale-in"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-4 py-3 border-b theme-border">
          <h2 className="text-sm font-semibold text-[var(--panel-fg)]">{t("Send feedback")}</h2>
          <button onClick={onClose} className="theme-muted hover:opacity-100 transition-opacity" aria-label={t("Close")}>
            <X size={16} strokeWidth={2} />
          </button>
        </div>

        <div className="px-4 py-3 space-y-2">
          <button type="button" onClick={() => openForm("bug")} className={option}>
            <Bug size={16} strokeWidth={1.75} className="mt-0.5 shrink-0 theme-muted" />
            <span className="min-w-0">
              <span className="block text-[13px] font-medium text-[var(--panel-fg)]">{t("Report a problem")}</span>
              <span className="block mt-0.5 text-[12px] theme-muted">{t("Something broke or did not work as expected.")}</span>
            </span>
          </button>
          <button type="button" onClick={() => openForm("idea")} className={option}>
            <Lightbulb size={16} strokeWidth={1.75} className="mt-0.5 shrink-0 theme-muted" />
            <span className="min-w-0">
              <span className="block text-[13px] font-medium text-[var(--panel-fg)]">{t("Suggest an idea")}</span>
              <span className="block mt-0.5 text-[12px] theme-muted">{t("Something you would like MultiAlt to do.")}</span>
            </span>
          </button>
          <p className="pt-1 text-[11.5px] leading-relaxed theme-muted">
            {t(
              "Opens a form on this project's GitHub page in your browser. You need a free GitHub account to send it. The app sends nothing on its own, and nothing about your accounts goes along."
            )}
          </p>
        </div>
      </div>
    </div>
  );
}
