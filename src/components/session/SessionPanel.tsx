import { useEffect, useMemo, useRef, useState } from "react";
import {
  AppWindow,
  Check,
  Coffee,
  Crosshair,
  Gamepad2,
  Globe,
  Loader2,
  PowerOff,
  RotateCw,
  SquareStop,
  UserCheck,
  UserPlus,
  UserX,
  X,
} from "lucide-react";
import { useConfirm } from "../../hooks/usePrompt";
import { useEscapeStack } from "../../hooks/useEscapeStack";
import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { accountLabel } from "../../utils/accountName";
import { ClientHealthNote, WrongAccountNote } from "./ClientHealthNote";
import { ClosedClientsSection } from "./ClosedClientsSection";
import { ReconnectBulkChip, ReconnectChip } from "./ReconnectChip";
import { MemoryBulkChip, MemoryChip, type MemoryLimitValue } from "./MemoryLimitControl";
import { MEMORY_LIMIT_SETTING, fieldsWithMemoryLimit, memoryLimitChoice } from "../../utils/memoryLimit";
import {
  autoReconnectLabel,
  fieldsWithReconnect,
  reconnectChoice,
  reconnectErrorText,
  type ReconnectChoice,
} from "../../utils/autoReconnect";
import { useNexusAutoRelaunch } from "../../hooks/useNexusAutoRelaunch";
import { isWindowsPlatform } from "../../utils/platform";
import { useCurrentSessions } from "../../hooks/useCurrentSessions";
import { CurrentSessionLine, SessionClock } from "./CurrentSessionLine";
import { SessionStats, type SessionCounts, type SessionFilter } from "./SessionStats";
import { AttentionGroupHeader, GameGroupHeader, NoGameGroupHeader, SelectBox, ServerHeader } from "./InGameGroups";
import { groupRunning, needsAttention, runningStatus, type RunningStatus } from "../../utils/sessionGroups";
import { clientHealthLabel } from "../../utils/clientHealth";
import { activeReconnectIds } from "../../utils/accountLiveState";
import { useGameIdentity } from "../../hooks/useGameIdentity";
import type {
  AutoReconnectEntry,
  ClientHealth,
  CurrentSession,
  FriendLinkAccountState,
  FollowUserAccountState,
  LaunchQueueEntry,
  LaunchQueueState,
  UnidentifiedClient,
} from "../../types";

/**
 * Painel de Sessão: o que está entrando agora e o que já está em jogo.
 *
 * O mesmo componente é montado em dois lugares (aba Console da Choose Game e
 * `SessionDialog`, aberto pela barra principal). Todo o estado vem da store
 * (`launchQueue` do evento `launch-queue` e `launchedByProgram` do polling de
 * `get_running_instances`), então os dois mostram exatamente a mesma coisa.
 *
 * Regra de produto que não pode se perder: **cancelar nunca fecha um cliente
 * já aberto**. Cancelar só tira a conta da fila; fechar é uma ação separada,
 * explícita, na seção "In game".
 */

type Translate = ReturnType<typeof useTr>;

function stateLabel(state: LaunchQueueState, t: Translate): string {
  switch (state) {
    case "queued":
      return t("Queued");
    case "launching":
      return t("Joining");
    case "done":
      return t("Joined");
    case "failed":
      return t("Failed");
    case "cancelled":
      return t("Cancelled");
    default:
      return state;
  }
}

const STATE_STYLES: Record<LaunchQueueState, { dot: string; text: string }> = {
  queued: { dot: "bg-[var(--panel-muted)]", text: "theme-muted" },
  launching: { dot: "bg-[var(--accent-color)] animate-pulse", text: "text-[var(--accent-color)]" },
  done: { dot: "bg-emerald-500", text: "text-emerald-400" },
  failed: { dot: "bg-red-500", text: "text-red-400" },
  cancelled: { dot: "bg-zinc-600", text: "theme-muted" },
};

/**
 * Estados de uma conta no Make Friends. Cores iguais às da fila: a bolinha quer
 * dizer a mesma coisa nas duas listas do painel.
 */
const FRIEND_STATE_STYLES: Record<FriendLinkAccountState, { dot: string; text: string }> = {
  pending: { dot: "bg-[var(--panel-muted)]", text: "theme-muted" },
  processing: { dot: "bg-[var(--accent-color)] animate-pulse", text: "text-[var(--accent-color)]" },
  done: { dot: "bg-emerald-500", text: "text-emerald-400" },
  failed: { dot: "bg-red-500", text: "text-red-400" },
};

function friendStateLabel(state: FriendLinkAccountState, t: Translate): string {
  switch (state) {
    case "pending":
      return t("Waiting");
    case "processing":
      return t("Processing");
    case "done":
      return t("Linked");
    case "failed":
      return t("Failed");
    default:
      return state;
  }
}

/** Estados de uma conta no "Follow a user" — cores iguais às do Make Friends. */
const FOLLOW_STATE_STYLES: Record<FollowUserAccountState, { dot: string; text: string }> = {
  pending: FRIEND_STATE_STYLES.pending,
  processing: FRIEND_STATE_STYLES.processing,
  browser: { dot: "bg-amber-400 animate-pulse", text: "text-amber-300" },
  followed: FRIEND_STATE_STYLES.done,
  already: { dot: "bg-emerald-500/60", text: "theme-muted" },
  failed: FRIEND_STATE_STYLES.failed,
};

function followStateLabel(state: FollowUserAccountState, t: Translate): string {
  switch (state) {
    case "pending":
      return t("Waiting");
    case "processing":
      return t("Processing");
    case "browser":
      return t("Finish in the browser");
    case "followed":
      return t("Followed");
    case "already":
      return t("Already following");
    case "failed":
      return t("Failed");
    default:
      return state;
  }
}

/** O que a operação está fazendo agora — as três fases custam tempo diferente. */
function friendPhaseLabel(phase: string, t: Translate): string {
  switch (phase) {
    case "checking":
      return t("Checking who is already friends");
    case "linking":
      return t("Sending friend requests");
    case "verifying":
      return t("Verifying the friendships");
    case "done":
      return t("Finished");
    default:
      return "";
  }
}

/**
 * Por que o cliente aberto fora do app não foi reconhecido sozinho. `name` é o
 * nome (já mascarado) da conta que o log citou, quando citou.
 */
function unidentifiedReasonLabel(client: UnidentifiedClient, name: string, t: Translate): string {
  switch (client.reason) {
    case "waitingForGame":
      return t("It hasn't joined a game yet, so the account is still unknown");
    case "noLog":
      return t("No Roblox log matched this client");
    case "unknownAccount":
      return t("Its account is not in your list");
    case "accountBusy":
      return t("{{name}} already has a client open", { name });
    default:
      return "";
  }
}

/**
 * O jogo de uma conta da fila. Só aparece quando a fila mistura jogos (um
 * launch disparado com outro rodando entra no fim da mesma fila, com o jogo
 * dele); com um jogo só, o nome em cada linha seria a mesma palavra repetida.
 */
function QueueEntryGame({ placeId, userId }: { placeId: number; userId: number }) {
  const t = useTr();
  const identity = useGameIdentity(placeId, userId);
  const name = identity?.name ?? t("Place {{placeId}}", { placeId });
  return (
    <span className="theme-muted truncate min-w-0" title={name}>
      {name}
    </span>
  );
}

/** Estados em que a conta ainda pode sair da fila. */
function isPending(entry: LaunchQueueEntry): boolean {
  return entry.state === "queued" || entry.state === "launching";
}

/** Uma queda de uma conta: a mesma conta caindo de novo é outra chave. */
function dropKey(entry: AutoReconnectEntry): string {
  return `${entry.userId}:${entry.drop?.sinceMs ?? 0}`;
}

