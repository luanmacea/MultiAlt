import { describe, expect, it } from "vitest";
import { placePanel } from "./placement";

const VIEW = { width: 1100, height: 700 };
const PANEL = { width: 340, height: 200 };

describe("placePanel", () => {
  it("centers the panel when the step has no target on screen", () => {
    expect(placePanel(null, PANEL, VIEW)).toEqual({ left: 380, top: 250 });
  });

  it("puts the panel under a small target near the top", () => {
    const pos = placePanel({ left: 100, top: 40, width: 200, height: 30 }, PANEL, VIEW);
    expect(pos.top).toBe(40 + 30 + 12);
    expect(pos.left).toBe(100);
  });

  it("puts the panel above a target near the bottom", () => {
    const pos = placePanel({ left: 700, top: 600, width: 200, height: 40 }, PANEL, VIEW);
    expect(pos.top).toBe(600 - 12 - 200);
    // Não passa da borda direita.
    expect(pos.left + PANEL.width).toBeLessThanOrEqual(VIEW.width - 8);
  });

  it("goes beside a tall target that leaves no room above or below", () => {
    const pos = placePanel({ left: 0, top: 0, width: 300, height: 700 }, PANEL, VIEW);
    expect(pos.left).toBe(300 + 12);
  });

  it("stays inside a target that fills the screen, without leaving the window", () => {
    const pos = placePanel({ left: 0, top: 0, width: 1100, height: 700 }, PANEL, VIEW);
    expect(pos.left).toBeGreaterThanOrEqual(8);
    expect(pos.top).toBeGreaterThanOrEqual(8);
    expect(pos.left + PANEL.width).toBeLessThanOrEqual(VIEW.width - 8);
    expect(pos.top + PANEL.height).toBeLessThanOrEqual(VIEW.height - 8);
  });

  it("never leaves the window on a narrow screen", () => {
    const narrow = { width: 360, height: 640 };
    const pos = placePanel({ left: 250, top: 100, width: 100, height: 30 }, { width: 330, height: 220 }, narrow);
    expect(pos.left).toBeGreaterThanOrEqual(8);
    expect(pos.left + 330).toBeLessThanOrEqual(narrow.width - 8 + 1);
  });
});
