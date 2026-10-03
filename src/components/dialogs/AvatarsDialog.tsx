import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Check,
  Dices,
  Loader2,
  Package,
  PersonStanding,
  RefreshCw,
  RotateCcw,
  Save,
  Shirt,
  Trash2,
  User,
  Users,
  X,
} from "lucide-react";
import { useStore } from "../../store";
import type { Account } from "../../types";
import { useModalClose } from "../../hooks/useModalClose";
import { useConfirm } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";
import {
  AVATAR_CATEGORIES,
  MAX_ACCESSORIES,
  REQUIRED_CATEGORIES,
  SKIN_COLORS,
  emptySelection,
  groupByCategory,
  newAvatarId,
  randomSkin,
  randomizeSelection,
  selectionFromAvatar,
  selectionItems,
  toggleItem,
  validateAvatarDraft,
  type AvatarCategory,
  type FreeCatalogItem,
  type SavedAvatar,
  type Selection,
} from "../../avatarBuilder";

/** Resultado de uma conta no lote (`AvatarAccountResult` do backend, em camelCase). */
interface AvatarAccountResult {
  userId: number;
  avatarId: string;
  status: "ok" | "skipped" | "failed";
  /** "challenge" | "cancelled" | texto livre do backend. */
  reason: string | null;
  claimed: number;
  missing: number;
}

/** Retrato completo do lote, que chega pelo evento `avatar-batch-state`. */
interface AvatarBatchSnapshot {
  running: boolean;
  total: number;
  done: number;
  currentUserId: number | null;
  accounts: AvatarAccountResult[];
}

interface ThumbnailResponse {
  targetId: number;
  imageUrl: string | null;
  state?: string;
}

type Tab = "build" | "distribute";

/** Rótulos na tela; a chave interna da categoria nunca aparece. */
const CATEGORY_LABEL: Record<AvatarCategory, string> = {
  hair: "Hair",
  hat: "Hat",
  accessory: "Accessories",
  shirt: "Shirt",
  pants: "Pants",
  tshirt: "T-Shirt",
  body: "Body",
  head: "Head",
};

const DEFAULT_SKIN = SKIN_COLORS[0].id;
/** O Roblox às vezes responde "Pending" enquanto gera a imagem: tenta de novo algumas vezes. */
const THUMB_RETRY_MS = 2500;
const THUMB_MAX_RETRIES = 3;

function thumbKey(item: Pick<FreeCatalogItem, "kind" | "id">): string {
  return `${item.kind}:${item.id}`;
}

function skinHex(id: number | null): string {
  return SKIN_COLORS.find((c) => c.id === id)?.hex ?? SKIN_COLORS[0].hex;
}

/** Imagem do item: `undefined` = carregando, `null` = sem imagem. */
function Thumb({ url, iconSize = 18 }: { url: string | null | undefined; iconSize?: number }) {
  if (url === undefined) return <div className="w-full h-full animate-pulse bg-[var(--panel-soft)]" />;
  if (url === null) {
    return (
      <div className="w-full h-full flex items-center justify-center theme-muted">
        <Package size={iconSize} strokeWidth={1.5} />
      </div>
    );
  }
  return <img src={url} alt="" loading="lazy" draggable={false} className="w-full h-full object-contain" />;
}

/** Grade 2x2 com as quatro primeiras peças de um avatar salvo. */
function Mosaic({ avatar, thumbs }: { avatar: SavedAvatar; thumbs: Map<string, string | null> }) {
  const cells = avatar.items.slice(0, 4);
  return (
    <div className="grid grid-cols-2 gap-px w-full h-full rounded-lg overflow-hidden bg-[var(--panel-soft)]">
      {[0, 1, 2, 3].map((i) => (
        <div key={i} className="bg-[var(--textboxes-bg)] overflow-hidden">
          {cells[i] ? <Thumb url={thumbs.get(thumbKey(cells[i]))} iconSize={12} /> : null}
        </div>
      ))}
    </div>
  );
}

