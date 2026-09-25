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
