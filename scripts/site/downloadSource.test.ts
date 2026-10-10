import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// Origem do download: o botão do site leva a /get/<origem>/, que o Cloudflare
// Web Analytics conta por caminho antes de mandar para o instalador. A origem
// é de onde a pessoa chegou ao site (site/download-source.js).
const SITE = join(__dirname, "..", "..", "site");
// É um script de navegador (sem módulo): roda com um `module` emprestado.
const mod = { exports: {} as unknown };
new Function("module", readFileSync(join(SITE, "download-source.js"), "utf8"))(mod);
const ds = mod.exports as {
  SOURCES: string[];
  fromLanding: (referrer: string, search: string, host: string) => string;
};
const HOST = "multialt.pages.dev";

describe("origem do download", () => {
  it("reconhece quem chega do YouTube, das buscas, do GitHub e das listagens", () => {
    expect(ds.fromLanding("https://www.youtube.com/", "", HOST)).toBe("youtube");
    expect(ds.fromLanding("https://m.youtube.com/watch?v=x", "", HOST)).toBe("youtube");
    expect(ds.fromLanding("https://www.google.com.br/", "", HOST)).toBe("google");
    expect(ds.fromLanding("https://www.bing.com/search?q=x", "", HOST)).toBe("bing");
    expect(ds.fromLanding("https://search.brave.com/", "", HOST)).toBe("search");
    expect(ds.fromLanding("https://duckduckgo.com/", "", HOST)).toBe("search");
    expect(ds.fromLanding("https://github.com/luanmacea/MultiAlt", "", HOST)).toBe("github");
    expect(ds.fromLanding("https://dev.to/luanmacea/x", "", HOST)).toBe("devto");
    expect(ds.fromLanding("https://alternativeto.net/software/multialt/", "", HOST)).toBe("alternativeto");
  });

  it("sem referência, ou vindo do próprio site, é acesso direto", () => {
    expect(ds.fromLanding("", "", HOST)).toBe("direct");
    expect(ds.fromLanding("https://multialt.pages.dev/pt/", "", HOST)).toBe("direct");
  });

  it("o ?ref= da página de entrada vence o referenciador, se for uma origem conhecida", () => {
    expect(ds.fromLanding("https://www.google.com/", "?ref=youtube", HOST)).toBe("youtube");
    expect(ds.fromLanding("https://www.google.com/", "?ref=qualquer-coisa", HOST)).toBe("google");
  });

  it("um site que não está na lista vira 'other'", () => {
    expect(ds.fromLanding("https://www.reddit.com/r/roblox", "", HOST)).toBe("other");
  });

  it("toda origem tem a sua página /get/, e o README tem a dele", () => {
    // Uma origem sem página levaria o botão de download a um 404.
    for (const src of [...ds.SOURCES, "readme"]) {
      expect(existsSync(join(SITE, "get", src, "index.html")), src).toBe(true);
    }
  });

  it("as páginas /get/ não entram no Google e levam ao instalador", () => {
    for (const src of [...ds.SOURCES, "readme"]) {
      const html = readFileSync(join(SITE, "get", src, "index.html"), "utf8");
      expect(html).toContain('<meta name="robots" content="noindex">');
      expect(html).toContain("https://github.com/luanmacea/MultiAlt/releases/latest/download/MultiAlt-Setup.msi");
    }
  });

  it("o botão do README passa pela página /get/readme/", () => {
    for (const readme of ["README.md", "README.pt-BR.md"]) {
      const text = readFileSync(join(__dirname, "..", "..", readme), "utf8");
      expect(text).toContain('href="https://multialt.pages.dev/get/readme/"');
    }
  });
});
