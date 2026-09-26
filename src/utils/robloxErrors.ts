/**
 * O Windows recusa a segunda instância do Roblox quando o mutex do cliente já
 * está tomado, e o backend devolve isso como texto. Quando é esse o caso, a tela
 * oferece "Fechar todos os Roblox", que é a única saída — daí a necessidade de
 * reconhecer a mensagem.
 *
 * A deteção vivia copiada em três telas (`App`, a sidebar de multi-seleção — hoje
 * apagada — e `BottingDialog`), com o
 * mesmo par de substrings escrito à mão nos três: mudar a frase no Rust exigia
 * lembrar de todos. Agora é um lugar só, com teste.
 */
export function isMultiRobloxCloseProcessError(message: string | null | undefined): boolean {
  const lower = (message || "").toLowerCase();
  return (
    lower.includes("failed to enable multi roblox") ||
    (lower.includes("multi roblox") && lower.includes("close all roblox process"))
  );
}
