import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import enCommon from "../locales/en/common.json";
import deCommon from "../locales/de/common.json";
import ptCommon from "../locales/pt/common.json";
import { toneFromMessage } from "../utils/toastTone";

const en = enCommon as Record<string, string>;
const de = deCommon as Record<string, string>;
const pt = ptCommon as Record<string, string>;

/** Nomes dos `{{placeholder}}` de uma frase, ordenados — é o que precisa sobreviver à tradução. */
function placeholders(text: string): string[] {
  return [...text.matchAll(/\{\{\s*([^}]+?)\s*\}\}/g)].map((m) => m[1]).sort();
}

/**
 * Chaves que ficam idênticas ao inglês de propósito: jargão do Roblox, nome de
 * funcionalidade do app e sigla que a comunidade usa em inglês. Qualquer outra
 * chave igual ao inglês é tradução esquecida, e o teste abaixo reprova.
 */
const IDENTICAL_BY_DESIGN = new Set<string>([
  "WebServer",
  "Watcher",
  "online",
  "botting",
  "studio",
  "Multi Roblox",
  // Nome da funcionalidade na tela; traduzir "Botting" isolado criaria um
  // segundo nome para a mesma coisa (o rotulo do diálogo ja e "Botting Mode").
  "Botting",
  "Botting ({{count}})",
  "Botting Mode",
  "OK",
  "Nexus",
  "ID: {{id}}",
  "Roblox Account Manager",
  "_|WARNING:-DO-NOT-SHARE...",
  "auth ticket",
  "cookie",
  "Cookie",
  "FPS",
  "ID: ********",
  "username:password",
  "Job",
  "Job ID",
  "Offline",
  "Online",
  "Ping",
  "Place",
  "Place ID",
  "Script",
  "theme-preset",
  "user ID",
  "User ID",
  "user:pass",
  "VIP",
  "Popup",
  "{\"assets\":[{\"id\":12345}]}",
  "<city>, <countryCode>",
  "C:\\path\\ClientAppSettings.json",
  "MB",
  "min",
  "ms",
  "Normal",
  "Roblox",
  "Universe ID",
  "Botting Bot",
  "Botting Player",
  "EcoQoS",
  "Social",
  "Catppuccin",
  "Global",
  "Graphite",
  "Legacy v4 (Original)",
  "Ocean",
  "Sunset",
  "Endpoint",
  "Cooldown",
  "Status",
  "{\n  \"DFFlagTextureQualityOverrideEnabled\": true,\n  \"DFIntTextureQualityOverride\": 0\n}",
  "ABCD-EFGH",
  "alt",
  "API",
  "Beta",
  "BLOX-XXXXXXXXXXXXXXXX",
  "BloxGen",
  "Bubble",
  "Console",
  "DM Sans",
  "dump",
  "Editor",
  "Fira Code",
  "IBM Plex Mono",
  "IBM Plex Sans",
  "Info",
  "Inter",
  "Jakarta",
  "JavaScript",
  "JetBrains Mono",
  "Link",
  "LIVE",
  "Logs",
  "Manrope",
  "MIT",
  "MIT, Latte Softworks",
  "Monitor {{n}}",
  "Nexus + WebServer",
  "Noto Sans",
  "Nunito",
  "place {{placeId}}",
  "Plex",
  "Plus Jakarta Sans",
  "Poppins",
  "Pre-Hyperion",
  "Roboto",
  "Roboto Mono",
  "Rubik",
  "Scripts",
  "Soft",
  "Source Code Pro",
  "Space Grotesk",
  "Space Mono",
  "Studio",
  "Terminal",
  "UI",
  "Use ram.invoke(command, args), ram.http.request(...), ram.ws.connect/send/on(...), ram.window.snapshot(), ram.settings.get/set(), ram.modal.confirm(), ram.ui.set().",
  "version-abcdef0123456789",
  "WebSocket",
  "WhatExpsAre.Online",
  "Auth ticket",
  "PIN",
  "{{n}} backups",
  "Backups",
  "{{count}} online",
  "antes da limpeza",
  "Entra de novo quando o cliente cai.",
  "Fixture do harness.",
  "Intel(R) Wi-Fi 6 AX201",
  "Realtek PCIe GbE Family Controller",
  "ws://localhost:{{port}}/Nexus",
]);

describe("catálogo pt-BR", () => {
  it("cobre o inglês inteiro, na mesma ordem e sem chave a mais", () => {
    expect(Object.keys(pt)).toEqual(Object.keys(en));
  });

  it("não tem valor vazio nem sobra de espaço nas pontas", () => {
    const empty = Object.keys(pt).filter((k) => !pt[k].trim());
    expect(empty).toEqual([]);
    const padded = Object.keys(pt).filter((k) => pt[k] !== pt[k].trim());
    expect(padded).toEqual([]);
  });

  it("traduziu tudo que não é jargão", () => {
    const untranslated = Object.keys(pt).filter((k) => pt[k] === en[k] && !IDENTICAL_BY_DESIGN.has(k));
    expect(untranslated).toEqual([]);
  });

  it("só mantém em inglês o que está na lista de jargão", () => {
    const stale = [...IDENTICAL_BY_DESIGN].filter((k) => !(k in en) || pt[k] !== en[k]);
    expect(stale).toEqual([]);
  });
});

