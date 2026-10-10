/**
 * Regras puras do histórico de sessões (ideia 6): duração, tempo de jogo por
 * jogo, quando "Join again" faz sentido, o texto do fim e o CSV. Ver
 * docs/features/history.md.
 */
import type { TOptions } from "i18next";
import type { Account, SessionRecord } from "../types";
import { clientHealthLabel } from "./clientHealth";
import { accountLabel, type NameMasking } from "./accountName";

type Translate = (text: string, options?: TOptions) => string;

const DAY_MS = 86_400_000;
/** Até quando um servidor que a conta deixou ainda pode existir. */
export const JOIN_AGAIN_WINDOW_MS = 3 * 60 * 60 * 1000;

/** Fim da sessão para contar tempo: o registrado, agora (em jogo) ou não se sabe. */
export function sessionEndMs(session: SessionRecord, now: number): number | null {
  if (session.endedAt !== null) return session.endedAt;
  return session.end === "ongoing" ? now : null;
}

export function sessionDurationMs(session: SessionRecord, now: number): number | null {
  if (session.end === "moderated") return null;
  const end = sessionEndMs(session, now);
  return end === null ? null : Math.max(0, end - session.startedAt);
}

/** Tempo de jogo por place nos últimos `days` dias, do maior para o menor. */
export function playtimeByGame(
  sessions: SessionRecord[],
  now: number,
  days = 14
): { placeId: number; ms: number }[] {
  const from = now - days * DAY_MS;
  const totals = new Map<number, number>();
  for (const s of sessions) {
    if (s.placeId === null || s.end === "moderated") continue;
    const end = sessionEndMs(s, now);
    if (end === null) continue;
    const ms = Math.min(end, now) - Math.max(s.startedAt, from);
    if (ms <= 0) continue;
    totals.set(s.placeId, (totals.get(s.placeId) ?? 0) + ms);
  }
  return [...totals.entries()].map(([placeId, ms]) => ({ placeId, ms })).sort((a, b) => b.ms - a.ms);
}

/**
 * O servidor ainda pode existir? Só servidor conhecido, sessão que acabou há
 * pouco (`JOIN_AGAIN_WINDOW_MS`) e que não terminou com o servidor fechando.
 */
export function canJoinAgain(session: SessionRecord, now: number): boolean {
  if (!session.jobId || !session.placeId) return false;
  if (session.end === "ongoing" || session.end === "moderated") return false;
  if (session.end === "dropped" && session.dropKind === "serverShutdown") return false;
  if (session.endedAt === null) return false;
  return now - session.endedAt <= JOIN_AGAIN_WINDOW_MS;
}

/** "45s", "12m", "1h 05m". */
export function formatDuration(ms: number): string {
  const totalMinutes = Math.floor(ms / 60_000);
  if (totalMinutes < 1) return `${Math.max(0, Math.round(ms / 1000))}s`;
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return hours > 0 ? `${hours}h ${String(minutes).padStart(2, "0")}m` : `${minutes}m`;
}

export interface SessionEndLabel {
  label: string;
  detail: string;
  tone: "playing" | "drop" | "muted";
}

/** Como a sessão terminou, nas mesmas palavras da Sessão para as quedas. */
export function sessionEndLabel(session: SessionRecord, t: Translate): SessionEndLabel {
  switch (session.end) {
    case "ongoing":
      return { label: t("Playing now"), detail: "", tone: "playing" };
    case "left":
      return { label: t("Left the game"), detail: "", tone: "muted" };
    case "teleported":
      return { label: t("Moved to another server"), detail: "", tone: "muted" };
    case "closed":
      return { label: t("Client closed"), detail: "", tone: "muted" };
    case "appClosed":
      return { label: t("MultiAlt was closed"), detail: "", tone: "muted" };
    case "unknown":
      return { label: t("No end recorded"), detail: "", tone: "muted" };
    case "moderated":
      return { label: t("Roblox reported this account as moderated"), detail: "", tone: "drop" };
    case "dropped": {
      const health = clientHealthLabel(
        {
          pid: 0,
          logFound: true,
          drop: session.dropKind
            ? {
                kind: session.dropKind,
                reason: session.reason,
                code: session.code,
                message: session.message,
                sinceMs: session.endedAt ?? 0,
              }
            : null,
        },
        t
      );
      return { label: health?.label ?? t("Disconnected from the game"), detail: health?.detail ?? "", tone: "drop" };
    }
  }
}

/**
 * Com os nomes ocultos, o nome da conta não pode aparecer nem dentro da
 * mensagem de kick do jogo ("You were kicked, BobAlt").
 */
export function maskNamesInText(text: string, account: Pick<Account, "Username" | "Alias">, masking: NameMasking): string {
  if (!masking.hideUsernames) return text;
  const masked = accountLabel(account, masking);
  let out = text;
  for (const name of [account.Alias, account.Username]) {
    if (!name?.trim()) continue;
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    out = out.replace(new RegExp(escaped, "gi"), masked);
  }
  return out;
}

/**
 * Uma célula de CSV segura: texto que começa com `=`, `+`, `-`, `@` (ou tab/CR)
 * ganha um `'` na frente, para a planilha não executar como fórmula; aspas,
 * vírgula, ponto e vírgula e quebra de linha fazem a célula ir entre aspas.
 */
export function csvCell(value: unknown): string {
  let text = value === null || value === undefined ? "" : String(value);
  if (/^[=+\-@\t\r]/.test(text)) text = `'${text}`;
  if (/[",;\n\r]/.test(text)) text = `"${text.replace(/"/g, '""')}"`;
  return text;
}

function localStamp(ms: number | null): string {
  if (ms === null) return "";
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** O CSV das sessões de uma conta. Sem nome de conta e sem nada de sessão. */
export function historyCsv(
  sessions: SessionRecord[],
  gameName: (placeId: number) => string | null,
  t: Translate,
  now: number,
  maskText: (text: string) => string = (x) => x
): string {
  const header = [
    t("Start time"),
    t("End time"),
    t("Minutes"),
    t("Game"),
    t("Place ID"),
    t("Server (Job ID)"),
    t("How it ended"),
    t("Code"),
    t("Message"),
  ];
  const rows = sessions.map((s) => {
    const duration = sessionDurationMs(s, now);
    const end = sessionEndLabel(s, t);
    return [
      localStamp(s.startedAt),
      localStamp(s.endedAt),
      duration === null ? "" : Math.round(duration / 60_000),
      s.placeId === null ? "" : (gameName(s.placeId) ?? ""),
      s.placeId ?? "",
      s.jobId ?? "",
      maskText(end.label),
      s.code ?? "",
      s.message ? maskText(s.message) : "",
    ];
  });
  return [header, ...rows].map((row) => row.map(csvCell).join(",")).join("\r\n") + "\r\n";
}
