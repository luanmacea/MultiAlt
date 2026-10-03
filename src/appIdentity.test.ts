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

/**
 * Troca de nome para MultiAlt (03/10/2026, docs/rebrand-multialt.md). O nome do
 * produto mudou, mas duas coisas que o Tauri deriva dele **não** podem mudar,
 * senão quem já instalou ganha uma segunda cópia em vez de uma atualização.
 */
describe("troca de nome sem quebrar quem já instalou", () => {
  it("o produto se chama MultiAlt", () => {
    expect(config.productName).toBe("MultiAlt");
  });

  it("o MSI mantém o código de atualização das versões Roblox Account Manager", () => {
    // Sem fixar, o Tauri calcula o código a partir do nome do produto — e um
    // código novo faz o MSI instalar ao lado da versão antiga.
    expect(config.bundle?.windows?.wix?.upgradeCode).toBe("ea97e4e5-4634-554b-8c91-822b0b369761");
  });

  it("a entrada de iniciar com o Windows mantém o nome antigo", () => {
    // O plugin de autostart usa o nome do produto como nome da entrada no
    // registro; com o nome novo, quem tem a opção ligada ficaria com duas.
    const lib = readFileSync(path.resolve(__dirname, "..", "src-tauri", "src", "lib.rs"), "utf8");
    expect(lib).toMatch(/\.app_name\("Roblox Account Manager"\)/);
  });
});
