import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ThemeData } from "./types";

vi.mock("./themeFonts", () => ({ applyThemeFontLoading: vi.fn(async () => {}) }));

import { DEFAULT_THEME, THEME_PRESETS, applyThemeCssVariables } from "./theme";

function cssVar(name: string) {
  return document.documentElement.style.getPropertyValue(name);
}

function channel(c: number) {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}
function luminance([r, g, b]: number[]) {
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}
function rgbOf(value: string): number[] {
  const hex = value.trim().replace(/^#/, "");
  if (/^[0-9a-f]{6}$/i.test(hex)) return [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16));
  const m = /rgba?\(([^)]+)\)/.exec(value);
  if (!m) throw new Error(`not a color: ${value}`);
  return m[1]
    .split(",")
    .slice(0, 3)
    .map((p) => Number(p.trim()));
}
function over(fg: number[], alpha: number, bg: number[]) {
  return fg.map((c, i) => c * alpha + bg[i] * (1 - alpha));
}
function ratio(a: number[], b: number[]) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

beforeEach(() => {
  document.documentElement.removeAttribute("style");
  delete document.documentElement.dataset.themeTone;
});

/**
 * Contraste do texto apagado (`--panel-muted`): era a cor dos rótulos com 85%
 * de opacidade, e o `.theme-muted` ainda tirava mais 15% — 2,5 a 3,2:1 no tema
 * padrão (detector do Impeccable, página Session, 11/10/2026). Agora a cor é
 * opaca e ajustada para chegar a 4,5:1 (WCAG AA) sobre o fundo do painel e
 * sobre o fundo dos cartões (`--panel-soft`), em qualquer tema, claro também.
 */
describe("readable muted text", () => {
  const LIGHT: ThemeData = {
    ...DEFAULT_THEME,
    accounts_background: "#FFFFFF",
    accounts_foreground: "#18181B",
    buttons_background: "#E4E4E7",
    buttons_border: "#D4D4D8",
    forms_background: "#FAFAFA",
    forms_foreground: "#18181B",
    label_foreground: "#A1A1AA",
  };

  const themes: [string, ThemeData][] = [
    ["default", DEFAULT_THEME],
    ...THEME_PRESETS.map((p) => [p.id, p.theme] as [string, ThemeData]),
    ["light", LIGHT],
  ];

  it.each(themes)("reaches 4.5:1 on the panel and on the cards (%s)", (_id, t) => {
    applyThemeCssVariables(t);
    expect(cssVar("--panel-muted")).not.toMatch(/rgba/);
    const muted = rgbOf(cssVar("--panel-muted"));
    const panel = rgbOf(t.accounts_background);
    const card = over(rgbOf(t.buttons_background), 0.55, panel);
    expect(ratio(muted, panel)).toBeGreaterThanOrEqual(4.5);
    expect(ratio(muted, card)).toBeGreaterThanOrEqual(4.5);
  });

  it("keeps the theme's own label color when it already reads well", () => {
    applyThemeCssVariables({ ...DEFAULT_THEME, label_foreground: "#B4B4BC" });
    expect(cssVar("--panel-muted")).toBe("#b4b4bc");
  });

  it("marks light themes so status colors can darken", () => {
    applyThemeCssVariables(LIGHT);
    expect(document.documentElement.dataset.themeTone).toBe("light");
    applyThemeCssVariables(DEFAULT_THEME);
    expect(document.documentElement.dataset.themeTone).toBe("dark");
  });
});
