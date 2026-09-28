import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";

/**
 * O app foi bifurcado de `niccsprojects/Roblox-Account-Manager` e nasceu com a
 * identidade do autor original: `identifier` = `com.niccdevs.…`. Sem um
 * `publisher` declarado, o Tauri tira o nome do fabricante do segundo segmento
 * do identificador — e foi assim que a v0.1.0-beta saiu com
 * `CompanyName: niccdevs` nas propriedades do arquivo e no instalador.
 *
 * Estes campos são o que o usuário lê antes de instalar (e o que um dia vai ter
 * de casar com o certificado de assinatura), então ficam travados aqui.
 *
 * ⚠️ Não confundir com o `niccdevs` do [crypto.rs](../src-tauri/src/data/crypto.rs):
 * aquele é o cabeçalho do formato do `AccountData.json` e mudá-lo torna
 * ilegíveis as contas já gravadas.
 */
const CONFIG = path.resolve(__dirname, "..", "src-tauri", "tauri.conf.json");
const config = JSON.parse(readFileSync(CONFIG, "utf8"));

describe("identidade do app no instalador", () => {
  it("não carrega o nome do autor original", () => {
    expect(config.identifier).not.toMatch(/niccdev/i);
    expect(JSON.stringify(config.bundle ?? {})).not.toMatch(/niccdev/i);
  });

  it("declara o fornecedor explicitamente", () => {
    // Deixar em branco devolve o problema: o Tauri volta a derivar o nome do
    // fabricante do identificador.
    expect(config.bundle?.publisher).toBeTruthy();
  });

  it("usa o identificador deste projeto", () => {
    expect(config.identifier).toBe("com.luanmacea.roblox-account-manager");
  });
});
