/**
 * Dublê de `@tauri-apps/api/webview` para o harness de UI.
 *
 * O app só usa o zoom nativo (tamanho da interface, `hooks/useUiScale.ts`).
 * No navegador ele não faz nada — o harness mede o layout em 100% —, mas fica
 * registrado em `window.__harness.calls()` com o nome do comando real, para o
 * agente conferir qual fator a tela pediu.
 */
import { recordHarnessCall } from "./bus";

export function getCurrentWebview() {
  return {
    setZoom: async (factor: number) => {
      recordHarnessCall("plugin:webview|set_webview_zoom", { value: factor });
    },
  };
}
