import { describe, expect, it } from "vitest";
import { LIVE_GRID_DEFAULTS, liveGrid, toPhysical } from "./liveGrid";

const NO_LABEL = { ...LIVE_GRID_DEFAULTS, labelHeight: 0, gap: 0 };

describe("liveGrid", () => {
  it("is empty without accounts or space", () => {
    expect(liveGrid(0, 1280, 720)).toEqual([]);
    expect(liveGrid(3, 0, 720)).toEqual([]);
  });

  it("one account fills the window keeping the 4:3 shape, centered", () => {
    const [cell] = liveGrid(1, 1600, 900, NO_LABEL);
    expect(cell.frame).toEqual({ x: 200, y: 0, width: 1200, height: 900 });
  });

  /** Pedido do dono (28/10/2026): sempre o maior tamanho em que todas cabem. */
  it("picks the column count that gives the biggest frames", () => {
    // 4 contas numa janela larga: 4 lado a lado são pequenas; 2x2 é maior.
    const cells = liveGrid(4, 1600, 900, NO_LABEL);
    expect(cells[0].frame.width).toBe(600);
    expect(cells[1].frame.x).toBe(cells[0].frame.x + 600);
    expect(cells[2].frame.y).toBe(cells[0].frame.y + 450);
    // 10 contas como no exemplo que mandaram: 4 colunas, 3 linhas.
    const ten = liveGrid(10, 1440, 760, LIVE_GRID_DEFAULTS);
    const cols = new Set(ten.map((c) => c.frame.x)).size;
    const rows = new Set(ten.map((c) => c.frame.y)).size;
    expect([cols, rows]).toEqual([4, 3]);
  });

  it("every frame stays inside the window and none overlap", () => {
    for (const count of [1, 2, 3, 5, 7, 12, 20]) {
      const cells = liveGrid(count, 1280, 720, LIVE_GRID_DEFAULTS);
      expect(cells).toHaveLength(count);
      for (const c of cells) {
        expect(c.label.y).toBeGreaterThanOrEqual(0);
        expect(c.frame.x + c.frame.width).toBeLessThanOrEqual(1280);
        expect(c.frame.y + c.frame.height).toBeLessThanOrEqual(720);
        expect(c.frame.y).toBe(c.label.y + LIVE_GRID_DEFAULTS.labelHeight);
      }
      for (let i = 0; i < cells.length; i++) {
        for (let j = i + 1; j < cells.length; j++) {
          const a = cells[i].frame;
          const b = cells[j].frame;
          const apart = a.x + a.width <= b.x || b.x + b.width <= a.x || a.y + a.height <= b.y || b.y + b.height <= a.y;
          expect(apart, `${count}: ${i} and ${j}`).toBe(true);
        }
      }
    }
  });

  it("converts to the physical pixels the backend uses", () => {
    expect(toPhysical({ x: 10, y: 20, width: 100, height: 75 }, 1.5)).toEqual({ x: 15, y: 30, width: 150, height: 113 });
  });
});
