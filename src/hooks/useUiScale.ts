import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { normalizeUiScaleSetting, resolveUiScale, type LogicalSize } from "../uiScale";

/** Evento com que Settings › App avisa a escolha antes de a store recarregar. */
const UI_SCALE_EVENT = "ram-ui-scale";
/** Arrastar a janela dispara dezenas de eventos: calcula só no fim. */
const RESIZE_DEBOUNCE_MS = 150;

/**
 * Zoom aplicado agora, para Settings mostrar o "Automatic (170%)". Fica fora
 * do React porque quem aplica (App) e quem mostra (AppTab) não se veem.
 */
let appliedFactor: number | null = null;
const appliedListeners = new Set<() => void>();

function publishApplied(factor: number | null): void {
  if (appliedFactor === factor) return;
  appliedFactor = factor;
  for (const listener of appliedListeners) listener();
}

function subscribeApplied(listener: () => void): () => void {
  appliedListeners.add(listener);
  return () => appliedListeners.delete(listener);
}

/** Fator de zoom em vigor (1.7 = 170%); `null` antes do primeiro `setZoom`. */
export function useAppliedUiScale(): number | null {
  return useSyncExternalStore(subscribeApplied, () => appliedFactor, () => appliedFactor);
}

/**
 * Settings grava no INI pelo `useSettings` dela, e a store só relê as settings
 * ao sair da página. Este aviso aplica o zoom na hora em que o usuário escolhe.
 */
export function announceUiScale(value: string): void {
  window.dispatchEvent(new CustomEvent<string>(UI_SCALE_EVENT, { detail: value }));
}

/**
 * Aplica o tamanho da interface (`General.InterfaceScale`) com o zoom nativo
 * do WebView. O automático olha duas coisas (regra em `uiScale.ts`): a
 * **janela** só para crescer (janela grande num monitor 4K) e o **monitor**
 * só para encolher — uma janela restaurada num monitor grande não encolhe a
 * interface (era assim na primeira versão, e o monitor de 2560x1440 do dono
 * ficaria em 80% com a janela no tamanho padrão). Recalcula quando a janela
 * anda (pode ter ido para outro monitor) ou muda de tamanho; `setZoom` só é
 * chamado quando o fator muda. Troca de escala reaplica em qualquer modo.
 *
 * `ready` falso (settings ainda não lidas do INI) não aplica nada: sem isso,
 * quem escolheu um valor fixo veria o automático piscar na abertura.
 *
 * Tudo é defensivo: fora do Tauri (testes, navegador) a API não existe, e o
 * pior caso é a interface ficar em 100%.
 */
export function useUiScale(setting: string | null | undefined, ready = true): void {
  const [announced, setAnnounced] = useState<string | null>(null);
  const applied = useRef<number | null>(null);

  useEffect(() => {
    const onAnnounce = (e: Event) => {
      const detail = (e as CustomEvent<unknown>).detail;
      if (typeof detail === "string") setAnnounced(detail);
    };
    window.addEventListener(UI_SCALE_EVENT, onAnnounce);
    return () => window.removeEventListener(UI_SCALE_EVENT, onAnnounce);
  }, []);

  // A store recarregou: o valor dela passa a mandar de novo.
  useEffect(() => {
    setAnnounced(null);
  }, [setting]);

  const mode = normalizeUiScaleSetting(announced ?? setting);
  const active = ready || announced !== null;

  useEffect(() => {
    if (!active) return;
    let win: ReturnType<typeof getCurrentWindow>;
    let webview: ReturnType<typeof getCurrentWebview>;
    try {
      win = getCurrentWindow();
      webview = getCurrentWebview();
    } catch {
      return;
    }

    let disposed = false;
    let seq = 0;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const unlisteners: Array<() => void> = [];

    const readLogicalSize = async (): Promise<LogicalSize | null> => {
      try {
        const monitor = await currentMonitor();
        const scale = monitor?.scaleFactor ?? 0;
        if (!monitor || !(scale > 0)) return null;
        return { width: monitor.size.width / scale, height: monitor.size.height / scale };
      } catch {
        return null;
      }
    };

    // Área interna da janela em px lógicos. `innerSize` é físico e não muda
    // com o zoom do WebView (`window.innerWidth` muda: realimentaria a conta).
    const readWindowSize = async (): Promise<LogicalSize | null> => {
      try {
        const [size, scale] = await Promise.all([win.innerSize(), win.scaleFactor()]);
        if (!size || !(scale > 0)) return null;
        return { width: size.width / scale, height: size.height / scale };
      } catch {
        return null;
      }
    };

    const apply = async () => {
      const mine = ++seq;
      const [monitorSize, windowSize] =
        mode === "auto" ? await Promise.all([readLogicalSize(), readWindowSize()]) : [null, null];
      // Um cálculo mais novo começou (ou o efeito acabou): este fica para trás.
      if (disposed || mine !== seq) return;
      const factor = resolveUiScale(mode, monitorSize, windowSize);
      if (applied.current === factor) return;
      applied.current = factor;
      try {
        await webview.setZoom(factor);
        publishApplied(factor);
      } catch {
        // Sem permissão/sem WebView: tenta de novo na próxima mudança.
        applied.current = null;
      }
    };

    void apply();

    const schedule = () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        timer = null;
        void apply();
      }, RESIZE_DEBOUNCE_MS);
    };
    // Troca de monitor (DPI diferente) mexe na escala do WebView2: reaplica
    // mesmo com o mesmo fator, em qualquer modo, para o zoom não ficar para trás.
    const onScaleChanged = () => {
      applied.current = null;
      schedule();
    };
    const keep = (promise: Promise<() => void> | undefined) => {
      void promise
        ?.then((unlisten) => {
          if (disposed) unlisten();
          else unlisteners.push(unlisten);
        })
        .catch(() => {});
    };
    try {
      if (mode === "auto") {
        keep(win.onMoved(schedule));
        // Maximizar/arrastar a borda: a janela grande cresce a interface.
        keep(win.onResized?.(schedule));
      }
      keep(win.onScaleChanged?.(onScaleChanged));
    } catch {
      // API ausente: fica só o cálculo inicial.
    }

    return () => {
      disposed = true;
      if (timer) clearTimeout(timer);
      for (const unlisten of unlisteners.splice(0)) unlisten();
    };
  }, [mode, active]);
}
