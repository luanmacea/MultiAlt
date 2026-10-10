/**
 * Resumo do "Check accounts" (`check_accounts` em `commands/account_check.rs`).
 * Conferir é só leitura: não renova sessão, não desloga ninguém.
 */
export type AccountCheckOutcome = "ok" | "warned" | "invalid" | "banned" | "unknown";

export interface AccountCheckSummary {
  total: number;
  ok: number;
  warned: number;
  invalid: number;
  banned: number;
  /** Não deu para conferir agora (Roblox limitando, rede): tentar depois. */
  unknown: number;
  results: { userId: number; outcome: AccountCheckOutcome }[];
}

type Translate = (text: string, options?: Record<string, unknown>) => string;

/** "18 ok, 3 invalid, 1 banned, 1 couldn't check" (+ ", 1 warned" quando há). */
export function accountCheckSummaryText(summary: AccountCheckSummary, t: Translate): string {
  const base = t("{{ok}} ok, {{invalid}} invalid, {{banned}} banned, {{unknown}} couldn't check", {
    ok: summary.ok,
    invalid: summary.invalid,
    banned: summary.banned,
    unknown: summary.unknown,
  });
  return summary.warned > 0
    ? t("{{summary}}, {{warned}} warned", { summary: base, warned: summary.warned })
    : base;
}
