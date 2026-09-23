import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ThemeFontSpec } from "./types";

let tauri = false;
const invokeMock = vi.fn(async (_cmd: string, args: { file: string }) => `C:/RAMThemeFonts/${args.file}`);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: { file: string }) => invokeMock(cmd, args),
  isTauri: () => tauri,
  convertFileSrc: (p: string) => `asset://localhost/${p}`,
}));

const GOOGLE_LINK_ID = "ram-google-fonts";
const LOCAL_STYLE_ID = "ram-local-font-faces";

/**
 * Links are intercepted instead of being attached so happy-dom never tries to
 * fetch the Google Fonts stylesheet.
 */
let appendedLinks: HTMLLinkElement[] = [];
let appendSpy: ReturnType<typeof vi.spyOn>;
let realAppend: <T extends Node>(node: T) => T;

async function loadModule() {
  vi.resetModules();
  return (await import("./themeFonts")).applyThemeFontLoading;
}

function google(family: string, weights: number[] = [400]): ThemeFontSpec {
  return { source: "google", family, fallbacks: ["sans-serif"], google: { weights } };
}

function local(family: string, file: string, weight = 400, style: "normal" | "italic" = "normal"): ThemeFontSpec {
  return { source: "local", family, fallbacks: ["sans-serif"], local: { file, weight, style } };
}

const systemSans: ThemeFontSpec = { source: "system", family: "system-ui", fallbacks: ["sans-serif"] };
const systemMono: ThemeFontSpec = { source: "system", family: "ui-monospace", fallbacks: ["monospace"] };

function styleText() {
  return document.getElementById(LOCAL_STYLE_ID)?.textContent ?? null;
}

beforeEach(() => {
  tauri = false;
  invokeMock.mockClear();
  appendedLinks = [];
  document.getElementById(GOOGLE_LINK_ID)?.remove();
  document.getElementById(LOCAL_STYLE_ID)?.remove();
  realAppend = document.head.appendChild.bind(document.head);
  appendSpy = vi.spyOn(document.head, "appendChild").mockImplementation(((node: Node) => {
    if ((node as Element).tagName === "LINK") {
      appendedLinks.push(node as HTMLLinkElement);
      return node;
    }
    return realAppend(node);
  }) as typeof document.head.appendChild);
});

afterEach(() => {
  appendSpy.mockRestore();
});

describe("applyThemeFontLoading (browser)", () => {
  it("creates a Google Fonts stylesheet link for google specs", async () => {
    const apply = await loadModule();
    await apply(google("Outfit", [400, 700]), google("JetBrains Mono", [400]));

    expect(appendedLinks).toHaveLength(1);
    expect(appendedLinks[0].id).toBe(GOOGLE_LINK_ID);
    expect(appendedLinks[0].rel).toBe("stylesheet");
    expect(appendedLinks[0].getAttribute("href")).toBe(
      "https://fonts.googleapis.com/css2?family=Outfit:wght@400;700&family=JetBrains+Mono:wght@400&display=swap"
    );
  });

  it("removes an existing link when no google font is used", async () => {
    const existing = document.createElement("link");
    existing.id = GOOGLE_LINK_ID;
    existing.rel = "stylesheet";
    realAppend(existing);

    const apply = await loadModule();
    await apply(systemSans, systemMono);

    expect(document.getElementById(GOOGLE_LINK_ID)).toBeNull();
    expect(appendedLinks).toHaveLength(0);
  });

  it("does not emit @font-face rules outside Tauri", async () => {
    const apply = await loadModule();
    await apply(local("My Font", "my.ttf"), systemMono);

    expect(invokeMock).not.toHaveBeenCalled();
    expect(styleText()).toBeNull();
  });
});

describe("applyThemeFontLoading (Tauri)", () => {
  beforeEach(() => {
    tauri = true;
  });

  it("resolves local font files and writes @font-face rules", async () => {
    const apply = await loadModule();
    await apply(local("My Font", "my.woff2", 500, "italic"), systemMono);

    expect(invokeMock).toHaveBeenCalledWith("resolve_theme_font_asset", { file: "my.woff2" });
    const css = styleText() ?? "";
    expect(css).toContain("@font-face {");
    expect(css).toContain("font-family: 'My Font';");
    expect(css).toContain("src: url('asset://localhost/C:/RAMThemeFonts/my.woff2');");
    expect(css).toContain("font-weight: 500;");
    expect(css).toContain("font-style: italic;");
    expect(css).toContain("font-display: swap;");
  });

  it("clamps the font weight into 1..1000 and rounds it", async () => {
    const apply = await loadModule();
    await apply(local("A", "a.ttf", 5000), local("B", "b.ttf", 0));

    const css = styleText() ?? "";
    expect(css).toContain("font-weight: 1000;");
    expect(css).toContain("font-weight: 1;");
  });

  it("escapes quotes and backslashes in the family name", async () => {
    const apply = await loadModule();
    await apply(local("O'Neil\\Sans", "x.ttf"), systemMono);

    expect(styleText()).toContain("font-family: 'O\\'Neil\\\\Sans';");
  });

  it("clears the style element when no local font is configured", async () => {
    const apply = await loadModule();
    await apply(local("My Font", "my.ttf"), systemMono);
    expect(styleText()).toContain("@font-face");

    await apply(google("Inter"), systemMono);
    expect(document.getElementById(LOCAL_STYLE_ID)).toBeNull();
  });

  it("skips re-resolving identical local fonts", async () => {
    const apply = await loadModule();
    await apply(local("My Font", "my.ttf"), systemMono);
    expect(invokeMock).toHaveBeenCalledTimes(1);

    await apply(local("My Font", "my.ttf"), systemMono);
    expect(invokeMock).toHaveBeenCalledTimes(1);

    await apply(local("My Font", "other.ttf"), systemMono);
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("clears the cache when resolution fails so a later attempt retries", async () => {
    const apply = await loadModule();
    invokeMock.mockRejectedValueOnce(new Error("missing file"));

    await apply(local("My Font", "my.ttf"), systemMono);
    expect(document.getElementById(LOCAL_STYLE_ID)).toBeNull();

    await apply(local("My Font", "my.ttf"), systemMono);
    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(styleText()).toContain("@font-face");
  });
});