/**
 * Estado de uma conta do "Em jogo", curto, ao lado do nome: reconectando
 * (âmbar), jogando (verde) ou ainda sem jogo. A queda e o "não responde" vão
 * numa linha própria embaixo (`ClientHealthNote`), que pode quebrar: o motivo
 * cortado em 0 px não dizia nada. Sem log achado e sem sessão, não diz nada.
 */
function RunningState({
  session,
  health,
  reconnecting,
}: {
  session: CurrentSession | undefined;
  health: ClientHealth | undefined;
  reconnecting: boolean;
}) {
  const t = useTr();
  if (reconnecting) {
    return (
      <span className="shrink-0 inline-flex items-center gap-1 text-amber-400">
        <RotateCw size={11} strokeWidth={1.75} aria-hidden="true" />
        {t("Reconnecting")}
      </span>
    );
  }
  if (health?.drop || health?.notResponding) return null;
  if (session) {
    return (
      <span className="shrink-0 inline-flex items-center gap-1 text-emerald-400">
        <span aria-hidden="true" className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
        {t("Playing")}
      </span>
    );
  }
  if (health?.logFound && !health.inGame) {
    return <span className="shrink-0 theme-muted">{t("Not in a game yet")}</span>;
  }
  return null;
}

interface SessionPanelProps {
  className?: string;
  /**
   * Faixa de números em cima (jogando, reconectando, não responde, entrando),
   * cada um um filtro da lista. Só a página Session mostra.
   */
  showStats?: boolean;
  /**
   * Cada lista com rolagem própria (teto de altura). Embutido em outro lugar o
   * painel precisa disso; na página Session a página rola inteira e uma caixa
   * de rolagem dentro dela escondia contas.
   */
  contained?: boolean;
}

