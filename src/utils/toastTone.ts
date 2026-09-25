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
];
const WARN = ["warning", "aviso", "warnung"];

export function toneFromMessage(message: string): ToastTone {
  const lower = message.toLowerCase();
  if (ERROR.some((m) => lower.includes(m))) return "error";
  if (SUCCESS.some((m) => lower.includes(m))) return "success";
  if (WARN.some((m) => lower.includes(m))) return "warn";
  return "info";
}