function Headshot({ url, size = 24 }: { url: string | undefined; size?: number }) {
  return (
    <span
      className="shrink-0 rounded-full overflow-hidden bg-[var(--panel-soft)] flex items-center justify-center theme-muted"
      style={{ width: size, height: size }}
    >
      {url ? (
        <img src={url} alt="" className="w-full h-full object-cover theme-avatar" draggable={false} />
      ) : (
        <User size={Math.round(size * 0.55)} strokeWidth={1.75} />
      )}
    </span>
  );
}

function SectionTitle({ children, aside }: { children: ReactNode; aside?: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-2 mb-2">
      <div className="text-[11px] font-semibold uppercase tracking-wide theme-muted">{children}</div>
      {aside}
    </div>
  );
}

/**
 * Avatares gratuitos: monta avatares só com itens oficiais grátis do Roblox,
 * salva, e distribui os salvos entre as contas selecionadas na lista principal.
 *
 * A lógica (categorias, sorteio, validação) mora em `avatarBuilder.ts`; aqui só
 * se liga aquilo aos comandos. O lote roda no backend, uma conta por vez, e
 * chega inteiro a cada passo pelo evento `avatar-batch-state` — a tela pode
 * fechar e reabrir no meio que retoma de onde está (`get_avatar_batch_state`).
 */
