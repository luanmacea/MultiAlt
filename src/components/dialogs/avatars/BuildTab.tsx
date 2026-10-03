import { Check, Dices, Loader2, PersonStanding, RefreshCw, RotateCcw, Save, Trash2 } from "lucide-react";
import { useTr } from "../../../i18n/text";
import {
  AVATAR_CATEGORIES,
  MAX_ACCESSORIES,
  REQUIRED_CATEGORIES,
  SKIN_COLORS,
  type FreeCatalogItem,
  type SavedAvatar,
} from "../../../avatarBuilder";
import { CATEGORY_LABEL, Mosaic, SectionTitle, Thumb, skinHex, thumbKey, type ThumbMap } from "./shared";
import type { AvatarDraft, CatalogByCategory } from "./useAvatarDraft";

/**
 * Aba Montar: categorias à esquerda, itens da categoria no meio e, à direita,
 * a prévia, a pele, o sorteio, o nome/salvar e os avatares salvos.
 */
export function BuildTab({
  catalog,
  catalogLoading,
  catalogError,
  onRetryCatalog,
  grouped,
  thumbs,
  draft,
  saved,
  savedLoading,
  saving,
  onSave,
  onDelete,
}: {
  catalog: FreeCatalogItem[] | null;
  catalogLoading: boolean;
  catalogError: string | null;
  onRetryCatalog: () => void;
  grouped: CatalogByCategory;
  thumbs: ThumbMap;
  draft: AvatarDraft;
  saved: SavedAvatar[];
  savedLoading: boolean;
  saving: boolean;
  onSave: () => void;
  onDelete: (avatar: SavedAvatar) => void;
}) {
  const t = useTr();
  const { activeCategory, selection, skinColor } = draft;

  return (
    <div className="h-full grid grid-cols-[176px_minmax(0,1fr)_272px] gap-3">
      {/* Categorias */}
      <nav
        aria-label={t("Categories")}
        className="theme-surface rounded-xl border theme-border p-2 overflow-y-auto min-h-0 space-y-0.5"
      >
        {catalog === null
          ? AVATAR_CATEGORIES.map((category) => (
              <div key={category} className="flex items-center gap-2 px-2 py-1.5">
                <div className="w-8 h-8 rounded-md animate-pulse bg-[var(--panel-soft)]" />
                <div className="flex-1 space-y-1">
                  <div className="h-2.5 w-16 rounded animate-pulse bg-[var(--panel-soft)]" />
                  <div className="h-2 w-10 rounded animate-pulse bg-[var(--panel-soft)]" />
                </div>
              </div>
            ))
          : AVATAR_CATEGORIES.map((category) => {
              const active = category === activeCategory;
              const picks = selection[category];
              const first = picks[0];
              const required = REQUIRED_CATEGORIES.includes(category);
              const count = grouped[category].length;
              const label = t(CATEGORY_LABEL[category]);
              const subtitle =
                category === "accessory" && picks.length > 0
                  ? t("{{count}} of {{max}} picked", { count: picks.length, max: MAX_ACCESSORIES })
                  : first
                    ? first.name
                    : required
                      ? t("Required")
                      : t("Empty");
              return (
                <div
                  key={category}
                  className={`flex items-center gap-1 rounded-lg border transition-colors ${
                    active ? "theme-accent-border theme-accent-bg" : "border-transparent hover:bg-[var(--panel-soft)]"
                  }`}
                >
                  <button
                    onClick={() => draft.setActiveCategory(category)}
                    aria-current={active ? "true" : undefined}
                    className="flex-1 min-w-0 flex items-center gap-2 pl-1.5 py-1.5 text-left"
                  >
                    <span
                      className={`w-8 h-8 shrink-0 rounded-md overflow-hidden border ${
                        first ? "theme-border bg-[var(--textboxes-bg)]" : "border-dashed theme-border"
                      }`}
                    >
                      {first ? <Thumb url={thumbs.get(thumbKey(first))} iconSize={13} /> : null}
                    </span>
                    <span className="flex-1 min-w-0">
                      <span className="flex items-center gap-1 text-[12px] font-medium text-[var(--panel-fg)]">
                        <span className="truncate">{label}</span>
                        <span className="text-[11px] font-normal theme-muted tabular-nums">{count}</span>
                      </span>
                      <span
                        className={`block text-[11px] truncate ${
                          !first && required ? "text-amber-400/90" : "theme-muted"
                        }`}
                      >
                        {subtitle}
                      </span>
                    </span>
                  </button>
                  <button
                    onClick={() => draft.reroll(grouped, category)}
                    disabled={count === 0}
                    aria-label={t("Randomize {{category}}", { category: label })}
                    title={t("Randomize {{category}}", { category: label })}
                    className="p-1.5 mr-1 rounded-md theme-muted hover:text-[var(--accent-color)] hover:bg-[var(--panel-soft)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    <Dices size={14} strokeWidth={1.75} />
                  </button>
                </div>
              );
            })}
      </nav>

      {/* Itens da categoria */}
      <section className="theme-surface rounded-xl border theme-border min-h-0 flex flex-col overflow-hidden">
        <div className="px-3 py-2 border-b theme-border flex items-center justify-between gap-2 shrink-0">
          <div className="text-[12.5px] font-semibold text-[var(--panel-fg)]">
            {t(CATEGORY_LABEL[activeCategory])}
            {catalog ? (
              <span className="ml-1.5 text-[11px] font-normal theme-muted">
                {t("{{count}} free items", { count: grouped[activeCategory].length })}
              </span>
            ) : null}
          </div>
          <div className="text-[11px] theme-muted">
            {activeCategory === "accessory"
              ? t("Pick up to {{max}}", { max: MAX_ACCESSORIES })
              : REQUIRED_CATEGORIES.includes(activeCategory)
                ? t("Pick one — required")
                : t("Pick one or leave empty")}
          </div>
        </div>
        <div className="flex-1 min-h-0 overflow-y-auto p-3">
          {catalogError ? (
            <div className="h-full flex flex-col items-center justify-center text-center gap-2 px-6">
              <div className="text-[13px] text-[var(--panel-fg)]">{t("Could not load the free catalog")}</div>
              <div className="text-[11px] theme-muted break-words max-w-[360px]">{catalogError}</div>
              <button
                onClick={onRetryCatalog}
                disabled={catalogLoading}
                className="sidebar-btn-sm mt-1 flex items-center gap-1.5 disabled:opacity-50"
              >
                <RefreshCw size={13} strokeWidth={1.75} className={catalogLoading ? "animate-spin" : ""} />
                {t("Try again")}
              </button>
            </div>
          ) : catalog === null ? (
            <div>
              <div className="flex items-center gap-2 text-[11px] theme-muted mb-3">
                <Loader2 size={13} className="animate-spin" />
                {t("Loading the free catalog...")}
              </div>
              <div className="grid grid-cols-[repeat(auto-fill,minmax(96px,1fr))] gap-2">
                {Array.from({ length: 12 }, (_, i) => (
                  <div key={i} className="rounded-xl border theme-border p-1.5">
                    <div className="aspect-square rounded-lg animate-pulse bg-[var(--panel-soft)]" />
                    <div className="mt-1.5 h-2.5 w-3/4 rounded animate-pulse bg-[var(--panel-soft)]" />
                  </div>
                ))}
              </div>
            </div>
          ) : grouped[activeCategory].length === 0 ? (
            <div className="h-full flex items-center justify-center text-[12px] theme-muted text-center px-6">
              {t("No free items in this category right now")}
            </div>
          ) : (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(96px,1fr))] gap-2">
              {grouped[activeCategory].map((item) => {
                const selected = selection[activeCategory].some((c) => c.id === item.id && c.kind === item.kind);
                const full =
                  activeCategory === "accessory" && !selected && selection.accessory.length >= MAX_ACCESSORIES;
                return (
                  <button
                    key={thumbKey(item)}
                    onClick={() => draft.toggle(item)}
                    disabled={full}
                    aria-pressed={selected}
                    aria-label={item.name}
                    title={full ? t("Up to {{max}} accessories", { max: MAX_ACCESSORIES }) : item.name}
                    className={`group relative rounded-xl border p-1.5 text-left transition-all ${
                      selected
                        ? "theme-accent-border theme-accent-bg ring-1 ring-[var(--accent-strong)]"
                        : "theme-border hover:bg-[var(--panel-soft)] hover:-translate-y-px"
                    } disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:translate-y-0`}
                  >
                    <div className="aspect-square rounded-lg overflow-hidden bg-[var(--textboxes-bg)]">
                      <Thumb url={thumbs.get(thumbKey(item))} iconSize={22} />
                    </div>
                    <div className="mt-1 text-[11px] leading-tight text-[var(--panel-fg)] line-clamp-2 min-h-[2lh]">
                      {item.name}
                    </div>
                    {selected ? (
                      <span className="absolute top-2.5 right-2.5 w-5 h-5 rounded-full bg-[var(--accent-color)] text-[var(--panel-bg)] flex items-center justify-center shadow">
                        <Check size={12} strokeWidth={3} />
                      </span>
                    ) : null}
                  </button>
                );
              })}
            </div>
          )}
        </div>
      </section>

      {/* Prévia, pele, sorteio, salvar e salvos */}
      <div className="min-h-0 overflow-y-auto space-y-3 pr-0.5">
        <section aria-label={t("Preview")} className="theme-surface rounded-xl border theme-border p-3">
          <SectionTitle
            aside={
              <span
                className="w-7 h-7 rounded-full border theme-border flex items-center justify-center"
                style={{ background: skinHex(skinColor) }}
                title={t("Skin tone")}
              >
                <PersonStanding size={15} strokeWidth={2} className="text-black/50" />
              </span>
            }
          >
            {t("Preview")}
          </SectionTitle>
          <div className="grid grid-cols-4 gap-1.5">
            {AVATAR_CATEGORIES.flatMap((category) => {
              const picks = selection[category];
              const slots: (FreeCatalogItem | null)[] = picks.length > 0 ? picks : [null];
              return slots.map((item, i) => (
                <button
                  key={`${category}-${item ? thumbKey(item) : i}`}
                  onClick={() => draft.setActiveCategory(category)}
                  title={item ? item.name : t(CATEGORY_LABEL[category])}
                  className="min-w-0 text-left"
                >
                  <div
                    className={`aspect-square rounded-lg overflow-hidden border ${
                      item
                        ? "theme-border bg-[var(--textboxes-bg)]"
                        : `border-dashed ${
                            REQUIRED_CATEGORIES.includes(category) ? "border-amber-500/40" : "theme-border"
                          }`
                    }`}
                  >
                    {item ? <Thumb url={thumbs.get(thumbKey(item))} iconSize={14} /> : null}
                  </div>
                  <div
                    className={`mt-0.5 text-[11px] leading-tight truncate ${
                      item ? "text-[var(--panel-fg)]" : "theme-muted"
                    }`}
                  >
                    {item ? item.name : t(CATEGORY_LABEL[category])}
                  </div>
                </button>
              ));
            })}
          </div>

          <div className="mt-3 text-[11px] theme-muted mb-1.5">{t("Skin tone")}</div>
          <div className="flex flex-wrap gap-1.5">
            {SKIN_COLORS.map((color, i) => {
              const active = color.id === skinColor;
              return (
                <button
                  key={color.id}
                  onClick={() => draft.setSkinColor(color.id)}
                  aria-pressed={active}
                  aria-label={t("Skin tone {{n}}", { n: i + 1 })}
                  className={`w-6 h-6 rounded-full border transition-transform hover:scale-110 ${
                    active
                      ? "border-[var(--accent-color)] ring-2 ring-[var(--accent-strong)] ring-offset-1 ring-offset-[var(--forms-bg)]"
                      : "theme-border"
                  }`}
                  style={{ background: color.hex }}
                />
              );
            })}
          </div>

          <div className="mt-3 flex gap-1.5">
            <button
              onClick={() => draft.randomizeAll(grouped)}
              disabled={!catalog}
              className="flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-[var(--button-radius)] border theme-accent-border theme-accent-bg text-[12.5px] font-semibold text-[var(--panel-fg)] transition-all hover:brightness-110 active:scale-[0.98] disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <Dices size={16} strokeWidth={1.75} className="theme-accent" />
              {t("Randomize all")}
            </button>
            <button
              onClick={draft.startOver}
              aria-label={t("Start over")}
              title={t("Start over")}
              className="sidebar-btn-sm px-2.5 flex items-center"
            >
              <RotateCcw size={14} strokeWidth={1.75} />
            </button>
          </div>
        </section>

        <section className="theme-surface rounded-xl border theme-border p-3 space-y-2">
          <input
            value={draft.name}
            onChange={(e) => draft.setName(e.target.value)}
            onKeyDown={(e) => {
              // Segurar/repetir Enter não pode salvar duas vezes.
              if (e.key === "Enter" && !e.repeat && !saving) onSave();
            }}
            maxLength={60}
            aria-label={t("Avatar name")}
            placeholder={t("Avatar name")}
            className="sidebar-input text-xs"
          />
          <button
            onClick={onSave}
            disabled={saving}
            aria-label={t("Save avatar")}
            className="sidebar-btn-sm w-full flex items-center justify-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {saving ? <Loader2 size={13} className="animate-spin" /> : <Save size={13} strokeWidth={1.75} />}
            {t("Save")}
          </button>
          {draft.error ? (
            <div role="alert" className="text-[11px] text-amber-400 leading-4">
              {t(draft.error)}
            </div>
          ) : draft.editingId ? (
            <div className="text-[11px] theme-muted leading-4">
              {t("Saving again updates this avatar. Start over to make a new one.")}
            </div>
          ) : null}
        </section>

        <section className="theme-surface rounded-xl border theme-border p-3">
          <SectionTitle aside={savedLoading ? <Loader2 size={12} className="animate-spin theme-muted" /> : null}>
            {t("Saved avatars ({{count}})", { count: saved.length })}
          </SectionTitle>
          {saved.length === 0 ? (
            <div className="text-[11px] theme-muted leading-4">
              {savedLoading ? t("Loading...") : t("No saved avatars yet. Build one and save it.")}
            </div>
          ) : (
            <div className="space-y-1">
              {saved.map((avatar) => (
                <div
                  key={avatar.id}
                  className={`flex items-center gap-1 rounded-lg border ${
                    avatar.id === draft.editingId ? "theme-accent-border theme-accent-bg" : "theme-border"
                  }`}
                >
                  <button
                    onClick={() => draft.load(avatar)}
                    aria-label={t("Load {{name}}", { name: avatar.name })}
                    className="flex-1 min-w-0 flex items-center gap-2 p-1.5 text-left rounded-lg hover:bg-[var(--panel-soft)]"
                  >
                    <span className="w-9 h-9 shrink-0">
                      <Mosaic avatar={avatar} thumbs={thumbs} />
                    </span>
                    <span className="min-w-0">
                      <span className="block text-[12px] text-[var(--panel-fg)] truncate">{avatar.name}</span>
                      <span className="block text-[11px] theme-muted">
                        {t("{{count}} items", { count: avatar.items.length })}
                      </span>
                    </span>
                  </button>
                  <button
                    onClick={() => onDelete(avatar)}
                    aria-label={t("Delete {{name}}", { name: avatar.name })}
                    title={t("Delete {{name}}", { name: avatar.name })}
                    className="p-1.5 mr-1 rounded-md theme-muted hover:text-red-400 hover:bg-red-500/10 transition-colors"
                  >
                    <Trash2 size={13} strokeWidth={1.75} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
