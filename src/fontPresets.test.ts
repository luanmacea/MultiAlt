import { describe, expect, it } from "vitest";
import type { ThemeFontSpec } from "./types";
import {
  MONO_GOOGLE_PRESETS,
  SANS_GOOGLE_PRESETS,
  buildGoogleFontsHref,
  googlePresetToSpec,
} from "./fontPresets";

function spec(family: string, weights?: number[], source: ThemeFontSpec["source"] = "google"): ThemeFontSpec {
  return {
    source,
    family,
    fallbacks: ["sans-serif"],
    ...(weights ? { google: { weights } } : {}),
  };
}

describe("buildGoogleFontsHref", () => {
  it("returns null when no spec uses Google Fonts", () => {
    expect(buildGoogleFontsHref([])).toBeNull();
    expect(buildGoogleFontsHref([spec("system-ui", undefined, "system")])).toBeNull();
    expect(
      buildGoogleFontsHref([
        { source: "local", family: "X", fallbacks: [], local: { file: "x.ttf", weight: 400, style: "normal" } },
      ])
    ).toBeNull();
  });

  it("returns null when every google family name is blank", () => {
    expect(buildGoogleFontsHref([spec("   ", [400])])).toBeNull();
  });

  it("builds one family segment per font with its weights", () => {
    expect(buildGoogleFontsHref([spec("Inter", [400, 700])])).toBe(
      "https://fonts.googleapis.com/css2?family=Inter:wght@400;700&display=swap"
    );
  });

  it("encodes spaces as + and other characters percent-style", () => {
    expect(buildGoogleFontsHref([spec("Plus Jakarta Sans", [500])])).toContain(
      "family=Plus+Jakarta+Sans:wght@500"
    );
    expect(buildGoogleFontsHref([spec("Font&Co", [400])])).toContain("family=Font%26Co");
  });

  it("merges, dedupes and sorts weights for a repeated family", () => {
    expect(buildGoogleFontsHref([spec("Inter", [700, 400]), spec("Inter", [400, 300])])).toBe(
      "https://fonts.googleapis.com/css2?family=Inter:wght@300;400;700&display=swap"
    );
  });

  it("defaults to 400;500 when a family declares no weights", () => {
    expect(buildGoogleFontsHref([spec("Inter", [])])).toContain("family=Inter:wght@400;500");
    expect(buildGoogleFontsHref([spec("Inter")])).toContain("family=Inter:wght@400;500");
  });

  it("joins several families with &", () => {
    const href = buildGoogleFontsHref([spec("Outfit", [400]), spec("Fira Code", [500])])!;
    expect(href.startsWith("https://fonts.googleapis.com/css2?")).toBe(true);
    expect(href).toContain("family=Outfit:wght@400&family=Fira+Code:wght@500");
    expect(href.endsWith("&display=swap")).toBe(true);
  });
});

describe("presets", () => {
  it("converts a preset into a google font spec", () => {
    const preset = SANS_GOOGLE_PRESETS[0];
    expect(googlePresetToSpec(preset)).toEqual({
      source: "google",
      family: preset.family,
      fallbacks: preset.fallbacks,
      google: { weights: preset.weights },
    });
  });

  it("ships unique ids and the expected kinds", () => {
    const ids = [...SANS_GOOGLE_PRESETS, ...MONO_GOOGLE_PRESETS].map((p) => p.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(SANS_GOOGLE_PRESETS.every((p) => p.kind === "sans")).toBe(true);
    expect(MONO_GOOGLE_PRESETS.every((p) => p.kind === "mono")).toBe(true);
  });

  it("gives every preset at least one weight and a fallback chain", () => {
    for (const preset of [...SANS_GOOGLE_PRESETS, ...MONO_GOOGLE_PRESETS]) {
      expect(preset.weights.length).toBeGreaterThan(0);
      expect(preset.fallbacks.length).toBeGreaterThan(0);
    }
  });

  it("produces a usable href from any preset", () => {
    for (const preset of MONO_GOOGLE_PRESETS) {
      expect(buildGoogleFontsHref([googlePresetToSpec(preset)])).toContain(
        `:wght@${preset.weights.join(";")}`
      );
    }
  });
});
