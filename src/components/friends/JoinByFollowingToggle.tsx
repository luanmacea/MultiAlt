import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import { tr, useTr } from "../../i18n/text";

/**
 * `General.JoinFriendsByFollowing` (padrão ligado): entrar no jogo de um amigo
 * **seguindo** ele (`RequestFollowUser`), como o Roblox Account Manager antigo
 * fazia, em vez de pedir o servidor exato (`RequestGameJob` + Job ID).
 *
 * Por quê (14/10/2026): um usuário relatou CAPTCHA em toda entrada das alts no
 * MultiAlt, enquanto "entrar pelo amigo" no RAM antigo entrava sem nada. A
 * diferença entre os dois era só essa: o antigo segue o usuário e o Roblox
 * escolhe; o MultiAlt pedia o servidor do amigo pelo Job ID.
 */
export function useJoinByFollowing(): [boolean, (value: boolean) => void] {
  const store = useStore();
  const on = store.settings?.General?.JoinFriendsByFollowing !== "false";
  const set = (value: boolean) => {
    invoke("update_setting", { section: "General", key: "JoinFriendsByFollowing", value: value ? "true" : "false" })
      .then(() => store.reloadSettings())
      .catch((e) => store.addToast(tr("Could not save: {{error}}", { error: String(e) })));
  };
  return [on, set];
}

/** A caixa nas abas Amigos e Seguir, com a explicação do CAPTCHA. */
export function JoinByFollowingToggle({ className = "" }: { className?: string }) {
  const t = useTr();
  const [on, set] = useJoinByFollowing();
  return (
    <label
      data-testid="join-by-following"
      className={`flex items-start gap-2 rounded-lg border theme-border bg-[var(--panel-soft)] px-3 py-2 cursor-pointer select-none ${className}`}
    >
      <input
        type="checkbox"
        checked={on}
        onChange={(e) => set(e.target.checked)}
        className="mt-[3px] accent-[var(--accent-color)]"
      />
      <span className="min-w-0">
        <span className="block text-[12px] font-medium text-[var(--panel-fg)]">
          {t("Join by following the player (helps avoid CAPTCHA)")}
        </span>
        <span className="block text-[11px] theme-muted leading-snug">
          {t(
            "Joins the way the old Roblox Account Manager did: Roblox puts the account in the player's server by itself. Asking for the exact server can make Roblox show alts a CAPTCHA. Turn it off to ask for the exact server."
          )}
        </span>
      </span>
    </label>
  );
}
