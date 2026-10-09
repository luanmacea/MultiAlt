import type { ModerationStatus } from "../types";

type Translate = (text: string, options?: Record<string, unknown>) => string;

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

/** `dd/mm` no horário local (o formato pedido para "Banned until dd/mm"). */
export function formatDayMonth(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return `${pad(d.getDate())}/${pad(d.getMonth() + 1)}`;
}

/** Ban que ainda vale (sem data de fim, vale). Encerrada sempre vale. */
export function isActiveBan(status: ModerationStatus, now: Date = new Date()): boolean {
  if (status.state === "terminated") return true;
  if (status.state !== "banned") return false;
  if (!status.until) return true;
  const end = new Date(status.until);
  return Number.isNaN(end.getTime()) || end.getTime() > now.getTime();
}

/** Uma linha curta para a conta: "Banned until 12/10", "Warned", "Terminated". */
export function moderationLabel(status: ModerationStatus, t: Translate, now: Date = new Date()): string {
  switch (status.state) {
    case "terminated":
      return t("Terminated");
    case "warned":
      return t("Warned");
    case "banned": {
      if (!status.until) return t("Banned");
      const day = formatDayMonth(status.until);
      return isActiveBan(status, now)
        ? t("Banned until {{date}}", { date: day })
        : t("Ban ended {{date}} — reactivate it on roblox.com", { date: day });
    }
    default:
      return t("No moderation");
  }
}

/** Selo da linha: banida/encerrada = `banned`; advertida (ou ban vencido) = `warned`. */
export function moderationBadge(
  status: ModerationStatus | undefined,
  now: Date = new Date()
): "banned" | "warned" | null {
  if (!status || status.state === "clean") return null;
  if (isActiveBan(status, now)) return "banned";
  return "warned";
}
