/**
 * Rótulo, na tela, de cada tecla da lista fechada do Modo AFK (`get_afk_keys`).
 * As teclas simples aparecem com o próprio nome; os pares de setas que se
 * anulam (`AFK_KEY_PAIRS` em `commands/afk.rs`, ideia 24) ganham uma frase que
 * diz o que fazem. O `Select` traduz o rótulo.
 */
const AFK_KEY_LABELS: { key: string; label: string }[] = [
  { key: "Right+Left", label: "→ then ← (turns the camera and back)" },
  { key: "Up+Down", label: "↑ then ↓ (a step forward and back)" },
];

export function afkKeyLabel(key: string): string {
  return AFK_KEY_LABELS.find((entry) => entry.key === key)?.label ?? key;
}