describe.each([
  ["pt", pt],
  ["de", de],
])("catálogo %s", (_name, dict) => {
  it("não inventa chave fora do inglês", () => {
    expect(Object.keys(dict).filter((k) => !(k in en))).toEqual([]);
  });

  it("preserva os placeholders de cada frase", () => {
    const broken = Object.keys(dict)
      .filter((k) => placeholders(k).join(",") !== placeholders(dict[k]).join(","))
      .map((k) => `${JSON.stringify(k)} -> ${JSON.stringify(dict[k])}`);
    expect(broken).toEqual([]);
  });
});

/**
 * Atributo JSX entre aspas **não é string literal de JS**: `attr="a\\b"` entrega
 * ao componente as duas barras, e `attr="linha1\nlinha2"` entrega o `\n` como
 * dois caracteres visíveis. O estrago é triplo, porque essas strings são chaves
 * do catálogo:
 *
 * 1. a tela mostra `HKLM\\SOFTWARE\\...` ou um JSON de uma linha com `\n` cru;
 * 2. a chave pedida (escapada) não existe no catálogo — o `en` guarda a forma
 *    desescapada — então `t()` cai no `defaultValue` e a frase sai em inglês em
 *    **todos** os idiomas;
 * 3. a tradução correspondente vira chave morta que ninguém nunca vê.
 *
 * A forma certa é `attr={"a\\b"}`: dentro de `{}` é expressão JS e o escape é
 * processado uma vez. Este teste varre o frontend para que um quarto caso não
 * entre em silêncio.
 */
const SRC_ROOT = resolve(process.cwd(), "src");

/** Atributo JSX de valor literal, numa linha: `nome="..."` ou `nome='...'`. */
const JSX_LITERAL_ATTR = /(?:^|[\s{])([A-Za-z][A-Za-z0-9]*(?:-[A-Za-z0-9]+)*)=("[^"\n]*"|'[^'\n]*')/g;

/**
 * Texto **filho** de JSX na mesma linha: `<span>C:\\caminho</span>`. Ali o escape
 * também sai cru na tela, e a varredura de atributos não o alcança.
 */
const JSX_TEXT_CHILD = />([^<>{}]*\\(?:\\|[nrt])[^<>{}]*)</g;

/** Escape que chega cru à tela: barra dupla, `\n`, `\t` ou `\r`. */
const RAW_ESCAPE = /\\\\|\\[nrt]/;

function tsxFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...tsxFiles(full));
    else if (entry.endsWith(".tsx")) out.push(full);
  }
  return out.sort();
}

function rawEscapesInJsxAttributes(): string[] {
  const found: string[] = [];
  for (const file of tsxFiles(SRC_ROOT)) {
    const lines = readFileSync(file, "utf8").split("\n");
    lines.forEach((line, index) => {
      const where = `${relative(process.cwd(), file).replace(/\\/g, "/")}:${index + 1}`;
      for (const match of line.matchAll(JSX_LITERAL_ATTR)) {
        const literal = match[2].slice(1, -1);
        if (!RAW_ESCAPE.test(literal)) continue;
        found.push(`${where} ${match[1]}=${match[2]}`);
      }
      for (const match of line.matchAll(JSX_TEXT_CHILD)) {
        found.push(`${where} texto JSX: ${match[1].trim()}`);
      }
    });
  }
  return found;
}

describe("escape em atributo JSX", () => {
  it("nenhum atributo nem texto JSX carrega escape cru (`\\\\`, `\\n`, `\\t`, `\\r`)", () => {
    expect(rawEscapesInJsxAttributes()).toEqual([]);
  });

  it("a varredura realmente enxerga arquivos (protege contra regex morta)", () => {
    expect(tsxFiles(SRC_ROOT).length).toBeGreaterThan(50);
  });
});

/**
 * O tom do toast é deduzido do texto da mensagem (`toneFromMessage`), porque a
 * maior parte dos call sites entrega a frase já traduzida. Logo a tradução pode
 * **apagar** um tom: "Launch failed" vira "Não foi possível iniciar", e sem
 * marcador em português o erro cairia como `info`.
 *
 * A regra é não enfraquecer: se o inglês já indica desfecho, o português tem de
 * indicar o mesmo. O contrário é permitido — o português pega "Não foi possível"
 * onde o heurístico inglês deixa passar "Could not", e isso é melhoria, não bug.
 */
const TONE_EXCEPTIONS = new Set<string>([
  // Texto de corpo e título de passo, nunca vão a toast: o inglês só cai em
  // "success" porque a frase contém "launched"/"started".
  "All {{count}} selected accounts will join the same game that this player is currently in, launched one at a time.",
  "Every few seconds the watcher checks each Roblox client this app launched and closes the ones that match a rule below.",
  "It never reopens them, ignores clients you started outside the app, and skips the window you are using right now.",
  "Getting Started",
]);

describe.each([
  ["pt", pt],
  ["de", de],
])("tom do toast sobrevive à tradução (%s)", (_name, dict) => {
  it("nenhuma tradução apaga o tom que o inglês indica", () => {
    const weakened = Object.keys(dict)
      .filter((k) => !TONE_EXCEPTIONS.has(k))
      .filter((k) => {
        const source = toneFromMessage(en[k]);
        return source !== "info" && toneFromMessage(dict[k]) !== source;
      })
      .map((k) => `${JSON.stringify(k)} (${toneFromMessage(en[k])}) -> ${JSON.stringify(dict[k])} (${toneFromMessage(dict[k])})`);
    expect(weakened).toEqual([]);
  });

  it("as exceções continuam sendo texto de corpo presente no catálogo", () => {
    expect([...TONE_EXCEPTIONS].filter((k) => !(k in en))).toEqual([]);
  });
});
