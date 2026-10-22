import type { ModerationStatus } from "../types";

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

/** Uma conta no relatório: o resultado da conferência + o detalhe de moderação, se houver. */
export interface CheckReportRow {
  userId: number;
  outcome: AccountCheckOutcome;
  /** Só em banida/advertida, e só quando o evento `account-moderation` já chegou. */
  moderation?: ModerationStatus;
}

export interface CheckReport {
  rows: CheckReportRow[];
  counts: Record<AccountCheckOutcome, number>;
}

/** Ordem do relatório: o que pede ação do dono primeiro, o que está bem por último. */
const OUTCOME_RANK: Record<AccountCheckOutcome, number> = {
  banned: 0,
  warned: 1,
  invalid: 2,
  unknown: 3,
  ok: 4,
};

/**
 * Relatório do "Check accounts" para a janela de resultados: banidas (e
 * encerradas, que o backend também marca como `banned`) primeiro, depois
 * advertidas, sessão inválida, não conferidas e as que estão bem. Dentro de cada
 * grupo vale a ordem em que as contas foram conferidas.
 */
export function buildCheckReport(
  results: AccountCheckSummary["results"],
  moderationByUserId: Map<number, ModerationStatus>
): CheckReport {
  const counts: CheckReport["counts"] = { banned: 0, warned: 0, invalid: 0, unknown: 0, ok: 0 };
  const rows: CheckReportRow[] = results.map((r) => {
    counts[r.outcome] += 1;
    const detail = r.outcome === "banned" || r.outcome === "warned" ? moderationByUserId.get(r.userId) : undefined;
    return detail ? { userId: r.userId, outcome: r.outcome, moderation: detail } : { userId: r.userId, outcome: r.outcome };
  });
  // `sort` é estável: o empate fica na ordem de entrada.
  rows.sort((a, b) => OUTCOME_RANK[a.outcome] - OUTCOME_RANK[b.outcome]);
  return { rows, counts };
}

/**
 * Junta uma nova conferência (as contas que não deu para conferir, tentadas de
 * novo) ao resultado anterior: quem foi conferido de novo troca de resultado, o
 * resto fica como estava. Sem isso a nova conferência apagaria as banidas que a
 * primeira já tinha achado.
 */
export function mergeCheckResults(
  previous: AccountCheckSummary["results"],
  retried: AccountCheckSummary["results"]
): AccountCheckSummary["results"] {
  const latest = new Map(retried.map((r) => [r.userId, r]));
  return previous.map((r) => latest.get(r.userId) ?? r);
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
