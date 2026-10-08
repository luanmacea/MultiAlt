/**
 * Gera as versões em português e espanhol do site (`site/pt/index.html` e
 * `site/es/index.html`) a partir do `site/index.html` (inglês) e das traduções
 * do `site/i18n.js`.
 *
 * Por que existe: a troca de idioma do site é feita por JavaScript, e o Google
 * indexa só o HTML que chega — sem página própria, o site nunca aparecia para
 * "gerenciador de contas roblox" ou "administrador de cuentas roblox". Cada
 * idioma ganha um endereço (`/pt/`, `/es/`) com o texto já traduzido, ligado
 * aos outros por `hreflang`.
 *
 * As páginas geradas ficam no repositório (o site não tem build: o Cloudflare
 * publica a pasta como está). O teste `locales.test.ts` falha se elas ficarem
 * desatualizadas — rode `bun scripts/site/build-locales.ts` depois de mexer no
 * `index.html` ou no `i18n.js`.
 */
import { Window } from "happy-dom";

export const SITE_URL = "https://multialt.pages.dev/";
export const LOCALES = ["pt", "es"] as const;
export type Locale = (typeof LOCALES)[number];
export type Dict = Record<string, Record<string, string>>;

/** Valor do `lang` do `<html>` e do `og:locale` de cada idioma. */
const HTML_LANG: Record<Locale, string> = { pt: "pt-BR", es: "es" };
const OG_LOCALE: Record<Locale, string> = { pt: "pt_BR", es: "es_ES" };

/** Lê o `window.RAM_I18N` do `i18n.js` sem navegador. */
export function loadDict(i18nSource: string): Dict {
  const fakeWindow: { RAM_I18N?: Dict } = {};
  new Function("window", i18nSource)(fakeWindow);
  if (!fakeWindow.RAM_I18N) throw new Error("i18n.js não definiu window.RAM_I18N");
  return fakeWindow.RAM_I18N;
}

/** Endereço público de cada idioma (o inglês fica na raiz). */
export function localeUrl(lang: Locale | "en"): string {
  return lang === "en" ? SITE_URL : `${SITE_URL}${lang}/`;
}

/** Caminho relativo que, saindo de `/pt/`, aponta para o mesmo arquivo da raiz. */
function rebase(url: string): string {
  if (!url || /^(?:[a-z]+:|#|\/|\.\.\/)/i.test(url)) return url;
  if (url === "./") return "../";
  return `../${url}`;
}

/** Perguntas e respostas da seção de dúvidas, no texto que a página mostra. */
export function faqEntries(doc: Document): { q: string; a: string }[] {
  return Array.from(doc.querySelectorAll(".faq-list details")).map((d) => ({
    q: (d.querySelector("summary")?.textContent ?? "").trim(),
    a: (d.querySelector("p")?.textContent ?? "").trim(),
  }));
}

/** JSON-LD `FAQPage` montado a partir das dúvidas da página. */
export function faqJsonLd(doc: Document): string {
  const data = {
    "@context": "https://schema.org",
    "@type": "FAQPage",
    mainEntity: faqEntries(doc).map(({ q, a }) => ({
      "@type": "Question",
      name: q,
      acceptedAnswer: { "@type": "Answer", text: a },
    })),
  };
  return `\n  ${JSON.stringify(data, null, 2).replace(/\n/g, "\n  ")}\n  `;
}

function parse(html: string): Document {
  const window = new Window({ url: SITE_URL });
  return new window.DOMParser().parseFromString(html, "text/html") as unknown as Document;
}

/** Reescreve o bloco `#faq-ld` do HTML de entrada (string, para não reformatar o arquivo). */
export function withFaqJsonLd(html: string): string {
  const doc = parse(html);
  const re = /(<script type="application\/ld\+json" id="faq-ld">)[\s\S]*?(<\/script>)/;
  if (!re.test(html)) throw new Error('index.html sem <script type="application/ld+json" id="faq-ld">');
  return html.replace(re, (_m, open: string, close: string) => open + faqJsonLd(doc) + close);
}

/** Monta a página de um idioma a partir do `index.html` em inglês. */
export function renderLocale(englishHtml: string, dict: Dict, lang: Locale): string {
  const tr = dict[lang];
  if (!tr) throw new Error(`i18n.js sem o idioma ${lang}`);
  const doc = parse(englishHtml);
  const root = doc.documentElement;
  root.setAttribute("lang", HTML_LANG[lang]);
  root.setAttribute("data-site-lang", lang);

  doc.querySelectorAll("[data-i18n]").forEach((el) => {
    const value = tr[el.getAttribute("data-i18n") ?? ""];
    if (value != null) el.innerHTML = value;
  });
  doc.querySelectorAll("[data-i18n-attr]").forEach((el) => {
    const [attr, key] = (el.getAttribute("data-i18n-attr") ?? "").split(":");
    const value = tr[key];
    if (attr && value != null) el.setAttribute(attr, value);
  });

  // Endereço próprio: canônico e og:url apontam para a página do idioma.
  doc.querySelector('link[rel="canonical"]')?.setAttribute("href", localeUrl(lang));
  doc.querySelector('meta[property="og:url"]')?.setAttribute("content", localeUrl(lang));
  const ogDesc = doc.querySelector('meta[property="og:description"]');
  if (ogDesc && tr["hero.title"]) ogDesc.setAttribute("content", tr["hero.title"]);
  const ogLocale = doc.querySelector('meta[property="og:locale"]');
  if (ogLocale) ogLocale.setAttribute("content", OG_LOCALE[lang]);

  // Os arquivos moram na raiz: daqui de /pt/ é um nível acima.
  doc.querySelectorAll("[src]").forEach((el) => el.setAttribute("src", rebase(el.getAttribute("src") ?? "")));
  doc.querySelectorAll("a[href], link[href]").forEach((el) => el.setAttribute("href", rebase(el.getAttribute("href") ?? "")));

  const faq = doc.getElementById("faq-ld");
  if (faq) faq.textContent = faqJsonLd(doc);

  return `<!doctype html>\n${root.outerHTML}\n`;
}

/** Chaves `data-i18n` do HTML que um idioma não traduz (cairiam no inglês). */
export function missingKeys(englishHtml: string, dict: Dict, lang: Locale): string[] {
  const doc = parse(englishHtml);
  const keys = new Set<string>();
  doc.querySelectorAll("[data-i18n]").forEach((el) => keys.add(el.getAttribute("data-i18n") ?? ""));
  doc.querySelectorAll("[data-i18n-attr]").forEach((el) => keys.add((el.getAttribute("data-i18n-attr") ?? "").split(":")[1]));
  return [...keys].filter((k) => k && dict[lang]?.[k] == null).sort();
}
