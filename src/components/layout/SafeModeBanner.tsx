import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { WebviewSafeModeState } from "../../types";
import { useTr } from "../../i18n/text";

/**
 * Faixa do safe mode de vídeo do WebView2 (ver
 * `src-tauri/src/webview_recovery.rs` e `docs/features/webview-recovery.md`).
 *
 * Sem ela o modo degradado é invisível e não tem saída: o app liga o safe mode
 * para se recuperar de uma tela em branco, passa a pintar, o watchdog nunca
 * dispara de novo — e a pessoa fica com a GPU desligada até o runtime do
 * WebView2 mudar, o que pode nunca acontecer se a atualização automática estiver
 * desligada. A única saída seria achar e apagar `webview.safemode` à mão.
 *
 * O botão só aparece quando há marcador (`sticky`): safe mode que veio do Shift,
 * de `--safe-mode` ou da variável de ambiente não deixou nada no disco e acaba
 * sozinho na próxima abertura.
 */
export function SafeModeBanner() {
  const t = useTr();
  const [state, setState] = useState<WebviewSafeModeState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [leaving, setLeaving] = useState(false);

  useEffect(() => {
    let disposed = false;
    void invoke<WebviewSafeModeState | undefined>("get_webview_safe_mode")
      .then((next) => {
        if (!disposed && next) setState(next);
      })
      .catch(() => {});
    return () => {
      disposed = true;
    };
  }, []);

  if (!state || (!state.active && !state.sticky)) return null;

  // O comando reinicia o app quando dá certo, então só o caminho de erro volta
  // para cá.
  const leave = async () => {
    setLeaving(true);
    setError(null);
    try {
      await invoke("leave_webview_safe_mode");
    } catch (e) {
      setError(String(e));
      setLeaving(false);
    }
  };

  const message = state.active
    ? t("Graphics safe mode is on")
    : t("The next time RAM opens it will use graphics safe mode");

  return (
    <div className="flex items-center justify-between gap-3 px-4 py-1.5 bg-amber-600/15 border-b border-amber-500/20 shrink-0">
      <span className="text-xs text-amber-300 truncate">{error ?? message}</span>
      {state.sticky && (
        <button
          onClick={leave}
          disabled={leaving}
          className="text-xs font-medium text-amber-400 hover:text-amber-300 transition-colors disabled:opacity-50 shrink-0"
        >
          {t("Back to normal mode")}
        </button>
      )}
    </div>
  );
}
