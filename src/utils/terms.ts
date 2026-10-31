import type { TermsAcceptResult, TermsCheckEntry, TermsState } from "../types";

type Translate = (text: string, options?: Record<string, unknown>) => string;

/** Estado de uma conta a partir da resposta do backend. */
export function termsStateOf(entry: Pick<TermsCheckEntry, "status" | "error">): TermsState {
  if (entry.status) {
    return entry.status.pending.length > 0
      ? { kind: "pending", pending: entry.status.pending }
      : { kind: "none" };
  }
  return { kind: "error", message: entry.error ?? "" };
}

/** "Terms of Use, Privacy Policy" (sem repetir título). */
export function pendingTitles(state: TermsState): string {
  if (state.kind !== "pending") return "";
  return Array.from(new Set(state.pending.map((p) => p.title))).join(", ");
}

/** A linha curta da conta: "Terms pending: …", "No pending terms", ou o motivo. */
export function termsLabel(state: TermsState | undefined, t: Translate): string {
  if (!state) return t("Terms not checked yet");
  switch (state.kind) {
    case "none":
      return t("No pending terms");
    case "pending":
      return t("Terms pending: {{titles}}", { titles: pendingTitles(state) });
    default:
      return t("Couldn't check terms: {{error}}", { error: state.message });
  }
}

/** Só dá para aceitar pela API se o Roblox disse o id de cada acordo. */
export function canAcceptByApi(state: TermsState | undefined): boolean {
  return state?.kind === "pending" && state.pending.every((p) => p.id.trim() !== "");
}

/**
 * O que dizer depois de um aceite que não fechou, com a resposta exata do
 * Roblox (status e corpo). Nunca uma frase genérica.
 */
export function acceptFailureText(result: TermsAcceptResult, t: Translate): string {
  const last = result.attempts[result.attempts.length - 1];
  if (!last) return t("Roblox did not answer. Use Open in browser.");
  const answer = t("Roblox answered {{status}}: {{body}}", {
    status: last.status,
    body: last.body.trim() || t("(empty)"),
  });
  if (last.status >= 200 && last.status < 300) {
    return result.remaining
      ? t("{{answer}} — but {{count}} agreement(s) are still pending. Use Open in browser.", {
          answer,
          count: result.remaining.length,
        })
      : t("{{answer}} — but the terms couldn't be checked afterwards. Check them again.", { answer });
  }
  return t("{{answer}}. Use Open in browser to accept it yourself.", { answer });
}
