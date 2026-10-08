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
    onMoved: async () => () => {},
    onScaleChanged: async () => () => {},
    onFocusChanged: async () => () => {},
    listen: async () => () => {},
  };
}

/**
 * Monitor da janela (tamanho da interface, useUiScale). No navegador o
 * "monitor" é a própria aba: físico = CSS x devicePixelRatio, como o Tauri
 * entrega. O zoom do dublê não faz nada, então isto não realimenta.
 */
export async function currentMonitor() {
  const ratio = window.devicePixelRatio || 1;
  return {
    name: "harness",
    size: { width: Math.round(window.innerWidth * ratio), height: Math.round(window.innerHeight * ratio) },
    position: { x: 0, y: 0 },
    scaleFactor: ratio,
  };
}
