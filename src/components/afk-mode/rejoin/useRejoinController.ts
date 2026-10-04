import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../../store";
import { useGameIdentity } from "../../../hooks/useGameIdentity";
import { useAccountInitial, useAccountLabel, useHideAccountAvatar } from "../../../hooks/useAccountLabel";
import { useConfirm } from "../../../hooks/usePrompt";
import { isMultiRobloxCloseProcessError } from "../../../utils/robloxErrors";
import { useTr } from "../../../i18n/text";
import { loadFavorites, type FavoriteGame } from "../../server-list/types";
import { useGameListsChanged } from "../../server-list/gameListsSync";
import { canRunBottingActionOnRow, type BottingRowAction } from "./rejoinShared";

export interface RejoinTabOptions {
  /**
   * Contas que chegam marcadas (o "Em jogo" do Painel de Sessão). Sem isto, a
   * seleção da lista principal — só as que têm cliente aberto aparecem.
   */
  targetUserIds?: number[];
  /**
   * Aberto pelo "Em jogo": o servidor é **sempre** onde as contas já estão,
   * mesmo que um jogo tenha vindo junto na abertura.
   */
  adoptRunning?: boolean;
  /**
   * Jogo escolhido na abertura (clique direito num jogo → "Auto Rejoin"): a
   * opção "um jogo escolhido" já abre marcada com ele.
   */
  initialPlaceId?: string | null;
}

/**
 * Para onde o ciclo rejoga:
 * - `current`: o jogo em que as contas marcadas estão agora (presença). O Start
 *   **adota** os clientes abertos, sem fechar ninguém; cada rejoin volta a esse
 *   jogo. É o padrão — ninguém quer ficar digitando Place ID nem Job ID.
 * - `game`: um jogo escolhido (Favoritos, um VIP salvo neles, ou o Place ID
 *   digitado no Avançado). O Start relança as contas marcadas nesse jogo.
 */
export type ServerMode = "current" | "game";

/** O jogo das contas marcadas, pela presença de cada uma. */
export type PlaceDetection = "idle" | "detecting" | "found" | "mixed" | "missing";

/**
 * Todo o estado e as ações da aba Auto Rejoin. A tela só desenha o que este
 * hook entrega.
 *
 * Não existe mais conta main na tela (pedido do dono, 03/10/2026): todo Start
 * manda `playerUserIds: []`. O backend continua aceitando mains — uma sessão
 * antiga que tenha uma segue funcionando, e a lista ao vivo mostra a fase dela.
 */
