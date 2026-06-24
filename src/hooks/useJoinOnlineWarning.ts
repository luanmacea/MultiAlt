import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import { useConfirmWithOptOut } from "./usePrompt";

interface PresenceEntry {
  userId?: number;
  userPresenceType?: number;
  user_id?: number;
  user_presence_type?: number;
}

function presenceLabel(t: number): string {
  if (t === 3) return "In Studio";
  if (t === 2) return "In Game";
  if (t === 1) return "Online";
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
          name: account ? account.Alias || account.Username : `User ${id}`,
          type,
        };
      })
      .filter((v): v is { name: string; type: number } => v !== null);

    if (risky.length === 0) return true;

    const preview = risky
      .slice(0, 4)
      .map((a) => `${a.name} (${presenceLabel(a.type)})`)
      .join(", ");
    const more = risky.length > 4 ? ` and ${risky.length - 4} more` : "";

    const message =
      risky.length === 1
        ? `${risky[0].name} is currently ${presenceLabel(risky[0].type)}. Joining can disconnect its existing Roblox session. Continue anyway?`
        : `${risky.length} selected accounts are already online: ${preview}${more}. Joining can disconnect their existing Roblox sessions. Continue anyway?`;

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
      store.addToast("Online-join warning disabled");
    }

    return result.confirmed;
  };
}
