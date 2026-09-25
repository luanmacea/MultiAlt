import { describe, expect, it } from "vitest";
import { TONE_STYLES, toneFromMessage, type ToastTone } from "./toastTone";

describe("toneFromMessage", () => {
  it("reconhece falha em inglês e em português", () => {
    expect(toneFromMessage("Something failed")).toBe("error");
    expect(toneFromMessage("Launch failed: access denied")).toBe("error");
    expect(toneFromMessage("Error: boom")).toBe("error");
    expect(toneFromMessage("Não foi possível fechar o Roblox: acesso negado")).toBe("error");
    expect(toneFromMessage("Nao foi possivel copiar")).toBe("error");
    expect(toneFromMessage("A busca falhou: tempo esgotado")).toBe("error");
    expect(toneFromMessage("Erro: {{error}}")).toBe("error");
  });

  it("reconhece sucesso em inglês e em português", () => {
    expect(toneFromMessage("Accounts saved")).toBe("success");
    expect(toneFromMessage("Alias updated")).toBe("success");
    expect(toneFromMessage("Launched by Roblox Account Manager")).toBe("success");
    expect(toneFromMessage("Contas salvas")).toBe("success");
    expect(toneFromMessage("Apelido atualizado")).toBe("success");
    expect(toneFromMessage("Botting Mode iniciado (3 contas)")).toBe("success");
  });

  it("reconhece aviso em inglês e em português", () => {
    expect(toneFromMessage("Some warning here")).toBe("warn");
    expect(toneFromMessage("Aviso de otimização: memória baixa")).toBe("warn");
  });

  it("cai em info quando nada indica desfecho", () => {
    expect(toneFromMessage("Neutral message")).toBe("info");
    expect(toneFromMessage("Launching game...")).toBe("info");
    expect(toneFromMessage("Iniciando o jogo...")).toBe("info");
    expect(toneFromMessage("")).toBe("info");
  });

  it("falha vence sucesso na mesma frase", () => {
    // "Não foi possível salvar" tem marcador dos dois lados: erro manda.
    expect(toneFromMessage("Não foi possível salvar as configurações")).toBe("error");
    expect(toneFromMessage("Failed to save settings")).toBe("error");
  });

  /**
   * O login pelo navegador termina sem cookie quando o Roblox não entregou a
   * sessão — é falha, e a frase antiga ("No .ROBLOSECURITY cookie found after
   * login...") não tinha marcador nenhum, então o aviso de login quebrado saía
   * com a mesma cara de um "Iniciando o jogo...".
   */
  it("trata o login que não devolveu cookie como erro", () => {
    expect(
      toneFromMessage("Login failed: no .ROBLOSECURITY cookie found. Please try again.")
    ).toBe("error");
    expect(
      toneFromMessage("Falha no login: nenhum cookie .ROBLOSECURITY foi encontrado. Tente de novo.")
    ).toBe("error");
  });
});

/**
 * A paleta é uma só: o Console de launch (`ChooseGameScreen`) e o feedback de
 * ação (toast + rodapé) pintam o mesmo tom com a mesma cor. Enquanto o mapa
 * vivia dentro do Console, o toast não tinha cor nenhuma.
 */
describe("TONE_STYLES", () => {
  const TONES: ToastTone[] = ["info", "success", "warn", "error"];

  it("cobre os quatro tons com bolinha e texto", () => {
    for (const tone of TONES) {
      expect(TONE_STYLES[tone].dot).toBeTruthy();
      expect(TONE_STYLES[tone].text).toBeTruthy();
    }
  });

  it("mantém exatamente a paleta que o Console de launch já usava", () => {
    expect(TONE_STYLES.info).toEqual({ dot: "bg-[var(--panel-muted)]", text: "text-[var(--panel-fg)]" });
    expect(TONE_STYLES.success).toEqual({ dot: "bg-emerald-500", text: "text-emerald-400" });
    expect(TONE_STYLES.warn).toEqual({ dot: "bg-amber-500", text: "text-amber-400" });
    expect(TONE_STYLES.error).toEqual({ dot: "bg-red-500", text: "text-red-400" });
  });

  it("não deixa dois tons com a mesma cor", () => {
    expect(new Set(TONES.map((tone) => TONE_STYLES[tone].dot)).size).toBe(TONES.length);
    expect(new Set(TONES.map((tone) => TONE_STYLES[tone].text)).size).toBe(TONES.length);
  });
});
