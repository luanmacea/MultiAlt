export type ToastTone = "info" | "success" | "warn" | "error";

/**
 * O tom do toast é deduzido do próprio texto da mensagem, porque a maioria dos
 * call sites chama `addToast(tr("..."))` — ou seja, entrega a frase **já
 * traduzida**, sem a chave. Por isso os marcadores existem em cada idioma
 * completo do app: procurar só por "failed"/"saved" faria todo erro em
 * português cair como `info`.
 *
 * Um idioma sem marcador aqui não quebra nada — cai em `info`, que é o padrão.
 */
const ERROR = [
  // inglês
  "error",
  "failed",
  // português
  "erro",
  "falhou",
  "falha",
  "não foi possível",
  "nao foi possivel",
  // alemão
  "fehler",
  "fehlgeschlagen",
  "konnte nicht",
  // espanhol ("error" já vem do inglês)
  "falló",
  "fallid",
  "no se pudo",
];
const SUCCESS = [
  "saved",
  "updated",
  "launched",
  "started",
  "salv",
  "atualizad",
  "iniciad",
  "gespeichert",
  "aktualisiert",
  "gestartet",
  // espanhol ("iniciad" já vem do português)
  "guardad",
  "actualizad",
];
// "aviso" serve ao português e ao espanhol.
const WARN = ["warning", "aviso", "warnung", "advertencia"];

export function toneFromMessage(message: string): ToastTone {
  const lower = message.toLowerCase();
  if (ERROR.some((m) => lower.includes(m))) return "error";
  if (SUCCESS.some((m) => lower.includes(m))) return "success";
  if (WARN.some((m) => lower.includes(m))) return "warn";
  return "info";
}

/** Classes Tailwind de um tom: a bolinha e o texto. */
export interface ToneStyle {
  dot: string;
  text: string;
}

/**
 * A paleta do feedback de ação, uma só para todo o app: o Console de launch
 * (`ChooseGameScreen`), a fila de toasts (`App`) e a linha de estado do rodapé
 * (`StatusBar`) pintam o mesmo tom com a mesma cor. Enquanto este mapa vivia
 * dentro do Console, o toast não tinha condicional de estilo nenhuma — erro e
 * sucesso saíam visualmente idênticos.
 *
 * `info` é deliberadamente neutro (as cores do painel): a maioria das mensagens
 * cai nele, e colorir tudo tiraria o sentido de colorir as outras.
 */
export const TONE_STYLES: Record<ToastTone, ToneStyle> = {
  info: { dot: "bg-[var(--panel-muted)]", text: "text-[var(--panel-fg)]" },
  success: { dot: "bg-emerald-500", text: "text-emerald-400" },
  warn: { dot: "bg-amber-500", text: "text-amber-400" },
  error: { dot: "bg-red-500", text: "text-red-400" },
};
