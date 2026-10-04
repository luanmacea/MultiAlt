/**
 * Onde o painel do tutorial fica em relação ao que ele aponta.
 *
 * Ordem de preferência: embaixo, em cima, à direita, à esquerda. Alvo que não
 * deixa espaço em volta (a área inteira de uma aba, por exemplo) recebe o
 * painel por dentro, no canto de baixo à direita. Sem alvo, no centro. Em
 * qualquer caso o painel não sai da janela.
 */
export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface Size {
  width: number;
  height: number;
}

const GAP = 12;
const EDGE = 8;

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), Math.max(min, max));
}

export function placePanel(target: Rect | null, panel: Size, view: Size): { left: number; top: number } {
  const maxLeft = view.width - panel.width - EDGE;
  const maxTop = view.height - panel.height - EDGE;

  if (!target) {
    return {
      left: clamp(Math.round((view.width - panel.width) / 2), EDGE, maxLeft),
      top: clamp(Math.round((view.height - panel.height) / 2), EDGE, maxTop),
    };
  }

  const right = target.left + target.width;
  const bottom = target.top + target.height;

  if (bottom + GAP + panel.height <= view.height - EDGE) {
    return { left: clamp(target.left, EDGE, maxLeft), top: bottom + GAP };
  }
  if (target.top - GAP - panel.height >= EDGE) {
    return { left: clamp(target.left, EDGE, maxLeft), top: target.top - GAP - panel.height };
  }
  if (right + GAP + panel.width <= view.width - EDGE) {
    return { left: right + GAP, top: clamp(target.top, EDGE, maxTop) };
  }
  if (target.left - GAP - panel.width >= EDGE) {
    return { left: target.left - GAP - panel.width, top: clamp(target.top, EDGE, maxTop) };
  }
  // Sem espaço em volta: por dentro do alvo, no canto de baixo à direita.
  return {
    left: clamp(Math.min(right, view.width) - panel.width - 16, EDGE, maxLeft),
    top: clamp(Math.min(bottom, view.height) - panel.height - 16, EDGE, maxTop),
  };
}
