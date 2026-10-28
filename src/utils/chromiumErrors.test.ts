import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import ptCommon from "../locales/pt/common.json";
import esCommon from "../locales/es/common.json";
import { chromiumErrorText } from "./chromiumErrors";

/**
 * Relato (23/10/2026): o Chromium do criador de contas abria e ficava em
 * about:blank. Agora o backend fecha a janela e devolve esta frase; o teste lê
 * a constante do Rust, então frase nova lá sem tradução aqui reprova.
 */
describe("chromiumErrorText", () => {
  const source = readFileSync(resolve(__dirname, "../../src-tauri/src/chromium/cdp.rs"), "utf8");
  const phrase = source.match(/pub const BROWSER_DID_NOT_OPEN: &str = "([^"]+)";/)?.[1] ?? "";
  const pt = ptCommon as Record<string, string>;
  const es = esCommon as Record<string, string>;
  const ptT = (text: string) => pt[text] ?? text;

  it("reads the phrase the backend writes", () => {
    expect(phrase).toContain("did not open");
  });

  it("translates it in pt and es, alone or inside a longer error", () => {
    expect(pt[phrase]).toBeTruthy();
    expect(es[phrase]).toBeTruthy();
    expect(pt[phrase]).not.toBe(phrase);
    expect(chromiumErrorText(phrase, ptT)).toBe(pt[phrase]);
    expect(chromiumErrorText(`Erro: ${phrase}`, ptT)).toBe(`Erro: ${pt[phrase]}`);
  });

  it("leaves any other error as it came", () => {
    expect(chromiumErrorText("Could not attach to the browser", ptT)).toBe("Could not attach to the browser");
    expect(chromiumErrorText(new Error("x"), ptT)).toBe("Error: x");
  });
});
