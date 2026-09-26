/**
 * Jogos de mentira compartilhados pelos cenários do harness.
 *
 * Existe para a tela poder mostrar **qual jogo** é um Place ID, e para haver
 * algum caso com imagem: antes o `batched_get_game_icon` devolvia sempre `null`
 * e nenhuma tela era vista com ícone carregado.
 */
export interface HarnessGame {
  name: string;
  universeId: number;
}

/**
 * Ícone embutido, sem rede: um quadrado colorido com a inicial do jogo. Serve
 * para ver o enquadramento e o alinhamento reais do ícone na tela.
 */
export function fixtureIcon(letter: string, color: string): string {
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">` +
    `<rect width="64" height="64" rx="12" fill="${color}"/>` +
    `<text x="32" y="43" font-family="sans-serif" font-size="30" text-anchor="middle" fill="white">${letter}</text>` +
    `</svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

/** Cor estável por place, para o mesmo jogo ter sempre o mesmo ícone. */
const COLORS = ["#4f46e5", "#0e7490", "#b45309", "#be123c", "#15803d", "#7c3aed"];

export function iconForGame(placeId: number, name: string): string {
  const letter = (name.trim()[0] || "?").toUpperCase();
  return fixtureIcon(letter, COLORS[placeId % COLORS.length]);
}

/**
 * Places que o "backend" do harness conhece. Place fora desta lista devolve
 * vazio de propósito: é assim que se vê na tela o estado "não sei que jogo é".
 */
export const GAME_FIXTURES: Record<number, HarnessGame> = {
  606849621: { name: "Jailbreak", universeId: 245662005 },
  6516141723: { name: "Blox Fruits", universeId: 2680623874 },
  15101393044: { name: "Steal a Brainrot", universeId: 5272564064 },
};
