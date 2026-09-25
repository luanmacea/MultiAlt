import { describe, expect, it } from "vitest";
import { toneFromMessage } from "./toastTone";

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
});
