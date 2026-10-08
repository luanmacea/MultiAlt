import { describe, expect, it } from "vitest";
import { autoUiScale, normalizeUiScaleSetting, resolveUiScale, UI_SCALE_SETTINGS } from "./uiScale";

/**
 * Tamanho da interface (Settings › General). Em monitor de notebook (janela
 * lógica ~1280x690, 1920x1080 a 150%) a interface ocupava a tela toda; no
 * monitor grande (2560x1440) estava certa. "Automatic" encolhe pelo tamanho
 * lógico da janela — que não muda com o zoom, senão o zoom realimentaria a conta.
 */
describe("autoUiScale", () => {
  it("monitor grande fica em 100%", () => {
    expect(autoUiScale(2560, 1400)).toBe(1);
  });

  it("notebook 1920x1080 a 150% (janela ~1280x690) vai para 85%", () => {
    expect(autoUiScale(1280, 690)).toBe(0.85);
  });

  it("nunca passa de 100%, mesmo em janela maior que a referência", () => {
    expect(autoUiScale(3840, 2100)).toBe(1);
    expect(autoUiScale(1440, 800)).toBe(1);
  });

  it("nunca desce de 80%", () => {
    // 1100x650 daria 0.75: fica no piso.
    expect(autoUiScale(1100, 650)).toBe(0.8);
    // Mínimo da janela do app (tauri.conf.json: 750x450).
    expect(autoUiScale(750, 450)).toBe(0.8);
  });

  it("usa o lado mais apertado da janela", () => {
    // Largura sobra (1.0), altura manda (720/800 = 0.9).
    expect(autoUiScale(2000, 720)).toBe(0.9);
    // Altura sobra, largura manda (1300/1440 = 0.902...).
    expect(autoUiScale(1300, 1200)).toBe(0.9);
  });

  it("arredonda para baixo em passos de 5%", () => {
    // 0.899 -> 0.85, nunca 0.9.
    expect(autoUiScale(1440 * 0.899, 800)).toBe(0.85);
    // Exatamente no degrau não cai para o de baixo (erro de ponto flutuante).
    expect(autoUiScale(1440 * 0.95, 800)).toBe(0.95);
    expect(autoUiScale(1440, 800 * 0.9)).toBe(0.9);
  });

  it("tamanho inválido não muda nada", () => {
    expect(autoUiScale(0, 0)).toBe(1);
    expect(autoUiScale(Number.NaN, 700)).toBe(1);
  });
});

describe("normalizeUiScaleSetting", () => {
  it("aceita as opções da tela", () => {
    for (const value of UI_SCALE_SETTINGS) {
      expect(normalizeUiScaleSetting(value)).toBe(value);
    }
  });

  it("ausente ou desconhecido vira automático", () => {
    expect(normalizeUiScaleSetting(undefined)).toBe("auto");
    expect(normalizeUiScaleSetting(null)).toBe("auto");
    expect(normalizeUiScaleSetting("")).toBe("auto");
    expect(normalizeUiScaleSetting("75")).toBe("auto");
    expect(normalizeUiScaleSetting("banana")).toBe("auto");
  });

  it("aceita maiúsculas e espaços do INI editado à mão", () => {
    expect(normalizeUiScaleSetting(" AUTO ")).toBe("auto");
    expect(normalizeUiScaleSetting(" 90 ")).toBe("90");
  });
});

describe("resolveUiScale", () => {
  it("valor fixo ignora o tamanho da janela", () => {
    expect(resolveUiScale("100", { width: 750, height: 450 })).toBe(1);
    expect(resolveUiScale("90", { width: 2560, height: 1400 })).toBe(0.9);
    expect(resolveUiScale("80", null)).toBe(0.8);
    expect(resolveUiScale("110", { width: 1280, height: 690 })).toBe(1.1);
  });

  it("automático segue a janela", () => {
    expect(resolveUiScale("auto", { width: 1280, height: 690 })).toBe(0.85);
    expect(resolveUiScale("auto", { width: 2560, height: 1400 })).toBe(1);
  });

  it("automático sem tamanho conhecido fica em 100%", () => {
    expect(resolveUiScale("auto", null)).toBe(1);
  });
});
