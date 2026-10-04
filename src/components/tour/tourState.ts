import { useSyncExternalStore } from "react";
import type { TourId } from "./tours";

/**
 * Estado dos tutoriais de tela: qual está aberto agora e quais a pessoa já
 * abriu alguma vez.
 *
 * - O aberto vive só na memória: fechar o app fecha o tutorial.
 * - O "já visto" fica no `localStorage` (`ram_tours_seen`) e só serve para o
 *   pontinho de "novo" no botão Tutorial. É conveniência de quem está na
 *   máquina — a mesma categoria da barra lateral recolhida
 *   (docs/architecture.md, "Exceções à regra"). Storage bloqueado não quebra
 *   nada: o pontinho só volta a aparecer na próxima vez que o app abrir.
 *
 * Nenhum tutorial abre sozinho. Quem abre é o botão Tutorial de cada tela.
 */
export const TOURS_SEEN_KEY = "ram_tours_seen";

type Listener = () => void;

let activeTour: TourId | null = null;
let seen: Set<string> = readSeen();
const listeners = new Set<Listener>();

function readSeen(): Set<string> {
  try {
    const raw = localStorage.getItem(TOURS_SEEN_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return new Set(Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === "string") : []);
  } catch {
    return new Set();
  }
}

function writeSeen() {
  try {
    localStorage.setItem(TOURS_SEEN_KEY, JSON.stringify([...seen]));
  } catch {
    // Storage bloqueado: o "visto" vale até o app fechar.
  }
}

function emit() {
  for (const listener of listeners) listener();
}

function subscribe(listener: Listener) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Abre o tutorial da tela e o marca como visto (some o pontinho de "novo"). */
export function startTour(id: TourId) {
  activeTour = id;
  if (!seen.has(id)) {
    seen = new Set(seen).add(id);
    writeSeen();
  }
  emit();
}

export function closeTour() {
  if (activeTour === null) return;
  activeTour = null;
  emit();
}

export function getActiveTour(): TourId | null {
  return activeTour;
}

export function isTourSeen(id: TourId): boolean {
  return seen.has(id);
}

export function useActiveTour(): TourId | null {
  return useSyncExternalStore(subscribe, getActiveTour, getActiveTour);
}

export function useTourSeen(id: TourId): boolean {
  return useSyncExternalStore(
    subscribe,
    () => seen.has(id),
    () => seen.has(id)
  );
}

/** Só para testes: volta ao estado de um app recém-aberto, relendo o storage. */
export function resetTourStateForTests() {
  activeTour = null;
  seen = readSeen();
  emit();
}
