import type { ThemeData, ThemeFontSpec } from "./types";
import { applyThemeFontLoading } from "./themeFonts";

const HEX_COLOR_RE = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i;
const RGB_COLOR_RE = /^rgba?\((.+)\)$/i;
const DEFAULT_ACCENT = "#38bdf8";

export const DEFAULT_FONT_SANS: ThemeFontSpec = {
  source: "google",
  family: "Outfit",
  fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  google: { weights: [300, 400, 500, 600, 700] },
};

export const DEFAULT_FONT_MONO: ThemeFontSpec = {
  source: "google",
  family: "JetBrains Mono",
  fallbacks: ["Cascadia Code", "Consolas", "monospace"],
  google: { weights: [400, 500] },
};

export const SYSTEM_FONT_SANS: ThemeFontSpec = {
  source: "system",
  family: "system-ui",
  fallbacks: ["-apple-system", "Segoe UI", "sans-serif"],
};

export const SYSTEM_FONT_MONO: ThemeFontSpec = {
  source: "system",
  family: "ui-monospace",
  fallbacks: ["Cascadia Code", "Consolas", "monospace"],
};

const LEGACY_V4_THEME: ThemeData = {
  accounts_background: "#09090B",
  accounts_foreground: "#E4E4E7",
  buttons_background: "#27272A",
  buttons_foreground: "#A1A1AA",
  buttons_border: "#3F3F46",
  toggle_on_background: "#0EA5E9",
  toggle_off_background: "#3F3F46",
  toggle_knob_background: "#FFFFFF",
  forms_background: "#09090B",
  forms_foreground: "#E4E4E7",
  textboxes_background: "#18181B",
  textboxes_foreground: "#D4D4D8",
  textboxes_border: "#27272A",
  label_background: "#09090B",
  label_foreground: "#71717A",
  label_transparent: true,
  dark_top_bar: true,
  show_headers: true,
  light_images: false,
  button_style: "Flat",
  font_sans: { ...DEFAULT_FONT_SANS },
  font_mono: { ...DEFAULT_FONT_MONO },
};

const CATPPUCCIN_THEME: ThemeData = {
  accounts_background: "#1E1E2E",
  accounts_foreground: "#CDD6F4",
  buttons_background: "#313244",
  buttons_foreground: "#CDD6F4",
  buttons_border: "#45475A",
  toggle_on_background: "#0EA5E9",
  toggle_off_background: "#3F3F46",
  toggle_knob_background: "#FFFFFF",
  forms_background: "#1E1E2E",
  forms_foreground: "#CDD6F4",
  textboxes_background: "#313244",
  textboxes_foreground: "#CDD6F4",
  textboxes_border: "#45475A",
  label_background: "#1E1E2E",
  label_foreground: "#CDD6F4",
  label_transparent: true,
  dark_top_bar: true,
  show_headers: true,
  light_images: false,
  button_style: "Flat",
  font_sans: { ...DEFAULT_FONT_SANS },
  font_mono: { ...DEFAULT_FONT_MONO },
};

export const DEFAULT_THEME: ThemeData = { ...LEGACY_V4_THEME };

