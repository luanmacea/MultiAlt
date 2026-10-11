import { describe, expect, it } from "vitest";
import { autoUiScale, growUiScale, normalizeUiScaleSetting, resolveUiScale, UI_SCALE_SETTINGS } from "./uiScale";

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

  it("o monitor só encolhe: nunca passa de 100% (crescer é com a janela, growUiScale)", () => {
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

/**
 * Monitor 4K a 100% (dono, 11/10/2026): tudo ficava pequeno no canto. O
 * automático cresce a interface quando a **janela** passa de 2200x1240 lógicos.
 */
describe("growUiScale", () => {
  it("janela padrão e janelas comuns ficam em 100%", () => {
    // Desenho do app (tauri.conf.json: 1100x700) e mínimo (750x450).
    expect(growUiScale(1100, 700)).toBe(1);
    expect(growUiScale(750, 450)).toBe(1);
    // 1920x1080 maximizada.
    expect(growUiScale(1920, 1040)).toBe(1);
    expect(growUiScale(2200, 1240)).toBe(1);
  });

  it("4K a 100% maximizada vai para 170%", () => {
    // min(3840/2200, 2090/1240) = min(1.745, 1.685) -> 1.7
    expect(growUiScale(3840, 2090)).toBe(1.7);
  });

  it("2560x1440 maximizada vai para 110%", () => {
    expect(growUiScale(2560, 1390)).toBe(1.1);
  });

  it("usa o lado mais apertado da janela", () => {
    // Largura daria 1.745, altura manda (1240 -> 1.0).
    expect(growUiScale(3840, 1240)).toBe(1);
    // Altura daria 1.6, largura manda (2640/2200 = 1.2).
    expect(growUiScale(2640, 2000)).toBe(1.2);
  });

  it("arredonda para o degrau de 5% mais próximo", () => {
    expect(growUiScale(2200 * 1.24, 1240 * 2)).toBe(1.25);
    expect(growUiScale(2200 * 1.22, 1240 * 2)).toBe(1.2);
    // Exatamente no degrau, sem erro de ponto flutuante.
    expect(growUiScale(2200 * 1.15, 1240 * 2)).toBe(1.15);
  });

  it("nunca passa de 200%", () => {
    expect(growUiScale(7680, 4320)).toBe(2);
  });

  it("tamanho inválido não muda nada", () => {
    expect(growUiScale(0, 0)).toBe(1);
    expect(growUiScale(Number.NaN, 2000)).toBe(1);
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
    expect(resolveUiScale("auto", null, null)).toBe(1);
  });

  it("automático cresce com a janela grande", () => {
    const monitor4k = { width: 3840, height: 2160 };
    expect(resolveUiScale("auto", monitor4k, { width: 3840, height: 2090 })).toBe(1.7);
    // A janela padrão no mesmo monitor não encolhe nem cresce.
    expect(resolveUiScale("auto", monitor4k, { width: 1100, height: 700 })).toBe(1);
    // Janela grande sem monitor conhecido ainda cresce.
    expect(resolveUiScale("auto", null, { width: 3840, height: 2090 })).toBe(1.7);
  });

  it("automático em monitor pequeno continua encolhendo (a janela não cresce ali)", () => {
    expect(resolveUiScale("auto", { width: 1280, height: 720 }, { width: 1280, height: 680 })).toBe(0.85);
  });

  it("tamanho salvo vence o automático, mesmo na janela 4K", () => {
    const monitor4k = { width: 3840, height: 2160 };
    const big = { width: 3840, height: 2090 };
    expect(resolveUiScale(normalizeUiScaleSetting("100"), monitor4k, big)).toBe(1);
    expect(resolveUiScale(normalizeUiScaleSetting("90"), monitor4k, big)).toBe(0.9);
    // Nunca escolhido (chave ausente no INI): automático.
    expect(resolveUiScale(normalizeUiScaleSetting(undefined), monitor4k, big)).toBe(1.7);
  });
});
