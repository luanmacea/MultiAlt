import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../../../store";
import type { Account } from "../../../types";
import { useGameIdentity } from "../../../hooks/useGameIdentity";
import { useConfirm } from "../../../hooks/usePrompt";
import { isMultiRobloxCloseProcessError } from "../../../utils/robloxErrors";
import { useTr } from "../../../i18n/text";
import { canRunBottingActionOnRow, type BottingRowAction } from "./rejoinShared";

export interface RejoinTabOptions {
  /**
   * Contas que o Auto Rejoin vai usar. Sem isto, são as selecionadas na lista
   * principal (o comportamento de sempre).
   */
  targetUserIds?: number[];
  /**
   * As contas já estão em jogo (aberto pelo "Em jogo" do Painel de Sessão): o
   * Start **adota** os clientes abertos, sem fechar nem relançar ninguém.
   */
  adoptRunning?: boolean;
  /**
   * Jogo escolhido na abertura (clique direito num jogo → "Auto Rejoin").
   * **Vence o rascunho salvo**: quem acabou de escolher o jogo quer aquele
   * jogo, não o place da vez passada.
   */
  initialPlaceId?: string | null;
}

/** Busca do jogo em que as contas adotadas estão jogando agora. */
export type PlaceDetection = "idle" | "detecting" | "found" | "missing";

/**
 * Todo o estado e as ações da aba Auto Rejoin. A tela (split ou classic,
 * modal ou página) só desenha o que este hook entrega — antes era um diálogo de
 * 2 mil linhas com cada bloco escrito duas vezes, uma por visão.
 */
