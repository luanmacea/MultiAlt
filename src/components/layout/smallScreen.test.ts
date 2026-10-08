import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Janela pequena (monitor comum, ou o mínimo do app: 750x450). O jsdom não
 * mede layout, então quem acha corte é `bun run ui:audit`; estes testes travam
 * as duas garantias que ele achou faltando em 08/10/2026:
 * - a área das abas da Choose Game rola (era `overflow-hidden`, e a aba Follow,
 *   sem rolagem própria, cortava a parte de baixo sem barra de rolagem);
 * - todo diálogo tem teto de altura preso à janela (`100vh`), para nunca
 *   passar da tela — o resto do diálogo rola por dentro.
 */
const ROOT = join(__dirname, "..", "..", "..");
const read = (rel: string) => readFileSync(join(ROOT, rel), "utf8");

describe("telas pequenas", () => {
  it("a área das abas da Choose Game rola em vez de cortar", () => {
    const src = read("src/components/ChooseGameScreen.tsx");
    const block = src.slice(src.indexOf("{/* ── Tab content ── */}"));
    const container = /<div className="([^"]*)">/.exec(block)?.[1] ?? "";
    expect(container).toContain("overflow-y-auto");
    expect(container).not.toContain("overflow-hidden");
  });

  it("a aba Follow ocupa a altura da área e rola sozinha", () => {
    const src = read("src/components/ChooseGameScreen.tsx");
    const follow = src.slice(src.indexOf("function FollowTab("));
    const root = /return \(\s*<div className="([^"]*)"/.exec(follow)?.[1] ?? "";
    expect(root).toContain("h-full");
    expect(root).toContain("overflow-y-auto");
  });

  const dialogDirs = ["src/components/dialogs", "src/components/afk-mode", "src/components/server-list"];
  const dialogs = dialogDirs.flatMap((dir) =>
    readdirSync(join(ROOT, dir))
      .filter((f) => f.endsWith(".tsx") && !f.includes(".test."))
      .map((f) => `${dir}/${f}`),
  ).filter((rel) => read(rel).includes("fixed inset-0"));

  it("acha os diálogos", () => {
    expect(dialogs.length).toBeGreaterThan(5);
  });

  for (const rel of dialogs) {
    it(`${rel}: o painel tem teto de altura preso à janela`, () => {
      const src = read(rel);
      // O painel é o primeiro elemento com `theme-panel` depois do fundo escuro.
      const after = src.slice(src.indexOf("fixed inset-0"));
      const panel =
        /className=\{`([^`]*theme-panel[^`]*)`/.exec(after)?.[1] ?? /className="([^"]*theme-panel[^"]*)"/.exec(after)?.[1] ?? "";
      expect(panel, "painel do diálogo não encontrado").not.toBe("");
      expect(panel).toMatch(/max-h-\[[^\]]*100vh/);
    });
  }
});
