/**
 * Account names inside Python log messages, under the "Names hidden" mode.
 *
 * The backend writes names into some lines ("Run started: farm.py on Main_01",
 * "Blocked: Main_01's window is not in front.") and scripts print them too
 * (`log("hello from", account.name)`). Python needs the real names, so the
 * masking happens here, on what the terminal shows, with the app's own rule
 * (`accountLabel` in utils/accountName.ts).
 */
import type { Account } from "../../types";
import { accountLabel, type NameMasking } from "../../utils/accountName";

type NamedAccount = Pick<Account, "Alias" | "Username">;

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Replaces every whole username and alias of `accounts` in `text` with the
 * label the app shows for that account (masked when names are hidden).
 * Case-insensitive; a name inside a longer word (`Main_010`) is left alone.
 */
export function maskNamesInText(text: string, accounts: readonly NamedAccount[], masking: NameMasking): string {
  if (!masking.hideUsernames || !text) return text;
  const replacements = new Map<string, string>();
  for (const account of accounts) {
    const label = accountLabel(account, masking);
    for (const name of [account.Username, account.Alias]) {
      const trimmed = (name ?? "").trim();
      if (trimmed.length >= 2 && !replacements.has(trimmed.toLowerCase())) replacements.set(trimmed.toLowerCase(), label);
    }
  }
  if (replacements.size === 0) return text;
  // Longest first, so "Main_01_alt" wins over "Main_01".
  const names = [...replacements.keys()].sort((a, b) => b.length - a.length).map(escapeRegExp);
  const pattern = new RegExp(`(?<![A-Za-z0-9_])(?:${names.join("|")})(?![A-Za-z0-9_])`, "gi");
  return text.replace(pattern, (found) => replacements.get(found.toLowerCase()) ?? found);
}
