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

/**
 * O código que o backend devolve quando o usuário dispara um launch com outro
 * em andamento (`LAUNCH_ALREADY_ACTIVE` em `commands/launch.rs`).
 *
 * Vem como código e não como frase porque a tradução é aqui: o Rust não tem o
 * catálogo do i18n.
 */
export const LAUNCH_ALREADY_ACTIVE_CODE = "launch-already-active";

/**
 * Duas sequências de launch não podem rodar juntas — elas disputariam o mutex
 * do Multi Roblox, o registro e o `ClientAppSettings.json`. Quando o backend
 * recusa, a tela tem de dizer isso com palavras, não despejar o código.
 */
export function isLaunchAlreadyActiveError(error: unknown): boolean {
  return String(error ?? "").includes(LAUNCH_ALREADY_ACTIVE_CODE);
}

const EVENT_HELD_ADMIN =
  "Another Roblox window could not be unlocked (it may be running as administrator). Launching now would close it, so this launch was skipped.";
const EVENT_HELD =
  "Another Roblox window could not be unlocked. Launching now would close it, so this launch was skipped.";

/**
 * Erro de launch na língua da tela. As frases que o backend escreve em inglês
 * quando pula o launch porque outro cliente ainda segura o
 * `ROBLOX_singletonEvent` (`MULTI_ROBLOX_EVENT_HELD*` em
 * platform/windows/core.rs — o teste lê a lista do fonte do Rust) são
 * trocadas pela tradução, inclusive dentro de um texto maior. O resto passa
 * como veio.
 */
export function launchErrorText(error: unknown, t: (text: string) => string): string {
  const text = String(error ?? "");
  const held = [
    [EVENT_HELD_ADMIN, () => t("Another Roblox window could not be unlocked (it may be running as administrator). Launching now would close it, so this launch was skipped.")],
    [EVENT_HELD, () => t("Another Roblox window could not be unlocked. Launching now would close it, so this launch was skipped.")],
  ] as const;
  for (const [english, translated] of held) {
    if (text.includes(english)) return text.replace(english, translated());
  }
  return text;
}