export function AvatarsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useTr();
  const store = useStore();
  const confirm = useConfirm();
  const { visible, closing, handleClose } = useModalClose(open, onClose);

  const storeRef = useRef(store);
  storeRef.current = store;
  const tRef = useRef(t);
  tRef.current = t;
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const [tab, setTab] = useState<Tab>("build");

  // ---- Catálogo ------------------------------------------------------------
  const [catalog, setCatalog] = useState<FreeCatalogItem[] | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const grouped = useMemo(() => groupByCategory(catalog ?? []), [catalog]);

  const loadCatalog = useCallback(async () => {
    setCatalogLoading(true);
    setCatalogError(null);
    try {
      const items = await invoke<FreeCatalogItem[]>("avatar_free_catalog");
      if (!mountedRef.current) return;
      setCatalog(Array.isArray(items) ? items : []);
    } catch (e) {
      if (mountedRef.current) setCatalogError(String(e));
    } finally {
      if (mountedRef.current) setCatalogLoading(false);
    }
  }, []);

  // ---- Miniaturas (lazy, por item) ----------------------------------------
  const [thumbs, setThumbs] = useState<Map<string, string | null>>(new Map());
  const requestedRef = useRef<Set<string>>(new Set());
  const retriesRef = useRef<Map<string, number>>(new Map());

  const ensureThumbs = useCallback((items: FreeCatalogItem[]) => {
    const missing = items.filter((item) => !requestedRef.current.has(thumbKey(item)));
    if (missing.length === 0) return;
    for (const item of missing) requestedRef.current.add(thumbKey(item));
    for (const kind of ["Asset", "Bundle"] as const) {
      const group = missing.filter((item) => item.kind === kind);
      if (group.length === 0) continue;
      const requests = group.map((item) => ({
        requestId: thumbKey(item),
        type: kind === "Asset" ? "Asset" : "BundleThumbnail",
        targetId: item.id,
        size: "150x150",
        format: "Png",
      }));
      invoke<ThumbnailResponse[]>("batch_thumbnails", { requests })
        .then((result) => {
          if (!mountedRef.current) return;
          const list = Array.isArray(result) ? result : [];
          const byId = new Map(list.map((r) => [r.targetId, r]));
          const retry: FreeCatalogItem[] = [];
          setThumbs((prev) => {
            const next = new Map(prev);
            for (const item of group) {
              const key = thumbKey(item);
              const hit = byId.get(item.id);
              if (hit?.imageUrl) {
                next.set(key, hit.imageUrl);
                continue;
              }
              const tries = retriesRef.current.get(key) ?? 0;
              if (hit?.state === "Pending" && tries < THUMB_MAX_RETRIES) {
                retriesRef.current.set(key, tries + 1);
                retry.push(item);
              } else {
                next.set(key, null);
              }
            }
            return next;
          });
          if (retry.length > 0) {
            window.setTimeout(() => {
              for (const item of retry) requestedRef.current.delete(thumbKey(item));
              if (mountedRef.current) ensureThumbs(retry);
            }, THUMB_RETRY_MS);
          }
        })
        .catch(() => {
          if (!mountedRef.current) return;
          // Sem imagem agora; o pedido volta a valer na próxima vez que o item aparecer.
          for (const item of group) requestedRef.current.delete(thumbKey(item));
          setThumbs((prev) => {
            const next = new Map(prev);
            for (const item of group) next.set(thumbKey(item), null);
            return next;
          });
        });
    }
  }, []);

  // ---- Rascunho do avatar -------------------------------------------------
  const [activeCategory, setActiveCategory] = useState<AvatarCategory>(AVATAR_CATEGORIES[0]);
  const [selection, setSelection] = useState<Selection>(() => emptySelection());
  const [skinColor, setSkinColor] = useState<number>(DEFAULT_SKIN);
  const [name, setName] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftError, setDraftError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // ---- Avatares salvos ----------------------------------------------------
  const [saved, setSaved] = useState<SavedAvatar[]>([]);
  const [savedLoading, setSavedLoading] = useState(false);

  const loadSaved = useCallback(async () => {
    setSavedLoading(true);
    try {
      const list = await invoke<SavedAvatar[]>("avatar_list_saved");
      if (mountedRef.current) setSaved(Array.isArray(list) ? list : []);
    } catch (e) {
      if (mountedRef.current) storeRef.current.addToast(String(e), "error");
    } finally {
      if (mountedRef.current) setSavedLoading(false);
    }
  }, []);

  // ---- Lote ---------------------------------------------------------------
  const [batch, setBatch] = useState<AvatarBatchSnapshot | null>(null);
  const [starting, setStarting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  /** O usuário fechou o resumo do último lote e voltou à seleção. */
  const [dismissed, setDismissed] = useState(false);
  const prevRunningRef = useRef(false);

  const receive = useCallback(
    (snapshot: AvatarBatchSnapshot | null | undefined) => {
      if (!snapshot || typeof snapshot !== "object") return;
      const finished = prevRunningRef.current && !snapshot.running;
      prevRunningRef.current = snapshot.running;
      setBatch(snapshot);
      if (snapshot.running) setDismissed(false);
      if (!snapshot.running) setCancelling(false);
      if (finished && snapshot.accounts.length > 0) {
        // O avatar mudou: a foto em cache dessas contas está velha.
        void storeRef.current.refreshAvatarHeadshots(snapshot.accounts.map((a) => a.userId));
        const ok = snapshot.accounts.filter((a) => a.status === "ok").length;
        storeRef.current.addToast(
          tRef.current("Avatar batch finished: {{ok}} of {{total}} accounts done", { ok, total: snapshot.total })
        );
      }
    },
    []
  );

  // O ouvinte vive enquanto o diálogo existe (não só aberto): o lote pode acabar
  // com a tela fechada, e as fotos das contas ainda precisam ser atualizadas.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    listen<AvatarBatchSnapshot>("avatar-batch-state", (event) => receive(event.payload))
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [receive]);

  useEffect(() => {
    if (!visible) return;
    if (!catalog && !catalogLoading) void loadCatalog();
    void loadSaved();
    invoke<AvatarBatchSnapshot>("get_avatar_batch_state")
      .then((snapshot) => {
        if (!mountedRef.current || !snapshot) return;
        receive(snapshot);
        if (snapshot.running) setTab("distribute");
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible]);

  useEffect(() => {
    if (visible && catalog) ensureThumbs(grouped[activeCategory]);
  }, [visible, catalog, grouped, activeCategory, ensureThumbs]);

  useEffect(() => {
    if (visible) ensureThumbs(selectionItems(selection));
  }, [visible, selection, ensureThumbs]);

  useEffect(() => {
    if (visible) ensureThumbs(saved.flatMap((a) => a.items.slice(0, 4)));
  }, [visible, saved, ensureThumbs]);

  // ---- Ações do montador --------------------------------------------------
  function randomizeAll() {
    if (!catalog) return;
    setSelection(randomizeSelection(grouped, Math.random));
    setSkinColor(randomSkin(Math.random));
    setDraftError(null);
  }

  function reroll(category: AvatarCategory) {
    if (!catalog) return;
    setSelection((prev) => randomizeSelection(grouped, Math.random, category, prev));
    setDraftError(null);
  }

  function startOver() {
    setSelection(emptySelection());
    setSkinColor(DEFAULT_SKIN);
    setName("");
    setEditingId(null);
    setDraftError(null);
  }

  function loadIntoBuilder(avatar: SavedAvatar) {
    setSelection(selectionFromAvatar(avatar));
    setSkinColor(avatar.skinColor ?? DEFAULT_SKIN);
    setName(avatar.name);
    setEditingId(avatar.id);
    setDraftError(null);
  }

  async function handleSave() {
    const problem = validateAvatarDraft(name, selection);
    if (problem) {
      setDraftError(problem);
      return;
    }
    setDraftError(null);
    setSaving(true);
    try {
      const avatar: SavedAvatar = {
        id: editingId ?? newAvatarId(),
        name: name.trim(),
        items: selectionItems(selection),
        skinColor,
      };
      const stored = (await invoke<SavedAvatar>("avatar_save", { avatar })) ?? avatar;
      if (!mountedRef.current) return;
      setSaved((prev) => {
        const index = prev.findIndex((a) => a.id === stored.id);
        if (index < 0) return [...prev, stored];
        const next = [...prev];
        next[index] = stored;
        return next;
      });
      setEditingId(stored.id);
      setName(stored.name);
      store.addToast(t("Avatar saved: {{name}}", { name: stored.name }));
    } catch (e) {
      store.addToast(t("Could not save the avatar: {{error}}", { error: String(e) }), "error");
    } finally {
      if (mountedRef.current) setSaving(false);
    }
  }

  async function handleDelete(avatar: SavedAvatar) {
    const ok = await confirm(t("Delete the avatar “{{name}}”?", { name: avatar.name }), true);
    if (!ok) return;
    try {
      await invoke<boolean>("avatar_delete", { id: avatar.id });
      if (!mountedRef.current) return;
      setSaved((prev) => prev.filter((a) => a.id !== avatar.id));
      setChecked((prev) => {
        const next = new Set(prev);
        next.delete(avatar.id);
        return next;
      });
      if (editingId === avatar.id) setEditingId(null);
    } catch (e) {
      store.addToast(String(e), "error");
    }
  }

  // ---- Distribuição -------------------------------------------------------
  const [useAll, setUseAll] = useState(true);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const chosenIds = useMemo(
    () => (useAll ? saved.map((a) => a.id) : saved.filter((a) => checked.has(a.id)).map((a) => a.id)),
    [useAll, saved, checked]
  );

  function toggleAvatarCheck(id: string) {
    if (useAll) {
      setUseAll(false);
      setChecked(new Set(saved.map((a) => a.id).filter((other) => other !== id)));
      return;
    }
    setChecked((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function setUseAllAvatars(on: boolean) {
    setUseAll(on);
    setChecked(new Set());
  }

  const running = batch?.running === true;
  const selectedUserIds = store.selectedAccounts.map((a) => a.UserID);
  const canApply = !running && !starting && selectedUserIds.length > 0 && chosenIds.length > 0;

  async function handleApply() {
    if (!canApply) return;
    const userIds = selectedUserIds;
    setStarting(true);
    setDismissed(false);
    receive({ running: true, total: userIds.length, done: 0, currentUserId: null, accounts: [] });
    try {
      const final = await invoke<AvatarBatchSnapshot>("avatar_apply_batch", { userIds, avatarIds: chosenIds });
      if (mountedRef.current) receive(final);
    } catch (e) {
      store.addToast(String(e), "error");
      // Recusado (outro lote rodando, conta sumiu...): volta ao estado real do backend.
      try {
        const snapshot = await invoke<AvatarBatchSnapshot>("get_avatar_batch_state");
        receive(snapshot ?? { running: false, total: 0, done: 0, currentUserId: null, accounts: [] });
      } catch {
        receive({ running: false, total: 0, done: 0, currentUserId: null, accounts: [] });
      }
    } finally {
      if (mountedRef.current) setStarting(false);
    }
  }

  async function handleCancel() {
    setCancelling(true);
    try {
      await invoke("avatar_cancel_batch");
    } catch (e) {
      setCancelling(false);
      store.addToast(String(e), "error");
    }
  }

  function accountOf(userId: number): Account | undefined {
    return store.accounts.find((a) => a.UserID === userId);
  }

  function accountName(userId: number): string {
    const account = accountOf(userId);
    return account?.Alias || account?.Username || `${t("User ID")}: ${userId}`;
  }

  if (!visible) return null;

  const showBatch = batch !== null && (batch.running || batch.accounts.length > 0) && !dismissed;

  return (
    <div
      className={`fixed inset-0 z-[70] flex items-center justify-center bg-black/60 backdrop-blur-sm ${
        closing ? "animate-fade-out" : "animate-fade-in"
      }`}
      onClick={handleClose}
    >
      <div
        role="dialog"
        aria-label={t("Avatars")}
        className={`theme-panel theme-border rounded-2xl border w-[1000px] max-w-[calc(100vw-24px)] h-[640px] max-h-[calc(100vh-24px)] flex flex-col overflow-hidden shadow-2xl ${
          closing ? "animate-scale-out" : "animate-scale-in"
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-5 py-3.5 border-b theme-border flex items-center justify-between gap-3 shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            <span className="w-9 h-9 shrink-0 rounded-xl border theme-accent-border theme-accent-bg theme-accent flex items-center justify-center">
              <Shirt size={18} strokeWidth={1.75} />
            </span>
            <div className="min-w-0">
              <div className="text-[15px] font-semibold text-[var(--panel-fg)]">{t("Avatars")}</div>
              <div className="text-[11.5px] theme-muted truncate">
                {t("Free official Roblox items only — never spends Robux")}
              </div>
            </div>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            {running ? (
              <span className="px-2 py-1 rounded-full text-[11px] border border-emerald-500/30 bg-emerald-500/15 text-emerald-300 animate-pulse">
                {t("Applying {{done}}/{{total}}", { done: batch?.done ?? 0, total: batch?.total ?? 0 })}
              </span>
            ) : null}
            <button
              onClick={handleClose}
              aria-label={t("Close")}
              className="p-1 rounded-md theme-muted hover:text-[var(--panel-fg)] transition-colors"
            >
              <X size={16} strokeWidth={2} />
            </button>
          </div>
        </div>

        <div role="tablist" aria-label={t("Avatars")} className="px-5 pt-3 flex items-center gap-1.5 shrink-0">
          {([
            { id: "build" as const, label: t("Build"), icon: <Shirt size={13} strokeWidth={1.75} /> },
            { id: "distribute" as const, label: t("Distribute"), icon: <Users size={13} strokeWidth={1.75} /> },
          ]).map((item) => {
            const active = tab === item.id;
            return (
              <button
                key={item.id}
                role="tab"
                aria-selected={active}
                onClick={() => setTab(item.id)}
                className={`flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium rounded-lg border transition-colors ${
                  active
                    ? "theme-accent-border theme-accent-bg text-[var(--panel-fg)]"
                    : "theme-border theme-muted theme-btn-ghost"
                }`}
              >
                <span className={active ? "theme-accent" : undefined}>{item.icon}</span>
                {item.label}
                {item.id === "distribute" && saved.length > 0 ? (
                  <span
                    aria-hidden="true"
                    className="ml-0.5 px-1.5 rounded-full text-[11px] tabular-nums bg-[var(--panel-soft)] theme-muted"
                  >
                    {saved.length}
                  </span>
                ) : null}
              </button>
            );
          })}
        </div>

        <div className="flex-1 min-h-0 p-4 pt-3">
          {tab === "build" ? (
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
                            active
                              ? "theme-accent-border theme-accent-bg"
                              : "border-transparent hover:bg-[var(--panel-soft)]"
                          }`}
                        >
                          <button
                            onClick={() => setActiveCategory(category)}
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
                            onClick={() => reroll(category)}
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
                        onClick={() => void loadCatalog()}
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
                        const selected = selection[activeCategory].some(
                          (c) => c.id === item.id && c.kind === item.kind
                        );
                        const full =
                          activeCategory === "accessory" &&
                          !selected &&
                          selection.accessory.length >= MAX_ACCESSORIES;
                        return (
                          <button
                            key={thumbKey(item)}
                            onClick={() => {
                              setSelection((prev) => toggleItem(prev, item));
                              setDraftError(null);
                            }}
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
                <section
                  aria-label={t("Preview")}
                  className="theme-surface rounded-xl border theme-border p-3"
                >
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
                          onClick={() => setActiveCategory(category)}
                          title={item ? item.name : t(CATEGORY_LABEL[category])}
                          className="min-w-0 text-left"
                        >
                          <div
                            className={`aspect-square rounded-lg overflow-hidden border ${
                              item
                                ? "theme-border bg-[var(--textboxes-bg)]"
                                : `border-dashed ${
                                    REQUIRED_CATEGORIES.includes(category)
                                      ? "border-amber-500/40"
                                      : "theme-border"
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
                          onClick={() => setSkinColor(color.id)}
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
                      onClick={randomizeAll}
                      disabled={!catalog}
                      className="flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-[var(--button-radius)] border theme-accent-border theme-accent-bg text-[12.5px] font-semibold text-[var(--panel-fg)] transition-all hover:brightness-110 active:scale-[0.98] disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      <Dices size={16} strokeWidth={1.75} className="theme-accent" />
                      {t("Randomize all")}
                    </button>
                    <button
                      onClick={startOver}
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
                    value={name}
                    onChange={(e) => {
                      setName(e.target.value);
                      setDraftError(null);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void handleSave();
                    }}
                    maxLength={60}
                    aria-label={t("Avatar name")}
                    placeholder={t("Avatar name")}
                    className="sidebar-input text-xs"
                  />
                  <button
                    onClick={() => void handleSave()}
                    disabled={saving}
                    aria-label={t("Save avatar")}
                    className="sidebar-btn-sm w-full flex items-center justify-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    {saving ? <Loader2 size={13} className="animate-spin" /> : <Save size={13} strokeWidth={1.75} />}
                    {t("Save")}
                  </button>
                  {draftError ? (
                    <div role="alert" className="text-[11px] text-amber-400 leading-4">
                      {t(draftError)}
                    </div>
                  ) : editingId ? (
                    <div className="text-[11px] theme-muted leading-4">
                      {t("Saving again updates this avatar. Start over to make a new one.")}
                    </div>
                  ) : null}
                </section>

                <section className="theme-surface rounded-xl border theme-border p-3">
                  <SectionTitle
                    aside={
                      savedLoading ? <Loader2 size={12} className="animate-spin theme-muted" /> : null
                    }
                  >
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
                            avatar.id === editingId ? "theme-accent-border theme-accent-bg" : "theme-border"
                          }`}
                        >
                          <button
                            onClick={() => loadIntoBuilder(avatar)}
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
                            onClick={() => void handleDelete(avatar)}
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
          ) : (
            <div className="h-full grid grid-cols-[minmax(0,1fr)_320px] gap-3">
              {/* Avatares salvos para sortear */}
              <section className="theme-surface rounded-xl border theme-border min-h-0 flex flex-col overflow-hidden">
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
                      onClick={() => setUseAllAvatars(!useAll)}
                      className={`w-8 h-[18px] rounded-full transition-colors relative border ${
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
                          <button onClick={() => setTab("build")} className="sidebar-btn-sm mt-1">
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
                            onClick={() => toggleAvatarCheck(avatar.id)}
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
                            <span
                              className={`absolute top-3 right-3 w-5 h-5 rounded-full border flex items-center justify-center ${
                                on
                                  ? "bg-[var(--accent-color)] border-[var(--accent-color)] text-[var(--panel-bg)]"
                                  : "theme-border bg-[var(--panel-bg)]"
                              }`}
                            >
                              {on ? <Check size={12} strokeWidth={3} /> : null}
                            </span>
                          </button>
                        );
                      })}
                    </div>
                  )}
                </div>
              </section>

              {/* Contas e progresso */}
              <section className="theme-surface rounded-xl border theme-border min-h-0 flex flex-col overflow-hidden">
                {showBatch && batch ? (
                  <BatchPanel
                    batch={batch}
                    cancelling={cancelling}
                    onCancel={() => void handleCancel()}
                    onDismiss={() => setDismissed(true)}
                    accountName={accountName}
                    avatarName={(id) => saved.find((a) => a.id === id)?.name ?? null}
                    headshot={(id) => store.avatarUrls.get(id)}
                  />
                ) : (
                  <>
                    <div className="px-3 py-2.5 border-b theme-border shrink-0">
                      <div className="text-[12.5px] font-semibold text-[var(--panel-fg)]">
                        {t("Selected accounts ({{count}})", { count: store.selectedAccounts.length })}
                      </div>
                    </div>
                    <div className="flex-1 min-h-0 overflow-y-auto p-2">
                      {store.selectedAccounts.length === 0 ? (
                        <div className="h-full flex flex-col items-center justify-center gap-2 text-center px-5">
                          <Users size={26} strokeWidth={1.25} className="theme-muted" />
                          <div className="text-[11.5px] theme-muted leading-4">
                            {t("Select accounts in the main list first — the avatars go to the accounts selected there.")}
                          </div>
                        </div>
                      ) : (
                        <div className="space-y-0.5">
                          {store.selectedAccounts.map((account) => (
                            <div key={account.UserID} className="flex items-center gap-2 px-1.5 py-1 rounded-md">
                              <Headshot url={store.avatarUrls.get(account.UserID)} />
                              <span className="text-[12px] text-[var(--panel-fg)] truncate">
                                {account.Alias || account.Username}
                              </span>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                    <div className="p-3 border-t theme-border shrink-0 space-y-2">
                      <button
                        onClick={() => void handleApply()}
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
                      {store.selectedAccounts.length > 0 && chosenIds.length === 0 ? (
                        <div className="text-[11px] text-amber-400/90 leading-4">
                          {t("Check at least one saved avatar.")}
                        </div>
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
          )}
        </div>
      </div>
    </div>
  );
}

/** Progresso do lote e, no fim, o resultado de cada conta. */
function BatchPanel({
  batch,
  cancelling,
  onCancel,
  onDismiss,
  accountName,
  avatarName,
  headshot,
}: {
  batch: AvatarBatchSnapshot;
  cancelling: boolean;
  onCancel: () => void;
  onDismiss: () => void;
  accountName: (userId: number) => string;
  avatarName: (avatarId: string) => string | null;
  headshot: (userId: number) => string | undefined;
}) {
  const t = useTr();
  const total = Math.max(batch.total, batch.done);
  const percent = total > 0 ? Math.round((batch.done / total) * 100) : 0;

  function chip(result: AvatarAccountResult): { text: string; tone: string } {
    if (result.status === "ok") {
      return { text: t("Applied"), tone: "border-emerald-500/30 bg-emerald-500/15 text-emerald-300" };
    }
    if (result.status === "skipped") {
      if (result.reason === "challenge") {
        return {
          text: t("Verification required — skipped"),
          tone: "border-amber-500/30 bg-amber-500/15 text-amber-300",
        };
      }
      if (result.reason === "cancelled") {
        return { text: t("Cancelled"), tone: "theme-border bg-[var(--panel-soft)] theme-muted" };
      }
      return { text: t("Skipped"), tone: "border-amber-500/30 bg-amber-500/15 text-amber-300" };
    }
    return { text: t("Failed"), tone: "border-red-500/30 bg-red-500/15 text-red-300" };
  }

  function reasonText(result: AvatarAccountResult): string | null {
    if (!result.reason || result.reason === "challenge" || result.reason === "cancelled") return null;
    if (result.reason === "nothing to wear") return t("None of the items could be claimed or worn");
    return result.reason;
  }

  const ok = batch.accounts.filter((a) => a.status === "ok").length;

  return (
    <>
      <div className="px-3 py-2.5 border-b theme-border shrink-0 space-y-2">
        <div className="flex items-center justify-between gap-2">
          <div className="text-[12.5px] font-semibold text-[var(--panel-fg)]">
            {batch.running ? t("Applying avatars") : t("Finished")}
          </div>
          <div className="text-[11px] theme-muted tabular-nums">
            {t("{{done}} of {{total}}", { done: batch.done, total })}
          </div>
        </div>
        <div
          role="progressbar"
          aria-label={t("Applying avatars")}
          aria-valuemin={0}
          aria-valuemax={total}
          aria-valuenow={batch.done}
          className="h-2 rounded-full bg-[var(--panel-soft)] overflow-hidden"
        >
          <div
            className={`h-full rounded-full transition-[width] duration-500 ${
              batch.running ? "bg-[var(--accent-color)]" : "bg-emerald-500"
            }`}
            style={{ width: `${percent}%` }}
          />
        </div>
        {batch.running ? (
          <div className="flex items-center gap-2 min-h-[24px]">
            {batch.currentUserId !== null ? (
              <>
                <Headshot url={headshot(batch.currentUserId)} size={22} />
                <span className="flex-1 min-w-0 text-[11.5px] text-[var(--panel-fg)] truncate">
                  {t("Now: {{name}}", { name: accountName(batch.currentUserId) })}
                </span>
              </>
            ) : (
              <span className="flex-1 text-[11.5px] theme-muted">{t("Starting...")}</span>
            )}
            <Loader2 size={14} className="animate-spin theme-accent shrink-0" />
          </div>
        ) : (
          <div className="text-[11.5px] theme-muted">
            {t("{{ok}} of {{total}} accounts got their avatar", { ok, total })}
          </div>
        )}
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto p-2 space-y-1">
        {batch.accounts.length === 0 ? (
          <div className="text-[11px] theme-muted px-1.5 py-2">{t("Results appear here as each account finishes.")}</div>
        ) : (
          batch.accounts.map((result, index) => {
            const status = chip(result);
            const reason = reasonText(result);
            const avatar = avatarName(result.avatarId);
            return (
              <div
                key={`${result.userId}-${index}`}
                className="rounded-lg border theme-border px-2 py-1.5 animate-fade-in-up"
              >
                <div className="flex items-center gap-2">
                  <Headshot url={headshot(result.userId)} size={24} />
                  <div className="flex-1 min-w-0">
                    <div className="text-[12px] text-[var(--panel-fg)] truncate">{accountName(result.userId)}</div>
                    {avatar ? <div className="text-[11px] theme-muted truncate">{avatar}</div> : null}
                  </div>
                  <span className={`shrink-0 px-1.5 py-0.5 rounded-full border text-[11px] ${status.tone}`}>
                    {status.text}
                  </span>
                </div>
                {result.claimed > 0 || result.missing > 0 || reason ? (
                  <div className="mt-1 pl-8 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px]">
                    {result.claimed > 0 ? (
                      <span className="text-emerald-400/90">{t("{{count}} claimed", { count: result.claimed })}</span>
                    ) : null}
                    {result.missing > 0 ? (
                      <span className="text-amber-400/90">{t("{{count}} missing", { count: result.missing })}</span>
                    ) : null}
                    {reason ? <span className="theme-muted break-words">{reason}</span> : null}
                  </div>
                ) : null}
              </div>
            );
          })
        )}
      </div>

      <div className="p-3 border-t theme-border shrink-0">
        {batch.running ? (
          <button
            onClick={onCancel}
            disabled={cancelling}
            aria-label={t("Cancel")}
            className="sidebar-btn-sm w-full text-red-300 border-red-400/40 hover:bg-red-500/15 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {cancelling ? t("Cancelling — finishing the current item...") : t("Cancel")}
          </button>
        ) : (
          <button onClick={onDismiss} className="sidebar-btn-sm w-full">
            {t("Clear results")}
          </button>
        )}
      </div>
    </>
  );
}