export function useRejoinController({
  targetUserIds,
  adoptRunning = false,
  initialPlaceId = null,
}: RejoinTabOptions) {
  const t = useTr();
  const store = useStore();
  const confirm = useConfirm();

  // A lista vem de fora como array novo a cada render; a chave em texto é o que
  // os efeitos podem acompanhar sem rodar em loop.
  const status = store.bottingStatus;
  const selectedAccounts = store.selectedAccounts;
  // Aberto sem contas e sem seleção com o ciclo rodando: os alvos são as contas
  // do ciclo — senão a tela dizia "nenhuma conta" ao lado da lista ao vivo, e o
  // menu de mains ficava vazio.
  const sessionFallback =
    !targetUserIds && selectedAccounts.length === 0 && status?.active && status.userIds.length > 0
      ? status.userIds
      : null;
  const targetKey = targetUserIds
    ? targetUserIds.join(",")
    : sessionFallback
      ? sessionFallback.join(",")
      : null;
  const targetIds = useMemo(
    () =>
      targetKey !== null
        ? targetKey
            .split(",")
            .map((s) => parseInt(s, 10))
            .filter((n) => Number.isFinite(n) && n > 0)
        : selectedAccounts.map((a) => a.UserID),
    [targetKey, selectedAccounts]
  );
  /** Adoção só existe com contas vindas de quem abriu (as que estão em jogo). */
  const adopt = adoptRunning && !!targetUserIds;

  const accountById = useMemo(
    () => new Map(store.accounts.map((a) => [a.UserID, a])),
    [store.accounts]
  );
  const targetAccounts = useMemo(
    (): Pick<Account, "UserID" | "Username" | "Alias">[] =>
      targetKey === null
        ? selectedAccounts
        : targetIds.map(
            (id) => accountById.get(id) ?? { UserID: id, Username: `${t("User ID")}: ${id}`, Alias: "" }
          ),
    // `t` muda de identidade a cada render; o idioma não muda com a tela aberta.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [targetKey, targetIds, selectedAccounts, accountById]
  );
  const [placeId, setPlaceId] = useState("");
  /** Que jogo é o place do ciclo — o número sozinho não diz nada. */
  const game = useGameIdentity(placeId, targetIds[0] ?? null);
  /** O jogo da sessão que está rodando (pode ser outro que o do campo). */
  const runningGame = useGameIdentity(
    status?.active && status.placeId ? String(status.placeId) : "",
    status?.userIds?.[0] ?? null
  );
  const [jobId, setJobId] = useState("");
  const [launchData, setLaunchData] = useState("");
  const [shareLaunchFields, setShareLaunchFields] = useState(false);
  const [intervalMinutes, setIntervalMinutes] = useState(19);
  const [launchDelaySeconds, setLaunchDelaySeconds] = useState(20);
  const [playerGraceMinutes, setPlayerGraceMinutes] = useState(15);
  const [playerUserIds, setPlayerUserIds] = useState<number[]>([]);
  const [busy, setBusy] = useState(false);
  const [rowBusy, setRowBusy] = useState<number | null>(null);
  const [bulkBusy, setBulkBusy] = useState(false);
  const [bulkSelectedUserIds, setBulkSelectedUserIds] = useState<number[]>([]);
  const [bottingStartError, setBottingStartError] = useState<string | null>(null);
  const [closingRoblox, setClosingRoblox] = useState(false);
  const [nowMs, setNowMs] = useState(Date.now());
  const [playerMenuOpen, setPlayerMenuOpen] = useState(false);
  const [bottingLayout, setBottingLayout] = useState<"split" | "classic">("split");
  const [detection, setDetection] = useState<{ state: PlaceDetection; placeId: string | null }>({
    state: "idle",
    placeId: null,
  });
  const playerMenuRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);

  // Adoção: o place vem de onde as contas ESTÃO (presença), não do rascunho —
  // com o place errado, o primeiro reinício do ciclo as jogaria em outro jogo.
  // A tela mostra o que achou antes do Start, e a pessoa pode corrigir.
  useEffect(() => {
    if (!adopt || initialPlaceId?.trim() || targetIds.length === 0) return;
    let cancelled = false;
    setDetection({ state: "detecting", placeId: null });
    store
      .detectRunningGamePlace(targetIds)
      .then((found) => {
        if (cancelled) return;
        setDetection(
          found ? { state: "found", placeId: String(found) } : { state: "missing", placeId: null }
        );
      })
      .catch(() => {
        if (!cancelled) setDetection({ state: "missing", placeId: null });
      });
    return () => {
      cancelled = true;
    };
    // `store` muda de identidade a cada render; a busca é por abertura.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [adopt, initialPlaceId, targetKey]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      let general = store.settings?.General || {};
      try {
        const fresh = await invoke<Record<string, Record<string, string>>>("get_all_settings");
        if (fresh?.General) {
          general = fresh.General;
        }
      } catch {}
      if (cancelled) return;

      const shouldShareLaunchFields = !adopt && general.BottingAutoShareLaunchFields === "true";
      const escolhidoNaAbertura = initialPlaceId?.trim() || "";
      const draftPlace = adopt
        ? // Adotando, nunca o rascunho: só o jogo de quem abriu ou o detectado.
          escolhidoNaAbertura || detection.placeId || ""
        : escolhidoNaAbertura ||
          (shouldShareLaunchFields
            ? store.placeId || ""
            : general.BottingDraftPlaceId || store.placeId || "");
      const draftJob = adopt
        ? ""
        : shouldShareLaunchFields
          ? store.jobId || ""
          : general.BottingDraftJobId || store.jobId || "";
      const draftData = adopt
        ? ""
        : shouldShareLaunchFields
          ? store.launchData || ""
          : general.BottingDraftLaunchData || store.launchData || "";
      const draftInterval = parseInt(general.BottingDefaultIntervalMinutes || "19", 10);
      const draftDelay = parseInt(general.BottingLaunchDelaySeconds || "20", 10);
      const draftGrace = parseInt(
        general.BottingPlayerGraceMinutes || String(status?.playerGraceMinutes ?? 15),
        10
      );
      const draftPlayerIdsRaw =
        general.BottingDraftPlayerAccountIds || general.BottingDraftPlayerAccountId || "";
      const draftPlayerIds = draftPlayerIdsRaw
        .split(",")
        .map((s) => parseInt(s.trim(), 10))
        .filter((n) => Number.isFinite(n));
      const dualPanelEnabled = general.BottingDualPanelDialog !== "false";

      setShareLaunchFields(shouldShareLaunchFields);
      setPlaceId(draftPlace);
      setJobId(draftJob);
      setLaunchData(draftData);
      setIntervalMinutes(Number.isFinite(draftInterval) ? draftInterval : 19);
      setLaunchDelaySeconds(Number.isFinite(draftDelay) ? draftDelay : 20);
      setPlayerGraceMinutes(Number.isFinite(draftGrace) ? draftGrace : 15);
      setPlayerUserIds(draftPlayerIds.filter((id) => targetIds.includes(id)));
      setBottingLayout(dualPanelEnabled ? "split" : "classic");
    })();

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    adopt,
    detection.placeId,
    initialPlaceId,
    targetIds,
    status?.playerGraceMinutes,
    store.jobId,
    store.launchData,
    store.placeId,
    store.settings,
  ]);

  useEffect(() => {
    const timer = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (!status?.active) return;
    if (typeof status.playerGraceMinutes === "number" && status.playerGraceMinutes > 0) {
      setPlayerGraceMinutes(status.playerGraceMinutes);
    }
  }, [status?.active, status?.playerGraceMinutes]);

  useEffect(() => {
    if (!playerMenuOpen) return;
    function onMouseDown(e: MouseEvent) {
      if (!playerMenuRef.current) return;
      if (!playerMenuRef.current.contains(e.target as Node)) {
        setPlayerMenuOpen(false);
      }
    }
    document.addEventListener("mousedown", onMouseDown);
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
    };
  }, [playerMenuOpen]);

  /**
   * Grava o rascunho. Adotando, só o tempo e as mains: o place é o do jogo em
   * que as contas estão, e Job/JoinData ficam de fora do ciclo — gravá-los
   * apagaria o rascunho de quem usa o Auto Rejoin pela lista.
   */
  async function saveDraft(
    nextPlaceId: string,
    nextJobId: string,
    nextLaunchData: string,
    nextPlayers: number[],
    nextInterval: number,
    nextDelay: number,
    nextGraceMinutes: number
  ) {
    const entries: [string, string][] = [
      ["BottingDraftPlayerAccountIds", nextPlayers.join(",")],
      ["BottingDefaultIntervalMinutes", String(nextInterval)],
      ["BottingLaunchDelaySeconds", String(nextDelay)],
      ["BottingPlayerGraceMinutes", String(nextGraceMinutes)],
    ];
    if (!adopt) {
      entries.unshift(
        ["BottingDraftPlaceId", nextPlaceId],
        ["BottingDraftJobId", nextJobId],
        ["BottingDraftLaunchData", nextLaunchData]
      );
      entries.splice(4, 0, ["BottingDraftSelectedUserIds", targetIds.join(",")]);
    }
    await Promise.all(
      entries.map(([key, value]) =>
        invoke("update_setting", { section: "General", key, value }).catch(() => {})
      )
    );
  }

  /** `saveDraft` com o que está na tela, trocando só o que acabou de mudar. */
  function saveCurrentDraft(
    overrides: Partial<{
      players: number[];
      interval: number;
      delay: number;
      grace: number;
    }> = {}
  ) {
    return saveDraft(
      placeId.trim(),
      jobId.trim(),
      launchData,
      overrides.players ?? playerUserIds,
      overrides.interval ?? intervalMinutes,
      overrides.delay ?? launchDelaySeconds,
      overrides.grace ?? playerGraceMinutes
    );
  }

  const multiRbxEnabled = store.settings?.General?.EnableMultiRbx === "true";
  const sessionIds = useMemo(() => new Set(status?.active ? status.userIds ?? [] : []), [status]);
  /** Adotando com sessão ligada: quem ainda não está no ciclo. */
  const missingFromSession = targetIds.filter((id) => !sessionIds.has(id));

  async function handleStart() {
    setBottingStartError(null);
    const pid = parseInt(placeId.trim(), 10);
    if (!Number.isFinite(pid) || pid <= 0) {
      store.addToast(t("Place ID is required"));
      return;
    }
    if (!multiRbxEnabled) {
      const msg = t("Auto Rejoin currently requires Multi Roblox to be enabled");
      setBottingStartError(msg);
      store.addToast(msg);
      return;
    }
    if (targetIds.length < 2) {
      store.addToast(t("Select at least 2 accounts"));
      return;
    }
    setBusy(true);
    try {
      await saveCurrentDraft();
      if (adopt) {
        // Nada fecha nem relança: as contas ficam no servidor em que estão até
        // o primeiro rejoin delas.
        await store.adoptRunningIntoBotting(targetIds, {
          placeId: pid,
          intervalMinutes,
          launchDelaySeconds,
          playerGraceMinutes,
          playerUserIds,
        });
      } else {
        await store.startBottingMode({
          userIds: targetIds,
          placeId: pid,
          jobId: jobId.trim(),
          launchData,
          playerUserIds,
          intervalMinutes,
          launchDelaySeconds,
          playerGraceMinutes,
        });
      }
    } catch (e) {
      setBottingStartError(String(e));
      store.addToast(
        t("Auto Rejoin start failed: {{error}}", {
          error: String(e),
        })
      );
    }
    setBusy(false);
  }

  /** Adotando com a sessão já ligada: só entra nela, sem relançar ninguém. */
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

  async function handleTogglePlayer(userId: number) {
    const next = playerUserIds.includes(userId)
      ? playerUserIds.filter((id) => id !== userId)
      : [...playerUserIds, userId];
    setPlayerUserIds(next);
    await saveCurrentDraft({ players: next });
    if (status?.active) {
      await store.setBottingPlayerAccounts(next);
    }
  }

  function handleClearPlayers() {
    setPlayerUserIds([]);
    void saveCurrentDraft({ players: [] });
    if (status?.active) void store.setBottingPlayerAccounts([]);
    setPlayerMenuOpen(false);
  }

  function updatePlaceId(next: string) {
    setPlaceId(next);
    if (shareLaunchFields) store.setPlaceId(next);
  }

  function updateJobId(next: string) {
    setJobId(next);
    if (shareLaunchFields) store.setJobId(next);
  }

  function updateLaunchData(next: string) {
    setLaunchData(next);
    if (shareLaunchFields) store.setLaunchData(next);
  }

  function applyCurrentLaunchFields() {
    setPlaceId(store.placeId);
    setJobId(store.jobId);
    setLaunchData(store.launchData);
  }

  const uiActionLocked = busy || bulkBusy || rowBusy !== null;
  const actionButtonsLocked = uiActionLocked;

  /**
   * A confirmacao da linha nao segue o lote cegamente: o que decide e o que a
   * acao custa. `closeDisconnect` tira a conta do ciclo de rejoin ate alguem
   * reconectar (`botting_action_flags`, no Rust), e isso nao se desfaz sozinho —
   * pergunta. `close` sozinho e transitorio (o loop reabre o cliente no proximo
   * restart) e e justamente o gesto rapido de quem viu um cliente travado, por
   * isso segue sem pergunta. O lote pergunta nos dois porque age sobre uma
   * selecao que a pessoa pode ter esquecido que fez.
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
      store.addToast(
        t("Auto Rejoin account action failed: {{error}}", {
          error: String(e),
        })
      );
    } finally {
      setRowBusy(null);
    }
  }

  async function runRowFocus(userId: number) {
    if (actionButtonsLocked) return;
    setRowBusy(userId);
    try {
      const focused = await store.focusRobloxClient(userId);
      if (!focused) {
        store.addToast(t("No active Roblox window found for this account"));
      }
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

  function handleLayoutModeChange(nextLayout: "split" | "classic") {
    if (nextLayout === bottingLayout) return;
    setBottingLayout(nextLayout);
    void invoke("update_setting", {
      section: "General",
      key: "BottingDualPanelDialog",
      value: nextLayout === "split" ? "true" : "false",
    }).catch(() => {});
  }

  const statusMap = new Map((status?.accounts || []).map((a) => [a.userId, a]));
  const liveUserIds =
    status?.active && (status.userIds?.length || 0) > 0 ? status.userIds : targetIds;
  const liveRows = liveUserIds.map((userId) => ({
    userId,
    account: accountById.get(userId) || null,
    row: statusMap.get(userId) || null,
  }));
  const canStart =
    targetIds.length >= 2 && !!placeId.trim() && !uiActionLocked && multiRbxEnabled;
  /** Por que o Start não liga — a primeira coisa que falta, na barra de estado. */
  const startBlocker: string | null = !multiRbxEnabled
    ? t("Auto Rejoin currently requires Multi Roblox to be enabled")
    : targetIds.length < 2
      ? t("Select at least 2 accounts")
      : !placeId.trim()
        ? adopt && detection.state === "detecting"
          ? t("Finding the game these accounts are in...")
          : t("Place ID is required")
        : null;
  const dialogError = bottingStartError || store.error || null;
  const rowConflictError =
    (status?.accounts || [])
      .map((a) => a.lastError)
      .find((msg) => isMultiRobloxCloseProcessError(msg)) || null;
  const closeRobloxAlertMessage = isMultiRobloxCloseProcessError(dialogError)
    ? dialogError
    : rowConflictError;
  const showCloseRobloxAction = !!closeRobloxAlertMessage;
  const playerAccountLabel =
    playerUserIds.length === 0
      ? t("None")
      : playerUserIds.length === 1
        ? accountById.get(playerUserIds[0])?.Alias ||
          accountById.get(playerUserIds[0])?.Username ||
          t("Unknown")
        : t("{{count}} selected", { count: playerUserIds.length });
  /**
   * O rótulo do botão Main Accounts corta o nome (ou diz só "N selected"): o
   * `title` traz quem são, por inteiro. Mesmo padrão do chip de Targets.
   */
  const playerAccountTitle =
    playerUserIds.length === 0
      ? undefined
      : playerUserIds
          .map((id) => accountById.get(id)?.Alias || accountById.get(id)?.Username || t("Unknown"))
          .join(", ");
  const splitPlayersCount = liveRows.filter(
    ({ userId, row }) => !!row?.isPlayer || playerUserIds.includes(userId)
  ).length;
  // Quantos clientes o "Stop + Close" fecha: as contas bot desta sessão.
  // `stop_botting_mode` fecha `user_ids` menos `player_user_ids` — nada mais.
  const splitBotCount = liveRows.length - splitPlayersCount;
  const splitDisconnectedCount = liveRows.filter(({ row }) => !!row?.disconnected).length;
  const splitRetryingCount = liveRows.filter(({ row }) => (row?.retryCount || 0) >= 2).length;
  /** O próximo rejoin agendado da sessão (alts conectadas). */
  const nextRejoinAtMs = (status?.active ? status.accounts : [])
    .filter((a) => !a.disconnected && !a.isPlayer && typeof a.nextRestartAtMs === "number")
    .reduce<number | null>(
      (min, a) => (min === null || (a.nextRestartAtMs as number) < min ? (a.nextRestartAtMs as number) : min),
      null
    );
  const bulkSelectedSet = useMemo(() => new Set(bulkSelectedUserIds), [bulkSelectedUserIds]);
  const liveUserIdSet = useMemo(() => new Set(liveUserIds), [liveUserIds]);
  const liveRowsByUserId = useMemo(
    () => new Map(liveRows.map(({ userId, row }) => [userId, row])),
    [liveRows]
  );
  const allVisibleBulkSelected =
    liveRows.length > 0 && liveRows.every(({ userId }) => bulkSelectedSet.has(userId));
  const visibleBotRowIds = liveRows
    .filter(({ row }) => !!row && !row.isPlayer)
    .map(({ userId }) => userId);
  const bulkEligibleCounts = useMemo<Record<BottingRowAction, number>>(() => {
    const counts: Record<BottingRowAction, number> = {
      disconnect: 0,
      close: 0,
      closeDisconnect: 0,
      restartClient: 0,
      restartLoop: 0,
    };
    if (!status?.active) return counts;
    for (const userId of bulkSelectedUserIds) {
      const row = liveRowsByUserId.get(userId) || null;
      if (canRunBottingActionOnRow(row, "disconnect")) counts.disconnect += 1;
      if (canRunBottingActionOnRow(row, "close")) counts.close += 1;
      if (canRunBottingActionOnRow(row, "closeDisconnect")) counts.closeDisconnect += 1;
      if (canRunBottingActionOnRow(row, "restartClient")) counts.restartClient += 1;
      if (canRunBottingActionOnRow(row, "restartLoop")) counts.restartLoop += 1;
    }
    return counts;
  }, [bulkSelectedUserIds, liveRowsByUserId, status?.active]);

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
            "Close the Roblox client of 1 alt account? It leaves the server now, and the loop rejoins it at its next scheduled rejoin."
          )
        : t(
            "Close the Roblox client of {{count}} alt accounts? They leave the server now, and the loop rejoins each one at its next scheduled rejoin.",
            { count }
          );
    }
    if (action === "closeDisconnect") {
      return count === 1
        ? t(
            "Close the Roblox client of 1 alt account and take it out of the rejoin cycle? It stays out until you reconnect it."
          )
        : t(
            "Close the Roblox client of {{count}} alt accounts and take them out of the rejoin cycle? They stay out until you reconnect them.",
            { count }
          );
    }
    return null;
  }

  async function runBulkAction(action: BottingRowAction) {
    if (!status?.active || actionButtonsLocked) return;
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

    if (failed === 0) {
      store.addToast(
        t("Applied {{action}} to {{count}} accounts", {
          action: actionLabel,
          count: succeeded,
        })
      );
    } else {
      store.addToast(
        t("Applied {{action}}: {{success}} succeeded, {{failed}} failed", {
          action: actionLabel,
          success: succeeded,
          failed,
        })
      );
    }
  }

  function handleStop() {
    void store.stopBottingMode(false);
  }

  /**
   * Parar fechando os clientes bot: a frase diz quantos clientes fecham e o
   * que continua aberto (a mesma regra explicada em "How each cycle works").
   */
  async function handleStopAndCloseBots() {
    const ok = await confirm(
      splitBotCount === 1
        ? t(
            "Stop Auto Rejoin and close the Roblox client of 1 alt account in this session? Main accounts keep their client, and clients of accounts outside this session are left alone."
          )
        : t(
            "Stop Auto Rejoin and close the Roblox clients of {{count}} alt accounts in this session? Main accounts keep their client, and clients of accounts outside this session are left alone.",
            { count: splitBotCount }
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

  useEffect(() => {
    setBulkSelectedUserIds((prev) => {
      const next = prev.filter((id) => liveUserIdSet.has(id));
      return next.length === prev.length ? prev : next;
    });
  }, [liveUserIdSet]);

  return {
    t,
    store,
    adopt,
    targetIds,
    targetAccounts,
    accountById,
    status,
    running: !!status?.active,
    statusMap,
    game,
    runningGame,
    placeId,
    jobId,
    launchData,
    shareLaunchFields,
    intervalMinutes,
    launchDelaySeconds,
    playerGraceMinutes,
    playerUserIds,
    detection: detection.state,
    nowMs,
    playerMenuOpen,
    setPlayerMenuOpen,
    playerMenuRef,
    contentRef,
    bottingLayout,
    useSplitLayout: bottingLayout === "split",
    multiRbxEnabled,
    canStart,
    startBlocker,
    startError: bottingStartError,
    closeRobloxAlertMessage,
    showCloseRobloxAction,
    closingRoblox,
    playerAccountLabel,
    playerAccountTitle,
    liveRows,
    splitPlayersCount,
    splitBotCount,
    splitDisconnectedCount,
    splitRetryingCount,
    nextRejoinAtMs,
    missingFromSession,
    actionButtonsLocked,
    rowBusy,
    bulkSelectedUserIds,
    bulkSelectedSet,
    allVisibleBulkSelected,
    visibleBotRowIds,
    bulkEligibleCounts,
    setIntervalMinutes,
    setLaunchDelaySeconds,
    setPlayerGraceMinutes,
    updatePlaceId,
    updateJobId,
    updateLaunchData,
    applyCurrentLaunchFields,
    saveCurrentDraft,
    handleStart,
    handleAddToSession,
    handleStop,
    handleStopAndCloseBots,
    handleTogglePlayer,
    handleClearPlayers,
    handleCloseRobloxBannerAction,
    handleLayoutModeChange,
    dismissError,
    runRowAction,
    runRowFocus,
    runBulkAction,
    toggleBulkSelected,
    setBulkSelected,
  };
}

export type RejoinController = ReturnType<typeof useRejoinController>;