export function SessionPanel({ className = "", showStats = false, contained = true }: SessionPanelProps) {
  const t = useTr();
  const store = useStore();
  const confirm = useConfirm();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  // Cliente não identificado com o seletor de conta aberto, e a conta escolhida.
  const [identifying, setIdentifying] = useState<{ pid: number; userId: string } | null>(null);
  // Grupos recolhidos da lista "Em jogo" (chave do grupo).
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  // "Close" da linha armado ("Close?"), esperando o segundo clique.
  const [armedClose, setArmedClose] = useState<number | null>(null);
  const armTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (armTimer.current) clearTimeout(armTimer.current);
    },
    []
  );
  useEscapeStack(armedClose !== null, () => setArmedClose(null));
  // Shift+clique: a última conta clicada e a ordem das linhas na tela.
  const anchorRef = useRef<number | null>(null);
  const visibleOrderRef = useRef<number[]>([]);
  // Filtro da faixa de números (só o estado escolhido na lista).
  const [filter, setFilter] = useState<SessionFilter | null>(null);
  const joiningRef = useRef<HTMLElement>(null);

  const entries = store.launchQueue?.entries ?? [];
  const pending = entries.filter(isPending);
  // Fila com mais de um jogo: cada linha diz o seu.
  const queueMixesGames = new Set(entries.map((entry) => entry.placeId).filter((id) => !!id)).size > 1;
  // Só aparece depois que houve um Make Friends: uma seção vazia permanente
  // roubaria altura de um painel que já tem teto de 45% na aba Console.
  const friendLink = store.friendLinkState;
  const showFriendLink = !!friendLink && friendLink.total > 0;
  // "Follow a user": mesma regra — só depois de um lote.
  const followRun = store.followUserState;
  const showFollow = !!followRun && followRun.total > 0;

  // Contas com cliente rodando agora, na ordem da lista de contas (estável
  // entre polls). IDs sem conta correspondente ainda aparecem, pelo ID.
  const runningIds = useMemo(() => {
    const running = store.launchedByProgram;
    const known = store.accounts.filter((a) => running.has(a.UserID)).map((a) => a.UserID);
    const extras = [...running].filter((id) => !known.includes(id));
    return [...known, ...extras];
  }, [store.accounts, store.launchedByProgram]);

  // Clientes abertos fora do app que o backend não reconheceu (ver
  // docs/features/external-clients.md). Ficam fora da contagem e do lote de
  // fechar: não têm conta, e o app nunca fecha cliente que não sabe de quem é.
  const unidentified = store.unidentifiedClients;

  // A seleção é derivada: contas que fecharam sozinhas somem do lote.
  const selected = runningIds.filter((id) => checked.has(id));

  // Reconexão automática (commands/reconnect.rs). A contagem regressiva
  // ("em 30 s") só anda enquanto alguma conta espera a próxima tentativa.
  // "Dispensar" esconde a linha na hora, sem esperar o evento do backend. A
  // chave é a queda (conta + início): se a conta cair de novo, a linha volta.
  const [dismissed, setDismissed] = useState<Set<string>>(() => new Set());
  // Contas com um comando de reconexão em andamento (botões desabilitados).
  const [reconnectBusy, setReconnectBusy] = useState<Set<number>>(() => new Set());
  const reconnecting = (store.autoReconnect ?? []).filter((entry) => !dismissed.has(dropKey(entry)));
  const countingDown = reconnecting.some((entry) => entry.phase === "waiting");
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!countingDown) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [countingDown]);

  // Reconexão automática por conta (commands/reconnect.rs lê o log do Roblox,
  // então só no Windows). O campo da conta manda; sem ele, o padrão
  // `General.AutoReconnect`; o AutoRelaunch do Nexus liga por cima.
  const isWindows = isWindowsPlatform(store.platformCapabilities);
  const reconnectDefault = store.settings?.General?.AutoReconnect === "true";
  const nexusRelaunch = useNexusAutoRelaunch(isWindows, runningIds.join(","));
  // Jogo e início da sessão de cada conta (o mesmo do histórico). Só no
  // Windows: é lá que o log do Roblox é lido.
  const sessions = useCurrentSessions(isWindows, runningIds.join(","));
  const reconnectingIds = activeReconnectIds(store.autoReconnect);
  const [reconnectSaving, setReconnectSaving] = useState(false);

  // Teto de memória (commands/memory_ceiling.rs): só com a feature
  // `memory-trim` no binário, e só nos clientes que o app abriu. O campo
  // `MemoryLimit` da conta manda; sem ele, `Optimization.MemoryLimit`.
  const memoryTrim = isWindows && store.platformCapabilities?.supportsMemoryTrim === true;
  const memoryDefaultRaw = store.settings?.[MEMORY_LIMIT_SETTING.section]?.[MEMORY_LIMIT_SETTING.key];
  const memoryDefaultMb = memoryLimitChoice(undefined, memoryDefaultRaw).defaultMb;
  const [memorySaving, setMemorySaving] = useState(false);
  const memoryBulkIds = selected.filter((id) => !store.adoptedClients.has(id));

  /**
   * Grava o limite (`null` = volta ao padrão) no `Fields.MemoryLimit` de cada
   * conta, uma por vez. Vale na próxima passada do backend (2 s), sem relançar
   * e sem fechar nada.
   */
  async function saveMemoryLimit(userIds: number[], value: MemoryLimitValue) {
    const targets = store.accounts.filter((a) => userIds.includes(a.UserID));
    if (targets.length === 0) return;
    setError(null);
    setMemorySaving(true);
    try {
      for (const account of targets) {
        await store.updateAccount({ ...account, Fields: fieldsWithMemoryLimit(account.Fields, value) });
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setMemorySaving(false);
    }
  }

  function nameFor(userId: number): string {
    const account = store.accounts.find((a) => a.UserID === userId);
    return accountLabel(account, store, userId);
  }

  function choiceFor(userId: number): ReconnectChoice | null {
    const account = store.accounts.find((a) => a.UserID === userId);
    if (!account) return null;
    const nexusForced = nexusRelaunch.has((account.Username ?? "").toLowerCase());
    return reconnectChoice(account.Fields, reconnectDefault, nexusForced);
  }

  /**
   * Grava a escolha (`null` = volta ao padrão) no `Fields.AutoReconnect` de
   * cada conta, uma por vez. Não fecha nem abre cliente nenhum: o backend lê o
   * campo na próxima queda.
   */
  async function saveReconnect(userIds: number[], value: boolean | null) {
    const targets = store.accounts.filter((a) => userIds.includes(a.UserID));
    if (targets.length === 0) return;
    setError(null);
    setReconnectSaving(true);
    try {
      for (const account of targets) {
        await store.updateAccount({ ...account, Fields: fieldsWithReconnect(account.Fields, value) });
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setReconnectSaving(false);
    }
  }

  /**
   * Marca/desmarca uma conta. Com Shift, o intervalo desde a última clicada
   * (na ordem em que as linhas estão na tela) recebe o novo estado da clicada,
   * como na lista de contas.
   */
  function toggleChecked(userId: number, event?: { shiftKey: boolean }) {
    const anchor = anchorRef.current;
    anchorRef.current = userId;
    const order = visibleOrderRef.current;
    if (event?.shiftKey && anchor !== null && anchor !== userId && order.includes(anchor) && order.includes(userId)) {
      const [from, to] = [order.indexOf(anchor), order.indexOf(userId)].sort((a, b) => a - b);
      const range = order.slice(from, to + 1);
      const on = !checked.has(userId);
      setChecked((prev) => {
        const next = new Set(prev);
        for (const id of range) {
          if (on) next.add(id);
          else next.delete(id);
        }
        return next;
      });
      return;
    }
    setChecked((prev) => {
      const next = new Set(prev);
      if (next.has(userId)) next.delete(userId);
      else next.add(userId);
      return next;
    });
  }

  function toggleAll() {
    setChecked(selected.length === runningIds.length ? new Set() : new Set(runningIds));
  }

  async function handleCancel(userId: number) {
    setError(null);
    try {
      await store.cancelAccountLaunch(userId);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleStopQueue() {
    setError(null);
    try {
      await store.stopLaunchQueue();
    } catch (e) {
      setError(String(e));
    }
  }

  /**
   * "Tentar agora" / "Parar" / dispensar. `false` do backend: a conta já não
   * está na reconexão (a lista da tela estava velha) — avisa em vez de calar.
   */
  async function handleReconnectAction(entry: AutoReconnectEntry, action: "retry" | "stop" | "dismiss") {
    const { userId } = entry;
    if (reconnectBusy.has(userId)) return;
    const key = dropKey(entry);
    setError(null);
    setReconnectBusy((prev) => new Set(prev).add(userId));
    if (action === "dismiss") setDismissed((prev) => new Set(prev).add(key));
    // A mesma conta com o aviso de "fechou" (seção Closed, linha da lista de
    // contas): dispensar a linha da reconexão dispensa os dois — senão o
    // aviso escondido reaparecia na seção de baixo.
    if (action === "dismiss" && store.closedClients?.has(userId)) {
      void store.dismissClosedClient(userId).catch(() => {});
    }
    try {
      const ok =
        action === "retry" ? await store.retryAutoReconnect(userId) : await store.stopAutoReconnect(userId);
      if (!ok) {
        store.addToast(t("{{name}} is no longer being reconnected.", { name: nameFor(userId) }), "info");
      }
    } catch (e) {
      if (action === "dismiss") {
        setDismissed((prev) => {
          const next = new Set(prev);
          next.delete(key);
          return next;
        });
      }
      setError(String(e));
    } finally {
      setReconnectBusy((prev) => {
        const next = new Set(prev);
        next.delete(userId);
        return next;
      });
    }
  }

  async function handleFocus(userId: number) {
    setError(null);
    try {
      const ok = await store.focusRobloxClient(userId);
      if (!ok) setError(t("Could not bring that Roblox window to the front."));
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleShowWindow(pid: number) {
    setError(null);
    try {
      const ok = await store.focusClientWindow(pid);
      if (!ok) setError(t("Could not bring that Roblox window to the front."));
    } catch (e) {
      setError(String(e));
    }
  }

  function startIdentify(client: UnidentifiedClient) {
    setError(null);
    // Se o log citou uma conta salva (ocupada), ela já vem escolhida.
    const named =
      client.userId !== null && store.accounts.some((a) => a.UserID === client.userId)
        ? String(client.userId)
        : "";
    setIdentifying({ pid: client.pid, userId: named });
  }

  async function confirmIdentify() {
    if (!identifying || !identifying.userId) return;
    setError(null);
    setBusy(true);
    try {
      await store.identifyExternalClient(identifying.pid, Number(identifying.userId));
      setIdentifying(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /**
   * Abre o Modo AFK com as contas em jogo (as marcadas, ou todas). Não liga
   * nada: o Start de lá é que adota as contas no Auto Rejoin **sem fechar
   * nenhum cliente** — e antes dele a pessoa vê e ajusta o tempo do ciclo e as
   * contas main. Ligar direto daqui escondia onde configurar e onde parar.
   *
   * Abre nos cliques AFK (a aba padrão), a não ser que só o Auto Rejoin esteja
   * ligado.
   */
  function handleOpenAfkMode() {
    const alvo = selected.length > 0 ? selected : runningIds;
    if (alvo.length === 0) return;
    const onlyRejoinActive = !!store.bottingStatus?.active && !store.afkStatus?.active;
    store.openAfkMode({
      tab: onlyRejoinActive ? "rejoin" : "clicks",
      targetUserIds: alvo,
      adoptRunning: true,
    });
  }

  /** Fecha só as contas da lista (`closeRobloxClients`), nunca cliente de fora dela. */
  async function closeNow(userIds: number[]) {
    if (userIds.length === 0) return;
    setError(null);
    setArmedClose(null);
    setBusy(true);
    try {
      await store.closeRobloxClients(userIds);
      setChecked((prev) => {
        const next = new Set(prev);
        for (const id of userIds) next.delete(id);
        return next;
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /**
   * "Close all clients" e "Close selected": sempre perguntam, uma vez para o
   * lote. O diálogo lista as contas (as cinco primeiras e "and N more"), marca
   * as abertas pelo site e o botão diz quantos clientes fecha.
   */
  async function confirmAndClose(userIds: number[]) {
    if (userIds.length === 0) return;
    setError(null);
    const lines = userIds.map((id) =>
      store.adoptedClients.has(id)
        ? t("{{name}} (opened from the website)", { name: nameFor(id) })
        : nameFor(id)
    );
    const details =
      lines.length > 5 ? [...lines.slice(0, 5), t("and {{count}} more", { count: lines.length - 5 })] : lines;
    const website = userIds.filter((id) => store.adoptedClients.has(id)).length;
    const note =
      website === 0
        ? undefined
        : website === 1
          ? t("Includes 1 client opened from the Roblox website.")
          : t("Includes {{count}} clients opened from the Roblox website.", { count: website });
    const ok = await confirm(t("Close the Roblox client of these accounts?"), true, {
      confirmLabel:
        userIds.length === 1 ? t("Close 1 client") : t("Close {{count}} clients", { count: userIds.length }),
      details,
      note,
    });
    if (ok) await closeNow(userIds);
  }

  /**
   * "Close" da linha: o primeiro clique só arma ("Close?"), o segundo fecha.
   * Desarma sozinho em 4 s, no Escape ou quando o foco sai do botão.
   */
  function armClose(userId: number) {
    setArmedClose(userId);
    if (armTimer.current) clearTimeout(armTimer.current);
    armTimer.current = setTimeout(() => setArmedClose(null), 4_000);
  }

  // Lista "Em jogo" por jogo e servidor; quem precisa de atenção vai para o
  // topo (utils/sessionGroups.ts). Sem nenhuma sessão conhecida (fora do
  // Windows, ou nada entrou em jogo ainda) fica a lista simples de antes.
  const statusOf = (userId: number) =>
    runningStatus(sessions.get(userId), store.clientHealth?.get(userId), reconnectingIds.has(userId));
  // A faixa de números conta das mesmas linhas da lista.
  const counts: SessionCounts = { playing: 0, reconnecting: 0, notResponding: 0, dropped: 0, joining: pending.length };
  for (const id of runningIds) {
    const status = statusOf(id);
    if (status === "playing" || status === "reconnecting" || status === "notResponding" || status === "dropped") {
      counts[status] += 1;
    }
  }
  // Filtro que ficou sem ninguém (a conta voltou ao jogo) se desfaz sozinho.
  const activeFilter = filter && counts[filter] > 0 ? filter : null;
  const shownIds = activeFilter ? runningIds.filter((id) => statusOf(id) === activeFilter) : runningIds;
  const groups = groupRunning(shownIds, sessions, statusOf);
  const grouped = sessions.size > 0 || groups.attention.length > 0;
  const listBox = contained ? "max-h-56 overflow-y-auto" : "";
  // Lista larga (página Session num monitor grande): as linhas do "Em jogo"
  // viram colunas pela largura da própria lista (`@container/ingame`), não da
  // janela — no painel estreito e na janela padrão continua uma coluna só.
  const rowGrid = "grid grid-cols-1 gap-x-3 @6xl/ingame:grid-cols-2 @min-[110rem]/ingame:grid-cols-3";
  // Reconexão: quem ainda tem cliente aberto mostra a tentativa na própria
  // linha do "Em jogo" (uma conta, um lugar); a seção "Reconnecting" fica só
  // com quem não tem cliente aberto (fechou, desistiu). Antes as duas listas
  // repetiam as mesmas contas e empurravam o "Em jogo" para baixo.
  const runningSet = new Set(runningIds);
  // Região e tempo no ar do servidor (log do cliente): as contas do grupo estão
  // no mesmo servidor, então vale a primeira que já sabe.
  const serverInfoOf = (ids: number[]) =>
    ids.map((id) => store.clientHealth?.get(id)?.server).find((info) => info != null) ?? null;
  const wrongAccountIds = runningIds.filter((id) => store.clientHealth?.get(id)?.wrongAccountUserId != null);
  const reconnectInRow = new Map(
    reconnecting.filter((entry) => runningSet.has(entry.userId)).map((entry) => [entry.userId, entry] as const)
  );
  const reconnectApart = reconnecting.filter((entry) => !runningSet.has(entry.userId));

  // Mudança de estado anunciada ao leitor de tela, com calma (aria-live
  // polite): caiu, não responde, reconectando, voltou a jogar. A primeira
  // leitura não anuncia nada; a contagem regressiva não fala a cada segundo.
  const statusSignature = runningIds.map((id) => `${id}:${statusOf(id)}`).join(",");
  const previousStatus = useRef<Map<number, RunningStatus> | null>(null);
  const [announcement, setAnnouncement] = useState("");
  useEffect(() => {
    const next = new Map(runningIds.map((id) => [id, statusOf(id)] as const));
    const previous = previousStatus.current;
    previousStatus.current = next;
    if (!previous) return;
    const messages: string[] = [];
    for (const [id, status] of next) {
      const before = previous.get(id);
      if (before === undefined || before === status) continue;
      const name = nameFor(id);
      if (status === "dropped" || status === "notResponding") {
        const info = clientHealthLabel(store.clientHealth?.get(id), t);
        if (info) messages.push(`${name}: ${info.label}`);
      } else if (status === "reconnecting") {
        messages.push(t("{{name}} is reconnecting", { name }));
      } else if (status === "playing" && needsAttention(before)) {
        messages.push(t("{{name}} is playing again", { name }));
      }
    }
    if (messages.length > 0) setAnnouncement(messages.join(". "));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [statusSignature]);
  // Ordem das linhas como aparecem (grupos recolhidos ficam de fora).
  visibleOrderRef.current = grouped
    ? [
        ...(collapsed.has("attention") ? [] : groups.attention),
        ...groups.games.flatMap((game) => (collapsed.has(game.key) ? [] : game.servers.flatMap((sv) => sv.userIds))),
        ...(collapsed.has("no-game") ? [] : groups.noGame),
      ]
    : shownIds;

  function selectionOf(ids: number[]): "all" | "some" | "none" {
    const count = ids.filter((id) => checked.has(id)).length;
    return count === 0 ? "none" : count === ids.length ? "all" : "some";
  }

  /** A caixa do grupo: marca todas as contas dele, ou desmarca se já estavam todas marcadas. */
  function toggleGroup(ids: number[]) {
    const all = ids.every((id) => checked.has(id));
    setChecked((prev) => {
      const next = new Set(prev);
      for (const id of ids) {
        if (all) next.delete(id);
        else next.add(id);
      }
      return next;
    });
  }

  function toggleCollapsed(key: string) {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  /**
   * A tentativa de reconexão dentro da linha da conta: o que está acontecendo
   * (contagem regressiva como `timer`), a causa e os botões pequenos de tentar
   * agora / parar / dispensar. Mesmas ações da seção "Reconnecting".
   */
  function renderReconnectLine(entry: AutoReconnectEntry) {
    const info = autoReconnectLabel(entry, now, t);
    const name = nameFor(entry.userId);
    const pending = info.tone === "pending";
    const lineBusy = reconnectBusy.has(entry.userId);
    const turnedOff = choiceFor(entry.userId)?.effective === false;
    const small =
      "inline-flex h-6 items-center gap-1 rounded-md px-1.5 text-[11.5px] text-[var(--panel-fg)] hover:bg-[var(--row-hover)] disabled:opacity-50 disabled:cursor-default";
    return (
      <span
        data-testid={`session-reconnect-${entry.userId}`}
        data-phase={entry.phase}
        aria-busy={lineBusy || undefined}
        className="flex flex-wrap items-center gap-x-2 text-[11.5px]"
      >
        <span role={entry.phase === "waiting" ? "timer" : undefined} className={pending ? "text-amber-400" : "text-red-400"}>
          {info.label}
        </span>
        {info.detail && (
          <span data-testid={`session-reconnect-detail-${entry.userId}`} className="theme-muted">
            {info.detail}
          </span>
        )}
        {pending && turnedOff && <span className="theme-muted">{t("Turned off: it stops after the attempt in progress.")}</span>}
        <span className="inline-flex items-center gap-0.5">
          {(info.canTryNow || info.canTryAgain) && (
            <button
              type="button"
              onClick={() => void handleReconnectAction(entry, "retry")}
              disabled={lineBusy}
              title={t("Reopen this account now, in the same game")}
              className={small}
            >
              <RotateCw size={11} strokeWidth={1.75} aria-hidden="true" className={lineBusy ? "animate-spin" : undefined} />
              {info.canTryNow ? t("Try now") : t("Try again")}
            </button>
          )}
          {pending ? (
            <button
              type="button"
              onClick={() => void handleReconnectAction(entry, "stop")}
              disabled={lineBusy}
              title={t("Stop reconnecting this account (does not close any client)")}
              className={small}
            >
              <SquareStop size={11} strokeWidth={1.75} aria-hidden="true" />
              {t("Stop")}
            </button>
          ) : (
            <button
              type="button"
              onClick={() => void handleReconnectAction(entry, "dismiss")}
              disabled={lineBusy}
              aria-label={t("Dismiss {{name}}", { name })}
              title={t("Hide this line")}
              className="inline-flex w-6 h-6 items-center justify-center rounded-md theme-muted hover:text-[var(--panel-fg)] hover:bg-[var(--row-hover)] disabled:opacity-50"
            >
              <X size={12} strokeWidth={2} aria-hidden="true" />
            </button>
          )}
        </span>
      </span>
    );
  }

  /**
   * Uma conta da lista "Em jogo". `context` "game": a linha está dentro do
   * grupo do jogo e do servidor, então só leva o tempo em jogo; "full": fora
   * deles (atenção, sem jogo, lista simples), leva jogo · servidor · tempo.
   */
  function renderRow(userId: number, context: "game" | "full") {
    const session = sessions.get(userId);
    const health = store.clientHealth?.get(userId);
    const name = nameFor(userId);
    const choice = isWindows ? choiceFor(userId) : null;
    const account = store.accounts.find((a) => a.UserID === userId);
    const memoryChoice =
      memoryTrim && account && !store.adoptedClients.has(userId)
        ? memoryLimitChoice(account.Fields, memoryDefaultRaw)
        : null;
    return (
      <li
        key={userId}
        data-testid={`session-running-${userId}`}
        className={`flex items-start gap-2 px-3 py-1.5 text-[12px] ${
          checked.has(userId) ? "session-row-selected" : "hover:bg-[var(--row-hover)]"
        }`}
      >
        <span className="pt-0.5">
          <SelectBox
            label={t("Select {{name}}", { name })}
            checked={checked.has(userId)}
            onChange={(event) => toggleChecked(userId, event)}
          />
        </span>
        {/* A informação ao vivo tem a largura que sobra; os controles são
            chips compactos à direita. Em cima: conta, estado e tempo; embaixo,
            quando há, o motivo da queda (quebra linha) e, fora do grupo do
            jogo, jogo · servidor · tempo. */}
        <span className="min-w-0 flex-1 flex flex-col gap-0.5 pt-[3px]">
          <span className="flex items-center gap-2 min-w-0">
            <span className="text-[var(--panel-fg)] truncate min-w-[3ch]" title={name}>
              {name}
            </span>
            <RunningState session={session} health={health} reconnecting={reconnectingIds.has(userId)} />
            {session && context === "game" && (
              <span className="shrink-0 text-[11.5px] theme-muted">
                <SessionClock userId={userId} sinceMs={session.sinceMs} />
              </span>
            )}
            {store.adoptedClients.has(userId) && (
              <span
                className="min-w-0 flex items-center gap-1 theme-muted"
                title={t("Opened from the Roblox website and recognized by the Roblox log")}
              >
                <Globe size={11} strokeWidth={1.5} className="shrink-0" aria-hidden="true" />
                <span className="truncate">{t("Opened outside the app")}</span>
              </span>
            )}
          </span>
          {health?.wrongAccountUserId != null && <WrongAccountNote health={health} />}
          {(health?.drop || health?.notResponding) && <ClientHealthNote health={health} wrap />}
          {reconnectInRow.has(userId) && renderReconnectLine(reconnectInRow.get(userId)!)}
          {session && context === "full" && <CurrentSessionLine session={session} server={health?.server} />}
        </span>
        <span className="flex items-center gap-1 shrink-0">
          {memoryChoice && (
            <MemoryChip
              name={name}
              choice={memoryChoice}
              memory={store.clientMemory?.get(userId)}
              disabled={memorySaving}
              onChange={(value) => void saveMemoryLimit([userId], value)}
            />
          )}
          {choice && (
            <ReconnectChip
              name={name}
              choice={choice}
              disabled={reconnectSaving}
              onChange={(value) => void saveReconnect([userId], value)}
            />
          )}
          <button
            onClick={() => void handleFocus(userId)}
            title={t("Bring this client's window to the front")}
            className="session-row-btn"
          >
            <Crosshair size={12} strokeWidth={1.5} aria-hidden="true" />
            {t("Focus")}
          </button>
          {/* Longe do Focus: fechar não pode ser o clique errado. */}
          <span aria-hidden="true" className="w-3" />
          <button
            onClick={() => (armedClose === userId ? void closeNow([userId]) : armClose(userId))}
            onBlur={() => {
              if (armedClose === userId) setArmedClose(null);
            }}
            disabled={busy}
            title={
              armedClose === userId
                ? t("Click again to close this client")
                : t("Close this account's Roblox client")
            }
            className={`session-row-btn ${armedClose === userId ? "session-row-btn-armed" : "session-row-btn-danger"}`}
          >
            <PowerOff size={12} strokeWidth={1.5} aria-hidden="true" />
            {armedClose === userId ? t("Close?") : t("Close")}
          </button>
        </span>
      </li>
    );
  }

  return (
    <div className={`flex flex-col gap-3 min-h-0 ${className}`} data-testid="session-panel">
      <div data-testid="session-live" aria-live="polite" aria-atomic="true" className="sr-only">
        {announcement}
      </div>
      {error && (
        <div
          role="alert"
          className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-[12px] text-red-400"
        >
          {error}
        </div>
      )}

      {store.launchPreparing && (
        // Clique em Entrar: a fila ainda não existe (conferindo quem está
        // online, escolhendo o servidor). Sem isto o clique ficava sem
        // resposta por alguns segundos (relato do dono, 15/10/2026).
        <div
          role="status"
          data-testid="launch-preparing"
          className="flex items-center gap-2 rounded-lg border theme-border bg-[var(--panel-soft)] px-3 py-2 text-[12px] text-[var(--panel-fg)]"
        >
          <Loader2 size={13} className="shrink-0 animate-spin text-[var(--accent-color)]" aria-hidden="true" />
          <span>
            {store.launchPreparing.step === "checking"
              ? t("Preparing to open {{count}} account(s): checking who is already online...", {
                  count: store.launchPreparing.userIds.length,
                })
              : store.launchPreparing.step === "server"
                ? t("Preparing to open {{count}} account(s): choosing the best server...", {
                    count: store.launchPreparing.userIds.length,
                  })
                : t("Starting the launch...")}
          </span>
        </div>
      )}

      {showStats && (
        <SessionStats
          counts={counts}
          filter={activeFilter}
          onFilter={setFilter}
          onJoining={() => joiningRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest" })}
        />
      )}

      {/* ── Entrando ───────────────────────────────────────────────────── */}
      {/* Fila vazia é só o cabeçalho: uma caixa vazia empurrava o "Em jogo"
          para baixo. */}
      <section ref={joiningRef} data-tour="session-joining" className="rounded-lg border theme-border bg-[var(--panel-soft)]">
        <header
          className={`flex items-center justify-between gap-2 px-3 ${entries.length > 0 ? "py-2 border-b theme-border" : "py-1.5"}`}
        >
          <div className="flex items-baseline gap-2 min-w-0">
            <h2 className="text-[12px] font-semibold text-[var(--panel-fg)]">{t("Joining")}</h2>
            <span className="text-[12px] theme-muted truncate">
              {pending.length > 0
                ? t("{{count}} waiting", { count: pending.length })
                : t("Nothing waiting")}
            </span>
          </div>
          {entries.length > 0 && (
            <button
              onClick={() => void handleStopQueue()}
              disabled={pending.length === 0}
              className="sidebar-btn-sm flex items-center gap-1.5 shrink-0 disabled:opacity-40 disabled:cursor-not-allowed"
              title={t("Remove every waiting account from the queue")}
            >
              <SquareStop size={13} strokeWidth={1.5} />
              {t("Stop queue")}
            </button>
          )}
        </header>

        {entries.length > 0 && (
          <ul className={`${contained ? "max-h-48 overflow-y-auto " : ""}py-1`}>
            {entries.map((entry) => {
              const style = STATE_STYLES[entry.state] ?? STATE_STYLES.queued;
              const name = nameFor(entry.userId);
              // A tentativa da reconexão automática também passa pela fila.
              const error = entry.error ? reconnectErrorText(entry.error, t) : null;
              return (
                <li
                  key={entry.userId}
                  data-testid={`session-queue-${entry.userId}`}
                  className="flex items-center gap-2 px-3 py-1.5 text-[12px]"
                >
                  <span className={`shrink-0 w-1.5 h-1.5 rounded-full ${style.dot}`} />
                  <span className="text-[var(--panel-fg)] truncate max-w-[40%]">{name}</span>
                  <span className={`shrink-0 ${style.text}`}>{stateLabel(entry.state, t)}</span>
                  {queueMixesGames && entry.placeId ? (
                    <QueueEntryGame placeId={entry.placeId} userId={entry.userId} />
                  ) : null}
                  {error && (
                    <span className="text-red-400 truncate" title={error}>
                      {error}
                    </span>
                  )}
                  <span className="ml-auto shrink-0">
                    {isPending(entry) && (
                      <button
                        onClick={() => void handleCancel(entry.userId)}
                        aria-label={t("Cancel {{name}}", { name })}
                        title={t("Take this account out of the queue (does not close any client)")}
                        className="inline-flex w-6 h-6 items-center justify-center rounded-md theme-muted hover:text-red-400 hover:bg-[var(--row-hover)] transition-colors"
                      >
                        <X size={13} strokeWidth={2} />
                      </button>
                    )}
                  </span>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {/* ── Make Friends ───────────────────────────────────────────────── */}
      {showFriendLink && friendLink && (
        <section
          data-testid="friend-link-panel"
          className="rounded-lg border theme-border bg-[var(--panel-soft)]"
        >
          <header className="flex items-center justify-between gap-2 px-3 py-2 border-b theme-border">
            <div className="flex items-baseline gap-2 min-w-0">
              <h2 className="text-[12px] font-semibold text-[var(--panel-fg)] flex items-center gap-1.5">
                <UserPlus size={13} strokeWidth={1.5} aria-hidden="true" />
                {t("Make Friends")}
              </h2>
              <span className="text-[12px] theme-muted truncate">
                {t("{{done}} / {{total}} accounts processed", {
                  done: friendLink.processed,
                  total: friendLink.total,
                })}
              </span>
            </div>
            <span className="text-[12px] theme-muted shrink-0 truncate">
              {friendPhaseLabel(friendLink.phase, t)}
            </span>
          </header>

          <ul className={`${contained ? "max-h-48 overflow-y-auto " : ""}py-1`}>
            {friendLink.accounts.map((entry) => {
              const style = FRIEND_STATE_STYLES[entry.state] ?? FRIEND_STATE_STYLES.pending;
              return (
                <li
                  key={entry.userId}
                  data-testid={`friend-link-${entry.userId}`}
                  className="flex items-center gap-2 px-3 py-1.5 text-[12px]"
                >
                  <span className={`shrink-0 w-1.5 h-1.5 rounded-full ${style.dot}`} />
                  <span className="text-[var(--panel-fg)] truncate max-w-[40%]">
                    {nameFor(entry.userId)}
                  </span>
                  {friendLink.mainUserId === entry.userId && (
                    <span className="shrink-0 theme-muted">{t("main")}</span>
                  )}
                  <span className={`shrink-0 ${style.text}`}>
                    {friendStateLabel(entry.state, t)}
                  </span>
                  {entry.error && (
                    <span className="text-red-400 truncate" title={entry.error}>
                      {entry.error}
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {/* ── Follow a user ──────────────────────────────────────────────── */}
      {showFollow && followRun && (
        <section data-testid="follow-user-panel" className="rounded-lg border theme-border bg-[var(--panel-soft)]">
          <header className="flex items-center justify-between gap-2 px-3 py-2 border-b theme-border">
            <div className="flex items-baseline gap-2 min-w-0">
              <h2 className="text-[12px] font-semibold text-[var(--panel-fg)] flex items-center gap-1.5 shrink-0">
                <UserCheck size={13} strokeWidth={1.5} aria-hidden="true" />
                {t("Follow")}
              </h2>
              <span className="text-[12px] text-[var(--panel-fg)] truncate">
                {followRun.targetUserId !== null &&
                store.accounts.some((a) => a.UserID === followRun.targetUserId)
                  ? nameFor(followRun.targetUserId)
                  : followRun.targetName
                    ? `@${followRun.targetName}`
                    : followRun.targetUserId ?? ""}
              </span>
            </div>
            <span className="text-[12px] theme-muted shrink-0">
              {t("{{done}} / {{total}} accounts processed", {
                done: followRun.processed,
                total: followRun.total,
              })}
            </span>
          </header>

          <ul className={`${contained ? "max-h-48 overflow-y-auto " : ""}py-1`}>
            {followRun.accounts.map((entry) => {
              const style = FOLLOW_STATE_STYLES[entry.state] ?? FOLLOW_STATE_STYLES.pending;
              return (
                <li
                  key={entry.userId}
                  data-testid={`follow-user-${entry.userId}`}
                  className="flex items-center gap-2 px-3 py-1.5 text-[12px]"
                >
                  <span className={`shrink-0 w-1.5 h-1.5 rounded-full ${style.dot}`} />
                  <span className="text-[var(--panel-fg)] truncate max-w-[40%]">{nameFor(entry.userId)}</span>
                  <span className={`shrink-0 ${style.text}`}>{followStateLabel(entry.state, t)}</span>
                  {entry.error && (
                    <span className="text-red-400 truncate" title={entry.error}>
                      {entry.error}
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {/* ── Reconectando ───────────────────────────────────────────────── */}
      {reconnectApart.length > 0 && (
        <section data-testid="auto-reconnect-panel" className="rounded-lg border theme-border bg-[var(--panel-soft)]">
          <header className="flex items-center gap-2 px-3 py-2 border-b theme-border min-w-0">
            <h2 className="text-[12px] font-semibold text-[var(--panel-fg)] flex items-center gap-1.5 shrink-0">
              <RotateCw size={13} strokeWidth={1.5} aria-hidden="true" />
              {t("Reconnecting")}
            </h2>
            <span className="text-[12px] theme-muted truncate">
              {t("Accounts that dropped go back to the same game on their own")}
            </span>
          </header>
          <ul className={`${contained ? "max-h-48 overflow-y-auto " : ""}py-1`}>
            {reconnectApart.map((entry) => {
              const info = autoReconnectLabel(entry, now, t);
              const name = nameFor(entry.userId);
              const pending = info.tone === "pending";
              const busy = reconnectBusy.has(entry.userId);
              const turnedOff = choiceFor(entry.userId)?.effective === false;
              return (
                <li
                  key={entry.userId}
                  data-testid={`session-reconnect-${entry.userId}`}
                  data-phase={entry.phase}
                  aria-busy={busy || undefined}
                  className="flex items-center gap-2 px-3 py-1.5 text-[12px]"
                >
                  <span
                    className={`shrink-0 w-1.5 h-1.5 rounded-full ${pending ? "bg-amber-400 animate-pulse" : "bg-red-500"}`}
                  />
                  <span className="text-[var(--panel-fg)] truncate shrink-0 max-w-[40%]">{name}</span>
                  {/* A causa (erro da última tentativa) fica à vista, embaixo. */}
                  <span className="min-w-0 flex flex-col">
                    {/* Contagem regressiva: `timer` (não fala a cada segundo). */}
                    <span
                      role={entry.phase === "waiting" ? "timer" : undefined}
                      className={`truncate ${pending ? "text-amber-400" : "text-red-400"}`}
                      title={info.label}
                    >
                      {info.label}
                    </span>
                    {info.detail && (
                      <span
                        data-testid={`session-reconnect-detail-${entry.userId}`}
                        className="truncate text-[11px] theme-muted"
                        title={info.detail}
                      >
                        {info.detail}
                      </span>
                    )}
                    {/* Desligada com a tentativa em andamento: o backend só a solta depois dela. */}
                    {pending && turnedOff && (
                      <span className="truncate text-[11px] theme-muted">
                        {t("Turned off: it stops after the attempt in progress.")}
                      </span>
                    )}
                  </span>
                  <span className="ml-auto flex items-center gap-1.5 shrink-0">
                    {(info.canTryNow || info.canTryAgain) && (
                      <button
                        onClick={() => void handleReconnectAction(entry, "retry")}
                        disabled={busy}
                        className="sidebar-btn-sm flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
                        title={t("Reopen this account now, in the same game")}
                      >
                        <RotateCw size={12} strokeWidth={1.5} className={busy ? "animate-spin" : undefined} />
                        {info.canTryNow ? t("Try now") : t("Try again")}
                      </button>
                    )}
                    {pending ? (
                      <button
                        onClick={() => void handleReconnectAction(entry, "stop")}
                        disabled={busy}
                        className="sidebar-btn-sm flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
                        title={t("Stop reconnecting this account (does not close any client)")}
                      >
                        <SquareStop size={12} strokeWidth={1.5} />
                        {t("Stop")}
                      </button>
                    ) : (
                      <button
                        onClick={() => void handleReconnectAction(entry, "dismiss")}
                        disabled={busy}
                        aria-label={t("Dismiss {{name}}", { name })}
                        title={t("Hide this line")}
                        className="inline-flex w-6 h-6 items-center justify-center rounded-md theme-muted hover:text-[var(--panel-fg)] hover:bg-[var(--row-hover)] transition-colors disabled:opacity-50"
                      >
                        <X size={13} strokeWidth={2} />
                      </button>
                    )}
                  </span>
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {/* ── Fechou (cliente que fechou no meio do jogo) ───────────────── */}
      <ClosedClientsSection
        hiddenIds={new Set([...runningIds, ...reconnecting.map((entry) => entry.userId)])}
        contained={contained}
      />

      {/* ── Em jogo ────────────────────────────────────────────────────── */}
      <section
        data-tour="session-in-game"
        data-testid="session-in-game"
        className="@container/ingame rounded-lg border theme-border bg-[var(--panel-soft)]"
      >
        <header className="flex flex-wrap items-center justify-between gap-x-2 gap-y-1 px-3 py-2 border-b theme-border">
          <div className="flex items-center gap-2 min-w-0">
            {runningIds.length > 0 && (
              <SelectBox
                label={t("Select all running clients")}
                checked={selected.length === runningIds.length}
                indeterminate={selected.length > 0}
                onChange={toggleAll}
              />
            )}
            <h2 className="text-[12px] font-semibold text-[var(--panel-fg)]">{t("In game")}</h2>
            <span className="text-[12px] theme-muted truncate">
              {t("{{count}} running", { count: runningIds.length })}
            </span>
            {unidentified.length > 0 && (
              <span className="text-[12px] text-amber-400 truncate">
                {t("{{count}} unidentified", { count: unidentified.length })}
              </span>
            )}
          </div>
          {/* Alvo do tutorial só com botões: vazio, ele cairia num ponto sem nada. */}
          <div
            data-tour={runningIds.length > 0 ? "session-in-game-actions" : undefined}
            className="flex items-center gap-1.5 shrink-0"
          >
            {runningIds.length > 0 && (
              <>
                <button
                  onClick={handleOpenAfkMode}
                  title={t(
                    "Opens AFK Mode with these accounts: Auto Rejoin keeps them in the cycle without closing the clients that are already open."
                  )}
                  className="sidebar-btn-sm flex items-center gap-1.5 shrink-0"
                >
                  <Coffee size={13} strokeWidth={1.5} />
                  {t("AFK Mode")}
                </button>
                {/* Sempre todas as da lista, com confirmação; as marcadas
                    fecham pela barra de lote ("Close selected"). */}
                <button
                  onClick={() => void confirmAndClose(runningIds)}
                  disabled={busy}
                  title={t("Closes the Roblox client of every account in this list")}
                  className="sidebar-btn-sm flex items-center gap-1.5 shrink-0 !text-red-400 disabled:opacity-40 disabled:cursor-not-allowed"
                >
                  <PowerOff size={13} strokeWidth={1.5} aria-hidden="true" />
                  {t("Close all clients ({{count}})", { count: runningIds.length })}
                </button>
              </>
            )}
          </div>
          {/* Onde a dúvida aparece: perto do "Close all". */}
          {runningIds.length > 0 && (
            <p className="basis-full text-[11.5px] leading-snug theme-muted">
              {t("Closing MultiAlt leaves every Roblox client running.")}
            </p>
          )}
        </header>

        {/* Cliente que entrou como outra conta: aviso no topo, além da linha
            da conta (commands/client_health.rs). O app não o fecha. */}
        {wrongAccountIds.length > 0 && (
          <div
            role="alert"
            data-testid="session-wrong-account"
            className="mx-3 mt-2 flex items-start gap-2 rounded-md border border-red-400/40 bg-red-400/10 px-2.5 py-2 text-[12px] text-red-400"
          >
            <UserX size={13} strokeWidth={1.75} aria-hidden="true" className="shrink-0 mt-[2px]" />
            <span className="min-w-0 flex flex-col gap-0.5">
              {wrongAccountIds.map((id) => (
                <span key={id}>
                  {t("{{name}} opened as a different Roblox account (user {{id}}).", {
                    name: nameFor(id),
                    id: store.clientHealth?.get(id)?.wrongAccountUserId ?? "",
                  })}
                </span>
              ))}
              <span className="theme-muted">{t("The client stays open. Check which account is logged in to it.")}</span>
            </span>
          </div>
        )}
        {runningIds.length === 0 && unidentified.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-2 px-3 py-6 theme-muted">
            <Gamepad2 size={20} strokeWidth={1.5} aria-hidden="true" />
            <p className="text-[12px] text-center">
              {t("No Roblox client is running. Accounts you launch show up here.")}
            </p>
            {/* Com contas marcadas, direto para a Choose Game; sem, para a
                lista de contas, onde se escolhe quem joga. */}
            <button
              type="button"
              onClick={() => {
                store.setActivePage("accounts");
                if (store.selectedIds.size > 0) store.setChooseGameOpen(true);
              }}
              className="theme-btn inline-flex items-center gap-1.5 min-h-[28px] px-3 py-1.5 text-[12px] font-medium"
            >
              <Gamepad2 size={13} strokeWidth={1.6} aria-hidden="true" />
              {t("Choose a game")}
            </button>
          </div>
        ) : (
          <div className={`${listBox} py-1`}>
            {grouped ? (
              <>
                {groups.attention.length > 0 && (
                  <div data-testid="session-attention" className="session-group">
                    <AttentionGroupHeader
                      count={groups.attention.length}
                      selection={selectionOf(groups.attention)}
                      onSelect={() => toggleGroup(groups.attention)}
                      expanded={!collapsed.has("attention")}
                      onToggle={() => toggleCollapsed("attention")}
                    />
                    {!collapsed.has("attention") && (
                      <ul className={rowGrid}>{groups.attention.map((id) => renderRow(id, "full"))}</ul>
                    )}
                  </div>
                )}
                {groups.games.map((game) => (
                  <div key={game.key} data-testid={`session-game-${game.placeId}`} className="session-group">
                    <GameGroupHeader
                      placeId={game.placeId}
                      userId={game.userIds[0]}
                      count={game.userIds.length}
                      selection={selectionOf(game.userIds)}
                      onSelect={() => toggleGroup(game.userIds)}
                      expanded={!collapsed.has(game.key)}
                      onToggle={() => toggleCollapsed(game.key)}
                    />
                    {!collapsed.has(game.key) &&
                      game.servers.map((server) => (
                        <div key={server.key} data-testid={`session-server-${server.key}`}>
                          <ServerHeader
                            server={server}
                            selection={selectionOf(server.userIds)}
                            onSelect={() => toggleGroup(server.userIds)}
                            selectable={game.servers.length > 1}
                            info={serverInfoOf(server.userIds)}
                          />
                          <ul className={rowGrid}>{server.userIds.map((id) => renderRow(id, "game"))}</ul>
                        </div>
                      ))}
                  </div>
                ))}
                {groups.noGame.length > 0 && (
                  <div data-testid="session-no-game" className="session-group">
                    <NoGameGroupHeader
                      count={groups.noGame.length}
                      selection={selectionOf(groups.noGame)}
                      onSelect={() => toggleGroup(groups.noGame)}
                      expanded={!collapsed.has("no-game")}
                      onToggle={() => toggleCollapsed("no-game")}
                    />
                    {!collapsed.has("no-game") && (
                      <ul className={rowGrid}>{groups.noGame.map((id) => renderRow(id, "full"))}</ul>
                    )}
                  </div>
                )}
              </>
            ) : (
              <ul className={rowGrid}>{shownIds.map((id) => renderRow(id, "full"))}</ul>
            )}
            <ul>
            {unidentified.map((client) => {
              const namedAccount =
                client.userId !== null
                  ? store.accounts.find((a) => a.UserID === client.userId)
                  : undefined;
              const namedLabel =
                client.userId !== null ? accountLabel(namedAccount, store, client.userId) : "";
              const picking = identifying?.pid === client.pid ? identifying : null;
              return (
                <li
                  key={`pid-${client.pid}`}
                  data-testid={`session-unidentified-${client.pid}`}
                  data-tour="session-unidentified"
                  className="flex flex-col gap-1.5 px-3 py-1.5 text-[12px]"
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <span aria-hidden="true" className="shrink-0 w-1.5 h-1.5 rounded-full bg-amber-400" />
                    {/* Em palavras simples: o que é. O PID (para quem procura no
                        Gerenciador de Tarefas) fica no tooltip. */}
                    <span className="text-[var(--panel-fg)] shrink-0" title={t("PID {{pid}}", { pid: client.pid })}>
                      {t("Roblox open, account unknown")}
                    </span>
                    <span className="theme-muted truncate" title={unidentifiedReasonLabel(client, namedLabel, t)}>
                      {unidentifiedReasonLabel(client, namedLabel, t)}
                    </span>
                    <span className="ml-auto flex items-center gap-1.5 shrink-0">
                      <button
                        onClick={() => void handleShowWindow(client.pid)}
                        title={t("Bring this client's window to the front")}
                        className="sidebar-btn-sm flex items-center gap-1.5"
                      >
                        <AppWindow size={12} strokeWidth={1.5} />
                        {t("Show window")}
                      </button>
                      {!picking && (
                        <button
                          onClick={() => startIdentify(client)}
                          title={t("Tell the app which account is playing in this client")}
                          className="sidebar-btn-sm flex items-center gap-1.5"
                        >
                          <UserCheck size={12} strokeWidth={1.5} aria-hidden="true" />
                          {t("Identify")}
                        </button>
                      )}
                    </span>
                  </div>
                  {!picking && (
                    <p className="pl-3.5 text-[11.5px] leading-snug theme-muted">
                      {t("Opened outside MultiAlt, for example from the website. Click Identify and pick the account playing in it to manage it here.")}
                    </p>
                  )}
                  {picking && (
                    <div className="flex items-center gap-1.5 pl-3.5">
                      <select
                        aria-label={t("Account for this client")}
                        value={picking.userId}
                        onChange={(e) => setIdentifying({ pid: client.pid, userId: e.target.value })}
                        className="min-w-0 flex-1 rounded-md border theme-border bg-[var(--panel-bg)] px-2 py-1 text-[12px] text-[var(--panel-fg)]"
                      >
                        <option value="">{t("Pick an account")}</option>
                        {store.accounts.map((a) => {
                          const label = accountLabel(a, store, a.UserID);
                          return (
                            <option key={a.UserID} value={String(a.UserID)}>
                              {store.launchedByProgram.has(a.UserID)
                                ? t("{{name}} (already in game)", { name: label })
                                : label}
                            </option>
                          );
                        })}
                      </select>
                      <button
                        onClick={() => void confirmIdentify()}
                        disabled={busy || !picking.userId}
                        className="sidebar-btn-sm flex items-center gap-1.5 shrink-0 disabled:opacity-40 disabled:cursor-not-allowed"
                      >
                        <Check size={12} strokeWidth={1.5} />
                        {t("Confirm")}
                      </button>
                      <button
                        onClick={() => setIdentifying(null)}
                        aria-label={t("Cancel")}
                        className="inline-flex w-6 h-6 items-center justify-center rounded-md theme-muted hover:text-[var(--panel-fg)] transition-colors shrink-0"
                      >
                        <X size={13} strokeWidth={2} />
                      </button>
                    </div>
                  )}
                </li>
              );
            })}
            </ul>
          </div>
        )}

        {/* Lote das contas marcadas, preso ao pé do cartão: aparecer não
            empurra as linhas (a faixa de antes entrava entre o cabeçalho e a
            lista). Reconexão e teto de memória nos mesmos menus das linhas; o
            teto pula os clientes abertos pelo site. */}
        {selected.length > 0 && (
          <div
            data-testid="session-bulk-bar"
            className="sticky bottom-0 z-10 flex flex-wrap items-center gap-1.5 rounded-b-lg border-t theme-border bg-[var(--panel-bg)] px-3 py-2 text-[12px] shadow-[0_-6px_16px_rgba(0,0,0,0.18)]"
          >
            <span className="font-medium text-[var(--panel-fg)] tabular-nums">
              {selected.length === 1 ? t("1 selected") : t("{{count}} selected", { count: selected.length })}
            </span>
            <button
              type="button"
              onClick={() => setChecked(new Set())}
              className="inline-flex items-center gap-1 h-6 rounded-md px-1.5 theme-muted hover:text-[var(--panel-fg)] hover:bg-[var(--row-hover)]"
            >
              <X size={12} strokeWidth={2} aria-hidden="true" />
              {t("Clear selection")}
            </button>
            <span className="ml-auto flex flex-wrap items-center gap-1.5">
              {isWindows && (
                <ReconnectBulkChip
                  count={selected.length}
                  globalDefault={reconnectDefault}
                  disabled={reconnectSaving}
                  onChange={(value) => void saveReconnect(selected, value)}
                />
              )}
              {memoryTrim && memoryBulkIds.length > 0 && (
                <MemoryBulkChip
                  count={memoryBulkIds.length}
                  defaultMb={memoryDefaultMb}
                  disabled={memorySaving}
                  onChange={(value) => void saveMemoryLimit(memoryBulkIds, value)}
                />
              )}
              <button
                type="button"
                onClick={() => void confirmAndClose(selected)}
                disabled={busy}
                className="sidebar-btn-sm !px-2 h-6 !py-0 ml-1 flex items-center gap-1 !text-red-400 disabled:opacity-40 disabled:cursor-not-allowed"
              >
                <PowerOff size={12} strokeWidth={1.5} aria-hidden="true" />
                {t("Close selected ({{count}})", { count: selected.length })}
              </button>
            </span>
          </div>
        )}
      </section>
    </div>
  );
}
