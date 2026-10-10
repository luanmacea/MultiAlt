import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * A edição completa só aparece nos anexos da release do GitHub (decisão do
 * dono, 10/10/2026): README, site e páginas de idioma recomendam só o
 * `MultiAlt-Setup.msi`, e o download do site nunca troca para a completa.
 */
const ROOT = join(import.meta.dirname, "..", "..");
const read = (...p: string[]) => readFileSync(join(ROOT, ...p), "utf8");

const PUBLIC_PAGES = [
  ["README.md"],
  ["README.pt-BR.md"],
  ["site", "index.html"],
  ["site", "pt", "index.html"],
  ["site", "es", "index.html"],
  ["site", "i18n.js"],
  ["site", "main.js"],
  ["site", "llms.txt"],
];

/** A `pickAssets` do site/main.js, isolada para rodar fora do navegador. */
function loadPickAssets(): (release: { assets: { name: string }[] }) => Record<string, { name: string } | null> {
  const src = read("site", "main.js");
  const start = src.indexOf("function pickAssets(");
  expect(start).toBeGreaterThan(-1);
  const end = src.indexOf("return by;", start);
  const body = src.slice(start, src.indexOf("}", end) + 1);
  return new Function(`${body}\nreturn pickAssets;`)();
}

describe("a edição completa fora das páginas públicas", () => {
  it("nenhuma página pública cita o arquivo nem a versão completa", () => {
    for (const p of PUBLIC_PAGES) {
      const text = read(...p);
      const name = p.join("/");
      expect(text, name).not.toMatch(/full-nexus-ws|Full-Setup|full-toggle|"fm\.full"|"fm\.note"/);
      expect(text, name).not.toMatch(/\b(full version|versão completa|versión completa)\b/i);
    }
  });

  it("o download do site é sempre o da edição padrão", () => {
    const pickAssets = loadPickAssets();
    const picked = pickAssets({
      assets: [
        { name: "MultiAlt-Setup.msi" },
        { name: "MultiAlt_1.3.0_Full-Setup.msi" },
        { name: "MultiAlt_1.3.0_Full-Portable.exe" },
        { name: "MultiAlt_1.2.0_x64_en-US_full-nexus-ws.msi" },
        { name: "MultiAlt_1.2.0_x64_portable_full-nexus-ws.exe" },
        { name: "MultiAlt_1.3.0_x64_portable.exe" },
        { name: "zz-MultiAlt_1.3.0_Full-Setup.msi.sig" },
      ],
    });
    expect(picked.msi?.name).toBe("MultiAlt-Setup.msi");
    expect(picked.portable?.name).toBe("MultiAlt_1.3.0_x64_portable.exe");
  });

  it("release só com a completa não vira link para ela", () => {
    const pickAssets = loadPickAssets();
    const picked = pickAssets({ assets: [{ name: "MultiAlt_1.3.0_Full-Setup.msi" }] });
    expect(picked.msi).toBeNull();
  });
});
