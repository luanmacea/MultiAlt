import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, posix } from "node:path";
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

/** Guias escritos à mão: `site/<pasta>/index.html` e `site/<pt|es>/<pasta>/index.html`. */
function guidePages(): string[] {
  const dirs = (base: string) =>
    readdirSync(join(SITE, base), { withFileTypes: true })
      .filter((d) => d.isDirectory() && existsSync(join(SITE, base, d.name, "index.html")))
      .map((d) => posix.join(base, d.name) + "/");
  const top = dirs(".").filter((d) => !["assets/", ...LOCALES.map((l) => `${l}/`)].includes(d));
  return [...top, ...LOCALES.flatMap((l) => dirs(l))];
}

/** Endereço público -> caminho do arquivo dentro de `site/`. */
const fileOf = (url: string) => join(SITE, url.slice(SITE_URL.length), "index.html");

describe("guias em vários idiomas", () => {
  const guides = guidePages();
  const HTML_LANG = { en: "en", pt: "pt-BR", es: "es" } as Record<string, string>;

  it("acha os guias (os dois de 04/10/2026 em cada idioma, no mínimo)", () => {
    expect(guides.length).toBeGreaterThanOrEqual(6);
  });

  it("o hreflang de cada guia é recíproco, se declara canônico e casa com o idioma da página", () => {
    for (const path of guides) {
      const html = read(path, "index.html");
      const self = SITE_URL + path;
      const alts = hreflangs(html);
      expect(Object.keys(alts).sort(), path).toEqual(["en", "es", "pt", "x-default"]);
      expect(alts["x-default"], path).toBe(alts.en);
      expect(html, path).toContain(`<link rel="canonical" href="${self}">`);
      const lang = Object.keys(HTML_LANG).find((l) => alts[l] === self);
      expect(lang, `${path} não aparece no próprio hreflang`).toBeDefined();
      expect(html, path).toContain(`<html lang="${HTML_LANG[lang!]}">`);
      expect(html, path).toContain(`"inLanguage": "${HTML_LANG[lang!]}"`);
      // Cada página apontada devolve exatamente o mesmo conjunto.
      for (const target of Object.values(alts)) {
        expect(existsSync(fileOf(target)), target).toBe(true);
        expect(hreflangs(readFileSync(fileOf(target), "utf8")), `${path} -> ${target}`).toEqual(alts);
      }
    }
  });

  it("os guias acham os arquivos que usam e estão no sitemap", () => {
    const sitemap = read("sitemap.xml");
    for (const path of guides) {
      const html = read(path, "index.html");
      expect(sitemap).toContain(`<loc>${SITE_URL}${path}</loc>`);
      for (const m of html.matchAll(/(?:src|href)="(\.\.\/[^"#?]*)/g)) {
        const target = join(SITE, path, m[1]);
        const file = m[1].endsWith("/") || m[1].endsWith("..") ? join(target, "index.html") : target;
        expect(existsSync(file), `${path}: ${m[1]}`).toBe(true);
      }
    }
  });

  it("o rodapé de cada página inicial leva aos guias do mesmo idioma", () => {
    for (const [key, enPath] of [["ft.guideMultiHref", "multiple-roblox-accounts/"], ["ft.guideSafeHref", "is-multialt-safe/"]]) {
      expect(english).toContain(`href="${enPath}"`);
      const alts = hreflangs(read(enPath, "index.html"));
      for (const lang of LOCALES) {
        const value = dict[lang][key];
        expect(SITE_URL + value, `${lang} ${key}`).toBe(alts[lang]);
        // Visto de /pt/ ou /es/, o caminho da raiz ganha "../".
        expect(pages[lang]).toContain(`href="../${value}"`);
        expect(existsSync(join(SITE, lang, "..", value, "index.html"))).toBe(true);
      }
    }
  });
});
