import { describe, expect, it } from "vitest";
import {
  isLaunchAlreadyActiveError,
  isMultiRobloxCloseProcessError,
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