export const THEME_PRESETS: Array<{ id: string; label: string; theme: ThemeData }> = [
  {
    id: "legacy-v4",
    label: "Legacy v4 (Original)",
    theme: { ...LEGACY_V4_THEME },
  },
  {
    id: "catppuccin",
    label: "Catppuccin",
    theme: { ...CATPPUCCIN_THEME },
  },
  {
    id: "studio",
    label: "Studio",
    theme: {
      ...LEGACY_V4_THEME,
      font_sans: {
        source: "google",
        family: "Space Grotesk",
        fallbacks: DEFAULT_FONT_SANS.fallbacks,
        google: { weights: [300, 400, 500, 600, 700] },
      },
      font_mono: { ...DEFAULT_FONT_MONO },
    },
  },
  {
    id: "terminal",
    label: "Terminal",
    theme: {
      ...LEGACY_V4_THEME,
      font_sans: {
        source: "google",
        family: "Inter",
        fallbacks: DEFAULT_FONT_SANS.fallbacks,
        google: { weights: [300, 400, 500, 600, 700] },
      },
      font_mono: {
        source: "google",
        family: "Fira Code",
        fallbacks: DEFAULT_FONT_MONO.fallbacks,
        google: { weights: [400, 500, 600] },
      },
    },
  },
  {
    id: "jakarta",
    label: "Jakarta",
    theme: {
      ...LEGACY_V4_THEME,
      font_sans: {
        source: "google",
        family: "Plus Jakarta Sans",
        fallbacks: DEFAULT_FONT_SANS.fallbacks,
        google: { weights: [300, 400, 500, 600, 700] },
      },
      font_mono: { ...DEFAULT_FONT_MONO },
      buttons_foreground: "#BAE6FD",
    },
  },
  {
    id: "plex",
    label: "Plex",
    theme: {
      ...LEGACY_V4_THEME,
      font_sans: {
        source: "google",
        family: "IBM Plex Sans",
        fallbacks: DEFAULT_FONT_SANS.fallbacks,
        google: { weights: [300, 400, 500, 600, 700] },
      },
      font_mono: {
        source: "google",
        family: "IBM Plex Mono",
        fallbacks: DEFAULT_FONT_MONO.fallbacks,
        google: { weights: [400, 500, 600] },
      },
      buttons_background: "#1F2937",
      buttons_border: "#334155",
    },
  },
  {
    id: "soft",
    label: "Soft",
    theme: {
      ...LEGACY_V4_THEME,
      font_sans: {
        source: "google",
        family: "Nunito",
        fallbacks: DEFAULT_FONT_SANS.fallbacks,
        google: { weights: [300, 400, 500, 600, 700] },
      },
      font_mono: {
        source: "google",
        family: "Source Code Pro",
        fallbacks: DEFAULT_FONT_MONO.fallbacks,
        google: { weights: [400, 500, 600] },
      },
      accounts_background: "#0B1220",
      forms_background: "#070C16",
      label_foreground: "#8B9AB6",
    },
  },
  {
    id: "bubble",
    label: "Bubble",
    theme: {
      ...LEGACY_V4_THEME,
      font_sans: {
        source: "google",
        family: "Rubik",
        fallbacks: DEFAULT_FONT_SANS.fallbacks,
        google: { weights: [300, 400, 500, 600, 700] },
      },
      font_mono: {
        source: "google",
        family: "Space Mono",
        fallbacks: DEFAULT_FONT_MONO.fallbacks,
        google: { weights: [400, 700] },
      },
      accounts_background: "#1B102A",
      forms_background: "#120A1E",
      buttons_background: "#2A1642",
      buttons_border: "#3E2360",
      buttons_foreground: "#FBCFE8",
      label_foreground: "#E9A8D1",
      button_style: "Popup",
    },
  },
  {
    id: "graphite",
    label: "Graphite",
    theme: {
      ...LEGACY_V4_THEME,
      accounts_background: "#16181D",
      accounts_foreground: "#E6E8EE",
      buttons_background: "#23262E",
      buttons_foreground: "#F3F4F8",
      buttons_border: "#3B3F4B",
      forms_background: "#111318",
      forms_foreground: "#E2E5EC",
      textboxes_background: "#1F222A",
      textboxes_foreground: "#E6E8EE",
      textboxes_border: "#363A46",
      label_background: "#111318",
      label_foreground: "#A5ABB9",
      dark_top_bar: true,
      button_style: "Standard",
      font_sans: { ...DEFAULT_FONT_SANS },
      font_mono: { ...DEFAULT_FONT_MONO },
    },
  },
  {
    id: "ocean",
    label: "Ocean",
    theme: {
      ...LEGACY_V4_THEME,
      accounts_background: "#102334",
      accounts_foreground: "#D9EEFF",
      buttons_background: "#17405F",
      buttons_foreground: "#E8F5FF",
      buttons_border: "#2E6A91",
      forms_background: "#0B1A29",
      forms_foreground: "#D6EBFB",
      textboxes_background: "#17364D",
      textboxes_foreground: "#E3F2FF",
      textboxes_border: "#2B6285",
      label_background: "#0B1A29",
      label_foreground: "#8FB8D8",
      dark_top_bar: true,
      button_style: "Popup",
      font_sans: { ...DEFAULT_FONT_SANS },
      font_mono: { ...DEFAULT_FONT_MONO },
    },
  },
  {
    id: "sunset",
    label: "Sunset",
    theme: {
      ...LEGACY_V4_THEME,
      accounts_background: "#2C1324",
      accounts_foreground: "#FFE8F2",
      buttons_background: "#542448",
      buttons_foreground: "#FFEAF4",
      buttons_border: "#7A3E68",
      forms_background: "#1D0D1A",
      forms_foreground: "#FFE4F0",
      textboxes_background: "#4A2241",
      textboxes_foreground: "#FFEBF5",
      textboxes_border: "#70355F",
      label_background: "#1D0D1A",
      label_foreground: "#DFA4C7",
      dark_top_bar: true,
      button_style: "Popup",
      font_sans: { ...DEFAULT_FONT_SANS },
      font_mono: { ...DEFAULT_FONT_MONO },
    },
  },
];

