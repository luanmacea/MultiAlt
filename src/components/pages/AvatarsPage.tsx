import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Shirt, Users } from "lucide-react";
import { useStore } from "../../store";
import { PageShell } from "./PageShell";
import { useConfirm } from "../../hooks/usePrompt";
import { useAccountLabel, useHideAccountAvatar } from "../../hooks/useAccountLabel";
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
/** Mapa vazio estável: as fotos das contas quando os nomes estão ocultos. */
const NO_AVATARS: Map<number, string> = new Map();

export function AvatarsPage({ active, onLeave }: { active: boolean; onLeave: () => void }) {
  const t = useTr();
  const store = useStore();
  const accountLabel = useAccountLabel();
  const hideAvatars = useHideAccountAvatar();
  const confirm = useConfirm();
  // A página fica montada o tempo todo (o ouvinte do lote precisa disso);
  // `active` é o antigo "aberto".
  const visible = active;

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

  const receive = useCallback((raw: AvatarBatchSnapshot | null | undefined) => {
    if (!raw || typeof raw !== "object") return;
    // Estado incompleto (sem a lista de contas) vira lista vazia em vez de
    // derrubar a tela no primeiro `.length`.
    const snapshot: AvatarBatchSnapshot = { ...raw, accounts: Array.isArray(raw.accounts) ? raw.accounts : [] };
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
      // As recusas do backend vêm em inglês, que é a chave do catálogo.
      store.addToast(t(String(e)), "error");
      // Recusado (outro lote rodando, conta sumiu...): volta ao estado real do backend.
      // O "rodando" acima foi otimista: sem desfazê-lo, o retrato do lote anterior
      // (já terminado) pareceria um fim agora e repetiria o aviso e a troca de fotos.
      prevRunningRef.current = false;
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

  /** Nome da conta no progresso do lote, com "Names hidden" aplicado. */
  function accountName(userId: number): string {
    const account = store.accounts.find((a) => a.UserID === userId);
    return accountLabel(account, `${t("User ID")}: ${userId}`);
  }

  if (!visible) return null;

  const showBatch = batch !== null && (batch.running || batch.accounts.length > 0) && !dismissed;

  const tabs = (
    <div role="tablist" aria-label={t("Avatars")} className="flex items-center gap-1 -mb-px">
      {[
        { id: "build" as const, label: t("Build"), icon: <Shirt size={14} strokeWidth={1.75} /> },
        { id: "distribute" as const, label: t("Distribute"), icon: <Users size={14} strokeWidth={1.75} /> },
      ].map((item) => {
        const selected = tab === item.id;
        return (
          <button
            key={item.id}
            role="tab"
            aria-selected={selected}
            data-tour={`avatars-tab-${item.id}`}
            onClick={() => setTab(item.id)}
            className={`flex items-center gap-1.5 px-3 py-2.5 text-[12.5px] font-medium border-b-2 transition-colors outline-none focus-visible:shadow-[inset_0_0_0_2px_var(--input-focus)] ${
              selected
                ? "border-[var(--accent-color)] text-[var(--panel-fg)]"
                : "border-transparent text-[var(--panel-muted)] hover:text-[var(--panel-fg)]"
            }`}
          >
            <span className={selected ? "theme-accent" : undefined}>{item.icon}</span>
            {item.label}
            {item.id === "distribute" && saved.length > 0 ? (
              <span
                aria-hidden="true"
                className="ml-0.5 px-1.5 rounded-full text-[11px] tabular-nums bg-[var(--panel-soft)] text-[var(--panel-muted)]"
              >
                {saved.length}
              </span>
            ) : null}
          </button>
        );
      })}
    </div>
  );

  return (
    <PageShell
      title={t("Avatars")}
      description={t("Free official Roblox items only — never spends Robux")}
      onLeave={onLeave}
      dataTour="avatars-page"
      tour="avatars"
      toolbar={tabs}
      bodyClassName="p-5"
      actions={
        running ? (
          <span className="px-2.5 py-1 rounded-full text-[11.5px] border border-emerald-500/30 bg-emerald-500/15 text-emerald-300 animate-pulse">
            {t("Applying {{done}}/{{total}}", { done: batch?.done ?? 0, total: batch?.total ?? 0 })}
          </span>
        ) : null
      }
    >
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
          // Com os nomes ocultos as fotos das contas somem junto (a menos que a
          // opção de manter avatares esteja ligada) — igual à lista de contas.
          avatarUrls={hideAvatars ? NO_AVATARS : store.avatarUrls}
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
    </PageShell>
  );
}
