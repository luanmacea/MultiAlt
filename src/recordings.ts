/**
 * Gravações: sequências de passos (tecla, clique num ponto relativo da janela,
 * espera) que o app toca na janela do Roblox de uma conta, uma janela por vez.
 * Espelho do `data/recordings.rs` — os limites aqui são os mesmos de lá, e o
 * backend normaliza de novo ao gravar. Ver docs/features/recordings.md.
 */

export type RecordingStep =
  | { type: "key"; key: string; holdMs: number }
  | { type: "keyDown"; key: string }
  | { type: "keyUp"; key: string }
  | { type: "click"; xPct: number; yPct: number }
  | { type: "wait"; ms: number };

export type RecordingStepType = RecordingStep["type"];

export interface Recording {
  id: string;
  name: string;
  steps: RecordingStep[];
  createdAt: number;
  updatedAt: number;
  /** Largura ÷ altura da janela em que foi gravada (importação do TinyTask). */
  sourceAspect?: number;
}

// ── importar do TinyTask (data/tinytask.rs) ─────────────────────────────────

/** Área interna de uma janela, em pixels de tela. */
export interface TinyTaskArea {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** O que a importação deixou de fora ou mudou. */
export interface TinyTaskSummary {
  events: number;
  skippedKeys: { key: string; count: number }[];
  clicksOutside: number;
  otherMouse: number;
  cappedWaits: number;
  drags: number;
  truncated: boolean;
  stopKey: string | null;
}

/** O que `import_tinytask_recording` devolve. */
export interface TinyTaskImport {
  steps: RecordingStep[];
  summary: TinyTaskSummary;
  sourceAspect: number;
}

/** Diferença de proporção a partir da qual a tela avisa. */
export const ASPECT_TOLERANCE = 0.05;

/** O nome da gravação a partir do arquivo: sem pasta, sem `.rec`, no limite. */
export function recordingNameFromFile(fileName: string): string {
  const base = fileName.split(/[\\/]/).pop() ?? "";
  const name = base.replace(/\.rec$/i, "").trim();
  return [...name].slice(0, MAX_RECORDING_NAME_CHARS).join("");
}

/**
 * A janela `width`×`height` tem outro formato que a da gravação? Os cliques
 * são porcentagens da janela, então em outro formato eles caem em outro ponto
 * do jogo. Sem proporção salva, ou sem janela, não há o que comparar.
 */
export function aspectDiffers(sourceAspect: number | undefined, width: number, height: number): boolean {
  if (!sourceAspect || !(width > 0) || !(height > 0)) return false;
  return Math.abs(width / height - sourceAspect) / sourceAspect > ASPECT_TOLERANCE;
}

/**
 * As vagas de gravação (pedido do dono, 11/10/2026): a que toca depois da
 * reconexão automática (posicionar o personagem), a que toca depois de um
 * launch do app (ideia 52, 15/10/2026 — normal, preset, Auto Rejoin) e a que o
 * Modo AFK repete (uma ação em outro lugar). A ordem é a das colunas da tela:
 * "After launch" fica ao lado de "After auto-reconnect".
 */
export const RECORDING_SLOTS = ["afterReconnect", "afterLaunch", "antiAfk"] as const;
export type RecordingSlot = (typeof RECORDING_SLOTS)[number];

/** Quem toca o quê numa vaga. */
export interface RecordingSlotChoices {
  /** A gravação global, que toda conta herda (`null` = nenhuma). */
  defaultId: string | null;
  /** user id (texto) → a própria da conta, ou `null` = nenhuma para ela. Fora do mapa = herda a global. */
  accounts: Record<string, string | null>;
}

/** O que `get_recordings` devolve. */
export interface RecordingsPayload {
  recordings: Recording[];
  afterReconnect: RecordingSlotChoices;
  /** Opcional: dublê de antes da vaga nova não manda (`slotChoices` trata como vazia). */
  afterLaunch?: RecordingSlotChoices;
  antiAfk: RecordingSlotChoices;
  /** A lista fechada de teclas, na ordem do backend. */
  keys: string[];
}

/** A escolha de uma conta numa vaga — o mesmo formato de `set_account_recording`. */
export type AccountSlotChoice = { kind: "inherit" } | { kind: "none" } | { kind: "recording"; id: string };

/** Resultado do "Tocar agora", por conta. */
export interface RecordingPlayResult {
  userId: number;
  errorCode: string | null;
  error: string | null;
}

export const MAX_RECORDING_STEPS = 500;
export const MAX_RECORDING_NAME_CHARS = 60;
export const MAX_WAIT_MS = 600_000;
export const MIN_HOLD_MS = 10;
export const MAX_HOLD_MS = 10_000;
export const DEFAULT_HOLD_MS = 40;
export const MAX_RECORDING_TOTAL_MS = 600_000;
/** O mesmo `CLICK_ESTIMATE_MS` do backend. */
export const CLICK_ESTIMATE_MS = 800;

/** Um passo novo, com valores que já podem ser tocados. */
export function newStep(type: RecordingStepType, firstKey = "Space"): RecordingStep {
  switch (type) {
    case "key":
      return { type, key: firstKey, holdMs: DEFAULT_HOLD_MS };
    case "keyDown":
    case "keyUp":
      return { type, key: firstKey };
    case "click":
      return { type, xPct: 50, yPct: 50 };
    case "wait":
      return { type, ms: 500 };
  }
}

/** Troca o tipo do passo, mantendo a tecla quando o novo tipo também tem. */
export function changeStepType(step: RecordingStep, type: RecordingStepType, firstKey = "Space"): RecordingStep {
  const key = "key" in step ? step.key : firstKey;
  return newStep(type, key);
}

/** Duração estimada (esperas + teclas seguradas + cliques), como o backend conta. */
export function recordingDurationMs(steps: RecordingStep[]): number {
  return steps.reduce((total, step) => {
    switch (step.type) {
      case "key":
        return total + step.holdMs;
      case "wait":
        return total + step.ms;
      case "click":
        return total + CLICK_ESTIMATE_MS;
      default:
        return total;
    }
  }, 0);
}

/** `1.2 s`, `45 s`, `3 min 5 s`. */
export function formatDurationMs(ms: number): string {
  if (ms < 10_000) return `${(Math.round(ms / 100) / 10).toFixed(1)} s`;
  const total = Math.round(ms / 1000);
  if (total < 60) return `${total} s`;
  const m = Math.floor(total / 60);
  const s = total % 60;
  return s ? `${m} min ${s} s` : `${m} min`;
}

/** O primeiro problema da gravação, como código; `null` quando pode ser salva. */
export type RecordingProblem = "noName" | "nameTooLong" | "tooManySteps" | "tooLong" | "badKey";

export function recordingProblem(name: string, steps: RecordingStep[], keys: string[]): RecordingProblem | null {
  const trimmed = name.trim();
  if (!trimmed) return "noName";
  if ([...trimmed].length > MAX_RECORDING_NAME_CHARS) return "nameTooLong";
  if (steps.length > MAX_RECORDING_STEPS) return "tooManySteps";
  if (steps.some((s) => "key" in s && !keys.includes(s.key))) return "badKey";
  if (recordingDurationMs(steps) > MAX_RECORDING_TOTAL_MS) return "tooLong";
  return null;
}

/** Inteiro dentro dos limites; o que não é número vira `fallback`. */
export function clampInt(value: number, min: number, max: number, fallback: number): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, Math.round(value)));
}

