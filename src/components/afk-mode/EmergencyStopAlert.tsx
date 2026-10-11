import { useEffect, useRef } from "react";
import { Hand } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { useEscapeStack } from "../../hooks/useEscapeStack";

/**
 * O alerta da parada de emergência (chacoalhar o mouse, `Afk.ShakeToStop` —
 * commands/shake_stop.rs). Fica no meio da janela, em qualquer página, até a
 * pessoa fechar: o toast de antes sumia em um segundo e a linha amarela da aba
 * passava batido (pedido do dono, 11/10/2026). Fechar aqui limpa também o aviso
 * das abas do Modo AFK (`dismissAfkEmergencyStop`).
 */
export function EmergencyStopAlert() {
  const t = useTr();
  const store = useStore();
  const stop = store.afkEmergencyStop;
  const okRef = useRef<HTMLButtonElement>(null);
  const open = !!stop;

  useEscapeStack(open, store.dismissAfkEmergencyStop);
  useEffect(() => {
    if (open) okRef.current?.focus();
  }, [open]);

  if (!stop) return null;

  const what =
    stop.afk && stop.recordings
      ? t("You shook the mouse, so MultiAlt stopped AFK mode and the recording.")
      : stop.recordings
        ? t("You shook the mouse, so MultiAlt stopped the recording.")
        : t("You shook the mouse, so MultiAlt stopped AFK mode.");

  return (
    <div className="fixed inset-0 z-[90] flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in">
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="emergency-stop-title"
        aria-describedby="emergency-stop-text"
        className="theme-modal-scope theme-panel border border-amber-500/40 rounded-2xl shadow-2xl w-[420px] max-w-[calc(100vw-24px)] animate-scale-in"
      >
        <div className="flex items-start gap-3 px-5 pt-5">
          <span
            aria-hidden="true"
            className="shrink-0 flex h-10 w-10 items-center justify-center rounded-full bg-amber-500/15 text-amber-400"
          >
            <Hand size={20} strokeWidth={1.8} />
          </span>
          <div className="min-w-0 space-y-1.5">
            <h2 id="emergency-stop-title" className="text-[15px] font-semibold text-[var(--panel-fg)]">
              {t("Emergency stop")}
            </h2>
            <div id="emergency-stop-text" className="space-y-1 text-[13px] leading-5 text-[var(--panel-fg)]">
              <p>{what}</p>
              <p className="theme-muted">{t("No Roblox client was closed.")}</p>
            </div>
          </div>
        </div>
        <div className="flex justify-end px-5 py-4">
          <button
            ref={okRef}
            type="button"
            onClick={store.dismissAfkEmergencyStop}
            className="theme-btn inline-flex items-center min-h-[30px] px-5 py-1.5 text-[13px] font-medium"
          >
            {t("OK")}
          </button>
        </div>
      </div>
    </div>
  );
}
