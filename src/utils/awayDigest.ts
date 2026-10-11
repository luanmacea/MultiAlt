/**
 * "While you were away": a lista curta do que aconteceu com as contas enquanto
 * ninguém olhava a tela — quedas, reconexões, janela travada, falha de envio
 * do Modo AFK, parada de emergência, Auto Rejoin que não reabriu.
 *
 * Esses fatos já chegavam como toast, que some em segundos: quem deixa o Modo
 * AFK rodando por horas voltava ao PC sem saber o que houve. Aqui eles ficam
 * guardados na memória da sessão do app (os últimos `AWAY_DIGEST_CAP`), do
 * mais novo para o mais antigo. Nada vai para disco.
 *
 * Só o que o backend mandou: este módulo não decide se uma conta caiu, só
 * anota o evento. O texto é montado na hora de mostrar (`awayEventText`), para
 * seguir o idioma e os nomes ocultos de agora.
 */
import type { TOptions } from "i18next";
import type { AfkStatus } from "../store";
import type { AutoReconnectEntry, ClientDrop } from "../types";
import { clientHealthLabel } from "./clientHealth";
import { autoReconnectLabel } from "./autoReconnect";

type Translate = (text: string, options?: TOptions) => string;

/** Quantos eventos ficam guardados. */
export const AWAY_DIGEST_CAP = 50;
/** O mesmo evento repetido dentro desta janela é o mesmo fato, não outro. */
export const AWAY_DEDUPE_MS = 5_000;

export type AwayEventKind =
  /** O log do cliente disse que a conta caiu (`roblox-client-health` com `drop`). */
  | "dropped"
  /** A conta que tinha caído entrou num jogo de novo (`drop: null`). */
  | "backInGame"
  | "notResponding"
  | "respondingAgain"
  /** A reconexão automática relançou e a conta ficou no jogo. */
  | "reconnected"
  /** A reconexão automática desistiu ou parou (`gaveUp` / `stopped`). */
  | "reconnectEnded"
  /** Um envio do Modo AFK não chegou à conta (`lastErrorCode`). */
  | "afkSendFailed"
  /** Chacoalhar o mouse parou o Modo AFK e/ou a gravação. */
  | "emergencyStop"
  /** O Auto Rejoin não conseguiu reabrir a conta (`botting-account-cycle`). */
  | "rejoinFailed";

export interface AwayEvent {
  id: number;
  kind: AwayEventKind;
  /** `null` no que não é de uma conta (parada de emergência). */
  userId: number | null;
  atMs: number;
  drop?: ClientDrop | null;
  reconnect?: AutoReconnectEntry;
  /** Código do erro do envio do Modo AFK. */
  code?: string | null;
  /** Texto cru do erro, quando o código não tem frase própria. */
  error?: string | null;
  afk?: boolean;
  recordings?: boolean;
}

export type AwayEventInput = Omit<AwayEvent, "id">;

export interface AwayDigestState {
  /** Do mais novo para o mais antigo. */
  events: AwayEvent[];
  nextId: number;
  /** Tudo com id até aqui já foi visto (página Session aberta, cartão clicado). */
  seenThroughId: number;
}

export const EMPTY_AWAY_DIGEST: AwayDigestState = { events: [], nextId: 1, seenThroughId: 0 };

export type AwayDigestAction = { type: "add"; event: AwayEventInput } | { type: "seen" } | { type: "clear" };

/** O que torna dois eventos "o mesmo fato" (para não repetir em segundos). */
function sameFact(a: AwayEventInput, b: AwayEventInput): boolean {
  if (a.kind !== b.kind || a.userId !== b.userId) return false;
  switch (a.kind) {
    case "dropped":
      return (
        a.drop?.kind === b.drop?.kind &&
        a.drop?.reason === b.drop?.reason &&
        a.drop?.code === b.drop?.code &&
        a.drop?.message === b.drop?.message
      );
    case "reconnectEnded":
      return a.reconnect?.phase === b.reconnect?.phase && a.reconnect?.reason === b.reconnect?.reason;
    case "afkSendFailed":
      return a.code === b.code;
    case "rejoinFailed":
      return a.error === b.error;
    case "emergencyStop":
      return a.afk === b.afk && a.recordings === b.recordings;
    default:
      return true;
  }
}

export function awayDigestReducer(state: AwayDigestState, action: AwayDigestAction): AwayDigestState {
  switch (action.type) {
    case "add": {
      const incoming = action.event;
      const repeated = state.events.some(
        (e) => Math.abs(e.atMs - incoming.atMs) < AWAY_DEDUPE_MS && sameFact(e, incoming)
      );
      if (repeated) return state;
      const event: AwayEvent = { ...incoming, id: state.nextId };
      // Ordem pela hora do fato: a falha do AFK chega com a hora do envio, que
      // pode ser anterior ao último evento anotado. `sort` é estável.
      const events = [event, ...state.events]
        .sort((a, b) => b.atMs - a.atMs)
        .slice(0, AWAY_DIGEST_CAP);
      return { ...state, events, nextId: state.nextId + 1 };
    }
    case "seen": {
      const latest = state.nextId - 1;
      return state.seenThroughId === latest ? state : { ...state, seenThroughId: latest };
    }
    case "clear":
      return state.events.length === 0 && state.seenThroughId === state.nextId - 1
        ? state
        : { ...state, events: [], seenThroughId: state.nextId - 1 };
  }
}

