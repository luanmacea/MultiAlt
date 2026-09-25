/**
 * Dublê de `@tauri-apps/api/window` para o harness de UI.
 *
 * A janela nativa não existe no navegador: minimizar/fechar viram no-ops, e o
 * que a UI pergunta (maximizado? focado?) responde o mais neutro possível.
 */
export function getCurrentWindow() {
  const noop = async () => {};
  return {
    minimize: noop,
    maximize: noop,
    unmaximize: noop,
    toggleMaximize: noop,
    close: noop,
    startDragging: noop,
    setAlwaysOnTop: noop,
    isMaximized: async () => false,
    isFocused: async () => true,
    onResized: async () => () => {},
    onFocusChanged: async () => () => {},
    listen: async () => () => {},
  };
}
