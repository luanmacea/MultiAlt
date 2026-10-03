import { useCallback, useRef, type MouseEvent } from "react";

/**
 * Props do fundo escurecido de um modal: fecha **só** quando o botão do mouse
 * desceu e subiu no próprio fundo.
 *
 * Com `onClick` cru no fundo, apertar dentro de um campo do modal (para
 * selecionar o texto, por exemplo) e soltar fora fechava o modal: o navegador
 * dispara o `click` no ancestral comum de onde o botão desceu e de onde subiu —
 * o fundo. Aqui o `mousedown` anota se começou no fundo, e o `click` só fecha
 * se começou e terminou nele.
 *
 * Uso: `const backdrop = useBackdropClose(handleClose)` e `<div {...backdrop}>`.
 * Sem `onClose` (diálogo que não pode ser dispensado agora), o fundo não fecha.
 * Uma guarda em `useBackdropClose.test.tsx` falha se um fundo voltar a usar
 * `onClick` cru.
 */
export function useBackdropClose(onClose?: () => void) {
  const pressedOnBackdrop = useRef(false);

  const onMouseDown = useCallback((e: MouseEvent<HTMLElement>) => {
    pressedOnBackdrop.current = e.target === e.currentTarget;
  }, []);

  const onClick = useCallback(
    (e: MouseEvent<HTMLElement>) => {
      const startedHere = pressedOnBackdrop.current;
      pressedOnBackdrop.current = false;
      if (startedHere && e.target === e.currentTarget) onClose?.();
    },
    [onClose]
  );

  return { onMouseDown, onClick };
}
