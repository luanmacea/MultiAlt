/**
 * Nome de conta na tela — a única regra do modo "Names hidden" da toolbar.
 *
 * Toda tela que mostra o nome de uma conta do usuário (texto, `title`,
 * `aria-label`, placeholder, toast) passa por aqui ou pelo hook
 * `useAccountLabel` (hooks/useAccountLabel.ts). Antes a máscara estava copiada
 * em sete arquivos e as telas que não tinham a cópia (AFK Mode, Avatars,
 * diálogos, toasts) mostravam o nome real com o modo ligado.
 *
 * Só o que **aparece** é mascarado: o que vai para o backend continua com o
 * nome de verdade.
 */
import type { Account } from "../types";

/** O que a tela mostra no lugar de um nome escondido por inteiro. */
export const HIDDEN_NAME = "************";

/** O que precisa da store para mascarar um nome. */
export interface NameMasking {
  hideUsernames: boolean;
  hiddenNameLetters: number;
}

/** O que precisa da store para decidir se o avatar some junto com o nome. */
export interface AvatarMasking {
  hideUsernames: boolean;
  showAvatarsWhenHidden: boolean;
}

type NamedAccount = Pick<Account, "Alias" | "Username">;

/**
 * Mascara o nome quando o modo "Hidden" está ligado: mostra as primeiras
 * "Preview Letters" e esconde o resto; sem letras de prévia (ou quando a prévia
 * mostraria o nome inteiro) o nome some por completo.
 */
export function maskAccountName(name: string, hidden: boolean, previewLetters: number): string {
  if (!hidden) return name;
  if (previewLetters > 0 && previewLetters < name.length) {
    return name.slice(0, previewLetters) + "********";
  }
  return HIDDEN_NAME;
}

/** Alias || Username, sem máscara; sem conta, o fallback (ex.: "User ID: 123"). */
export function rawAccountLabel(
  account: NamedAccount | null | undefined,
  fallback: string | number = ""
): string {
  return account?.Alias || account?.Username || String(fallback);
}

/**
 * O nome que a tela mostra: Alias || Username, mascarado com o modo ligado.
 * O fallback também é mascarado — o User ID identifica a conta tanto quanto o
 * nome (era assim nas telas que já mascaravam).
 */
export function accountLabel(
  account: NamedAccount | null | undefined,
  masking: NameMasking,
  fallback: string | number = ""
): string {
  return maskAccountName(rawAccountLabel(account, fallback), masking.hideUsernames, masking.hiddenNameLetters);
}

/** O avatar some com os nomes ocultos, a menos que "mostrar avatares" esteja ligado. */
export function hideAccountAvatar(masking: AvatarMasking): boolean {
  return masking.hideUsernames && !masking.showAvatarsWhenHidden;
}

/**
 * A letra do círculo que substitui o avatar quando a foto não carregou. É a
 * inicial do username (como sempre foi); com os nomes ocultos, só aparece se a
 * prévia já a mostraria.
 */
export function accountInitial(account: Pick<Account, "Username"> | null | undefined, masking: NameMasking): string {
  const username = account?.Username || "";
  if (!username) return "?";
  return maskAccountName(username, masking.hideUsernames, masking.hiddenNameLetters).charAt(0).toUpperCase();
}
