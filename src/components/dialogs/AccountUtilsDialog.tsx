import { useState, useEffect, useCallback } from "react";
import { X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../store";
import { accountLabel, hideAccountAvatar } from "../../utils/accountName";
import { useModalClose } from "../../hooks/useModalClose";
import { useBackdropClose } from "../../hooks/useBackdropClose";
import { SectionHeader } from "../ui/SectionHeader";
import { UtilInput } from "../ui/UtilInput";
import { UtilButton } from "../ui/UtilButton";
import { Select } from "../ui/Select";
import { useConfirm } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";

interface BlockedUser {
  userId: number;
  name: string;
}

interface OutfitInfo {
  id: number;
  name: string;
}

interface UniversePlace {
  id: number;
  name: string;
}

function toPositiveId(digits: string): number | null {
  const id = Number(digits);
  return Number.isSafeInteger(id) && id > 0 ? id : null;
}

/**
 * Lê o Universe ID digitado ou colado no campo "Universe ID".
 *
 * O campo só aceitava dígitos crus via `parseInt`; colar a URL de um jogo (que
 * não é puramente numérica) fazia `parseInt` devolver `NaN` e a função saía em
 * silêncio. Aqui também aceita um link que carregue `universeId=` na query (o
 * formato da página "Configure Universe" e do link de compartilhamento do
 * Creator Hub) — o resto continua devolvendo `null` para a tela explicar o que
 * faltou, em vez de não fazer nada.
 */
export function parseUniverseIdInput(text: string): number | null {
  const trimmed = text.trim();
  if (!trimmed) return null;

  if (/^\d+$/.test(trimmed)) return toPositiveId(trimmed);

  const queryParam = trimmed.match(/[?&#/]universeid=(\d+)/i);
  if (queryParam) return toPositiveId(queryParam[1]);

  // Creator Hub: .../experiences/<universeId>/...
  const experiencePath = trimmed.match(/\/experiences\/(\d+)/i);
  if (experiencePath) return toPositiveId(experiencePath[1]);

  return null;
}

/**
 * Valida o JSON colado em "Custom Avatar JSON" antes de chamar `set_avatar`.
 *
 * Antes, `JSON.parse` corria dentro do mesmo `try` da chamada ao backend: um
 * JSON malformado só virava o toast genérico "Error: {{error}}", depois de já
 * ter tentado (e falhado) a chamada. Aqui a mesma checagem roda no
 * onChange/onBlur do campo, então o erro aparece antes do clique em Apply.
 */
export function validateAvatarJson(text: string, t: (s: string, o?: Record<string, unknown>) => string): string {
  const trimmed = text.trim();
  if (!trimmed) return "";
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch (e) {
    // A mensagem do motor ("Expected property name or '}' in JSON at position 1")
    // e inglesa, cripta e nao ajuda quem joga: o que resolve e dizer onde olhar.
    return t("That is not valid JSON — check the commas, braces and quotes.");
  }
  const assets = (parsed as { assets?: unknown } | null)?.assets;
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed) || !Array.isArray(assets)) {
    return t('Expected an object with an "assets" array, like {"assets":[{"id":12345}]}.');
  }
  return "";
}

export function AccountUtilsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useTr();
  const store = useStore();
  const confirm = useConfirm();
  const { visible, closing, handleClose } = useModalClose(open, onClose);
  const backdropClose = useBackdropClose(handleClose);
  const account = store.selectedAccount;

  const [robux, setRobux] = useState<number | null>(null);
  const [emailStatus, setEmailStatus] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [followPrivacy, setFollowPrivacy] = useState("0");
  const [privateServerPrivacy, setPrivateServerPrivacy] = useState("AllUsers");

  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [emailInput, setEmailInput] = useState("");
  const [pinInput, setPinInput] = useState("");

  const [targetUsername, setTargetUsername] = useState("");
  const [blockedUsers, setBlockedUsers] = useState<BlockedUser[]>([]);
  const [blockedExpanded, setBlockedExpanded] = useState(false);
  const [blockedError, setBlockedError] = useState("");

  const [outfitUsername, setOutfitUsername] = useState("");
  const [outfits, setOutfits] = useState<OutfitInfo[]>([]);
  const [selectedOutfit, setSelectedOutfit] = useState<number | null>(null);
  const [outfitsLoaded, setOutfitsLoaded] = useState(false);
  const [universeIdInput, setUniverseIdInput] = useState("");
  const [universePlaces, setUniversePlaces] = useState<UniversePlace[]>([]);
  const [avatarJsonInput, setAvatarJsonInput] = useState("");
  const [avatarJsonError, setAvatarJsonError] = useState("");

  const [loading, setLoading] = useState("");

  const reset = useCallback(() => {
    setRobux(null);
    setEmailStatus("");
    setDisplayName("");
    setFollowPrivacy("0");
    setPrivateServerPrivacy("AllUsers");
    setCurrentPassword("");
    setNewPassword("");
    setEmailInput("");
    setPinInput("");
    setTargetUsername("");
    setBlockedUsers([]);
    setBlockedExpanded(false);
    setBlockedError("");
    setOutfitUsername("");
    setOutfits([]);
    setSelectedOutfit(null);
    setOutfitsLoaded(false);
    setUniverseIdInput("");
    setUniversePlaces([]);
    setAvatarJsonInput("");
    setAvatarJsonError("");
    setLoading("");
  }, []);

  useEffect(() => {
    if (!open || !account) return;
    reset();
    setOutfitUsername(account.Username);

    (async () => {
      try {
        const r = await invoke<number>("get_robux", { userId: account.UserID });
        setRobux(r);
      } catch {}
      try {
        const info = await invoke<{ user_id: number; name: string; is_email_verified: boolean }>(
          "validate_cookie",
          { cookie: account.SecurityToken }
        );
        setEmailStatus(info.is_email_verified ? t("Verified") : t("Not verified"));
      } catch {
        setEmailStatus(t("Unknown"));
      }
      try {
        const privacy = await invoke<string>("get_private_server_invite_privacy", {
          userId: account.UserID,
        });
        if (privacy === "AllUsers" || privacy === "Friends" || privacy === "NoOne") {
          setPrivateServerPrivacy(privacy);
        }
      } catch {}
    })();

  }, [open, account?.UserID]);

  if (!visible || !account) return null;

  // Com os nomes ocultos a foto some junto, como na lista de contas.
  const avatarUrl = hideAccountAvatar(store) ? undefined : store.avatarUrls.get(account.UserID);
  // Nome que aparece nas confirmações: o mesmo rótulo do cabeçalho do diálogo
  // (mascarado com "Names hidden").
  const accountName = accountLabel(account, store);

  async function handleSetDisplayName() {
    if (!displayName.trim()) return;
    setLoading("display");
    try {
      await invoke("set_display_name", { userId: account!.UserID, displayName: displayName.trim() });
      await store.loadAccounts();
      store.addToast(t("Display name updated"));
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
    setLoading("");
  }

  async function handleFollowPrivacy(value: string) {
    setFollowPrivacy(value);
    try {
      await invoke("set_follow_privacy", { userId: account!.UserID, privacy: value });
      await store.loadAccounts();
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
  }

  async function handlePrivateServerPrivacy(value: string) {
    setPrivateServerPrivacy(value);
    try {
      await invoke("set_private_server_invite_privacy", {
        userId: account!.UserID,
        privacy: value,
      });
      await store.loadAccounts();
      store.addToast(t("Private server privacy updated"));
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
  }

  async function handleChangePassword() {
    if (!currentPassword || !newPassword) return;
    // Troca de credencial acontece na conta Roblox de verdade: confirma antes,
    // no mesmo padrão destrutivo do resto do app.
    const confirmed = await confirm(
      t(
        'Change the Roblox password for "{{account}}"? This changes the password on the real Roblox account, not just in this app, and cannot be undone from here.',
        { account: accountName }
      ),
      true
    );
    if (!confirmed) return;
    setLoading("password");
    try {
      await invoke("change_password", {
        userId: account!.UserID,
        currentPassword,
        newPassword,
      });
      await store.loadAccounts();
      store.addToast(t("Password changed"));
      setCurrentPassword("");
      setNewPassword("");
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
    setLoading("");
  }

  async function handleChangeEmail() {
    if (!emailInput || !currentPassword) return;
    // Idem senha: o e-mail muda na conta Roblox, e é por ele que passa a
    // recuperação da conta — confirma antes de disparar.
    const confirmed = await confirm(
      t(
        'Change the Roblox email for "{{account}}" to {{email}}? This changes the email on the real Roblox account, not just in this app, and future account recovery goes to the new address.',
        { account: accountName, email: emailInput }
      ),
      true
    );
    if (!confirmed) return;
    setLoading("email");
    try {
      await invoke("change_email", {
        userId: account!.UserID,
        password: currentPassword,
        newEmail: emailInput,
      });
      await store.loadAccounts();
      store.addToast(t("Email changed"));
      setEmailInput("");
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
    setLoading("");
  }

  async function handleUnlockPin() {
    if (!pinInput || pinInput.length !== 4) return;
    setLoading("pin");
    try {
      const ok = await invoke<boolean>("unlock_pin", { userId: account!.UserID, pin: pinInput });
      await store.loadAccounts();
      store.addToast(ok ? t("PIN unlocked") : t("Invalid PIN"));
      setPinInput("");
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
    setLoading("");
  }

  async function handleSignOut() {
    if (!(await confirm(t("Sign out of all other sessions?")))) return;
    setLoading("signout");
    try {
      const ok = await store.refreshCookie(account!.UserID);
      store.addToast(ok ? t("Signed out of other sessions") : t("Sign out failed"));
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
    setLoading("");
  }

  async function resolveTarget(): Promise<{ id: number; name: string } | null> {
    if (!targetUsername.trim()) return null;
    try {
      return await invoke<{ id: number; name: string }>("lookup_user", { username: targetUsername.trim() });
    } catch (e) {
      store.addToast(t("User not found: {{error}}", { error: String(e) }));
      return null;
    }
  }

  async function handleBlock() {
    const target = await resolveTarget();
    if (!target) return;
    setLoading("block");
    try {
      await invoke("block_user", { userId: account!.UserID, targetUserId: target.id });
      await store.loadAccounts();
      store.addToast(t("Blocked {{name}}", { name: target.name }));
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
    setLoading("");
  }

  async function handleAddFriend() {
    const target = await resolveTarget();
    if (!target) return;
    setLoading("friend");
    try {
      await invoke("send_friend_request", { userId: account!.UserID, targetUserId: target.id });
      await store.loadAccounts();
      store.addToast(t("Friend request sent to {{name}}", { name: target.name }));
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
    setLoading("");
  }

  async function handleLoadBlocked() {
    setBlockedExpanded(!blockedExpanded);
    if (blockedExpanded) return;
    setBlockedError("");
    try {
      const users = await invoke<BlockedUser[]>("get_blocked_users", { userId: account!.UserID });
      setBlockedUsers(users);
    } catch {
      setBlockedError(t("Unable to load blocked users"));
      setBlockedUsers([]);
    }
  }

  async function handleUnblock(targetId: number, name: string) {
    try {
      await invoke("unblock_user", { userId: account!.UserID, targetUserId: targetId });
      await store.loadAccounts();
      setBlockedUsers((prev) => prev.filter((u) => u.userId !== targetId));
      store.addToast(t("Unblocked {{name}}", { name }));
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
  }

  async function handleUnblockAll() {
    if (!(await confirm(t("Unblock all {{count}} users?", { count: blockedUsers.length }), true))) return;
    setLoading("unblockall");
    try {
      const count = await invoke<number>("unblock_all_users", { userId: account!.UserID });
      await store.loadAccounts();
      store.addToast(t("Unblocked {{count}} users", { count }));
      setBlockedUsers([]);
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
    setLoading("");
  }

  async function handleLoadOutfits() {
    if (!outfitUsername.trim()) return;
    setLoading("outfits");
    setOutfitsLoaded(false);
    setSelectedOutfit(null);
    try {
      const user = await invoke<{ id: number; name: string }>("lookup_user", {
        username: outfitUsername.trim(),
      });
      const list = await invoke<OutfitInfo[]>("get_outfits", { targetUserId: user.id });
      setOutfits(list);
      setOutfitsLoaded(true);
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
    setLoading("");
  }

  async function handleWearOutfit() {
    if (selectedOutfit === null) return;
    setLoading("wear");
    try {
      const details = await invoke<{ assets: { id: number }[] }>("get_outfit_details", {
        outfitId: selectedOutfit,
      });
      const avatarJson = details;
      const invalidIds = await invoke<number[]>("set_avatar", {
        userId: account!.UserID,
        avatarJson,
      });
      await store.loadAccounts();
      if (invalidIds.length > 0) {
        store.setMissingAssets({
          userId: account!.UserID,
          username: account!.Alias || account!.Username,
          assetIds: invalidIds,
        });
      } else {
        store.addToast(t("Outfit applied"));
      }
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
    setLoading("");
  }

  async function handleLoadUniversePlaces() {
    const universeId = parseUniverseIdInput(universeIdInput);
    if (universeId === null) {
      // O campo fazia `parseInt` e, se não desse número, saía calado — colar a
      // URL de um jogo (que não é numérica) não fazia nada e não dizia por
      // quê. Agora aceita um link com `universeId=` e, quando mesmo assim não
      // resolve, explica o que faltou em vez de ficar quieto.
      store.addToast(
        t(
          'Couldn\'t find a Universe ID in "{{input}}". Paste a numeric Universe ID, or a link containing universeId=....',
          { input: universeIdInput.trim() }
        )
      );
      return;
    }
    setLoading("universe");
    try {
      const places = await invoke<UniversePlace[]>("get_universe_places", {
        universeId,
        userId: account!.UserID,
      });
      setUniversePlaces(places);
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
      setUniversePlaces([]);
    }
    setLoading("");
  }

  async function handleWearCustomAvatar() {
    const trimmed = avatarJsonInput.trim();
    if (!trimmed) return;
    // JSON.parse rodava dentro do try do set_avatar: um JSON inválido só
    // aparecia como o toast genérico "Error: ...", depois de já ter tentado
    // chamar o backend. Valida antes e mostra o motivo ao lado do campo.
    const error = validateAvatarJson(avatarJsonInput, t);
    if (error) {
      setAvatarJsonError(error);
      return;
    }
    setLoading("custom_avatar");
    try {
      const avatarJson = JSON.parse(trimmed);
      const invalidIds = await invoke<number[]>("set_avatar", {
        userId: account!.UserID,
        avatarJson,
      });
      await store.loadAccounts();
      if (invalidIds.length > 0) {
        store.setMissingAssets({
          userId: account!.UserID,
          username: account!.Alias || account!.Username,
          assetIds: invalidIds,
        });
      } else {
        store.addToast(t("Avatar applied"));
      }
    } catch (e) {
      store.addToast(t("Error: {{error}}", { error: String(e) }));
    }
    setLoading("");
  }

  function handleAvatarJsonChange(value: string) {
    setAvatarJsonInput(value);
    setAvatarJsonError(validateAvatarJson(value, t));
  }

  const privacyOptions = [
    { value: "0", label: "Everyone" },
    { value: "1", label: "Friends, Followed, Followers" },
    { value: "2", label: "Friends, Followed" },
    { value: "3", label: "Friends" },
    { value: "4", label: "No one" },
  ];
  const privateServerPrivacyOptions = [
    { value: "AllUsers", label: "Everyone" },
    { value: "Friends", label: "Friends" },
    { value: "NoOne", label: "No one" },
  ];

  return (
    <div
      className={`fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm ${closing ? "animate-fade-out" : "animate-fade-in"}`}
      {...backdropClose}
    >
      <div
        className={`theme-modal-scope theme-panel theme-border bg-zinc-900 border border-zinc-800/80 rounded-2xl shadow-2xl w-[540px] max-w-[calc(100vw-24px)] max-h-[min(600px,calc(100vh-24px))] flex flex-col overflow-hidden ${closing ? "animate-scale-out" : "animate-scale-in"}`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 pt-4 pb-3 shrink-0">
          <h2 className="text-sm font-semibold text-zinc-100">{t("Account Utilities")}</h2>
          <button onClick={handleClose} className="text-zinc-600 hover:text-zinc-400 transition-colors">
            <X size={16} strokeWidth={2} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 pb-5">
          <div className="flex items-center gap-3 p-3 bg-zinc-800/40 rounded-xl mb-1">
            {avatarUrl ? (
              <img src={avatarUrl} alt="" className="w-10 h-10 rounded-full bg-zinc-700" />
            ) : (
              <div className="w-10 h-10 rounded-full bg-zinc-700" />
            )}
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium text-zinc-100 truncate">
                {accountName}
              </div>
              <div className="flex items-center gap-3 text-[12px] text-zinc-500">
                <span className="font-mono">{account.UserID}</span>
                {robux !== null && <span>R$ {robux.toLocaleString()}</span>}
                <span>{emailStatus}</span>
              </div>
            </div>
          </div>

          <SectionHeader>Profile</SectionHeader>
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <div className="flex-1">
                <UtilInput
                  value={displayName}
                  onChange={setDisplayName}
                  placeholder="Display Name"
                />
              </div>
              <UtilButton onClick={handleSetDisplayName} disabled={loading === "display"}>
                Set
              </UtilButton>
            </div>
            <div className="flex items-center gap-2">
              <label className="text-xs text-zinc-400 shrink-0 w-28">{t("Follow Privacy")}</label>
              <Select
                value={followPrivacy}
                options={privacyOptions}
                onChange={handleFollowPrivacy}
                className="flex-1"
              />
            </div>
            <div className="flex items-center gap-2">
              <label className="text-xs text-zinc-400 shrink-0 w-28">{t("Private Servers")}</label>
              <Select
                value={privateServerPrivacy}
                options={privateServerPrivacyOptions}
                onChange={handlePrivateServerPrivacy}
                className="flex-1"
              />
            </div>
          </div>

          <SectionHeader>Security</SectionHeader>
          <div className="space-y-2">
            {/* O campo era só "PIN (4 digits)" + "Unlock": nada dizia que é o
                PIN da própria conta Roblox (nem a senha do app, nem a da
                criptografia local) e que quem decide quanto tempo o desbloqueio
                dura é o Roblox — o backend só lê `unlockedUntil`. */}
            <p className="text-[12px] text-zinc-500">
              {t(
                "This is the Roblox account PIN set on roblox.com, not this app's password and not the encryption password. Unlocking it lets this account's Roblox settings be changed again; Roblox decides how long that lasts and re-locks the account on its own."
              )}
            </p>
            <div className="flex items-center gap-2">
              <div className="flex-1">
                <UtilInput
                  value={pinInput}
                  onChange={(v) => setPinInput(v.replace(/\D/g, "").slice(0, 4))}
                  placeholder="Roblox account PIN (4 digits)"
                  onKeyDown={(e) => { if (e.key === "Enter") handleUnlockPin(); }}
                />
              </div>
              <UtilButton onClick={handleUnlockPin} disabled={loading === "pin"}>
                Unlock
              </UtilButton>
            </div>
            <UtilButton onClick={handleSignOut} disabled={loading === "signout"} variant="danger">
              Sign out of other sessions
            </UtilButton>
          </div>

          {/* Senha e e-mail mudam a conta Roblox de verdade — peso visual de
              Danger Zone, igual ao que a sidebar de multi-seleção já usa. */}
          <SectionHeader>Danger Zone</SectionHeader>
          <div className="space-y-2 rounded-xl border border-red-500/20 bg-red-500/[0.04] p-2.5">
            <p className="text-[12px] text-red-400/80">
              {t(
                'These change the real Roblox account "{{account}}", not just this app.',
                { account: accountName }
              )}
            </p>
            <UtilInput
              value={currentPassword}
              onChange={setCurrentPassword}
              placeholder="Current Password"
              type="password"
            />
            <div className="flex items-center gap-2">
              <div className="flex-1">
                <UtilInput
                  value={newPassword}
                  onChange={setNewPassword}
                  placeholder="New Password"
                  type="password"
                />
              </div>
              <UtilButton
                onClick={handleChangePassword}
                disabled={loading === "password"}
                variant="danger"
              >
                Change Password
              </UtilButton>
            </div>
            <div className="flex items-center gap-2">
              <div className="flex-1">
                <UtilInput
                  value={emailInput}
                  onChange={setEmailInput}
                  placeholder="New Email (requires current password)"
                />
              </div>
              <UtilButton
                onClick={handleChangeEmail}
                disabled={loading === "email"}
                variant="danger"
              >
                Change Email
              </UtilButton>
            </div>
          </div>

          <SectionHeader>Social</SectionHeader>
          <div className="space-y-2">
            <UtilInput
              value={targetUsername}
              onChange={setTargetUsername}
              placeholder="Username"
            />
            <div className="flex items-center gap-2">
              <UtilButton onClick={handleBlock} disabled={!!loading} variant="danger">
                Block
              </UtilButton>
              <UtilButton onClick={handleAddFriend} disabled={!!loading} variant="success">
                Add Friend
              </UtilButton>
            </div>
            <button
              onClick={handleLoadBlocked}
              className="text-[12px] text-zinc-500 hover:text-zinc-300 transition-colors"
            >
              {blockedExpanded ? t("Hide blocked users") : t("View blocked users")}
            </button>
            {blockedExpanded && (
              <div className="bg-zinc-800/30 rounded-lg p-2 space-y-1 max-h-[120px] overflow-y-auto">
                {blockedError && (
                  <p className="text-[12px] text-zinc-500">{blockedError}</p>
                )}
                {blockedUsers.length === 0 && !blockedError && (
                  <p className="text-[12px] text-zinc-500">{t("No blocked users")}</p>
                )}
                {blockedUsers.map((u) => (
                  <div key={u.userId} className="flex items-center justify-between">
                    <span className="text-xs text-zinc-300">{u.name}</span>
                    <button
                      onClick={() => handleUnblock(u.userId, u.name)}
                      className="text-[11px] text-red-400/60 hover:text-red-400 transition-colors"
                    >
                      {t("Unblock")}
                    </button>
                  </div>
                ))}
                {blockedUsers.length > 0 && (
                  <button
                    onClick={handleUnblockAll}
                    disabled={loading === "unblockall"}
                    className="text-[11px] text-red-400/60 hover:text-red-400 transition-colors mt-1"
                  >
                    {t("Unblock All ({{count}})", { count: blockedUsers.length })}
                  </button>
                )}
              </div>
            )}
          </div>

          <SectionHeader>Outfits</SectionHeader>
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <div className="flex-1">
                <UtilInput
                  value={outfitUsername}
                  onChange={setOutfitUsername}
                  placeholder="Username"
                  // Abre com o username da própria conta: com os nomes ocultos
                  // ele fica em bolinhas (outro nome digitado aparece normal).
                  masked={store.hideUsernames && outfitUsername === account.Username}
                />
              </div>
              <UtilButton onClick={handleLoadOutfits} disabled={loading === "outfits"}>
                Load Outfits
              </UtilButton>
            </div>
            {outfitsLoaded && (
              <div className="bg-zinc-800/30 rounded-lg p-2 max-h-[140px] overflow-y-auto space-y-0.5">
                {outfits.length === 0 && (
                  <p className="text-[12px] text-zinc-500">{t("No outfits found")}</p>
                )}
                {outfits.map((o) => (
                  <button
                    key={o.id}
                    onClick={() => setSelectedOutfit(o.id)}
                    className={`w-full text-left px-2 py-1 rounded text-xs transition-colors ${
                      selectedOutfit === o.id
                        ? "bg-sky-500/15 text-sky-300"
                        : "text-zinc-300 hover:bg-zinc-700/50"
                    }`}
                  >
                    {o.name}
                  </button>
                ))}
              </div>
            )}
            {selectedOutfit !== null && (
              <UtilButton onClick={handleWearOutfit} disabled={loading === "wear"}>
                Wear Outfit
              </UtilButton>
            )}
          </div>

          <SectionHeader>Universe</SectionHeader>
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <div className="flex-1">
                <UtilInput
                  value={universeIdInput}
                  onChange={setUniverseIdInput}
                  placeholder="Universe ID"
                />
              </div>
              <UtilButton onClick={handleLoadUniversePlaces} disabled={loading === "universe"}>
                Load Places
              </UtilButton>
            </div>
            {universePlaces.length > 0 && (
              <div className="bg-zinc-800/30 rounded-lg p-2 max-h-[120px] overflow-y-auto space-y-0.5">
                {universePlaces.map((p) => (
                  <button
                    key={p.id}
                    onClick={() => {
                      store.setPlaceId(String(p.id));
                      store.addToast(t("Place set to {{id}}", { id: p.id }));
                    }}
                    className="w-full text-left px-2 py-1 rounded text-xs text-zinc-300 hover:bg-zinc-700/50 transition-colors"
                  >
                    {p.name} ({p.id})
                  </button>
                ))}
              </div>
            )}
          </div>

          <SectionHeader>Custom Avatar JSON</SectionHeader>
          <div className="space-y-2">
            <p className="text-[12px] text-zinc-500">
              {t(
                "Looking for an item by name instead? Use Wear Outfit above — it lists outfits by name and builds this JSON for you."
              )}
            </p>
            <textarea
              value={avatarJsonInput}
              onChange={(e) => handleAvatarJsonChange(e.target.value)}
              onBlur={(e) => setAvatarJsonError(validateAvatarJson(e.target.value, t))}
              placeholder='{"assets":[{"id":12345}]}'
              className={`w-full min-h-[80px] p-2 bg-zinc-800/50 border rounded-lg text-xs text-zinc-300 font-mono placeholder-zinc-600 resize-none focus:outline-none transition-colors ${
                avatarJsonError ? "border-red-500/50 focus:border-red-500/70" : "border-zinc-700/50 focus:border-zinc-600"
              }`}
              spellCheck={false}
            />
            {avatarJsonError && <p className="text-[12px] text-red-400">{avatarJsonError}</p>}
            <UtilButton onClick={handleWearCustomAvatar} disabled={loading === "custom_avatar" || !!avatarJsonError}>
              Apply Avatar JSON
            </UtilButton>
          </div>
        </div>
      </div>
    </div>
  );
}
