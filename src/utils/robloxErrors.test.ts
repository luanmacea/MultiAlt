import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import ptCommon from "../locales/pt/common.json";
import esCommon from "../locales/es/common.json";
import {
  isLaunchAlreadyActiveError,
  isMultiRobloxCloseProcessError,
  launchErrorText,
  LAUNCH_ALREADY_ACTIVE_CODE,
} from "./robloxErrors";

/**
 * Esta deteção decide se a tela oferece o botão "Fechar todos os Roblox" — é o
 * único caminho de saída quando o Windows recusa a segunda instância. Ela vivia
 * copiada em três lugares (`App`, a sidebar de multi-seleção — hoje apagada — e
 * `BottingDialog`), com o
 * mesmo par de substrings escrito à mão em cada um: mudar a mensagem do backend
 * exigia lembrar dos três.
 */
describe("isMultiRobloxCloseProcessError", () => {
  it("reconhece a falha de habilitar multi Roblox", () => {
    expect(isMultiRobloxCloseProcessError("Failed to enable Multi Roblox")).toBe(true);
    expect(isMultiRobloxCloseProcessError("launch error: FAILED TO ENABLE MULTI ROBLOX")).toBe(true);
  });

  it("reconhece o pedido para fechar todos os processos", () => {
    expect(
      isMultiRobloxCloseProcessError("Multi Roblox needs you to close all Roblox process first")
    ).toBe(true);
  });

  it("não reage a erro de outra coisa", () => {
    expect(isMultiRobloxCloseProcessError("Launch failed: access denied")).toBe(false);
    expect(isMultiRobloxCloseProcessError("multi roblox is enabled")).toBe(false);
    expect(isMultiRobloxCloseProcessError("")).toBe(false);
    expect(isMultiRobloxCloseProcessError(null)).toBe(false);
    expect(isMultiRobloxCloseProcessError(undefined)).toBe(false);
  });
});

describe("isLaunchAlreadyActiveError", () => {
  it("reconhece a recusa do backend, com ou sem embrulho de Error", () => {
    expect(isLaunchAlreadyActiveError(LAUNCH_ALREADY_ACTIVE_CODE)).toBe(true);
    expect(isLaunchAlreadyActiveError(new Error(LAUNCH_ALREADY_ACTIVE_CODE))).toBe(true);
  });

  it("não reage a outro erro de launch nem a valor vazio", () => {
    expect(isLaunchAlreadyActiveError("Falha no auth ticket: 401")).toBe(false);
    expect(isLaunchAlreadyActiveError("")).toBe(false);
    expect(isLaunchAlreadyActiveError(null)).toBe(false);
    expect(isLaunchAlreadyActiveError(undefined)).toBe(false);
  });
});

/**
 * Ideia 33: o launch é pulado quando outro cliente ainda segura o
 * `ROBLOX_singletonEvent` (abrir fecharia esse cliente). A frase vem do Rust
 * em inglês (`MULTI_ROBLOX_EVENT_HELD*` em platform/windows/core.rs); a lista
 * sai do próprio fonte, então frase nova lá sem tradução aqui reprova.
 */
describe("launchErrorText", () => {
  const source = readFileSync(resolve(__dirname, "../../src-tauri/src/platform/windows/core.rs"), "utf8");
  const backendErrors = [...source.matchAll(/pub const MULTI_ROBLOX_EVENT_HELD\w*: &str =\s*"([^"]+)";/g)].map(
    (m) => m[1]
  );
  const pt = ptCommon as Record<string, string>;
  const es = esCommon as Record<string, string>;
  const ptT = (text: string) => pt[text] ?? text;

  it("lists the messages the backend writes", () => {
    expect(backendErrors).toHaveLength(3);
    expect(backendErrors.some((m) => m.includes("administrator"))).toBe(true);
  });

  it("translates every message the backend writes, alone or inside a longer error", () => {
    for (const message of backendErrors) {
      expect(pt[message], message).toBeTruthy();
      expect(es[message], message).toBeTruthy();
      expect(pt[message], message).not.toBe(message);
      expect(launchErrorText(message, ptT)).toBe(pt[message]);
      expect(launchErrorText(`Launch failed: ${message}`, ptT)).toBe(`Launch failed: ${pt[message]}`);
    }
  });

  it("keeps the program name after a lock held by another program", () => {
    const message = backendErrors.find((m) => m.startsWith("Another program"))!;
    expect(launchErrorText(`${message} Fishtrap.exe`, ptT)).toBe(`${pt[message]} Fishtrap.exe`);
  });

  it("leaves any other error as it came", () => {
    expect(launchErrorText("Falha no auth ticket: 401", ptT)).toBe("Falha no auth ticket: 401");
    expect(launchErrorText(new Error("x"), ptT)).toBe("Error: x");
  });
});
