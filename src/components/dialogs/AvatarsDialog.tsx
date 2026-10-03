import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Shirt, Users, X } from "lucide-react";
import { useStore } from "../../store";
import { useModalClose } from "../../hooks/useModalClose";
import { useConfirm } from "../../hooks/usePrompt";
import { useTr } from "../../i18n/text";
import {
  groupByCategory,
  newAvatarId,
  selectionItems,
  validateAvatarDraft,
  type FreeCatalogItem,
  type SavedAvatar,
} from "../../avatarBuilder";
import { BuildTab } from "./avatars/BuildTab";
import { DistributeTab } from "./avatars/DistributeTab";
import { IDLE_BATCH, type AvatarBatchSnapshot } from "./avatars/shared";
import { useAvatarDraft } from "./avatars/useAvatarDraft";
import { useAvatarThumbs } from "./avatars/useAvatarThumbs";

type Tab = "build" | "distribute";

/**
 * Avatares gratuitos: monta avatares só com itens oficiais grátis do Roblox,
 * salva, e distribui os salvos entre as contas marcadas.
 *
 * A lógica (categorias, sorteio, validação) mora em `avatarBuilder.ts`; as abas
 * em `avatars/`. Este arquivo é a casca: estado compartilhado e os comandos. O
 * lote roda no backend, uma conta por vez, e chega inteiro a cada passo pelo
 * evento `avatar-batch-state` — a tela pode fechar e reabrir no meio que retoma
 * de onde está (`get_avatar_batch_state`).
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

  const { thumbs, ensureThumbs } = useAvatarThumbs();
  const draft = useAvatarDraft();

  // ---- Avatares salvos ----------------------------------------------------
  const [saved, setSaved] = useState<SavedAvatar[]>([]);
  const [savedLoading, setSavedLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  /** Trava síncrona: dois Enter seguidos chegam antes de `saving` re-renderizar. */
  const savingRef = useRef(false);

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

  const receive = useCallback((snapshot: AvatarBatchSnapshot | null | undefined) => {
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
  }, []);

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

  // ---- Distribuição -------------------------------------------------------
  const [useAll, setUseAll] = useState(true);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const chosenIds = useMemo(
    () => (useAll ? saved.map((a) => a.id) : saved.filter((a) => checked.has(a.id)).map((a) => a.id)),
    [useAll, saved, checked]
  );
  /** Contas que recebem avatar: nasce da seleção da lista principal a cada abertura. */
  const [picked, setPicked] = useState<Set<number>>(new Set());

  useEffect(() => {
    if (!visible) return;
    setPicked(new Set(storeRef.current.selectedAccounts.map((a) => a.UserID)));
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
    if (visible && catalog) ensureThumbs(grouped[draft.activeCategory]);
  }, [visible, catalog, grouped, draft.activeCategory, ensureThumbs]);

  useEffect(() => {
    if (visible) ensureThumbs(selectionItems(draft.selection));
  }, [visible, draft.selection, ensureThumbs]);

  useEffect(() => {
    if (visible) ensureThumbs(saved.flatMap((a) => a.items.slice(0, 4)));
  }, [visible, saved, ensureThumbs]);

  // ---- Ações --------------------------------------------------------------
  async function handleSave() {
    if (savingRef.current) return;
    const problem = validateAvatarDraft(draft.name, draft.selection);
    if (problem) {
      draft.setError(problem);
      return;
    }
    draft.setError(null);
    savingRef.current = true;
    setSaving(true);
    try {
      const avatar: SavedAvatar = {
        id: draft.editingId ?? newAvatarId(),
        name: draft.name.trim(),
        items: selectionItems(draft.selection),
        skinColor: draft.skinColor,
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
      draft.setEditingId(stored.id);
      store.addToast(t("Avatar saved: {{name}}", { name: stored.name }));
    } catch (e) {
      store.addToast(t("Could not save the avatar: {{error}}", { error: String(e) }), "error");
    } finally {
      savingRef.current = false;
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
      if (draft.editingId === avatar.id) draft.setEditingId(null);
    } catch (e) {
      store.addToast(String(e), "error");
    }
  }

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
  // Na ordem da lista principal; conta marcada que sumiu da lista fica de fora.
  const pickedUserIds = store.accounts.filter((a) => picked.has(a.UserID)).map((a) => a.UserID);
  const canApply = !running && !starting && pickedUserIds.length > 0 && chosenIds.length > 0;

  async function handleApply() {
    if (!canApply) return;
    const userIds = pickedUserIds;
    setStarting(true);
    setDismissed(false);
    receive({ ...IDLE_BATCH, running: true, total: userIds.length });
    try {
      const final = await invoke<AvatarBatchSnapshot>("avatar_apply_batch", { userIds, avatarIds: chosenIds });
      if (mountedRef.current) receive(final);
    } catch (e) {
      store.addToast(String(e), "error");
      // Recusado (outro lote rodando, conta sumiu...): volta ao estado real do backend.
      try {
        const snapshot = await invoke<AvatarBatchSnapshot>("get_avatar_batch_state");
        receive(snapshot ?? IDLE_BATCH);
      } catch {
        receive(IDLE_BATCH);
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

  function accountName(userId: number): string {
    const account = store.accounts.find((a) => a.UserID === userId);
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
          {[
            { id: "build" as const, label: t("Build"), icon: <Shirt size={13} strokeWidth={1.75} /> },
            { id: "distribute" as const, label: t("Distribute"), icon: <Users size={13} strokeWidth={1.75} /> },
          ].map((item) => {
            const active = tab === item.id;
            return (
              <button
                key={item.id}
                role="tab"
                aria-selected={active}
                onClick={() => setTab(item.id)}
                className={`flex items-center gap-1.5 px-3 py-1.5 text-[12px] font-medium rounded-lg border transition-colors ${
                  active ? "theme-accent-border theme-accent-bg text-[var(--panel-fg)]" : "theme-border theme-muted theme-btn-ghost"
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
            <BuildTab
              catalog={catalog}
              catalogLoading={catalogLoading}
              catalogError={catalogError}
              onRetryCatalog={() => void loadCatalog()}
              grouped={grouped}
              thumbs={thumbs}
              draft={draft}
              saved={saved}
              savedLoading={savedLoading}
              saving={saving}
              onSave={() => void handleSave()}
              onDelete={(avatar) => void handleDelete(avatar)}
            />
          ) : (
            <DistributeTab
              saved={saved}
              savedLoading={savedLoading}
              thumbs={thumbs}
              useAll={useAll}
              chosenIds={chosenIds}
              onSetUseAll={setUseAllAvatars}
              onToggleAvatar={toggleAvatarCheck}
              onGoBuild={() => setTab("build")}
              accounts={store.accounts}
              avatarUrls={store.avatarUrls}
              picked={picked}
              onPickedChange={setPicked}
              batch={batch}
              showBatch={showBatch}
              starting={starting}
              cancelling={cancelling}
              canApply={canApply}
              onApply={() => void handleApply()}
              onCancel={() => void handleCancel()}
              onDismiss={() => setDismissed(true)}
              accountName={accountName}
            />
          )}
        </div>
      </div>
    </div>
  );
}
