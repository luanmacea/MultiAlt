import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";

/**
 * Janela grande (monitor 4K a 100%, dono, 11/10/2026): a página Session parava
 * em ~1040 px de lista e o resto da janela ficava vazio. Listas, cartões e
 * tabelas das páginas ocupam a largura; teto de largura fica só para texto de
 * leitura e formulário (rótulo longe do controle), e cada exceção diz por quê.
 *
 * O teste pega quem copiar um `max-w-[1040px]` de volta para uma página.
 */
const RAIZ = path.resolve(__dirname, "..");
const PASTAS = ["pages", "session", "afk-mode", "console", "accounts"];
/** Teto em px a partir do qual a classe conta como "trava a página". */
const LIMITE_PX = 600;
const NOMEADOS = new Set(["3xl", "4xl", "5xl", "6xl", "7xl"]);

/** Exceções aceitas: `arquivo: classe` → motivo. */
const PERMITIDOS: Record<string, string> = {
  "pages/SettingsPage.tsx: max-w-[760px]": "formulário: rótulo à esquerda, controle à direita",
  "pages/SettingsPage.tsx: 2xl:max-w-[920px]": "formulário, um pouco mais largo em tela grande",
  "pages/ChangelogPage.tsx: max-w-[880px]": "texto de leitura (notas de versão)",
  "afk-mode/RecordingsTab.tsx: max-w-3xl": "lista de passos do editor de gravação (formulário)",
  "afk-mode/RecordingsTab.tsx: max-w-5xl": "editor de gravação (formulário)",
};

function arquivos(dir: string): string[] {
  const achados: string[] = [];
  for (const nome of readdirSync(dir)) {
    const caminho = path.join(dir, nome);
    if (statSync(caminho).isDirectory()) achados.push(...arquivos(caminho));
    else if (nome.endsWith(".tsx") && !nome.endsWith(".test.tsx")) achados.push(caminho);
  }
  return achados;
}

function tetos(): string[] {
  const achados: string[] = [];
  for (const pasta of PASTAS) {
    for (const arquivo of arquivos(path.join(RAIZ, pasta))) {
      const rel = path.relative(RAIZ, arquivo).split(path.sep).join("/");
      const conteudo = readFileSync(arquivo, "utf8");
      for (const [classe, px, nome] of conteudo.matchAll(/(?<![\w-])(?:[\w-]+:)*max-w-(?:\[(\d+)px\]|(\d?xl)\b)/g)) {
        const trava = px !== undefined ? Number(px) >= LIMITE_PX : NOMEADOS.has(nome);
        if (trava) achados.push(`${rel}: ${classe}`);
      }
    }
  }
  return achados;
}

/** Todos os .tsx de `src/components`, com o caminho relativo. */
function todosOsComponentes(): Array<[string, string]> {
  return arquivos(RAIZ).map((arquivo) => [
    path.relative(RAIZ, arquivo).split(path.sep).join("/"),
    readFileSync(arquivo, "utf8"),
  ]);
}

describe("páginas ocupam a janela grande", () => {
  it("nenhuma página trava a largura de listas e cartões", () => {
    expect(tetos().filter((achado) => !(achado in PERMITIDOS))).toEqual([]);
  });

  /**
   * Breakpoint de janela em px (`min-[2000px]:`) sai no CSS do Tailwind 4
   * **antes** dos `sm:`…`2xl:` (em rem) e perde para eles: o `xl:w-[300px]`
   * vencia o `min-[2000px]:w-[640px]` da página Session. Em rem a ordem sai
   * certa (`min-[125rem]:`).
   */
  it("breakpoint de janela arbitrário é em rem, não em px", () => {
    const emPx: string[] = [];
    for (const [rel, conteudo] of todosOsComponentes()) {
      for (const [classe] of conteudo.matchAll(/(?<![\w@-])(?:min|max)-\[\d+(?:\.\d+)?px\]:[\w[\]().,%/-]+/g)) {
        emPx.push(`${rel}: ${classe}`);
      }
    }
    expect(emPx).toEqual([]);
  });

  it("a varredura realmente enxerga os tetos permitidos (protege contra regex morta)", () => {
    expect(tetos()).toEqual(expect.arrayContaining(Object.keys(PERMITIDOS)));
  });
});
