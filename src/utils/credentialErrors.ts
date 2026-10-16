/**
 * O motivo de uma troca de senha/e-mail recusada, em frase que a pessoa entende.
 * O backend devolve `credential-error:<tipo>:<status>:<texto do Roblox>`
 * (`classify_credential_failure` em src-tauri/src/api/auth.rs); antes era só
 * "Failed to change password" (relato do dono, 15/10/2026).
 */
export type CredentialErrorKind =
  | "verification-captcha"
  | "verification-2sv"
  | "verification-password"
  | "verification-other"
  | "rate-limited"
  | "session-expired"
  | "pin-locked"
  | "wrong-current-password"
  | "invalid-new-password"
  | "invalid-email"
  | "unknown";

export interface CredentialError {
  kind: CredentialErrorKind;
  status: number | null;
  /** O texto do próprio Roblox (pode vir vazio). */
  detail: string;
}

const PREFIX = "credential-error:";

export function parseCredentialError(raw: unknown): CredentialError | null {
  const text = String(raw ?? "");
  const at = text.indexOf(PREFIX);
  if (at === -1) return null;
  const [kind = "unknown", status = "", ...rest] = text.slice(at + PREFIX.length).split(":");
  const code = Number(status);
  return {
    kind: kind as CredentialErrorKind,
    status: Number.isFinite(code) && code > 0 ? code : null,
    detail: rest.join(":").trim(),
  };
}

type Translate = (key: string, options?: Record<string, unknown>) => string;

/** A frase para a tela. Erro que não é de credencial volta como veio. */
export function credentialErrorText(raw: unknown, t: Translate): string {
  const err = parseCredentialError(raw);
  if (!err) return String(raw ?? "");
  const said = err.detail ? ` ${t("(Roblox: \"{{detail}}\")", { detail: err.detail })}` : "";
  switch (err.kind) {
    case "verification-captcha":
      return t("Roblox asked for a CAPTCHA on this account. Open it in the browser, finish the check, then try again.");
    case "verification-2sv":
      return t("This account has 2-step verification and Roblox asked for the code. Change it in the browser.");
    case "verification-password":
      return t("Roblox asked to confirm the password again. Open the account in the browser, confirm it, then try again.");
    case "verification-other":
      return t("Roblox asked for a security check on this account. Open it in the browser, finish it, then try again.");
    case "rate-limited":
      return t("Roblox blocked it for now: too many changes in a row. Wait a few minutes and use Retry failed.");
    case "session-expired":
      return t("This account's session expired. Log in to it again, then try again.");
    case "pin-locked":
      return t("This account has a PIN lock. Unlock it first (Account Utilities › PIN).");
    case "wrong-current-password":
      return t("Roblox says the current password is wrong. Check the password saved for this account.") + said;
    case "invalid-new-password":
      return t("Roblox refused the new password (too simple, too common or contains the username).") + said;
    case "invalid-email":
      return t("Roblox refused the new email.") + said;
    default:
      return (
        t("Roblox refused the change (error {{status}}).", { status: err.status ?? "?" }) + said
      );
  }
}

/** Vale tentar de novo mais tarde (limite, verificação), sem mudar nada? */
export function isRetryLater(raw: unknown): boolean {
  const kind = parseCredentialError(raw)?.kind;
  return kind === "rate-limited" || (kind ?? "").startsWith("verification-");
}
