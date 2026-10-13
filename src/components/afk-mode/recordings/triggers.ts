/**
 * Os dois gatilhos das gravações — um por vaga —, lidos do INI e da
 * biblioteca: "On anti-AFK" é o **Modo AFK** no modo gravação (`Afk.Mode = recording`, a cada `Afk.IntervalMinutes` +
 * `Afk.IntervalSeconds`) e "After auto-reconnect" é **depois da reconexão** (`Recordings.AfterReconnect`,
 * `Recordings.AfterReconnectDelaySeconds`). Um resumo só, mostrado na aba
 * Recordings, no cartão da aba no Modo AFK e no resumo da página Session — para
 * a pessoa não ter de caçar onde cada coisa liga.
 */
import { parseAfkMode, readAfkInterval } from "../clicks/useClicksController";
import { accountsWithOwnChoice, globalRecording, type RecordingSlot, type RecordingsPayload } from "../../../recordings";

export interface RecordingTriggers {
  /** O Modo AFK, quando ligado, toca a gravação de cada conta. */
  afkRepeats: boolean;
  /** Intervalo do Modo AFK, em segundos. */
  intervalSeconds: number;
  /** A conta que a reconexão devolveu toca a gravação dela. */
  afterReconnect: boolean;
  /** Tempo no jogo antes de tocar depois da reconexão. */
  delaySeconds: number;
  /** A gravação global de cada vaga (`null` = nenhuma). */
  globalNames: Record<RecordingSlot, string | null>;
  /** Contas com escolha própria (uma gravação ou "nenhuma") em alguma vaga. */
  ownCount: number;
}

/** O clamp do backend (`Recordings.AfterReconnectDelaySeconds`). */
const MIN_DELAY_S = 5;
const MAX_DELAY_S = 3_600;

export function recordingTriggers(
  payload: RecordingsPayload | null,
  settings: Record<string, Record<string, string>> | null | undefined
): RecordingTriggers {
  const afk = settings?.Afk;
  const rec = settings?.Recordings ?? {};
  const interval = readAfkInterval(afk);
  const delay = Number.parseInt(rec.AfterReconnectDelaySeconds ?? "", 10);
  return {
    afkRepeats: parseAfkMode(afk?.Mode) === "recording",
    intervalSeconds: interval.minutes * 60 + interval.seconds,
    afterReconnect: rec.AfterReconnect === "true",
    delaySeconds: Number.isFinite(delay) ? Math.min(MAX_DELAY_S, Math.max(MIN_DELAY_S, delay)) : 30,
    globalNames: {
      afterReconnect: globalRecording(payload, "afterReconnect")?.name ?? null,
      antiAfk: globalRecording(payload, "antiAfk")?.name ?? null,
    },
    ownCount: accountsWithOwnChoice(payload).size,
  };
}

/** "10 min", "2 min 30 s", "45 s". */
export function formatAfkIntervalSeconds(total: number): string {
  const m = Math.floor(total / 60);
  const s = total % 60;
  if (m === 0) return `${s} s`;
  return s ? `${m} min ${s} s` : `${m} min`;
}