/** Eventos guardados que chegaram depois da última olhada. */
export function awayUnreadCount(state: AwayDigestState): number {
  return state.events.filter((e) => e.id > state.seenThroughId).length;
}

/**
 * Os três formatos do `roblox-client-health` (commands/client_health.rs,
 * `emit_health_notice`): `{drop}` caiu, `{drop: null}` voltou a um jogo,
 * `{notResponding}` travou ou voltou a responder.
 */
export function clientHealthEvent(
  payload: { userId?: number; drop?: ClientDrop | null; notResponding?: boolean },
  nowMs: number
): AwayEventInput | null {
  const userId = payload?.userId;
  if (typeof userId !== "number") return null;
  if (typeof payload.notResponding === "boolean") {
    return { kind: payload.notResponding ? "notResponding" : "respondingAgain", userId, atMs: nowMs };
  }
  if (payload.drop) return { kind: "dropped", userId, atMs: nowMs, drop: payload.drop };
  if ("drop" in payload) return { kind: "backInGame", userId, atMs: nowMs };
  return null;
}

/**
 * As falhas de envio novas num retrato do Modo AFK. Uma falha que se repete a
 * cada ciclo (a janela continua fechada) é anotada uma vez: só quando começa
 * ou muda de motivo. `next` é o retrato a guardar para a próxima comparação.
 */
export function afkFailureEvents(
  prev: ReadonlyMap<number, string | null>,
  status: AfkStatus | null
): { events: AwayEventInput[]; next: Map<number, string | null> } {
  const next = new Map<number, string | null>();
  const events: AwayEventInput[] = [];
  if (!status?.active) return { events, next };
  for (const account of status.accounts ?? []) {
    const failure = account.lastErrorCode ?? (account.lastError ? "internal" : null);
    next.set(account.userId, failure);
    if (failure && prev.get(account.userId) !== failure) {
      events.push({
        kind: "afkSendFailed",
        userId: account.userId,
        atMs: account.lastSendAtMs,
        code: account.lastErrorCode,
        error: account.lastError,
      });
    }
  }
  return { events, next };
}

function afkFailureText(event: AwayEvent, t: Translate): string {
  switch (event.code) {
    case "noWindow":
      return t("AFK send failed: no Roblox window open");
    case "focusDenied":
      return t("AFK send failed: the window did not come to the front");
    case "keyRefused":
      return t("AFK send failed: Windows refused the key");
    case "clickRefused":
      return t("AFK send failed: Windows refused the click");
    default:
      return event.error ? t("AFK send failed: {{error}}", { error: event.error }) : t("AFK send failed");
  }
}

export interface AwayEventText {
  text: string;
  /** `problem`: âmbar; `recovered`: verde. */
  tone: "problem" | "recovered";
}

export function awayEventText(event: AwayEvent, t: Translate): AwayEventText {
  switch (event.kind) {
    case "dropped": {
      const label = clientHealthLabel({ pid: 0, logFound: true, drop: event.drop ?? null }, t);
      return { text: label?.label ?? t("Disconnected from the game"), tone: "problem" };
    }
    case "backInGame":
      return { text: t("Joined a game again"), tone: "recovered" };
    case "notResponding":
      return { text: t("Not responding"), tone: "problem" };
    case "respondingAgain":
      return { text: t("Responding again"), tone: "recovered" };
    case "reconnected":
      return { text: t("Reconnected and stayed in the game"), tone: "recovered" };
    case "reconnectEnded":
      return {
        text: event.reconnect ? autoReconnectLabel(event.reconnect, event.atMs, t).label : t("Not reconnecting"),
        tone: "problem",
      };
    case "afkSendFailed":
      return { text: afkFailureText(event, t), tone: "problem" };
    case "emergencyStop":
      // As mesmas frases do alerta (EmergencyStopAlert).
      return {
        text:
          event.afk && event.recordings
            ? t("You shook the mouse, so MultiAlt stopped AFK mode and the recording.")
            : event.recordings
              ? t("You shook the mouse, so MultiAlt stopped the recording.")
              : t("You shook the mouse, so MultiAlt stopped AFK mode."),
        tone: "problem",
      };
    case "rejoinFailed":
      return {
        text: event.error
          ? t("Auto Rejoin could not reopen it: {{error}}", { error: event.error })
          : t("Auto Rejoin could not reopen it"),
        tone: "problem",
      };
  }
}

/** "14:32" na hora local, como o relógio do Windows. */
export function awayEventTime(atMs: number): string {
  return new Date(atMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
}
