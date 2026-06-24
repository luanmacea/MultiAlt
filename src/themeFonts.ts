import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import type { ThemeFontSpec } from "./types";
import { buildGoogleFontsHref } from "./fontPresets";

const GOOGLE_LINK_ID = "ram-google-fonts";
const LOCAL_STYLE_ID = "ram-local-font-faces";

let lastGoogleHref: string | null = null;
let lastLocalKey = "";

function ensureLink(href: string | null) {
  const head = document.head;
  const existing = document.getElementById(GOOGLE_LINK_ID) as HTMLLinkElement | null;

  if (!href) {
    if (existing) existing.remove();
    lastGoogleHref = null;
    return;
  }

  if (lastGoogleHref === href && existing?.href) return;

  const el = existing ?? document.createElement("link");
  el.id = GOOGLE_LINK_ID;
  el.rel = "stylesheet";
  el.href = href;
  if (!existing) head.appendChild(el);
  lastGoogleHref = href;
}

function ensureStyle(cssText: string) {
  const head = document.head;
  const existing = document.getElementById(LOCAL_STYLE_ID) as HTMLStyleElement | null;

  if (!cssText.trim()) {
    if (existing) existing.remove();
    lastLocalKey = "";
    return;
  }

  const el = existing ?? document.createElement("style");
  el.id = LOCAL_STYLE_ID;
  el.textContent = cssText;
  if (!existing) head.appendChild(el);
}

function escapeCssString(value: string) {
  return value.replace(/\\/g, "\\\\").replace(/'/g, "\\'");
}

function localKeyFor(specs: ThemeFontSpec[]) {
  const locals = specs
    .filter((s) => s.source === "local" && s.local?.file)
    .map((s) => `${s.family}|${s.local!.file}|${s.local!.weight}|${s.local!.style}`)
    .sort();
  return locals.join("||");
}

export async function applyThemeFontLoading(fontSans: ThemeFontSpec, fontMono: ThemeFontSpec) {
  // Google CSS link
  const href = buildGoogleFontsHref([fontSans, fontMono]);
  ensureLink(href);

  // Local @font-face rules (Tauri-only)
  if (!isTauri()) {
    ensureStyle("");
    return;
  }

  const specs = [fontSans, fontMono];
  const nextKey = localKeyFor(specs);
  if (nextKey === lastLocalKey) return;

  try {
    const locals = specs.filter((s) => s.source === "local" && !!s.local?.file);
    if (locals.length === 0) {
      ensureStyle("");
      lastLocalKey = nextKey;
      return;
    }

    const resolved = await Promise.all(
      locals.map(async (spec) => {
        const file = spec.local!.file;
        const abs = await invoke<string>("resolve_theme_font_asset", { file });
        const url = convertFileSrc(abs);
        return {
          family: spec.family,
          url,
          weight: spec.local!.weight ?? 400,
          style: spec.local!.style ?? "normal",
        };
      })
    );

    const css = resolved
      .map((f) => {
        const family = escapeCssString(f.family || "Custom Font");
        const style = f.style === "italic" ? "italic" : "normal";
        const weight = Number.isFinite(f.weight) ? Math.max(1, Math.min(1000, Math.round(f.weight))) : 400;
        return [
          "@font-face {",
          `  font-family: '${family}';`,
          `  src: url('${escapeCssString(f.url)}');`,
          `  font-weight: ${weight};`,
          `  font-style: ${style};`,
          "  font-display: swap;",
          "}",
        ].join("\n");
      })
      .join("\n\n");

    ensureStyle(css);
    lastLocalKey = nextKey;
  } catch {
    // If resolution fails, don't pin the cache key; allow future attempts after the user fixes fonts.
    lastLocalKey = "";
    ensureStyle("");
  }
}
