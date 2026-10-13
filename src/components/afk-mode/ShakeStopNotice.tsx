import { X } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";
import { useRecordings } from "./recordings/useRecordings";

/**
 * A linha da parada de emergência por chacoalhar o mouse (`Afk.ShakeToStop`,
 * desligada por padrão — commands/shake_stop.rs) na aba Recordings.
 *
 * Mora fora da aba de propósito: a gravação tocando leva o cursor para a janela
 * de cada conta, e o usuário não alcança o Parar dela. Esta linha diz que
 * chacoalhar o mouse para (com a opção ligada e algo tocando) e, depois, que
 * foi isso que parou. A aba de cliques mostra o mesmo na barra dela.
 */
export function ShakeStopNotice() {
  const t = useTr();
  const store = useStore();
  const { playing } = useRecordings();
  const shakeToStop = store.settings?.Afk?.ShakeToStop === "true";
  const afkRecording = store.afkStatus?.active === true && store.afkStatus?.mode === "recording";
  const stopped = store.afkEmergencyStop;

  if (stopped) {
    return (
      <div
        data-testid="afk-shake-notice"
        role="alert"
        className="flex shrink-0 items-center gap-2 rounded-lg border border-amber-400/35 bg-amber-500/10 px-3 py-2 text-[12px] text-amber-200"
      >
        <span className="min-w-0 flex-1">{t("Emergency stop: you shook the mouse.")}</span>
        <button
          type="button"
          onClick={store.dismissAfkEmergencyStop}
          aria-label={t("Dismiss")}
          title={t("Dismiss")}
          className="shrink-0 rounded p-0.5 hover:bg-amber-500/15"
        >
          <X size={14} strokeWidth={2} />
        </button>
      </div>
    );
  }
  if (shakeToStop && (playing || afkRecording)) {
    return (
      <div data-testid="afk-shake-notice" role="status" className="shrink-0 px-1 text-[12px] theme-muted">
        {t("Shake the mouse to stop.")}
      </div>
    );
  }
  return null;
}