function sanitizeColor(value: unknown, fallback: string): string {
  if (typeof value !== "string") {
    return fallback;
  }

  const next = value.trim();
  if (!next) {
    return fallback;
  }

  if (HEX_COLOR_RE.test(next) || RGB_COLOR_RE.test(next)) {
    return next;
  }

  return fallback;
}

function clampByte(value: number) {
  return Math.min(255, Math.max(0, Math.round(value)));
}

function parseHexToRgb(hex: string): [number, number, number] | null {
  const clean = hex.trim().replace(/^#/, "");
  if (clean.length === 3) {
    const r = parseInt(clean[0] + clean[0], 16);
    const g = parseInt(clean[1] + clean[1], 16);
    const b = parseInt(clean[2] + clean[2], 16);
    return [r, g, b];
  }

  if (clean.length === 6) {
    const r = parseInt(clean.slice(0, 2), 16);
    const g = parseInt(clean.slice(2, 4), 16);
    const b = parseInt(clean.slice(4, 6), 16);
    return [r, g, b];
  }

  return null;
}

function parseRgb(value: string): [number, number, number] | null {
  const match = RGB_COLOR_RE.exec(value.trim());
  if (!match) {
    return null;
  }

  const parts = match[1]
    .split(",")
    .slice(0, 3)
    .map((part) => Number(part.trim()));

  if (parts.length !== 3 || parts.some((p) => Number.isNaN(p))) {
    return null;
  }

  return [clampByte(parts[0]), clampByte(parts[1]), clampByte(parts[2])];
}

function colorToRgb(value: string, fallback: string): [number, number, number] {
  const fromHex = parseHexToRgb(value);
  if (fromHex) {
    return fromHex;
  }

  const fromRgb = parseRgb(value);
  if (fromRgb) {
    return fromRgb;
  }

  return parseHexToRgb(fallback) ?? [39, 39, 42];
}

function rgba(value: string, alpha: number, fallback = "#27272a") {
  const [r, g, b] = colorToRgb(value, fallback);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function pickAccentColor(value: string) {
  const [r, g, b] = colorToRgb(value, DEFAULT_ACCENT);
  const spread = Math.max(r, g, b) - Math.min(r, g, b);
  if (spread < 10) {
    return DEFAULT_ACCENT;
  }
  return value;
}

function normalizeButtonStyle(value: string): ThemeData["button_style"] {
  if (value === "Popup" || value === "Standard" || value === "Flat") {
    return value;
  }
  return "Flat";
}

function normalizeFontSource(value: unknown): ThemeFontSpec["source"] {
  if (value === "google" || value === "local" || value === "system") return value;
  return "google";
}

function normalizeFontSpec(value: unknown, fallback: ThemeFontSpec): ThemeFontSpec {
  if (!value || typeof value !== "object") return { ...fallback };
  const v = value as Partial<ThemeFontSpec>;
  const source = normalizeFontSource(v.source);
  const family = typeof v.family === "string" ? v.family.trim() : "";
  const fallbacks = Array.isArray(v.fallbacks) ? v.fallbacks.filter((x) => typeof x === "string" && x.trim()) as string[] : [];

  const next: ThemeFontSpec = {
    source,
    family: family || fallback.family,
    fallbacks: fallbacks.length > 0 ? fallbacks : fallback.fallbacks,
  };

  if (source === "google") {
    const weightsRaw = Array.isArray(v.google?.weights)
      ? v.google!.weights.filter((n) => Number.isFinite(n)).map((n) => Math.round(Number(n)))
      : [];
    const weights = Array.from(new Set(weightsRaw)).sort((a, b) => a - b);
    next.google = { weights: weights.length > 0 ? weights : (fallback.google?.weights ?? [400, 500]) };
  } else if (source === "local") {
    const file = typeof v.local?.file === "string" ? v.local!.file.trim() : "";
    next.local = {
      file,
      weight: Number.isFinite(v.local?.weight) ? Math.round(Number(v.local!.weight)) : 400,
      style: v.local?.style === "italic" ? "italic" : "normal",
    };
    if (!next.local.file) return { ...fallback };
  }

  return next;
}

function quoteFontToken(token: string) {
  const t = token.trim();
  if (!t) return "";
  // Quote if it contains whitespace or commas.
  if (/[\s,]/.test(t)) return `'${t.replace(/'/g, "\\'")}'`;
  return t;
}

function fontStack(spec: ThemeFontSpec) {
  const parts = [spec.family, ...(spec.fallbacks || [])].map(quoteFontToken).filter(Boolean);
  return parts.join(", ");
}

export function normalizeTheme(theme: ThemeData | null | undefined): ThemeData {
  const source = theme ?? DEFAULT_THEME;

  return {
    accounts_background: sanitizeColor(source.accounts_background, DEFAULT_THEME.accounts_background),
    accounts_foreground: sanitizeColor(source.accounts_foreground, DEFAULT_THEME.accounts_foreground),
    buttons_background: sanitizeColor(source.buttons_background, DEFAULT_THEME.buttons_background),
    buttons_foreground: sanitizeColor(source.buttons_foreground, DEFAULT_THEME.buttons_foreground),
    buttons_border: sanitizeColor(source.buttons_border, DEFAULT_THEME.buttons_border),
    toggle_on_background: sanitizeColor(source.toggle_on_background, pickAccentColor(sanitizeColor(source.buttons_foreground, DEFAULT_THEME.buttons_foreground))),
    toggle_off_background: sanitizeColor(source.toggle_off_background, DEFAULT_THEME.buttons_border),
    toggle_knob_background: sanitizeColor(source.toggle_knob_background, "#FFFFFF"),
    forms_background: sanitizeColor(source.forms_background, DEFAULT_THEME.forms_background),
    forms_foreground: sanitizeColor(source.forms_foreground, DEFAULT_THEME.forms_foreground),
    textboxes_background: sanitizeColor(source.textboxes_background, DEFAULT_THEME.textboxes_background),
    textboxes_foreground: sanitizeColor(source.textboxes_foreground, DEFAULT_THEME.textboxes_foreground),
    textboxes_border: sanitizeColor(source.textboxes_border, DEFAULT_THEME.textboxes_border),
    label_background: sanitizeColor(source.label_background, DEFAULT_THEME.label_background),
    label_foreground: sanitizeColor(source.label_foreground, DEFAULT_THEME.label_foreground),
    label_transparent: !!source.label_transparent,
    dark_top_bar: !!source.dark_top_bar,
    show_headers: !!source.show_headers,
    light_images: !!source.light_images,
    button_style: normalizeButtonStyle(source.button_style),
    font_sans: normalizeFontSpec(source.font_sans, DEFAULT_FONT_SANS),
    font_mono: normalizeFontSpec(source.font_mono, DEFAULT_FONT_MONO),
  };
}

export function applyThemeCssVariables(themeInput: ThemeData) {
  const t = normalizeTheme(themeInput);
  const accentBase = pickAccentColor(t.buttons_foreground);
  const root = document.documentElement;
  const style = root.style;

  style.setProperty("--accounts-bg", t.accounts_background);
  style.setProperty("--accounts-fg", t.accounts_foreground);
  style.setProperty("--buttons-bg", t.buttons_background);
  style.setProperty("--buttons-fg", t.buttons_foreground);
  style.setProperty("--buttons-bc", t.buttons_border);
  style.setProperty("--toggle-on-bg", t.toggle_on_background ?? accentBase);
  style.setProperty("--toggle-off-bg", t.toggle_off_background ?? DEFAULT_THEME.buttons_border);
  style.setProperty("--toggle-knob-bg", t.toggle_knob_background ?? "#ffffff");
  style.setProperty("--forms-bg", t.forms_background);
  style.setProperty("--forms-fg", t.forms_foreground);
  style.setProperty("--textboxes-bg", t.textboxes_background);
  style.setProperty("--textboxes-fg", t.textboxes_foreground);
  style.setProperty("--textboxes-bc", t.textboxes_border);
  style.setProperty("--labels-bg", t.label_transparent ? "transparent" : t.label_background);
  style.setProperty("--labels-fg", t.label_foreground);

  style.setProperty("--app-bg", t.forms_background);
  style.setProperty("--app-fg", t.forms_foreground);
  style.setProperty("--panel-bg", t.accounts_background);
  style.setProperty("--panel-fg", t.accounts_foreground);
  style.setProperty("--panel-muted", rgba(t.label_foreground, 0.85, DEFAULT_THEME.label_foreground));
  style.setProperty("--panel-soft", rgba(t.buttons_background, 0.55, DEFAULT_THEME.buttons_background));
  style.setProperty("--border-color", t.buttons_border);
  style.setProperty("--row-hover", rgba(t.buttons_background, 0.32, DEFAULT_THEME.buttons_background));
  style.setProperty("--row-selected", rgba(t.buttons_background, 0.62, DEFAULT_THEME.buttons_background));
  style.setProperty("--input-focus", rgba(accentBase, 0.22, DEFAULT_ACCENT));
  style.setProperty("--accent-color", accentBase);
  style.setProperty("--accent-soft", rgba(accentBase, 0.2, DEFAULT_ACCENT));
  style.setProperty("--accent-strong", rgba(accentBase, 0.38, DEFAULT_ACCENT));
  style.setProperty("--toggle-on-shadow", rgba(t.toggle_on_background ?? accentBase, 0.4, DEFAULT_ACCENT));
  style.setProperty("--titlebar-bg", t.dark_top_bar ? "#09090b" : t.forms_background);
  style.setProperty("--titlebar-fg", t.dark_top_bar ? "#a1a1aa" : t.forms_foreground);
  style.setProperty("--avatar-filter", t.light_images ? "brightness(1.08) contrast(1.03) saturate(1.08)" : "none");
  style.setProperty("--font-sans", fontStack(t.font_sans ?? DEFAULT_FONT_SANS));
  style.setProperty("--font-mono", fontStack(t.font_mono ?? DEFAULT_FONT_MONO));

  const buttonStyle = normalizeButtonStyle(t.button_style);
  style.setProperty("--button-radius", buttonStyle === "Standard" ? "6px" : buttonStyle === "Popup" ? "10px" : "8px");
  style.setProperty(
    "--button-shadow",
    buttonStyle === "Popup" ? `0 8px 22px ${rgba(t.buttons_background, 0.45, DEFAULT_THEME.buttons_background)}` : "none"
  );
  style.setProperty(
    "--button-inset",
    buttonStyle === "Standard" ? `inset 0 1px 0 ${rgba(t.buttons_foreground, 0.2, DEFAULT_THEME.buttons_foreground)}` : "none"
  );

  root.dataset.buttonStyle = buttonStyle;
  root.dataset.showHeaders = t.show_headers ? "true" : "false";

  void applyThemeFontLoading(t.font_sans ?? DEFAULT_FONT_SANS, t.font_mono ?? DEFAULT_FONT_MONO).catch(() => {});
}
