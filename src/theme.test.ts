import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ThemeData } from "./types";

const applyThemeFontLoadingMock = vi.fn(async () => {});

vi.mock("./themeFonts", () => ({
  applyThemeFontLoading: (...args: unknown[]) => applyThemeFontLoadingMock(...(args as [])),
}));

import {
  DEFAULT_FONT_MONO,
  DEFAULT_FONT_SANS,
  DEFAULT_THEME,
  SYSTEM_FONT_MONO,
  SYSTEM_FONT_SANS,
  THEME_PRESETS,
  applyThemeCssVariables,
  normalizeTheme,
} from "./theme";

function cssVar(name: string) {
  return document.documentElement.style.getPropertyValue(name);
}

beforeEach(() => {
  applyThemeFontLoadingMock.mockClear();
  document.documentElement.removeAttribute("style");
  delete document.documentElement.dataset.buttonStyle;
  delete document.documentElement.dataset.showHeaders;
});

describe("normalizeTheme", () => {
  it("returns the default theme for null/undefined", () => {
    expect(normalizeTheme(null)).toMatchObject({
      accounts_background: DEFAULT_THEME.accounts_background,
      button_style: DEFAULT_THEME.button_style,
    });
    expect(normalizeTheme(undefined).forms_background).toBe(DEFAULT_THEME.forms_background);
  });

  it("keeps valid hex and rgb colors", () => {
    const normalized = normalizeTheme({
      ...DEFAULT_THEME,
      accounts_background: "#abc",
      accounts_foreground: "#AABBCC",
      buttons_background: "rgb(1, 2, 3)",
      buttons_border: "rgba(1,2,3,0.5)",
    });

    expect(normalized.accounts_background).toBe("#abc");
    expect(normalized.accounts_foreground).toBe("#AABBCC");
    expect(normalized.buttons_background).toBe("rgb(1, 2, 3)");
    expect(normalized.buttons_border).toBe("rgba(1,2,3,0.5)");
  });

  it("trims colors and falls back for invalid, empty or non-string values", () => {
    const normalized = normalizeTheme({
      ...DEFAULT_THEME,
      accounts_background: "  #123456  ",
      accounts_foreground: "chartreuse",
      forms_background: "",
      forms_foreground: 42 as unknown as string,
      textboxes_background: "#12345",
    });

    expect(normalized.accounts_background).toBe("#123456");
    expect(normalized.accounts_foreground).toBe(DEFAULT_THEME.accounts_foreground);
    expect(normalized.forms_background).toBe(DEFAULT_THEME.forms_background);
    expect(normalized.forms_foreground).toBe(DEFAULT_THEME.forms_foreground);
    expect(normalized.textboxes_background).toBe(DEFAULT_THEME.textboxes_background);
  });

  it("derives the toggle colors when they are missing", () => {
    const normalized = normalizeTheme({
      ...DEFAULT_THEME,
      buttons_foreground: "#ff0000",
      toggle_on_background: undefined,
      toggle_off_background: undefined,
      toggle_knob_background: undefined,
    });

    // a saturated buttons_foreground is used as the accent
    expect(normalized.toggle_on_background).toBe("#ff0000");
    expect(normalized.toggle_off_background).toBe(DEFAULT_THEME.buttons_border);
    expect(normalized.toggle_knob_background).toBe("#FFFFFF");
  });

  it("falls back to the default accent when the button foreground is greyscale", () => {
    const normalized = normalizeTheme({
      ...DEFAULT_THEME,
      buttons_foreground: "#a1a1aa",
      toggle_on_background: undefined,
    });
    expect(normalized.toggle_on_background).toBe("#38bdf8");
  });

  it("coerces the boolean flags", () => {
    const normalized = normalizeTheme({
      ...DEFAULT_THEME,
      label_transparent: "yes" as unknown as boolean,
      dark_top_bar: 0 as unknown as boolean,
      show_headers: undefined as unknown as boolean,
      light_images: 1 as unknown as boolean,
    });

    expect(normalized.label_transparent).toBe(true);
    expect(normalized.dark_top_bar).toBe(false);
    expect(normalized.show_headers).toBe(false);
    expect(normalized.light_images).toBe(true);
  });

  it("normalizes the button style", () => {
    expect(normalizeTheme({ ...DEFAULT_THEME, button_style: "Popup" }).button_style).toBe("Popup");
    expect(normalizeTheme({ ...DEFAULT_THEME, button_style: "Standard" }).button_style).toBe("Standard");
    expect(normalizeTheme({ ...DEFAULT_THEME, button_style: "popup" }).button_style).toBe("Flat");
    expect(normalizeTheme({ ...DEFAULT_THEME, button_style: "" }).button_style).toBe("Flat");
  });

  describe("font specs", () => {
    it("falls back entirely for a missing or non-object spec", () => {
      expect(normalizeTheme({ ...DEFAULT_THEME, font_sans: undefined }).font_sans).toEqual(
        DEFAULT_FONT_SANS
      );
      expect(
        normalizeTheme({ ...DEFAULT_THEME, font_mono: "JetBrains" as never }).font_mono
      ).toEqual(DEFAULT_FONT_MONO);
    });

    it("defaults an unknown source to google and keeps known sources", () => {
      expect(
        normalizeTheme({
          ...DEFAULT_THEME,
          font_sans: { source: "bogus" as never, family: "X", fallbacks: ["y"] },
        }).font_sans?.source
      ).toBe("google");
      expect(normalizeTheme({ ...DEFAULT_THEME, font_sans: SYSTEM_FONT_SANS }).font_sans).toEqual(
        SYSTEM_FONT_SANS
      );
      expect(normalizeTheme({ ...DEFAULT_THEME, font_mono: SYSTEM_FONT_MONO }).font_mono).toEqual(
        SYSTEM_FONT_MONO
      );
    });

    it("uses the fallback family and fallbacks when they are empty", () => {
      const spec = normalizeTheme({
        ...DEFAULT_THEME,
        font_sans: { source: "google", family: "   ", fallbacks: ["", "   "] },
      }).font_sans!;

      expect(spec.family).toBe(DEFAULT_FONT_SANS.family);
      expect(spec.fallbacks).toEqual(DEFAULT_FONT_SANS.fallbacks);
    });

    it("dedupes, rounds and sorts google weights", () => {
      const spec = normalizeTheme({
        ...DEFAULT_THEME,
        font_sans: {
          source: "google",
          family: "Inter",
          fallbacks: ["sans-serif"],
          google: { weights: [700, 400.4, 400, Number.NaN, 300] as number[] },
        },
      }).font_sans!;

      expect(spec.google?.weights).toEqual([300, 400, 700]);
    });

    it("uses the fallback weights when none survive", () => {
      const spec = normalizeTheme({
        ...DEFAULT_THEME,
        font_mono: {
          source: "google",
          family: "Fira Code",
          fallbacks: ["monospace"],
          google: { weights: [] },
        },
      }).font_mono!;

      expect(spec.google?.weights).toEqual(DEFAULT_FONT_MONO.google?.weights);
    });

    it("normalizes a local font and defaults its weight/style", () => {
      const spec = normalizeTheme({
        ...DEFAULT_THEME,
        font_sans: {
          source: "local",
          family: "My Font",
          fallbacks: ["sans-serif"],
          local: { file: "  my.ttf  ", weight: Number.NaN, style: "oblique" as never },
        },
      }).font_sans!;

      expect(spec.local).toEqual({ file: "my.ttf", weight: 400, style: "normal" });
    });

    it("keeps an italic local font and rounds its weight", () => {
      const spec = normalizeTheme({
        ...DEFAULT_THEME,
        font_sans: {
          source: "local",
          family: "My Font",
          fallbacks: ["sans-serif"],
          local: { file: "my.ttf", weight: 612.7, style: "italic" },
        },
      }).font_sans!;

      expect(spec.local).toEqual({ file: "my.ttf", weight: 613, style: "italic" });
    });

    it("falls back completely when a local font has no file", () => {
      const spec = normalizeTheme({
        ...DEFAULT_THEME,
        font_sans: {
          source: "local",
          family: "My Font",
          fallbacks: ["sans-serif"],
          local: { file: "   ", weight: 400, style: "normal" },
        },
      }).font_sans!;

      expect(spec).toEqual(DEFAULT_FONT_SANS);
    });
  });

  it("keeps every shipped preset stable through normalization", () => {
    for (const preset of THEME_PRESETS) {
      expect(normalizeTheme(preset.theme)).toEqual(normalizeTheme(normalizeTheme(preset.theme)));
    }
    expect(THEME_PRESETS.map((p) => p.id)).toContain("legacy-v4");
  });
});

