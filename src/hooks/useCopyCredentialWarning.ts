import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import { useConfirmWithOptOut } from "./usePrompt";
import { tr } from "../i18n/text";
import { isWindowsPlatform } from "../utils/platform";
import { SECRET_CLEAR_SECONDS } from "../utils/copySecret";

/**
 * Confirmação antes de uma credencial ir para a área de transferência.
 *
 * Copiar cookie/senha saía num clique, em silêncio, para **todas** as contas
 * selecionadas — com 50 contas marcadas eram 50 `.ROBLOSECURITY` na área de
 * transferência. O aviso diz a consequência concreta e quantas contas vão
 * junto, e o opt-out fica gravado em `General.WarnOnCopyCredential`, no mesmo
 * molde de `useJoinOnlineWarning`.
 */
export type CopyCredentialKind = "cookie" | "password" | "userpass";

function credentialSentence(kind: CopyCredentialKind, count: number): string {
  if (kind === "cookie") {
    return count === 1
      ? tr(
          "Copy the .ROBLOSECURITY cookie of 1 account to the clipboard? This cookie is the account's whole session: anyone holding it is signed in as that account, with no password and no 2-step verification."
        )
      : tr(
          "Copy the .ROBLOSECURITY cookie of {{count}} accounts to the clipboard? Each cookie is that account's whole session: anyone holding one is signed in as that account, with no password and no 2-step verification.",
          { count }
        );
  }
  if (kind === "password") {
    return count === 1
      ? tr(
          "Copy the password of 1 account to the clipboard? Anyone holding it can sign in as that account on roblox.com."
        )
      : tr(
          "Copy the passwords of {{count}} accounts to the clipboard? Anyone holding them can sign in as those accounts on roblox.com.",
          { count }
        );
  }
  return count === 1
    ? tr(
        "Copy the username and password of 1 account to the clipboard? Anyone holding the pair can sign in as that account on roblox.com."
      )
    : tr(
        "Copy the username and password of {{count}} accounts to the clipboard? Anyone holding a pair can sign in as that account on roblox.com.",
        { count }
      );
}

export function useCopyCredentialWarning() {
  const store = useStore();
  const confirmWithOptOut = useConfirmWithOptOut();

  return async function confirmCopyCredential(
    kind: CopyCredentialKind,
    count: number
  ): Promise<boolean> {
    if (count <= 0) return true;
    if (store.settings?.General?.WarnOnCopyCredential === "false") return true;

    // No Windows a cópia vai pelo backend (utils/copySecret.ts): fica fora do
    // histórico do Win+V e é apagada sozinha. Fora dele, vale o aviso antigo.
    const clipboardNote = isWindowsPlatform(store.platformCapabilities)
      ? tr(
          "Anything else running on this PC can read the clipboard while it is there. Cleared from the clipboard in {{seconds}} s, and kept out of the Win+V history.",
          { seconds: SECRET_CLEAR_SECONDS }
        )
      : tr("Anything else running on this PC can read the clipboard until you copy something else.");
    const message = `${credentialSentence(kind, count)} ${clipboardNote}`;

    const result = await confirmWithOptOut(message, {
      destructive: true,
      confirmLabel: "Copy Anyway",
      cancelLabel: "Cancel",
      optOutLabel: "Don't show this warning again",
    });

    if (result.dontShowAgain) {
      await invoke("update_setting", {
        section: "General",
        key: "WarnOnCopyCredential",
        value: "false",
      }).catch(() => {});
      await store.reloadSettings().catch(() => {});
      // Tom explicito: confirmacao de ajuste, nao aviso de risco.
      store.addToast(tr("Credential copy warning disabled"), "info");
    }

    return result.confirmed;
  };
}
