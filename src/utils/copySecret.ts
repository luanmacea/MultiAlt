import { invoke } from "@tauri-apps/api/core";

/**
 * Cópia de credencial (cookie, senha, user:pass) para a área de transferência.
 *
 * No Windows quem copia é o backend (`copy_account_secret`): o texto vai
 * marcado para **não** entrar no histórico do Win+V nem na nuvem, e é apagado
 * sozinho depois de {@link SECRET_CLEAR_SECONDS} s — só se a pessoa não tiver
 * copiado outra coisa nesse meio-tempo. O texto sai do store do backend, não do
 * snapshot da tela, que fica velho quando o Roblox troca o cookie.
 *
 * Onde o backend não sabe fazer isso (macOS) ele responde
 * {@link CLIPBOARD_UNSUPPORTED} e a cópia cai no `navigator.clipboard` de
 * sempre, sem limpeza. Qualquer **outro** erro do Windows (área de
 * transferência ocupada) não cai no caminho sem proteção: vira erro na tela.
 */
export type SecretKind = "cookie" | "password" | "userpass";

export const SECRET_CLEAR_SECONDS = 30;
export const CLIPBOARD_UNSUPPORTED = "CLIPBOARD_UNSUPPORTED";

export interface SecretCopyResult {
  /** Quantas linhas foram copiadas. */
  count: number;
  /** Em quantos segundos o app apaga a área de transferência; `null` = não apaga. */
  clearsInSecs: number | null;
}

export async function copyAccountSecret(
  userIds: number[],
  kind: SecretKind,
  fallbackText: () => string
): Promise<SecretCopyResult> {
  try {
    const result = await invoke<SecretCopyResult>("copy_account_secret", { userIds, kind });
    return {
      count: Number(result?.count ?? 0),
      clearsInSecs: result?.clearsInSecs ?? null,
    };
  } catch (e) {
    if (String(e) !== CLIPBOARD_UNSUPPORTED) throw e;
    const text = fallbackText();
    await navigator.clipboard.writeText(text);
    return { count: text ? text.split("\n").length : 0, clearsInSecs: null };
  }
}
