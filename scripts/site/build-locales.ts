/**
 * Regrava `site/pt/index.html`, `site/es/index.html` e o FAQ estruturado do
 * `site/index.html`. Rode depois de mexer no texto do site:
 *
 *   bun scripts/site/build-locales.ts
 */
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { LOCALES, loadDict, missingKeys, renderLocale, withFaqJsonLd } from "./locales";

const SITE = join(import.meta.dir, "..", "..", "site");
const indexPath = join(SITE, "index.html");

const english = withFaqJsonLd(readFileSync(indexPath, "utf8"));
writeFileSync(indexPath, english);
const dict = loadDict(readFileSync(join(SITE, "i18n.js"), "utf8"));

for (const lang of LOCALES) {
  const missing = missingKeys(english, dict, lang);
  if (missing.length) console.warn(`${lang}: sem tradução (fica em inglês): ${missing.join(", ")}`);
  mkdirSync(join(SITE, lang), { recursive: true });
  writeFileSync(join(SITE, lang, "index.html"), renderLocale(english, dict, lang));
  console.log(`site/${lang}/index.html`);
}
