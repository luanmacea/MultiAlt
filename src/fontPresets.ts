import type { ThemeFontSpec } from "./types";

export type FontKind = "sans" | "mono";

export interface GoogleFontPreset {
  kind: FontKind;
  id: string;
  label: string;
  family: string;
  weights: number[];
  fallbacks: string[];
}

export const SANS_GOOGLE_PRESETS: GoogleFontPreset[] = [
  {
    kind: "sans",
    id: "outfit",
    label: "Outfit",
    family: "Outfit",
    weights: [300, 400, 500, 600, 700],
    fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  },
  {
    kind: "sans",
    id: "inter",
    label: "Inter",
    family: "Inter",
    weights: [300, 400, 500, 600, 700],
    fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  },
  {
    kind: "sans",
    id: "roboto",
    label: "Roboto",
    family: "Roboto",
    weights: [300, 400, 500, 700],
    fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  },
  {
    kind: "sans",
    id: "poppins",
    label: "Poppins",
    family: "Poppins",
    weights: [300, 400, 500, 600, 700],
    fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  },
  {
    kind: "sans",
    id: "nunito",
    label: "Nunito",
    family: "Nunito",
    weights: [300, 400, 500, 600, 700],
    fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  },
  {
    kind: "sans",
    id: "space-grotesk",
    label: "Space Grotesk",
    family: "Space Grotesk",
    weights: [300, 400, 500, 600, 700],
    fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  },
  {
    kind: "sans",
    id: "plus-jakarta-sans",
    label: "Plus Jakarta Sans",
    family: "Plus Jakarta Sans",
    weights: [300, 400, 500, 600, 700],
    fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  },
  {
    kind: "sans",
    id: "ibm-plex-sans",
    label: "IBM Plex Sans",
    family: "IBM Plex Sans",
    weights: [300, 400, 500, 600, 700],
    fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  },
  {
    kind: "sans",
    id: "noto-sans",
    label: "Noto Sans",
    family: "Noto Sans",
    weights: [300, 400, 500, 600, 700],
    fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  },
  {
    kind: "sans",
    id: "rubik",
    label: "Rubik",
    family: "Rubik",
    weights: [300, 400, 500, 600, 700],
    fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  },
  {
    kind: "sans",
    id: "dm-sans",
    label: "DM Sans",
    family: "DM Sans",
    weights: [300, 400, 500, 600, 700],
    fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  },
  {
    kind: "sans",
    id: "manrope",
    label: "Manrope",
    family: "Manrope",
    weights: [300, 400, 500, 600, 700],
    fallbacks: ["system-ui", "-apple-system", "Segoe UI", "sans-serif"],
  },
];

export const MONO_GOOGLE_PRESETS: GoogleFontPreset[] = [
  {
    kind: "mono",
    id: "jetbrains-mono",
    label: "JetBrains Mono",
    family: "JetBrains Mono",
    weights: [400, 500],
    fallbacks: ["Cascadia Code", "Consolas", "monospace"],
  },
  {
    kind: "mono",
    id: "fira-code",
    label: "Fira Code",
    family: "Fira Code",
    weights: [400, 500, 600],
    fallbacks: ["Cascadia Code", "Consolas", "monospace"],
  },
  {
    kind: "mono",
    id: "ibm-plex-mono",
    label: "IBM Plex Mono",
    family: "IBM Plex Mono",
    weights: [400, 500, 600],
    fallbacks: ["Cascadia Code", "Consolas", "monospace"],
  },
  {
    kind: "mono",
    id: "source-code-pro",
    label: "Source Code Pro",
    family: "Source Code Pro",
    weights: [400, 500, 600],
    fallbacks: ["Cascadia Code", "Consolas", "monospace"],
  },
  {
    kind: "mono",
    id: "roboto-mono",
    label: "Roboto Mono",
    family: "Roboto Mono",
    weights: [400, 500, 600],
    fallbacks: ["Cascadia Code", "Consolas", "monospace"],
  },
  {
    kind: "mono",
    id: "space-mono",
    label: "Space Mono",
    family: "Space Mono",
    weights: [400, 700],
    fallbacks: ["Cascadia Code", "Consolas", "monospace"],
  },
];

function encodeFamily(family: string) {
  // Google Fonts uses + for spaces; other chars should be URL-encoded.
  return encodeURIComponent(family).replace(/%20/g, "+");
}

export function buildGoogleFontsHref(specs: ThemeFontSpec[]) {
  const google = specs.filter((s) => s.source === "google");
  if (google.length === 0) return null;

  const families = new Map<string, number[]>();
  for (const spec of google) {
    const weights = spec.google?.weights ?? [];
    if (!spec.family.trim()) continue;
    const existing = families.get(spec.family) ?? [];
    const merged = Array.from(new Set([...existing, ...weights])).sort((a, b) => a - b);
    families.set(spec.family, merged.length > 0 ? merged : [400, 500]);
  }

  if (families.size === 0) return null;

  const parts: string[] = [];
  for (const [family, weights] of families) {
    parts.push(`family=${encodeFamily(family)}:wght@${weights.join(";")}`);
  }
  return `https://fonts.googleapis.com/css2?${parts.join("&")}&display=swap`;
}

export function googlePresetToSpec(preset: GoogleFontPreset): ThemeFontSpec {
  return {
    source: "google",
    family: preset.family,
    fallbacks: preset.fallbacks,
    google: { weights: preset.weights },
  };
}

