import type { FollowUserState, FriendLinkState } from "../types";

/**
 * Resultado por conta das ações do menu "Actions" (e do clique direito). Antes
 * só "Check accounts" mostrava conta por conta; o resto dava um toast de
 * contagem — ou nada, e o motivo das falhas se perdia (o "Refresh cookies"
 * engolia o erro e não dizia que a sessão tinha morrido). Ver
 * docs/features/accounts.md.
 */
export type ActionOutcome = "pending" | "running" | "ok" | "already" | "skipped" | "login" | "failed";

export type ActionKind =
  | "refresh-cookies"
  | "copy-cookies"
  | "make-friends"
  | "follow"
  | "move-group"
  | "restart-clients"
  | "add-auto-rejoin"
  | "close-all"
  | "remove-accounts"
  | "relogin";

export interface ActionReportRow {
  userId: number;
  outcome: ActionOutcome;
  /** Motivo ou detalhe, já no idioma da tela. */
  detail?: string;
}

export interface ActionReport {
  kind: ActionKind;
  /** Título da janela, já traduzido. */
  title: string;
  rows: ActionReportRow[];
  /** A ação ainda roda (linhas mudando). */
  running: boolean;
  /** Linha de resumo no rodapé (ex.: "3 formed, 1 already friends"). */
  summary?: string;
  /** Lote de remoção que o "Undo" desfaz. */
  undoBatchId?: number;
  at: number;
}

/** Ordem dos filtros e da lista: o que pede ação primeiro. */
export const ACTION_OUTCOME_ORDER: ActionOutcome[] = ["login", "failed", "running", "pending", "skipped", "already", "ok"];

/**
 * A falha é de sessão morta (resolve com novo login), não de rede ou de limite.
 * Mesmos sinais de `is_auth_session_error` no backend, mais as mensagens
 * prontas que ele devolve ("Re-login required", "invalid session").
 */
export function needsLogin(error: string): boolean {
  const lower = error.toLowerCase();
  return (
    lower.includes("status 401") ||
    lower.includes("[401") ||
    lower.includes("unauthorized") ||
    lower.includes("not authenticated") ||
    lower.includes("invalid cookie") ||
    lower.includes("authorization has been denied") ||
    lower.includes("code 9002") ||
    lower.includes('"code":9002') ||
    lower.includes("re-login required") ||
    lower.includes("invalid session") ||
    lower.includes("invalidated this session")
  );
}

/** Falha de uma conta: "precisa de login" ou "falhou", com o motivo. */
export function failureRow(userId: number, error: unknown, loginDetail: string): ActionReportRow {
  const message = String(error ?? "").trim();
  return needsLogin(message)
    ? { userId, outcome: "login", detail: loginDetail }
    : { userId, outcome: "failed", detail: message || undefined };
}

export function pendingRows(userIds: number[]): ActionReportRow[] {
  return Array.from(new Set(userIds)).map((userId) => ({ userId, outcome: "pending" as const }));
}

/** Troca a linha da conta (as outras ficam). */
export function patchRow(rows: ActionReportRow[], next: ActionReportRow): ActionReportRow[] {
  return rows.some((r) => r.userId === next.userId)
    ? rows.map((r) => (r.userId === next.userId ? next : r))
    : [...rows, next];
}

export function countOutcomes(rows: ActionReportRow[]): Record<ActionOutcome, number> {
  const counts: Record<ActionOutcome, number> = {
    pending: 0,
    running: 0,
    ok: 0,
    already: 0,
    skipped: 0,
    login: 0,
    failed: 0,
  };
  for (const row of rows) counts[row.outcome] += 1;
  return counts;
}

export function sortRows(rows: ActionReportRow[]): ActionReportRow[] {
  const rank = (o: ActionOutcome) => ACTION_OUTCOME_ORDER.indexOf(o);
  return [...rows].sort((a, b) => rank(a.outcome) - rank(b.outcome));
}

/** Linhas do "Make Friends" a partir do estado ao vivo do backend. */
export function rowsFromFriendLink(state: FriendLinkState, loginDetail: string): ActionReportRow[] {
  return state.accounts.map((entry) => {
    switch (entry.state) {
      case "done":
        return { userId: entry.userId, outcome: "ok" as const };
      case "failed":
        return failureRow(entry.userId, entry.error, loginDetail);
      case "processing":
        return { userId: entry.userId, outcome: "running" as const };
      default:
        return { userId: entry.userId, outcome: "pending" as const };
    }
  });
}

/** Linhas do "Follow a user" a partir do estado ao vivo do backend. */
export function rowsFromFollow(
  state: FollowUserState,
  loginDetail: string,
  browserDetail: string
): ActionReportRow[] {
  return state.accounts.map((entry) => {
    switch (entry.state) {
      case "followed":
        return { userId: entry.userId, outcome: "ok" as const };
      case "already":
        return { userId: entry.userId, outcome: "already" as const };
      case "failed":
        return failureRow(entry.userId, entry.error, loginDetail);
      case "browser":
        return { userId: entry.userId, outcome: "running" as const, detail: browserDetail };
      case "processing":
        return { userId: entry.userId, outcome: "running" as const };
      default:
        return { userId: entry.userId, outcome: "pending" as const };
    }
  });
}
