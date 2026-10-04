import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { LOCALES, SITE_URL, loadDict, localeUrl, missingKeys, renderLocale, withFaqJsonLd } from "./locales";

const SITE = join(__dirname, "..", "..", "site");
const read = (...p: string[]) => readFileSync(join(SITE, ...p), "utf8");
const english = read("index.html");
const dict = loadDict(read("i18n.js"));

/** Endereços `hreflang` declarados no `<head>` de uma página. */
function hreflangs(html: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of html.matchAll(/<link rel="alternate" hreflang="([^"]+)" href="([^"]+)">/g)) out[m[1]] = m[2];
  return out;
}

const pages = { en: english, ...Object.fromEntries(LOCALES.map((l) => [l, read(l, "index.html")])) } as Record<string, string>;

describe("site em vários idiomas", () => {
  it("as páginas /pt/ e /es/ estão em dia com o index.html e o i18n.js", () => {
    // Falhou? Rode `bun scripts/site/build-locales.ts` e commite o resultado.
    expect(withFaqJsonLd(english)).toBe(english);
    for (const lang of LOCALES) expect(read(lang, "index.html")).toBe(renderLocale(english, dict, lang));
  });

  it("todo texto do site tem tradução em português e espanhol", () => {
    for (const lang of LOCALES) expect(missingKeys(english, dict, lang)).toEqual([]);
  });

  it("cada página se declara canônica e aponta para as outras pelo hreflang", () => {
    const expected = { en: SITE_URL, pt: localeUrl("pt"), es: localeUrl("es"), "x-default": SITE_URL };
    for (const [lang, html] of Object.entries(pages)) {
      expect(hreflangs(html)).toEqual(expected);
      expect(html).toContain(`<link rel="canonical" href="${localeUrl(lang as "en")}">`);
    }
  });

  it("as páginas de idioma traduzem o texto e acham os arquivos da raiz", () => {
    const pt = pages.pt;
    expect(pt).toContain('<html lang="pt-BR" data-site-lang="pt">');
    expect(pt).toContain(dict.pt["hero.title"]);
    expect(pt).toContain('href="../styles.css"');
    expect(pt).toContain('src="../main.js"');
    expect(pt).not.toMatch(/(?:src|href)="assets\//);
    for (const m of pt.matchAll(/(?:src|href)="\.\.\/([^"#?]+)"/g)) {
      const target = m[1].endsWith("/") ? join(m[1], "index.html") : m[1];
      expect(existsSync(join(SITE, target)), m[1]).toBe(true);
    }
  });

  it("o FAQ estruturado tem as mesmas perguntas da página, no idioma dela", () => {
    const faq = (html: string) => JSON.parse(html.match(/<script type="application\/ld\+json" id="faq-ld">([\s\S]*?)<\/script>/)![1]);
    expect(faq(english).mainEntity[0].name).toBe("Why does my antivirus flag it?");
    expect(faq(pages.pt).mainEntity[0].name).toBe(dict.pt.q1);
    expect(faq(pages.es).mainEntity).toHaveLength(faq(english).mainEntity.length);
  });

  it("o sitemap lista as páginas de idioma", () => {
    const sitemap = read("sitemap.xml");
    for (const lang of ["en", ...LOCALES] as const) expect(sitemap).toContain(`<loc>${localeUrl(lang)}</loc>`);
  });
});
