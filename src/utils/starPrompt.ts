/**
 * Pedido de estrela no GitHub (pedido do dono, 15/10/2026): o projeto é open
 * source e grátis, e a estrela ajuda a continuar tendo update.
 *
 * Regra para não virar chateação:
 * - aparece já na primeira abertura (pedido do dono, 15/10/2026: "faz aparecer
 *   de cara"; antes esperava 3 dias de uso). O mínimo de dias continua
 *   configurável em `STAR_PROMPT_MIN_DAYS`;
 * - "Agora não" adia `STAR_PROMPT_SNOOZE_DAYS` dias;
 * - "Dar estrela" ou o × encerram para sempre.
 *
 * O estado mora no `localStorage` (preferência de quem está na máquina, como a
 * barra recolhida — ver docs/architecture.md, "Exceções à regra"). Sem
 * `localStorage` (bloqueado, modo privado) o cartão simplesmente não aparece.
 */
export const STAR_PROMPT_KEY = "ram_star_prompt";
export const STAR_PROMPT_MIN_DAYS = 1;
export const STAR_PROMPT_SNOOZE_DAYS = 14;

const DAY_MS = 24 * 60 * 60 * 1000;

export interface StarPromptState {
  /** Dias (AAAA-MM-DD, horário local) em que o app foi aberto, até o mínimo. */
  days: string[];
  /** Adiado até este instante (ms). */
  snoozeUntil?: number;
  /** Deu a estrela ou dispensou: nunca mais aparece. */
  done?: boolean;
}

export function localDay(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Conta o dia de hoje (uma vez por dia). Guarda só o necessário para a regra. */
export function recordUseDay(state: StarPromptState, today: string): StarPromptState {
  if (state.done || state.days.includes(today) || state.days.length >= STAR_PROMPT_MIN_DAYS) return state;
  return { ...state, days: [...state.days, today] };
}

export function shouldShowStarPrompt(state: StarPromptState, nowMs: number): boolean {
  if (state.done) return false;
  if (state.days.length < STAR_PROMPT_MIN_DAYS) return false;
  return !state.snoozeUntil || nowMs >= state.snoozeUntil;
}

export function snoozeStarPrompt(state: StarPromptState, nowMs: number): StarPromptState {
  return { ...state, snoozeUntil: nowMs + STAR_PROMPT_SNOOZE_DAYS * DAY_MS };
}

export function finishStarPrompt(state: StarPromptState): StarPromptState {
  return { ...state, done: true };
}

/** Lê o estado; qualquer coisa estranha vira o estado inicial. */
export function parseStarPrompt(raw: string | null): StarPromptState {
  try {
    const value = raw ? (JSON.parse(raw) as Partial<StarPromptState>) : null;
    if (!value || typeof value !== "object") return { days: [] };
    return {
      days: Array.isArray(value.days) ? value.days.filter((d): d is string => typeof d === "string") : [],
      snoozeUntil: typeof value.snoozeUntil === "number" ? value.snoozeUntil : undefined,
      done: value.done === true ? true : undefined,
    };
  } catch {
    return { days: [] };
  }
}

export function loadStarPrompt(): StarPromptState | null {
  try {
    return parseStarPrompt(window.localStorage.getItem(STAR_PROMPT_KEY));
  } catch {
    return null;
  }
}

export function saveStarPrompt(state: StarPromptState): void {
  try {
    window.localStorage.setItem(STAR_PROMPT_KEY, JSON.stringify(state));
  } catch {
    // Sem armazenamento, o pedido só não volta a aparecer.
  }
}
