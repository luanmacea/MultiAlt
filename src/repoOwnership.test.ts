import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { REPO_API_URL, REPO_SLUG, REPO_URL } from "./repo";

/**
 * Este projeto foi bifurcado de `niccsprojects/Roblox-Account-Manager` e o
 * endereço antigo ficou em seis lugares: botão do repositório, links do
 * diálogo de atualização, documentação do Nexus e o **updater**, que anunciava
 * a versão do outro projeto e a instalaria por cima desta (a chave pública
 * também era de lá, então a assinatura batia).
 *
 * Este teste varre o código dos dois lados e reprova qualquer volta do
 * endereço antigo — num merge, num bloco copiado, num arquivo novo.
 */
const RAIZ = path.resolve(__dirname, "..");
/**
 * `"."` cobre os arquivos soltos da raiz **sem descer** (o `arquivos()` so
 * desce nas pastas listadas). Foi exatamente o buraco por onde o README
 * escapou na primeira limpeza: ele continuou com os badges e o link de
 * download apontando para as releases do outro projeto.
 */
const PASTAS = [".", "src", path.join("src-tauri", "src"), ".github", "docs"];
const EXTENSOES = [".ts", ".tsx", ".rs", ".yml", ".yaml", ".mjs", ".json", ".md"];

/** O próprio teste e o comentário que explica a história citam o nome antigo. */
const PODEM_CITAR = new Set([
  path.join("src", "repo.ts"),
  path.join("src", "repoOwnership.test.ts"),
  // Trava a identidade do instalador; o comentário conta de onde veio o
  // `com.niccdevs.…` que o app carregava.
  path.join("src", "appIdentity.test.ts"),
  path.join("src-tauri", "src", "commands", "updater.rs"),
  path.join("src-tauri", "src", "commands", "services.rs"),
  // Conta a historia da bifurcacao e por que o updater foi redirecionado.
  path.join("docs", "development.md"),
  // Plano de auditoria/sincronização com o upstream: cita o nome dele de
  // propósito, é o assunto do documento.
  path.join("docs", "superpowers", "plans", "upstream-sync-2026-09.md"),
]);

function arquivos(dir: string, desceEmSubpastas = true): string[] {
  let achados: string[] = [];
  for (const nome of readdirSync(dir)) {
    const caminho = path.join(dir, nome);
    if (nome === "node_modules" || nome === "target" || nome === "dist") continue;
    if (statSync(caminho).isDirectory()) {
      if (desceEmSubpastas) achados = achados.concat(arquivos(caminho));
    } else if (EXTENSOES.includes(path.extname(nome))) {
      achados.push(caminho);
    }
  }
  return achados;
}

describe("dono do repositório", () => {
  it("aponta para este projeto, não para aquele de onde ele saiu", () => {
    expect(REPO_SLUG).toBe("luanmacea/MultiAlt");
    expect(REPO_URL).toBe("https://github.com/luanmacea/MultiAlt");
    expect(REPO_API_URL).toBe("https://api.github.com/repos/luanmacea/MultiAlt");
  });

  it("não sobrou nenhum link para o projeto original", () => {
    const sobras: string[] = [];
    for (const pasta of PASTAS) {
      const base = path.join(RAIZ, pasta);
      let lista: string[] = [];
      try {
        lista = arquivos(base, pasta !== ".");
      } catch {
        continue; // pasta ausente não é falha
      }
      for (const arquivo of lista) {
        const relativo = path.relative(RAIZ, arquivo);
        if (PODEM_CITAR.has(relativo)) continue;
        if (readFileSync(arquivo, "utf8").toLowerCase().includes("niccs")) {
          sobras.push(relativo);
        }
      }
    }

    expect(sobras).toEqual([]);
  });
});
