import { Check, Loader2, Shirt } from "lucide-react";
import { useTr } from "../../../i18n/text";
import type { Account } from "../../../types";
import type { SavedAvatar } from "../../../avatarBuilder";
import { AccountPicker } from "./AccountPicker";
import { BatchPanel } from "./BatchPanel";
import { Mosaic, skinHex, type AvatarBatchSnapshot, type ThumbMap } from "./shared";

/**
 * Aba Distribuir: os avatares salvos que entram no sorteio (à esquerda) e, à
 * direita, as contas que recebem — ou o progresso/resultado do lote, quando há um.
 */
export function DistributeTab({
  saved,
  savedLoading,
  thumbs,
  useAll,
  chosenIds,
  onSetUseAll,
  onToggleAvatar,
  onGoBuild,
  accounts,
  avatarUrls,
  picked,
  onPickedChange,
  batch,
  showBatch,
  starting,
  cancelling,
  canApply,
  onApply,
  onCancel,
  onDismiss,
  accountName,
}: {
  saved: SavedAvatar[];
  savedLoading: boolean;
  thumbs: ThumbMap;
  useAll: boolean;
  chosenIds: string[];
  onSetUseAll: (on: boolean) => void;
  onToggleAvatar: (id: string) => void;
  onGoBuild: () => void;
  accounts: Account[];
  avatarUrls: Map<number, string>;
  picked: ReadonlySet<number>;
  onPickedChange: (next: Set<number>) => void;
  batch: AvatarBatchSnapshot | null;
  showBatch: boolean;
  starting: boolean;
  cancelling: boolean;
  canApply: boolean;
  onApply: () => void;
  onCancel: () => void;
  onDismiss: () => void;
  accountName: (userId: number) => string;
}) {
  const t = useTr();
  const running = batch?.running === true;
  const pickedCount = accounts.filter((a) => picked.has(a.UserID)).length;

  return (
    <div className="h-full grid grid-cols-[minmax(0,1fr)_320px] gap-3">
      {/* Avatares salvos para sortear */}
      <section data-tour="avatars-handout" className="theme-surface rounded-xl border theme-border min-h-0 flex flex-col overflow-hidden">
        <div className="px-3 py-2.5 border-b theme-border flex items-start justify-between gap-3 shrink-0">
          <div className="min-w-0">
            <div className="text-[12.5px] font-semibold text-[var(--panel-fg)]">{t("Avatars to hand out")}</div>
            <div className="text-[11px] theme-muted leading-4">
              {t("Each account gets one of the checked avatars, spread so none repeats while another is unused.")}
            </div>
          </div>
          <label className="flex items-center gap-2 shrink-0 text-[12px] text-[var(--panel-fg)] cursor-pointer">
            {t("Use all")}
            <button
              role="switch"
              aria-checked={useAll}
              aria-label={t("Use all")}
              disabled={running}
              onClick={() => onSetUseAll(!useAll)}
              className={`w-8 h-[18px] rounded-full transition-colors relative border disabled:opacity-50 ${
                useAll
                  ? "bg-[var(--toggle-on-bg)] border-[var(--toggle-on-bg)]"
                  : "bg-[var(--toggle-off-bg)] border-[var(--toggle-off-bg)]"
              }`}
            >
              <span
                className={`w-3.5 h-3.5 rounded-full bg-[var(--toggle-knob-bg)] absolute top-[1px] transition-all ${
                  useAll ? "left-[15px]" : "left-[1px]"
                }`}
              />
            </button>
          </label>
        </div>
        <div className="flex-1 min-h-0 overflow-y-auto p-3">
          {saved.length === 0 ? (
            <div className="h-full flex flex-col items-center justify-center gap-2 text-center px-6">
              <Shirt size={28} strokeWidth={1.25} className="theme-muted" />
              <div className="text-[12.5px] text-[var(--panel-fg)]">
                {savedLoading ? t("Loading...") : t("No saved avatars yet")}
              </div>
              {!savedLoading ? (
                <>
                  <div className="text-[11px] theme-muted">{t("Build a few avatars first, then hand them out here.")}</div>
                  <button onClick={onGoBuild} className="sidebar-btn-sm mt-1">
                    {t("Go to Build")}
                  </button>
                </>
              ) : null}
            </div>
          ) : (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(132px,1fr))] gap-2">
              {saved.map((avatar) => {
                const on = chosenIds.includes(avatar.id);
                return (
                  <button
                    key={avatar.id}
                    role="checkbox"
                    aria-checked={on}
                    aria-label={avatar.name}
                    disabled={running}
                    onClick={() => onToggleAvatar(avatar.id)}
                    className={`relative rounded-xl border p-2 text-left transition-all disabled:cursor-not-allowed ${
                      on
                        ? "theme-accent-border theme-accent-bg ring-1 ring-[var(--accent-strong)]"
                        : "theme-border hover:bg-[var(--panel-soft)] opacity-80"
                    }`}
                  >
                    <div className="aspect-square">
                      <Mosaic avatar={avatar} thumbs={thumbs} />
                    </div>
                    <div className="mt-1.5 flex items-center gap-1.5">
                      <span
                        className="w-3 h-3 shrink-0 rounded-full border theme-border"
                        style={{ background: skinHex(avatar.skinColor) }}
                      />
                      <span className="text-[12px] text-[var(--panel-fg)] truncate">{avatar.name}</span>
                    </div>
                    <div className="text-[11px] theme-muted">
                      {t("{{count}} items", { count: avatar.items.length })}
                    </div>
                    {/* Mesmo círculo nos dois estados: marcado é cheio, desmarcado só o contorno. */}
                    <span
                      className={`absolute top-3 right-3 w-5 h-5 rounded-full border-2 flex items-center justify-center shadow-sm ${
                        on
                          ? "bg-[var(--accent-color)] border-[var(--accent-color)] text-[var(--panel-bg)]"
                          : "border-[var(--panel-fg)] bg-transparent opacity-70"
                      }`}
                    >
                      {on ? <Check size={11} strokeWidth={3} /> : null}
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </div>
      </section>

      {/* Contas e progresso */}
      <section data-tour="avatars-accounts" className="theme-surface rounded-xl border theme-border min-h-0 flex flex-col overflow-hidden">
        {showBatch && batch ? (
          <BatchPanel
            batch={batch}
            cancelling={cancelling}
            onCancel={onCancel}
            onDismiss={onDismiss}
            accountName={accountName}
            avatarName={(id) => saved.find((a) => a.id === id)?.name ?? null}
            headshot={(id) => avatarUrls.get(id)}
          />
        ) : (
          <>
            <AccountPicker
              accounts={accounts}
              avatarUrls={avatarUrls}
              picked={picked}
              onChange={onPickedChange}
              disabled={running || starting}
            />
            <div className="p-3 border-t theme-border shrink-0 space-y-2">
              <button
                onClick={onApply}
                disabled={!canApply}
                aria-label={t("Apply avatars")}
                className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-[var(--button-radius)] border theme-accent-border theme-accent-bg text-[12.5px] font-semibold text-[var(--panel-fg)] transition-all hover:brightness-110 active:scale-[0.98] disabled:opacity-45 disabled:cursor-not-allowed disabled:hover:brightness-100"
              >
                {starting ? (
                  <Loader2 size={15} className="animate-spin" />
                ) : (
                  <Shirt size={15} strokeWidth={1.75} className="theme-accent" />
                )}
                {t("Apply avatars")}
              </button>
              {accounts.length > 0 && pickedCount === 0 ? (
                <div className="text-[11px] text-amber-400/90 leading-4">
                  {t("Check the accounts that should get an avatar.")}
                </div>
              ) : saved.length > 0 && chosenIds.length === 0 ? (
                <div className="text-[11px] text-amber-400/90 leading-4">{t("Check at least one saved avatar.")}</div>
              ) : null}
              <div className="text-[11px] theme-muted leading-4">
                {t(
                  "One account at a time: items the account does not have are claimed for free (7 s apart), then the avatar is worn. If Roblox asks for a verification, that account is skipped."
                )}
              </div>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
