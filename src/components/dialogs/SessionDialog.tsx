import { Gamepad2, X } from "lucide-react";
import { useModalClose } from "../../hooks/useModalClose";
import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { Tooltip } from "../ui/Tooltip";
import { SessionPanel } from "../session/SessionPanel";

/**
 * Diálogo do Painel de Sessão, aberto pelo botão da barra principal — o mesmo
 * painel da aba Console, disponível a qualquer momento (a Choose Game pode
 * estar fechada e ainda assim o usuário precisa achar/fechar uma conta).
 */

interface SessionDialogProps {
  open: boolean;
  onClose: () => void;
}

export function SessionDialog({ open, onClose }: SessionDialogProps) {
  const t = useTr();
  const { visible, closing, handleClose } = useModalClose(open, onClose);

  if (!visible) return null;

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm ${
        closing ? "animate-fade-out" : "animate-fade-in"
      }`}
      onClick={handleClose}
    >
      <div
        role="dialog"
        aria-label={t("Session")}
        className={`theme-modal-scope theme-panel theme-border rounded-2xl shadow-2xl w-[560px] max-h-[80vh] flex flex-col overflow-hidden ${
          closing ? "animate-scale-out" : "animate-scale-in"
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 pt-4 pb-3 shrink-0">
          <div className="min-w-0">
            <h2 className="text-[15px] font-semibold text-[var(--panel-fg)] tracking-tight">
              {t("Session")}
            </h2>
            <p className="text-[12px] theme-muted mt-0.5">
              {t("Follow what is running now: accounts joining, Make Friends in progress, and clients already in game.")}
            </p>
          </div>
          <button
            onClick={handleClose}
            className="p-1 rounded-md theme-muted hover:text-[var(--panel-fg)] transition-colors"
            title={t("Close")}
            aria-label={t("Close")}
          >
            <X size={16} strokeWidth={2} />
          </button>
        </div>

        <div className="h-px bg-[var(--border-color)] mx-5" />

        <div className="flex-1 overflow-y-auto px-5 py-4 min-h-0">
          <SessionPanel />
        </div>
      </div>
    </div>
  );
}

/**
 * Botão da barra principal. O contador só aparece quando há cliente rodando,
 * para a barra não ganhar um "0" permanente.
 */
export function SessionToolbarButton() {
  const t = useTr();
  const store = useStore();
  const running = store.launchedByProgram.size;

  return (
    <Tooltip content={t("Session")} side="bottom">
      <button
        onClick={() => store.setSessionDialogOpen(true)}
        aria-label={t("Session")}
        className="theme-btn-ghost relative p-1.5 rounded-lg transition-colors"
      >
        <Gamepad2 size={16} strokeWidth={1.5} />
        {running > 0 && (
          <span
            data-testid="session-button-count"
            className="absolute -top-1 -right-1 min-w-[15px] h-[15px] px-1 rounded-full bg-[var(--accent-color)] text-[11px] font-semibold leading-[15px] text-center text-black"
          >
            {running}
          </span>
        )}
      </button>
    </Tooltip>
  );
}
