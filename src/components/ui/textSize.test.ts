import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";

/**
 * Piso de legibilidade da interface.
 *
 * O app foi escrito com tamanhos em pixel absoluto espalhados pelo JSX
 * (`text-[10px]`, `text-[11px]`…), então **não existe uma alavanca global**:
 * mudar a fonte do `:root` não mexe em nada. O dono relatou que não conseguia
 * ler certas descrições sem se aproximar da tela, e a correção foi subir a
 * escala pequena de uma vez (9 e 10 → 11, 11 → 12).
 *
 * Este teste existe para o piso não voltar a cair: quem copiar um bloco antigo
 * com `text-[10px]` descobre aqui, e não meses depois na tela de alguém.
 */
const RAIZ = path.resolve(__dirname, "../..");
const PISO_PX = 11;

function arquivosJsx(dir: string): string[] {
  const achados: string[] = [];
  for (const nome of readdirSync(dir)) {
    const caminho = path.join(dir, nome);
    if (statSync(caminho).isDirectory()) {
      achados.push(...arquivosJsx(caminho));
      continue;
    }
    if (nome.endsWith(".tsx") && !nome.endsWith(".test.tsx")) achados.push(caminho);
  }
  return achados;
}

describe("tamanho de texto da interface", () => {
  it("não usa fonte menor que o piso legível", () => {
    const pequenos: string[] = [];
    for (const arquivo of arquivosJsx(RAIZ)) {
      const conteudo = readFileSync(arquivo, "utf8");
      for (const [, px] of conteudo.matchAll(/text-\[([0-9.]+)px\]/g)) {
        if (Number(px) < PISO_PX) {
          pequenos.push(`${path.relative(RAIZ, arquivo)}: text-[${px}px]`);
        }
      }
    }

    expect(pequenos).toEqual([]);
  });
});
