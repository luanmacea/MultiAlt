import type { Account } from "../types";

/**
 * Trocar senha e/ou e-mail de várias contas de uma vez (menu Ações → pedido
 * do dono, issue #30). Lógica pura, sem rede: o diálogo
 * (`BulkCredentialsDialog`) percorre o plano e chama os comandos
 * `change_password` / `change_email` que já existem, uma conta por vez.
 *
 * O ponto delicado: **trocar senha ou e-mail exige a senha ATUAL de cada
 * conta**. Ela vem de uma de duas fontes, nesta ordem:
 *  1. a senha guardada da conta (`account.Password`, das contas importadas por
 *     usuário:senha);
 *  2. uma senha única que o usuário digita ("todas usam esta senha agora").
 * Conta sem nenhuma das duas é **pulada** (não dá para provar que é do dono).
 *
 * O Roblox pode pedir verificação (CAPTCHA/2SV/e-mail) numa troca de
 * credencial; nesses casos a conta falha com o motivo e as outras seguem — o
 * app não contorna desafio. Ver docs/features/accounts.md.
 */

/** Mínimo de caracteres que o Roblox aceita numa senha. */
export const MIN_ROBLOX_PASSWORD = 8;

export interface BulkCredentialsRequest {
  changePassword: boolean;
  /** Nova senha para todas (quando `changePassword`). */
  newPassword: string;
  changeEmail: boolean;
  /** Novo e-mail para todas (quando `changeEmail`). */
  newEmail: string;
  /** Senha atual compartilhada (fonte 2); vazia = só usar a guardada. */
  sharedCurrentPassword: string;
  /**
   * Uma senha aleatória diferente para cada conta (pedido do dono,
   * 27/10/2026), gerada pelo backend (`change_password_random`). Ligado, a
   * `newPassword` não é usada.
   */
  randomPassword: boolean;
}

export type BulkCredentialsError =
  | { kind: "nothing-selected" }
  | { kind: "weak-password"; min: number }
  | { kind: "bad-email" };

/** Valida o pedido antes de rodar. `null` = pode seguir. */
export function validateBulkRequest(req: BulkCredentialsRequest): BulkCredentialsError | null {
  if (!req.changePassword && !req.changeEmail) return { kind: "nothing-selected" };
  if (req.changePassword && !req.randomPassword && req.newPassword.trim().length < MIN_ROBLOX_PASSWORD) {
    return { kind: "weak-password", min: MIN_ROBLOX_PASSWORD };
  }
  if (req.changeEmail && !looksLikeEmail(req.newEmail)) return { kind: "bad-email" };
  return null;
}

/** Shape de e-mail, só o suficiente para pegar engano óbvio (o Roblox valida de verdade). */
export function looksLikeEmail(value: string): boolean {
  const v = value.trim();
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(v);
}

/** A senha atual que prova a posse da conta, ou `null` se não há nenhuma. */
export function resolveCurrentPassword(
  account: Pick<Account, "Password">,
  sharedCurrentPassword: string
): string | null {
  const stored = account.Password?.trim();
  if (stored) return stored;
  const shared = sharedCurrentPassword.trim();
  return shared.length > 0 ? shared : null;
}

export interface BulkCredentialsStep {
  userId: number;
  /** A senha atual a usar; `null` quando a conta é pulada. */
  currentPassword: string | null;
  /** Por que a conta foi pulada, se foi. */
  skip: "no-current-password" | null;
}

/**
 * Monta o plano, conta por conta, na ordem recebida. Quem não tem senha atual
 * entra como pulado (com o motivo), para o diálogo mostrar e não disparar nada.
 */
export function planBulkCredentials(
  accounts: Pick<Account, "UserID" | "Password">[],
  req: BulkCredentialsRequest
): BulkCredentialsStep[] {
  return accounts.map((account) => {
    const currentPassword = resolveCurrentPassword(account, req.sharedCurrentPassword);
    return {
      userId: account.UserID,
      currentPassword,
      skip: currentPassword === null ? "no-current-password" : null,
    };
  });
}

/** Quantas contas o plano vai de fato tentar mudar (as não puladas). */
export function runnableCount(steps: BulkCredentialsStep[]): number {
  return steps.filter((s) => s.skip === null).length;
}