describe("applyThemeCssVariables", () => {
  function theme(overrides: Partial<ThemeData> = {}): ThemeData {
    return { ...DEFAULT_THEME, ...overrides };
  }

  it("writes the core color variables", () => {
    applyThemeCssVariables(
      theme({
        accounts_background: "#010203",
        accounts_foreground: "#040506",
        buttons_background: "#070809",
        buttons_border: "#0a0b0c",
        textboxes_background: "#0d0e0f",
      })
    );

    expect(cssVar("--accounts-bg")).toBe("#010203");
    expect(cssVar("--accounts-fg")).toBe("#040506");
    expect(cssVar("--buttons-bg")).toBe("#070809");
    expect(cssVar("--border-color")).toBe("#0a0b0c");
    expect(cssVar("--textboxes-bg")).toBe("#0d0e0f");
    expect(cssVar("--app-bg")).toBe(DEFAULT_THEME.forms_background);
    expect(cssVar("--panel-bg")).toBe("#010203");
  });

  it("converts colors to rgba for the derived surfaces", () => {
    applyThemeCssVariables(theme({ buttons_background: "#102030" }));
    expect(cssVar("--row-hover")).toBe("rgba(16, 32, 48, 0.32)");
    expect(cssVar("--row-selected")).toBe("rgba(16, 32, 48, 0.62)");
  });

  it("supports 3-digit hex and rgb() inputs in derived colors", () => {
    applyThemeCssVariables(theme({ buttons_background: "#abc" }));
    expect(cssVar("--row-hover")).toBe("rgba(170, 187, 204, 0.32)");

    applyThemeCssVariables(theme({ buttons_background: "rgb(300, -5, 10.6)" }));
    expect(cssVar("--row-hover")).toBe("rgba(255, 0, 11, 0.32)");
  });

  it("uses the default accent for a greyscale button foreground", () => {
    applyThemeCssVariables(theme({ buttons_foreground: "#a1a1aa" }));
    expect(cssVar("--accent-color")).toBe("#38bdf8");
  });

  it("keeps a saturated button foreground as the accent", () => {
    applyThemeCssVariables(theme({ buttons_foreground: "#ff8800", toggle_on_background: undefined }));
    expect(cssVar("--accent-color")).toBe("#ff8800");
  });

  it("makes the label background transparent when requested", () => {
    applyThemeCssVariables(theme({ label_transparent: true, label_background: "#111111" }));
    expect(cssVar("--labels-bg")).toBe("transparent");

    applyThemeCssVariables(theme({ label_transparent: false, label_background: "#111111" }));
    expect(cssVar("--labels-bg")).toBe("#111111");
  });

  it("switches the titlebar and avatar filter on the theme flags", () => {
    applyThemeCssVariables(theme({ dark_top_bar: true, light_images: true }));
    expect(cssVar("--titlebar-bg")).toBe("#09090b");
    expect(cssVar("--titlebar-fg")).toBe("#a1a1aa");
    expect(cssVar("--avatar-filter")).toContain("brightness");

    applyThemeCssVariables(
      theme({ dark_top_bar: false, light_images: false, forms_background: "#222222", forms_foreground: "#eeeeee" })
    );
    expect(cssVar("--titlebar-bg")).toBe("#222222");
    expect(cssVar("--titlebar-fg")).toBe("#eeeeee");
    expect(cssVar("--avatar-filter")).toBe("none");
  });

  it("maps the button style to radius, shadow, inset and data attributes", () => {
    applyThemeCssVariables(theme({ button_style: "Standard", show_headers: true }));
    expect(cssVar("--button-radius")).toBe("6px");
    expect(cssVar("--button-shadow")).toBe("none");
    expect(cssVar("--button-inset")).toContain("inset");
    expect(document.documentElement.dataset.buttonStyle).toBe("Standard");
    expect(document.documentElement.dataset.showHeaders).toBe("true");

    applyThemeCssVariables(theme({ button_style: "Popup", show_headers: false }));
    expect(cssVar("--button-radius")).toBe("10px");
    expect(cssVar("--button-shadow")).toContain("rgba");
    expect(cssVar("--button-inset")).toBe("none");
    expect(document.documentElement.dataset.showHeaders).toBe("false");

    applyThemeCssVariables(theme({ button_style: "Flat" }));
    expect(cssVar("--button-radius")).toBe("8px");
  });

  it("builds quoted font stacks", () => {
    applyThemeCssVariables(
      theme({
        font_sans: {
          source: "google",
          family: "Plus Jakarta Sans",
          fallbacks: ["system-ui", "Segoe UI", "sans-serif"],
          google: { weights: [400] },
        },
        font_mono: {
          source: "system",
          family: "ui-monospace",
          fallbacks: ["monospace"],
        },
      })
    );

    expect(cssVar("--font-sans")).toBe("'Plus Jakarta Sans', system-ui, 'Segoe UI', sans-serif");
    expect(cssVar("--font-mono")).toBe("ui-monospace, monospace");
  });

  it("escapes single quotes in a font family", () => {
    applyThemeCssVariables(
      theme({
        font_sans: {
          source: "local",
          family: "My O'Font",
          fallbacks: ["sans-serif"],
          local: { file: "x.ttf", weight: 400, style: "normal" },
        },
      })
    );
    expect(cssVar("--font-sans")).toBe("'My O\\'Font', sans-serif");
  });

  it("asks the font loader for the resolved specs", () => {
    applyThemeCssVariables(theme());
    expect(applyThemeFontLoadingMock).toHaveBeenCalledTimes(1);
    expect(applyThemeFontLoadingMock).toHaveBeenCalledWith(DEFAULT_FONT_SANS, DEFAULT_FONT_MONO);
  });
});
