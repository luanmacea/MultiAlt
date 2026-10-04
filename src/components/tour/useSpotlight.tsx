import { useEffect, useState } from "react";
import type { Rect } from "./placement";

/**
 * O destaque dos tutoriais: acha o alvo do passo e acompanha onde ele está.
 *
 * Compartilhado entre o tour de boas-vindas (`FirstRunWalkthrough`) e os
 * tutoriais de tela (`ScreenTour`). O alvo é procurado de novo a cada 140 ms,
 * porque a tela muda por baixo do tutorial — uma aba abre, uma lista termina
 * de carregar, a janela muda de tamanho.
 *
 * `resolve` devolve o elemento a destacar (ou `null`); quando ele muda de
 * identidade, a busca recomeça na hora. `resetKey` faz o mesmo (troca de passo).
 */
export function useSpotlight(resolve: () => HTMLElement | null, resetKey: unknown) {
  const [target, setTarget] = useState<HTMLElement | null>(null);
  const [rect, setRect] = useState<Rect | null>(null);

  useEffect(() => {
    const update = () => {
      const next = resolve();
      setTarget((prev) => (prev === next ? prev : next));

      if (!next) {
        setRect(null);
        return;
      }

      const box = next.getBoundingClientRect();
      const left = Math.max(8, box.left - 8);
      const top = Math.max(8, box.top - 8);
      // Alvo que vai até a borda (a lista inteira) não pode empurrar o anel
      // para fora da janela: a borda direita/de baixo dele sumia.
      const right = Math.min(window.innerWidth - 4, box.right + 8);
      const bottom = Math.min(window.innerHeight - 4, box.bottom + 8);
      const padded: Rect = {
        left,
        top,
        width: Math.max(24, right - left),
        height: Math.max(24, bottom - top),
      };
      // Sem isto a tela inteira renderiza de novo a cada 140 ms à toa.
      setRect((prev) =>
        prev &&
        prev.left === padded.left &&
        prev.top === padded.top &&
        prev.width === padded.width &&
        prev.height === padded.height
          ? prev
          : padded
      );
    };

    update();
    const timer = window.setInterval(update, 140);
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);

    return () => {
      window.clearInterval(timer);
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [resolve, resetKey]);

  return { target, rect };
}

/** O anel em volta do alvo (que escurece o resto) ou, sem alvo, o véu inteiro. */
export function Spotlight({ rect }: { rect: Rect | null }) {
  // As `key` impedem o React de reaproveitar o véu (que ocupa a tela toda a
  // partir de 0,0) como anel: o anel nascia no canto e deslizava até o alvo.
  return rect ? (
    <div
      key="ring"
      className="walkthrough-focus-ring"
      style={{ left: rect.left, top: rect.top, width: rect.width, height: rect.height }}
    />
  ) : (
    <div key="scrim" className="walkthrough-soft-scrim" />
  );
}
