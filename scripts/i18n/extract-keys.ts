import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";

type JsonDict = Record<string, string>;

const repoRoot = path.resolve(import.meta.dir, "..", "..");
const srcRoot = path.join(repoRoot, "src");
const enPath = path.join(srcRoot, "locales", "en", "common.json");

function walk(dir: string, out: string[] = []): string[] {
  for (const ent of readdirSync(dir)) {
    const full = path.join(dir, ent);
    const st = statSync(full);
    if (st.isDirectory()) {
      if (ent === "locales" || ent === "i18n") continue;
      walk(full, out);
      continue;
    }
    if (!/\.(ts|tsx)$/.test(ent)) continue;
    out.push(full);
  }
  return out;
}

function loadJson(file: string): JsonDict {
  return JSON.parse(readFileSync(file, "utf8")) as JsonDict;
}

function saveJson(file: string, obj: JsonDict) {
  writeFileSync(file, JSON.stringify(obj, null, 2) + "\n", "utf8");
}

function unescapeTsString(s: string): string {
  // Minimal unescape for common escapes; this is not a full TS string parser.
  return s
    .replace(/\\\\/g, "\\")
    .replace(/\\n/g, "\n")
    .replace(/\\r/g, "\r")
    .replace(/\\t/g, "\t")
    .replace(/\\"/g, '"')
    .replace(/\\'/g, "'");
}

function isProbablyTranslatableKey(s: string): boolean {
  const v = s.trim();
  if (!v) return false;
  if (v.length === 1) return false;

  // File paths, URLs, or obvious non-UI payloads.
  if (/^[a-zA-Z]:\\\\/.test(v)) return false;
  if (/\\\//.test(v)) return false;
  if (/https?:\/\//.test(v)) return false;

  // Skip tokens / hashes / IDs that are unlikely to be UI strings.
  if (/^[0-9a-f]{20,}$/i.test(v)) return false;
  if (/^[0-9]{6,}$/.test(v)) return false;

  // Skip strings that are mostly symbols.
  const letters = (v.match(/[A-Za-z\u00C0-\u017F]/g) ?? []).length;
  if (letters === 0) return false;

  // Allow i18next-style placeholders like {{count}}, but skip template strings.
  if (/\$\{/.test(v)) return false;

  return true;
}

function addKey(set: Set<string>, raw: string) {
  const v = unescapeTsString(raw);
  if (!isProbablyTranslatableKey(v)) return;
  set.add(v);
}

function extractFromFile(text: string): Set<string> {
  const keys = new Set<string>();

  // t("...") / tr("...") / t('...') / tr('...')
  // Handle escaped quotes so we don't truncate on \" or \'.
  for (const m of text.matchAll(/\b(?:t|tr)\(\s*"((?:\\.|[^"\\])*)"/g)) addKey(keys, m[1]);
  for (const m of text.matchAll(/\b(?:t|tr)\(\s*'((?:\\.|[^'\\])*)'/g)) addKey(keys, m[1]);

  // Common UI props that are later passed through t() inside components.
  for (const m of text.matchAll(/\b(?:label|description|placeholder|suffix|title|tooltip|alt|aria-label)\s*=\s*"((?:\\.|[^"\\])*)"/g)) addKey(keys, m[1]);
  for (const m of text.matchAll(/\b(?:label|description|placeholder|suffix|title|tooltip|alt|aria-label)\s*=\s*'((?:\\.|[^'\\])*)'/g)) addKey(keys, m[1]);

  // Common object-style labels, e.g. options arrays: { label: "English", value: "en" }.
  for (const m of text.matchAll(/\b(?:label|description|placeholder|suffix|title|tooltip|alt)\s*:\s*"((?:\\.|[^"\\])*)"/g)) addKey(keys, m[1]);
  for (const m of text.matchAll(/\b(?:label|description|placeholder|suffix|title|tooltip|alt)\s*:\s*'((?:\\.|[^'\\])*)'/g)) addKey(keys, m[1]);

  // JSX fragment labels like:
  // label={<>
  //   Enable Web Server<RestartBadge />
  // </>}
  const extractTextNodes = (chunk: string) => {
    // Capture plain text in-between tags. This is intentionally conservative.
    for (const m of chunk.matchAll(/(^|>)([^<>{}\n][^<>{}]*?)(?=<|$)/g)) {
      addKey(keys, m[2]);
    }
  };

  for (const m of text.matchAll(/\blabel\s*=\s*{\s*<>\s*([\s\S]*?)\s*<\/>\s*}/g)) extractTextNodes(m[1]);
  for (const m of text.matchAll(/\blabel\s*=\s*<>\s*([\s\S]*?)\s*<\/>/g)) extractTextNodes(m[1]);

  // <SectionLabel>Text</SectionLabel>
  // Components that translate plain-text children internally.
  for (const m of text.matchAll(
    /<(SectionLabel|SectionHeader|WarningBadge|UtilButton)>\s*([^<{][^<]*?)\s*<\/\1>/g
  )) {
    addKey(keys, m[2]);
  }

  return keys;
}

function main() {
  const files = walk(srcRoot);
  const allKeys = new Set<string>();

  for (const f of files) {
    const text = readFileSync(f, "utf8");
    for (const k of extractFromFile(text)) allKeys.add(k);
  }

  const en = loadJson(enPath);
  const missing = [...allKeys].filter((k) => en[k] === undefined).sort((a, b) => a.localeCompare(b));

  for (const k of missing) {
    en[k] = k;
  }

  saveJson(enPath, en);

  // Helpful output for follow-up seeding in other locales.
  // eslint-disable-next-line no-console
  console.log(`i18n: scanned ${files.length} files, found ${allKeys.size} keys, added ${missing.length} new keys to en/common.json`);
}

main();
