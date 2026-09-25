import { invoke } from "@tauri-apps/api/core";
import { tr } from "../i18n/text";
import { useStore } from "../store";
import { useConfirmWithOptOut } from "./usePrompt";

interface PresenceEntry {
  userId?: number;
  userPresenceType?: number;
  user_id?: number;
  user_presence_type?: number;
}

/**
 * Chave de catálogo do estado de presença — e não o texto pronto. O rótulo entra
 * no meio da frase do aviso, então devolver "In Game" cru fazia a frase sair
 * pela metade em inglês mesmo com o catálogo traduzido. As quatro chaves já
 * existem no catálogo.
 */
export function presenceLabelKey(type: number): string {
  if (type === 3) return "In Studio";
  if (type === 2) return "In Game";
  if (type === 1) return "Online";
  return "Offline";
}

export function useJoinOnlineWarning() {
  const store = useStore();
  const confirmWithOptOut = useConfirmWithOptOut();

  return async function confirmJoin(userIds: number[]): Promise<boolean> {
    if (userIds.length === 0) return true;
    if (store.settings?.General?.WarnOnOnlineJoin === "false") return true;

    const uniqueIds = [...new Set(userIds)];
    const presenceById = new Map<number, number>();

    try {
      for (let i = 0; i < uniqueIds.length; i += 100) {
        const chunk = uniqueIds.slice(i, i + 100);
        const rows = await invoke<PresenceEntry[]>("get_presence", { userIds: chunk });
        for (const row of rows) {
          const userId = row.userId ?? row.user_id;
          const presenceType = row.userPresenceType ?? row.user_presence_type ?? 0;
          if (typeof userId === "number") {
            presenceById.set(userId, presenceType);
          }
        }
      }
    } catch {
      return true;
    }

    const accountById = new Map(store.accounts.map((a) => [a.UserID, a]));
    const risky = uniqueIds
      .map((id) => {
        const type = presenceById.get(id) ?? 0;
        if (type < 1) return null;
        const account = accountById.get(id);
        return {
          name: account ? account.Alias || account.Username : tr("User {{id}}", { id }),
          type,
        };
      })
      .filter((v): v is { name: string; type: number } => v !== null);

    if (risky.length === 0) return true;

    const preview = risky
      .slice(0, 4)
      .map((a) => `${a.name} (${tr(presenceLabelKey(a.type))})`)
      .join(", ");
    const list =
      risky.length > 4
        ? tr("{{list}} and {{n}} more", { list: preview, n: risky.length - 4 })
        : preview;

    // Frase montada por `tr()` com `{{placeholder}}`, nunca por template
    // literal: o extrator de chaves descarta de propósito qualquer literal com
    // `${` (`scripts/i18n/extract-keys.ts`), então a forma antiga não podia ser
    // chave e o aviso saía em inglês em todos os idiomas. Uma conta e várias
    // contas são chaves separadas — o padrão do app para singular/plural (ver
    // `store.tsx`, "Closed {{count}} Roblox process"/"...processes") — em vez de
    // um "(s)" que nenhum idioma conjuga igual.
    const message =
      risky.length === 1
        ? tr(
            "{{name}} is currently {{state}}. Joining can disconnect its existing Roblox session. Continue anyway?",
            { name: risky[0].name, state: tr(presenceLabelKey(risky[0].type)) }
          )
        : tr(
            "{{count}} selected accounts are already online: {{list}}. Joining can disconnect their existing Roblox sessions. Continue anyway?",
            { count: risky.length, list }
          );

    // Os rótulos vão como **chave**: o `PromptProvider` já passa cada um por
    // `t()` (`usePrompt.tsx`), então traduzir aqui só tiraria a chave do lugar.
    const result = await confirmWithOptOut(message, {
      confirmLabel: "Join Anyway",
      cancelLabel: "Cancel",
      optOutLabel: "Don't show this warning again",
    });

    if (result.dontShowAgain) {
      await invoke("update_setting", {
        section: "General",
        key: "WarnOnOnlineJoin",
        value: "false",
      }).catch(() => {});
      await store.reloadSettings().catch(() => {});
      // `addToast` traduz a chave sozinho e tira o tom do texto **em inglês**
      // (`store.tsx`), então aqui vai a chave crua — não `tr()`.
      // Tom explicito: e confirmacao de ajuste, nao aviso de risco (a frase tem a
      // palavra "aviso" e o heuristico a classificaria como `warn`).
      store.addToast("Online-join warning disabled", "info");
    }

    return result.confirmed;
  };
}
