/**
 * A grade da janela Live (docs/features/live.md): o maior tamanho de quadro em
 * que todas as contas cabem na janela, como o "Best size for each monitor" da
 * grade de janelas. Testa cada número de colunas e fica com o que dá o maior
 * quadro. A linha que sobra fica à esquerda, como nas outras linhas.
 */

export interface LiveRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface LiveCell {
  /** A faixa do nome, em cima do quadro (altura 0 sem nomes). */
  label: LiveRect;
  /** Onde a janela do Roblox aparece. */
  frame: LiveRect;
}

export interface LiveGridOptions {
  gap: number;
  labelHeight: number;
  /** Largura / altura dos quadros (as janelas do Roblox costumam ser 4:3). */
  aspect: number;
}

export const LIVE_GRID_DEFAULTS: LiveGridOptions = { gap: 8, labelHeight: 22, aspect: 4 / 3 };

export function liveGrid(count: number, width: number, height: number, options: LiveGridOptions = LIVE_GRID_DEFAULTS): LiveCell[] {
  const { gap, labelHeight, aspect } = options;
  if (count <= 0 || width <= 0 || height <= 0) return [];
  let best = { cols: 1, rows: count, tileW: 0, tileH: 0 };
  for (let cols = 1; cols <= count; cols++) {
    const rows = Math.ceil(count / cols);
    const cellW = (width - gap * (cols + 1)) / cols;
    const cellH = (height - gap * (rows + 1)) / rows - labelHeight;
    if (cellW <= 0 || cellH <= 0) continue;
    const tileW = Math.min(cellW, cellH * aspect);
    const tileH = tileW / aspect;
    if (tileW * tileH > best.tileW * best.tileH) best = { cols, rows, tileW, tileH };
  }
  if (best.tileW <= 0) return [];
  const tileW = Math.floor(best.tileW);
  const tileH = Math.floor(best.tileH);
  const gridW = best.cols * tileW + (best.cols - 1) * gap;
  const gridH = best.rows * (tileH + labelHeight) + (best.rows - 1) * gap;
  const left = Math.floor((width - gridW) / 2);
  const top = Math.floor((height - gridH) / 2);
  const cells: LiveCell[] = [];
  for (let i = 0; i < count; i++) {
    const col = i % best.cols;
    const row = Math.floor(i / best.cols);
    const x = left + col * (tileW + gap);
    const y = top + row * (tileH + labelHeight + gap);
    cells.push({
      label: { x, y, width: tileW, height: labelHeight },
      frame: { x, y: y + labelHeight, width: tileW, height: tileH },
    });
  }
  return cells;
}

/** Em pixels da tela: a janela mede em pixels CSS, o DWM pede pixels físicos. */
export function toPhysical(rect: LiveRect, scale: number): LiveRect {
  return {
    x: Math.round(rect.x * scale),
    y: Math.round(rect.y * scale),
    width: Math.round(rect.width * scale),
    height: Math.round(rect.height * scale),
  };
}