/** As escolhas de uma vaga; payload sem a vaga (dublê antigo) = nada escolhido. */
export function slotChoices(payload: RecordingsPayload | null, slot: RecordingSlot): RecordingSlotChoices {
  const choices = payload?.[slot];
  return { defaultId: choices?.defaultId ?? null, accounts: choices?.accounts ?? {} };
}

function findRecording(payload: RecordingsPayload | null, id: string | null | undefined): Recording | null {
  return id && payload ? payload.recordings.find((r) => r.id === id) ?? null : null;
}

/** A gravação global da vaga, se existe. */
export function globalRecording(payload: RecordingsPayload | null, slot: RecordingSlot): Recording | null {
  return findRecording(payload, slotChoices(payload, slot).defaultId);
}

/**
 * O que a conta escolheu na vaga. Escolha que aponta para gravação apagada
 * conta como herdar — é o que o backend toca (`recording_for_account`).
 */
export function accountSlotChoice(payload: RecordingsPayload | null, slot: RecordingSlot, userId: number): AccountSlotChoice {
  const accounts = slotChoices(payload, slot).accounts;
  const key = String(userId);
  if (!(key in accounts)) return { kind: "inherit" };
  const id = accounts[key];
  if (id === null) return { kind: "none" };
  return findRecording(payload, id) ? { kind: "recording", id } : { kind: "inherit" };
}

/** A gravação que vale para a conta na vaga: a escolha dela (a própria ou nenhuma), senão a global. */
export function recordingForAccount(payload: RecordingsPayload | null, userId: number, slot: RecordingSlot): Recording | null {
  const choice = accountSlotChoice(payload, slot, userId);
  if (choice.kind === "none") return null;
  if (choice.kind === "recording") return findRecording(payload, choice.id);
  return globalRecording(payload, slot);
}

/** Contas com alguma escolha própria (inclusive "nenhuma") em qualquer vaga. */
export function accountsWithOwnChoice(payload: RecordingsPayload | null): Set<number> {
  const out = new Set<number>();
  for (const slot of RECORDING_SLOTS) {
    for (const key of Object.keys(slotChoices(payload, slot).accounts)) {
      if (accountSlotChoice(payload, slot, Number(key)).kind !== "inherit") out.add(Number(key));
    }
  }
  return out;
}

/** Para quem a gravação toca numa vaga: é a global, e quantas contas a têm como própria. */
export function slotUsage(
  payload: RecordingsPayload | null,
  slot: RecordingSlot,
  recordingId: string
): { global: boolean; ownCount: number } {
  const choices = slotChoices(payload, slot);
  return {
    global: choices.defaultId === recordingId,
    ownCount: Object.values(choices.accounts).filter((id) => id === recordingId).length,
  };
}

/** Move o passo `index` uma posição para cima (`-1`) ou para baixo (`+1`). */
export function moveStep(steps: RecordingStep[], index: number, delta: -1 | 1): RecordingStep[] {
  const target = index + delta;
  if (index < 0 || index >= steps.length || target < 0 || target >= steps.length) return steps;
  const next = [...steps];
  [next[index], next[target]] = [next[target], next[index]];
  return next;
}