export function useRejoinController({
  targetUserIds,
  adoptRunning = false,
  initialPlaceId = null,
}: RejoinTabOptions) {
  const t = useTr();
  const store = useStore();
  const confirm = useConfirm();
  /** Nome na tela, com "Names hidden" aplicado — toda a aba usa este. */
  const accountLabel = useAccountLabel();
  const accountInitial = useAccountInitial();
  const hideAvatar = useHideAccountAvatar();

  const status = store.bottingStatus;
  const running = !!status?.active;
  const escolhidoNaAbertura = initialPlaceId?.trim() || "";

  // Quem chega marcado: as contas de quem abriu, ou a seleção da lista.
  const [draftUserIds, setDraftUserIds] = useState<number[]>(
    () => targetUserIds ?? store.selectedAccounts.map((a) => a.UserID)
  );
  const [serverMode, setServerMode] = useState<ServerMode>(
    adoptRunning || !escolhidoNaAbertura ? "current" : "game"
  );
  const [placeId, setPlaceId] = useState(escolhidoNaAbertura);
  const [jobId, setJobId] = useState("");
  const [favorites, setFavorites] = useState<FavoriteGame[]>(loadFavorites);
  // Só lê: favoritos mudados na Choose Game (ou restaurados de um backup) aparecem aqui.
  useGameListsChanged(() => setFavorites(loadFavorites()));
  const [intervalMinutes, setIntervalMinutes] = useState(19);
  const [launchDelaySeconds, setLaunchDelaySeconds] = useState(20);
  /** Só vai no Start: sem conta main na tela, a carência não tem campo. */
  const [playerGraceMinutes, setPlayerGraceMinutes] = useState(15);
  const [busy, setBusy] = useState(false);
  const [rowBusy, setRowBusy] = useState<number | null>(null);
  const [bulkBusy, setBulkBusy] = useState(false);
  const [bulkSelectedUserIds, setBulkSelectedUserIds] = useState<number[]>([]);
  const [bottingStartError, setBottingStartError] = useState<string | null>(null);
  const [closingRoblox, setClosingRoblox] = useState(false);
  const [nowMs, setNowMs] = useState(Date.now());
  /** O place de cada conta pela presença: número, `null` (não disse) ou ausente (buscando). */
  const [placeByUser, setPlaceByUser] = useState<Map<number, number | null>>(() => new Map());
  const asked = useRef(new Set<number>());
  const mountedRef = useRef(true);
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // Tempo do ciclo e último jogo escolhido vêm do INI. O jogo da abertura
  // vence o rascunho: quem acabou de escolher o jogo quer aquele jogo.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      let general = store.settings?.General || {};
      try {
        const fresh = await invoke<Record<string, Record<string, string>>>("get_all_settings");
        if (fresh?.General) general = fresh.General;
      } catch {}
      if (cancelled) return;
      const draftInterval = parseInt(general.BottingDefaultIntervalMinutes || "19", 10);
      const draftDelay = parseInt(general.BottingLaunchDelaySeconds || "20", 10);
      const draftGrace = parseInt(general.BottingPlayerGraceMinutes || "15", 10);
      setIntervalMinutes(Number.isFinite(draftInterval) ? draftInterval : 19);
      setLaunchDelaySeconds(Number.isFinite(draftDelay) ? draftDelay : 20);
      setPlayerGraceMinutes(Number.isFinite(draftGrace) ? draftGrace : 15);
      if (!escolhidoNaAbertura) {
        setPlaceId(general.BottingDraftPlaceId || "");
        setJobId(general.BottingDraftJobId || "");
      }
    })();
    return () => {
      cancelled = true;
    };
    // Lido uma vez por abertura (o modal remonta a cada abertura).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const timer = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const sessionIds = useMemo(
    () => (running ? status?.userIds ?? [] : []),
    [running, status]
  );
  const sessionSet = useMemo(() => new Set(sessionIds), [sessionIds]);

  // Quem está no ciclo fica marcado: depois do Stop, religar leva as mesmas
  // contas (como nos cliques AFK). Sem isto sobrava marcada só a última conta
  // acrescentada com o ciclo ligado.
  const sessionKey = sessionIds.join(",");
  useEffect(() => {
    if (sessionIds.length === 0) return;
    setDraftUserIds((prev) => {
      const missing = sessionIds.filter((id) => !prev.includes(id));
      return missing.length === 0 ? prev : [...prev, ...missing];
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionKey]);

  /**
   * Quem aparece para marcar: as contas com cliente aberto (o tracker, que
   * inclui os clientes abertos pelo site e reconhecidos), mais as que já estão
   * no ciclo — senão uma conta cujo cliente caiu sumia da tela. O mesmo critério
   * dos cliques AFK.
   */
  const candidates = useMemo(() => {
    const ids = new Set<number>([...store.launchedByProgram, ...sessionIds]);
    return store.accounts.filter((a) => ids.has(a.UserID));
  }, [store.accounts, store.launchedByProgram, sessionIds]);
  const candidateSet = useMemo(() => new Set(candidates.map((a) => a.UserID)), [candidates]);
  /** As contas marcadas que estão na lista, na ordem da lista. */
  const picked = useMemo(
    () => candidates.map((a) => a.UserID).filter((id) => draftUserIds.includes(id)),
    [candidates, draftUserIds]
  );
  const pickedKey = picked.join(",");

  // Presença de cada conta marcada, uma vez por conta. Só serve à opção
  // "onde estão agora", e só com o ciclo parado.
  useEffect(() => {
    if (running || serverMode !== "current") return;
    for (const id of picked) {
      if (asked.current.has(id)) continue;
      asked.current.add(id);
      store
        .detectRunningGamePlace([id])
        .catch(() => null)
        .then((found) => {
          if (!mountedRef.current) return;
          setPlaceByUser((prev) => new Map(prev).set(id, found && found > 0 ? found : null));
        });
    }
    // `store` muda de identidade a cada render; o que importa é quem está marcado.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pickedKey, serverMode, running]);

  const { detection, detectedPlaceId } = useMemo((): {
    detection: PlaceDetection;
    detectedPlaceId: number | null;
  } => {
    if (picked.length === 0) return { detection: "idle", detectedPlaceId: null };
    if (picked.some((id) => !placeByUser.has(id))) return { detection: "detecting", detectedPlaceId: null };
    const places = new Set(
      picked.map((id) => placeByUser.get(id)).filter((p): p is number => typeof p === "number")
    );
    if (places.size === 0) return { detection: "missing", detectedPlaceId: null };
    if (places.size > 1) return { detection: "mixed", detectedPlaceId: null };
    return { detection: "found", detectedPlaceId: [...places][0] };
  }, [picked, placeByUser]);

  const typedPlaceId = parseInt(placeId.trim(), 10);
  const chosenPlaceId = Number.isFinite(typedPlaceId) && typedPlaceId > 0 ? typedPlaceId : null;
  /** O place que o Start vai usar, conforme a opção marcada. */
  const effectivePlaceId = serverMode === "current" ? detectedPlaceId : chosenPlaceId;
  const game = useGameIdentity(effectivePlaceId ?? "", picked[0] ?? null);
  /** O jogo da sessão que está rodando. */
  const runningGame = useGameIdentity(
    running && status?.placeId ? String(status.placeId) : "",
    status?.userIds?.[0] ?? null
  );

  const accountById = useMemo(
    () => new Map(store.accounts.map((a) => [a.UserID, a])),
    [store.accounts]
  );

  function toggleAccount(userId: number) {
    setDraftUserIds((prev) =>
      prev.includes(userId) ? prev.filter((id) => id !== userId) : [...prev, userId]
    );
  }

  /** Marca todas as contas que dá para marcar agora (fora do ciclo, se ele roda). */
  function selectAllAccounts() {
    setDraftUserIds((prev) => [
      ...new Set([...prev, ...candidates.map((a) => a.UserID).filter((id) => !sessionSet.has(id))]),
    ]);
  }

  function clearAccounts() {
    setDraftUserIds([]);
  }

  function chooseServerMode(next: ServerMode) {
    setServerMode(next);
    // Os favoritos podem ter mudado na Choose Game com a tela aberta.
    if (next === "game") setFavorites(loadFavorites());
  }

  /** Um favorito (sem `link`) ou um servidor VIP salvo nele. */
  function pickGame(nextPlaceId: number, link = "") {
    setServerMode("game");
    setPlaceId(String(nextPlaceId));
    setJobId(link);
  }

  function updatePlaceId(next: string) {
    setServerMode("game");
    setPlaceId(next);
  }

  function updateJobId(next: string) {
    setServerMode("game");
    setJobId(next);
  }

  /** Grava o tempo do ciclo e, com um jogo escolhido, o jogo. */
  async function saveDraft(overrides: Partial<{ interval: number; delay: number; withGame: boolean }> = {}) {
    const entries: [string, string][] = [
      ["BottingDefaultIntervalMinutes", String(overrides.interval ?? intervalMinutes)],
      ["BottingLaunchDelaySeconds", String(overrides.delay ?? launchDelaySeconds)],
    ];
    if (overrides.withGame) {
      entries.push(["BottingDraftPlaceId", placeId.trim()], ["BottingDraftJobId", jobId.trim()]);
    }
    await Promise.all(
      entries.map(([key, value]) =>
        invoke("update_setting", { section: "General", key, value }).catch(() => {})
      )
    );
  }

  const multiRbxEnabled = store.settings?.General?.EnableMultiRbx === "true";
  /** Com o ciclo rodando: quem está marcado e ainda não está nele. */
  const missingFromSession = running ? picked.filter((id) => !sessionSet.has(id)) : [];

  const uiActionLocked = busy || bulkBusy || rowBusy !== null;
  const actionButtonsLocked = uiActionLocked;

  /** Por que o Start não liga — a primeira coisa que falta, na barra de estado. */
  const startBlocker: string | null = !multiRbxEnabled
    ? t("Auto Rejoin currently requires Multi Roblox to be enabled")
    : candidates.length === 0
      ? t("No Roblox client is open yet. Open the accounts first.")
      : picked.length < 2
        ? t("Select at least 2 accounts")
        : serverMode === "game"
          ? chosenPlaceId === null
            ? t("Pick a game")
            : null
          : detection === "detecting"
            ? t("Finding the game these accounts are in...")
            : detection === "mixed"
              ? t("The selected accounts are in different games. Tick accounts from one game, or pick a game.")
              : detection === "missing"
                ? t("Could not tell which game these accounts are in. Pick a game.")
                : null;
  const canStart = !running && startBlocker === null && !uiActionLocked;

  async function handleStart() {
    if (!canStart || effectivePlaceId === null) return;
    setBottingStartError(null);
    setBusy(true);
    try {
      if (serverMode === "current") {
        await saveDraft();
        // Nada fecha nem relança: as contas ficam no servidor em que estão até
        // o primeiro rejoin delas, e cada rejoin volta ao mesmo jogo.
        await store.adoptRunningIntoBotting(picked, {
          placeId: effectivePlaceId,
          intervalMinutes,
          launchDelaySeconds,
          playerGraceMinutes,
          playerUserIds: [],
        });
      } else {
        await saveDraft({ withGame: true });
        await store.startBottingMode({
          userIds: picked,
          placeId: effectivePlaceId,
          jobId: jobId.trim(),
          launchData: "",
          playerUserIds: [],
          intervalMinutes,
          launchDelaySeconds,
          playerGraceMinutes,
        });
      }
    } catch (e) {
      setBottingStartError(String(e));
      store.addToast(t("Auto Rejoin start failed: {{error}}", { error: String(e) }));
    } finally {
      if (mountedRef.current) setBusy(false);
    }
  }

  /** Com o ciclo ligado: só entra nele, sem relançar ninguém. */
  async function handleAddToSession() {
    if (missingFromSession.length === 0) return;
    setBottingStartError(null);
    setBusy(true);
    try {
      await store.adoptRunningIntoBotting(missingFromSession);
    } catch (e) {
      setBottingStartError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /**
   * A confirmação não segue o lote cegamente: o que decide é o que a ação
   * custa. `closeDisconnect` tira a conta do ciclo até alguém reconectar
   * (`botting_action_flags`, no Rust), e isso não se desfaz sozinho — pergunta.
   * `close` sozinho é transitório (o loop reabre o cliente no próximo
   * restart), por isso segue sem pergunta. O lote pergunta nos dois porque age
   * sobre uma seleção que a pessoa pode ter esquecido que fez.
   */
  async function runRowAction(userId: number, action: BottingRowAction) {
    if (actionButtonsLocked) return;
    if (action === "closeDisconnect") {
      const warning = bulkCloseWarning(action, 1);
      if (warning && !(await confirm(warning, true))) return;
    }
    setRowBusy(userId);
    try {
      await store.bottingAccountAction(userId, action);
    } catch (e) {
      store.addToast(t("Auto Rejoin account action failed: {{error}}", { error: String(e) }));
    } finally {
      setRowBusy(null);
    }
  }

  async function runRowFocus(userId: number) {
    if (actionButtonsLocked) return;
    setRowBusy(userId);
    try {
      const focused = await store.focusRobloxClient(userId);
      if (!focused) store.addToast(t("No active Roblox window found for this account"));
    } catch (e) {
      store.addToast(t("Failed to focus client: {{error}}", { error: String(e) }));
    } finally {
      setRowBusy(null);
    }
  }

  async function handleCloseRobloxBannerAction() {
    if (closingRoblox) return;
    setClosingRoblox(true);
    try {
      await store.killAllRobloxProcesses();
      setBottingStartError(null);
    } finally {
      setClosingRoblox(false);
    }
  }

  function dismissError() {
    setBottingStartError(null);
    store.setError(null);
  }

  const statusMap = new Map((status?.accounts || []).map((a) => [a.userId, a]));
  const liveRows = sessionIds.map((userId) => ({
    userId,
    account: accountById.get(userId) || null,
    row: statusMap.get(userId) || null,
  }));
  const dialogError = bottingStartError || store.error || null;
  const rowConflictError =
    (status?.accounts || []).map((a) => a.lastError).find((msg) => isMultiRobloxCloseProcessError(msg)) || null;
  const closeRobloxAlertMessage = isMultiRobloxCloseProcessError(dialogError) ? dialogError : rowConflictError;
  const showCloseRobloxAction = !!closeRobloxAlertMessage;
  // Quantos clientes o "Stop + Close" fecha: `stop_botting_mode` fecha
  // `user_ids` menos as mains — numa sessão nova, todas as contas dela.
  const closableCount = liveRows.filter(({ row }) => !row?.isPlayer).length;
  const disconnectedCount = liveRows.filter(({ row }) => !!row?.disconnected).length;
  const retryingCount = liveRows.filter(({ row }) => (row?.retryCount || 0) >= 2).length;
  /** O próximo rejoin agendado da sessão (contas conectadas). */
  const nextRejoinAtMs = (running ? status?.accounts ?? [] : [])
    .filter((a) => !a.disconnected && !a.isPlayer && typeof a.nextRestartAtMs === "number")
    .reduce<number | null>(
      (min, a) => (min === null || (a.nextRestartAtMs as number) < min ? (a.nextRestartAtMs as number) : min),
      null
    );
  const bulkSelectedSet = useMemo(() => new Set(bulkSelectedUserIds), [bulkSelectedUserIds]);
  const liveRowsByUserId = useMemo(
    () => new Map(liveRows.map(({ userId, row }) => [userId, row])),
    [liveRows]
  );
  const bulkEligibleCounts = useMemo<Record<BottingRowAction, number>>(() => {
    const counts: Record<BottingRowAction, number> = {
      disconnect: 0,
      close: 0,
      closeDisconnect: 0,
      restartClient: 0,
      restartLoop: 0,
    };
    if (!running) return counts;
    for (const userId of bulkSelectedUserIds) {
      const row = liveRowsByUserId.get(userId) || null;
      for (const action of Object.keys(counts) as BottingRowAction[]) {
        if (canRunBottingActionOnRow(row, action)) counts[action] += 1;
      }
    }
    return counts;
  }, [bulkSelectedUserIds, liveRowsByUserId, running]);

  function toggleBulkSelected(userId: number) {
    setBulkSelectedUserIds((prev) =>
      prev.includes(userId) ? prev.filter((id) => id !== userId) : [...prev, userId]
    );
  }

  function setBulkSelected(userIds: number[]) {
    setBulkSelectedUserIds(Array.from(new Set(userIds)));
  }

  /**
   * A frase de confirmação de um lote que fecha clientes, ou `null` quando a
   * ação não fecha nada (Disconnect) ou reabre em seguida (Restart).
   */
  function bulkCloseWarning(action: BottingRowAction, count: number): string | null {
    if (action === "close") {
      return count === 1
        ? t(
            "Close the Roblox client of 1 account? It leaves the server now, and Auto Rejoin brings it back at its next rejoin."
          )
        : t(
            "Close the Roblox clients of {{count}} accounts? They leave the server now, and Auto Rejoin brings each one back at its next rejoin.",
            { count }
          );
    }
    if (action === "closeDisconnect") {
      return count === 1
        ? t(
            "Close the Roblox client of 1 account and take it out of the rejoin cycle? It stays out until you reconnect it."
          )
        : t(
            "Close the Roblox clients of {{count}} accounts and take them out of the rejoin cycle? They stay out until you reconnect them.",
            { count }
          );
    }
    return null;
  }

  async function runBulkAction(action: BottingRowAction) {
    if (!running || actionButtonsLocked) return;
    const eligibleIds = bulkSelectedUserIds.filter((userId) =>
      canRunBottingActionOnRow(liveRowsByUserId.get(userId) || null, action)
    );
    if (eligibleIds.length === 0) {
      store.addToast(t("No selected accounts can run this action"));
      return;
    }

    // Fechar cliente é irreversível para quem está na partida: a frase diz
    // quantos fecham e o que acontece depois. Restart/Disconnect não fecham
    // nada que a pessoa não recupere no próprio ciclo, então não perguntam.
    const closeWarning = bulkCloseWarning(action, eligibleIds.length);
    if (closeWarning && !(await confirm(closeWarning, true))) return;

    setBulkBusy(true);
    let succeeded = 0;
    let failed = 0;
    try {
      for (const userId of eligibleIds) {
        setRowBusy(userId);
        try {
          await store.bottingAccountAction(userId, action);
          succeeded += 1;
        } catch {
          failed += 1;
        }
      }
    } finally {
      setRowBusy(null);
      setBulkBusy(false);
    }

    const actionLabel =
      action === "disconnect"
        ? t("Disconnect")
        : action === "close"
          ? t("Close client")
          : action === "restartClient"
            ? t("Restart client")
            : action === "closeDisconnect"
              ? t("Close + Disconnect")
              : t("Restart loop");

    store.addToast(
      failed === 0
        ? t("Applied {{action}} to {{count}} accounts", { action: actionLabel, count: succeeded })
        : t("Applied {{action}}: {{success}} succeeded, {{failed}} failed", {
            action: actionLabel,
            success: succeeded,
            failed,
          })
    );
  }

  function handleStop() {
    void store.stopBottingMode(false);
  }

  /** Parar fechando os clientes da sessão: a frase diz quantos fecham. */
  async function handleStopAndClose() {
    const ok = await confirm(
      closableCount === 1
        ? t(
            "Stop Auto Rejoin and close the Roblox client of 1 account in this session? Clients of accounts outside this session are left alone."
          )
        : t(
            "Stop Auto Rejoin and close the Roblox clients of {{count}} accounts in this session? Clients of accounts outside this session are left alone.",
            { count: closableCount }
          ),
      true
    );
    if (!ok) return;
    try {
      await store.stopBottingMode(true);
    } catch {
      // `stopBottingMode` já publica o erro no store.
    }
  }

  useEffect(() => {
    if (!showCloseRobloxAction) return;
    const node = contentRef.current;
    if (!node || typeof node.scrollTo !== "function") return;
    node.scrollTo({ top: 0, behavior: "smooth" });
  }, [showCloseRobloxAction, closeRobloxAlertMessage]);

  // A seleção em lote só guarda quem ainda está na sessão.
  useEffect(() => {
    setBulkSelectedUserIds((prev) => {
      const next = prev.filter((id) => sessionSet.has(id));
      return next.length === prev.length ? prev : next;
    });
  }, [sessionSet]);

  return {
    t,
    store,
    accountLabel,
    accountInitial,
    hideAvatar,
    status,
    running,
    candidates,
    candidateSet,
    picked,
    sessionSet,
    accountById,
    serverMode,
    favorites,
    placeId,
    jobId,
    chosenPlaceId,
    detection,
    detectedPlaceId,
    effectivePlaceId,
    game,
    runningGame,
    intervalMinutes,
    launchDelaySeconds,
    nowMs,
    contentRef,
    canStart,
    startBlocker,
    startError: bottingStartError,
    closeRobloxAlertMessage,
    showCloseRobloxAction,
    closingRoblox,
    liveRows,
    disconnectedCount,
    retryingCount,
    nextRejoinAtMs,
    missingFromSession,
    actionButtonsLocked,
    rowBusy,
    bulkSelectedUserIds,
    bulkSelectedSet,
    bulkEligibleCounts,
    toggleAccount,
    selectAllAccounts,
    clearAccounts,
    chooseServerMode,
    pickGame,
    updatePlaceId,
    updateJobId,
    setIntervalMinutes,
    setLaunchDelaySeconds,
    saveDraft,
    handleStart,
    handleAddToSession,
    handleStop,
    handleStopAndClose,
    handleCloseRobloxBannerAction,
    dismissError,
    runRowAction,
    runRowFocus,
    runBulkAction,
    toggleBulkSelected,
    setBulkSelected,
  };
}

export type RejoinController = ReturnType<typeof useRejoinController>;
