/**
 * Recado do app para ele mesmo entre a instalação de uma atualização e a volta.
 *
 * A instalação roda em silêncio (`installMode: quiet`): o app fecha, o
 * instalador troca os arquivos sem janela e abre o app de novo. Sem janela, nem
 * o sucesso nem a falha aparecem — então, logo antes de instalar, o app anota de
 * qual versão saiu e para qual vai; ao abrir, lê a anotação uma vez:
 *
 * - voltou na versão nova → "Atualizado para vX";
 * - voltou na versão antiga → a instalação não terminou (o log fica na pasta de
 *   dados, `RAMUpdateInstall.log`).
 *
 * `localStorage` é uma das exceções permitidas ao "frontend só fala com o
 * backend" (docs/architecture.md). Todo acesso aguenta o armazenamento recusar.
 */

export const UPDATE_HANDOFF_KEY = "ram.updateHandoff";

/** Anotação mais velha que isto é de uma tentativa esquecida, não desta abertura. */
export const UPDATE_HANDOFF_MAX_AGE_MS = 24 * 60 * 60 * 1000;

interface UpdateHandoff {
  from: string;
  to: string;
  at: number;
}

export type UpdateHandoffResult =
  | { kind: "updated"; version: string }
  | { kind: "failed"; version: string }
  | null;

type HandoffStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;

export function writeUpdateHandoff(storage: HandoffStorage, from: string, to: string, now: number): void {
  const handoff: UpdateHandoff = { from, to, at: now };
  try {
    storage.setItem(UPDATE_HANDOFF_KEY, JSON.stringify(handoff));
  } catch {
    // Sem armazenamento a atualização segue; só não há aviso na volta.
  }
}

function parseHandoff(raw: string | null): UpdateHandoff | null {
  if (!raw) return null;
  try {
    const value: unknown = JSON.parse(raw);
    if (!value || typeof value !== "object") return null;
    const { from, to, at } = value as Record<string, unknown>;
    if (typeof from !== "string" || typeof to !== "string" || typeof at !== "number") return null;
    return { from, to, at };
  } catch {
    return null;
  }
}

/** Lê e apaga a anotação: cada atualização é anunciada uma vez só. */
export function consumeUpdateHandoff(
  storage: HandoffStorage,
  currentVersion: string,
  now: number
): UpdateHandoffResult {
  let raw: string | null;
  try {
    raw = storage.getItem(UPDATE_HANDOFF_KEY);
    storage.removeItem(UPDATE_HANDOFF_KEY);
  } catch {
    return null;
  }

  const handoff = parseHandoff(raw);
  if (!handoff) return null;
  if (now - handoff.at > UPDATE_HANDOFF_MAX_AGE_MS) return null;
  if (currentVersion === handoff.to) return { kind: "updated", version: handoff.to };
  if (currentVersion === handoff.from) return { kind: "failed", version: handoff.to };
  return null;
}
