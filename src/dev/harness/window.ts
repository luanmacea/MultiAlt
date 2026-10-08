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
    // Tamanho da interface (useUiScale): físico = CSS x devicePixelRatio, como o
    // Tauri entrega. O zoom do dublê não faz nada, então isto não realimenta.
    innerSize: async () => {
      const ratio = window.devicePixelRatio || 1;
      return { width: Math.round(window.innerWidth * ratio), height: Math.round(window.innerHeight * ratio) };
    },
    scaleFactor: async () => window.devicePixelRatio || 1,
    onResized: async () => () => {},
    onScaleChanged: async () => () => {},
    onFocusChanged: async () => () => {},
    listen: async () => () => {},
  };
}
