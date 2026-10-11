import { useEffect, useRef } from "react";
import { fireEscapeForBack } from "./useEscapeStack";
import { createPageHistory, currentPage, pageHistoryReducer, type PageHistory } from "../utils/pageHistory";

/**
 * Botões laterais do mouse (Voltar = botão 3 / XButton1, Avançar = botão 4 /
 * XButton2) e Alt+Esquerda / Alt+Direita, como no navegador. Pedido do dono:
 * no MultiAlt eles não faziam nada.
 *
 * - **Voltar:** se há algo aberto por cima (o topo da pilha de Escape: diálogo,
 *   Choose Game, menu, tutorial), fecha exatamente como o Escape fecharia; senão
 *   volta para a página anterior. Sem página anterior, não faz nada.
 * - **Avançar:** anda para a frente no histórico (que some quando se navega para
 *   outro lugar depois de voltar).
 *
 * O histórico observa `page`, então qualquer troca de página entra nele — a
 * barra lateral, o "Open launch log", os setters antigos (`setSettingsOpen`...).
 *
 * O `preventDefault` nos botões laterais vale sempre (mesmo desligado): o
 * WebView2 não pode fazer nada sozinho com eles.
 */
const BACK_BUTTON = 3;
const FORWARD_BUTTON = 4;

/** mouseup e auxclick chegam do mesmo clique; o segundo dentro disto é ignorado. */
const SAME_PRESS_MS = 500;

function isTextField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT";
}

export function useMouseNavigation<P>({
  enabled,
  page,
  setPage,
}: {
  enabled: boolean;
  page: P;
  setPage: (page: P) => void;
}) {
  const historyRef = useRef<PageHistory<P>>(createPageHistory(page));
  const enabledRef = useRef(enabled);
  const setPageRef = useRef(setPage);
  enabledRef.current = enabled;
  setPageRef.current = setPage;

  // Toda troca de página entra no histórico. A que o próprio Voltar/Avançar
  // pediu já é a posição atual, e o reducer a ignora.
  useEffect(() => {
    historyRef.current = pageHistoryReducer(historyRef.current, { type: "visit", page });
  }, [page]);

  useEffect(() => {
    const move = (type: "back" | "forward") => {
      const next = pageHistoryReducer(historyRef.current, { type });
      if (next === historyRef.current) return;
      historyRef.current = next;
      setPageRef.current(currentPage(next));
    };

    const goBack = () => {
      if (fireEscapeForBack()) return;
      move("back");
    };
    const goForward = () => move("forward");

    let lastHandled: { button: number; at: number } | null = null;

    const onMouse = (event: MouseEvent) => {
      if (event.button !== BACK_BUTTON && event.button !== FORWARD_BUTTON) return;
      event.preventDefault();
      if (event.type === "mousedown") return;
      // O mesmo clique chega como mouseup e depois auxclick: age no primeiro.
      const now = Date.now();
      if (
        event.type === "auxclick" &&
        lastHandled &&
        lastHandled.button === event.button &&
        now - lastHandled.at < SAME_PRESS_MS
      ) {
        lastHandled = null;
        return;
      }
      lastHandled = { button: event.button, at: now };
      if (!enabledRef.current) return;
      if (event.button === BACK_BUTTON) goBack();
      else goForward();
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      if (!event.altKey || event.ctrlKey || event.shiftKey || event.metaKey) return;
      if (event.defaultPrevented || isTextField(event.target)) return;
      event.preventDefault();
      if (!enabledRef.current) return;
      if (event.key === "ArrowLeft") goBack();
      else goForward();
    };

    window.addEventListener("mousedown", onMouse);
    window.addEventListener("mouseup", onMouse);
    window.addEventListener("auxclick", onMouse);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("mousedown", onMouse);
      window.removeEventListener("mouseup", onMouse);
      window.removeEventListener("auxclick", onMouse);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);
}
