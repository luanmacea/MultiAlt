/**
 * Peças puras da aba Auto Rejoin do Modo AFK (antes no `BottingDialog`):
 * formatação dos relógios, cor da fase e quais ações cabem em cada linha.
 */

export type BottingRowAction =
  | "disconnect"
  | "close"
  | "closeDisconnect"
  | "restartClient"
  | "restartLoop";

export const BOTTING_ROW_ACTIONS: BottingRowAction[] = [
  "disconnect",
  "close",
  "closeDisconnect",
  "restartClient",
  "restartLoop",
];

export function canRunBottingActionOnRow(
  row: { disconnected?: boolean; isPlayer?: boolean } | null,
  action: BottingRowAction
): boolean {
  if (!row) return false;
  if (action === "disconnect" || action === "closeDisconnect") {
    return !row.disconnected && !row.isPlayer;
  }
  if (action === "restartClient") {
    return !row.disconnected;
  }
  return true;
}

export function formatCountdown(targetMs: number | null, nowMs: number): string {
  if (targetMs === null) return "--";
  if (targetMs <= nowMs) return "due";
  const secs = Math.ceil((targetMs - nowMs) / 1000);
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  if (h > 0) {
    return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${m}:${String(s).padStart(2, "0")}`;
}

export function formatDueTime(targetMs: number | null): string {
  if (targetMs === null) return "--";
  return new Date(targetMs).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

export function phaseTone(phase: string): string {
  const p = phase.toLowerCase();
  if (p.includes("disconnected")) return "text-zinc-300";
  if (p.includes("player")) return "text-sky-300";
  if (p.includes("retry") || p.includes("backoff")) return "text-amber-300";
  if (p.includes("error")) return "text-red-300";
  if (p.includes("launch") || p.includes("restart")) return "text-violet-300";
  if (p.includes("wait")) return "text-cyan-300";
  return "text-emerald-300";
}
