import { useEffect, useRef, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { tr } from "../../i18n/text";
import {
  evaluateWaitPresence,
  planNextCheck,
  WAIT_MAX_DURATION_MS,
  WAIT_POLL_INTERVAL_MS,
  type WaitDecision,
  type WaitPresence,
} from "../../utils/waitForPlayer";

/**
 * A espera do "Wait until they're in a game" (aba Seguir da Choose Game).
 *
 * Mora num módulo, não no estado da aba: trocar de aba dentro da Choose Game
 * desmonta a `FollowTab`, e a espera tem que continuar. Quem a sustenta é a
 * própria Choose Game (`useWaitForPlayerHost`): ela fornece o launch e, ao
 * fechar, cancela a espera. Uma espera por vez; um launch só, e para.
 *
 * A presença é lida pelo comando `get_presence` — o mesmo do Follow, que nunca
 * passa por `run_with_session_retry` (o refresh derrubaria as sessões abertas).
 */
export type WaitWatch = {
  id: number;
  targetUserId: number;
  name: string;
  userIds: number[];
  startedAt: number;
  phase: "watching" | "launching";
  checks: number;
  consecutiveErrors: number;
  lastError: string | null;
  nextCheckAt: number;
};

export type WaitJoin = Extract<WaitDecision, { joinable: true }>;

export type WaitHost = {
  /** O toggle "Join by following" no momento da consulta. */
  byFollowing: () => boolean;
  /** Lança as contas da espera no alvo; o retorno é ignorado (um launch só). */
  launch: (watch: WaitWatch, join: WaitJoin) => Promise<unknown>;
  notify: (message: string) => void;
};

let current: WaitWatch | null = null;
let timer: ReturnType<typeof setTimeout> | null = null;
let host: WaitHost | null = null;
let nextId = 1;
const listeners = new Set<() => void>();

function emit(next: WaitWatch | null) {
  current = next;
  listeners.forEach((listener) => listener());
}

function clearTimer() {
  if (timer) clearTimeout(timer);
  timer = null;
}

export function getWaitWatch(): WaitWatch | null {
  return current;
}

export function subscribeWaitWatch(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useWaitWatch(): WaitWatch | null {
  return useSyncExternalStore(subscribeWaitWatch, getWaitWatch, getWaitWatch);
}

export function setWaitHost(next: WaitHost | null) {
  host = next;
}

/** Começa a esperar (troca a espera anterior, se houver). A 1ª consulta sai depois de um intervalo. */
export function startWaitForPlayer(target: { targetUserId: number; name: string; userIds: number[] }) {
  cancelWaitForPlayer();
  const now = Date.now();
  const watch: WaitWatch = {
    id: nextId++,
    targetUserId: target.targetUserId,
    name: target.name,
    userIds: [...target.userIds],
    startedAt: now,
    phase: "watching",
    checks: 0,
    consecutiveErrors: 0,
    lastError: null,
    nextCheckAt: now + WAIT_POLL_INTERVAL_MS,
  };
  emit(watch);
  schedule(watch.id, WAIT_POLL_INTERVAL_MS);
}

export function cancelWaitForPlayer() {
  clearTimer();
  if (current) emit(null);
}

function schedule(id: number, delayMs: number) {
  clearTimer();
  timer = setTimeout(() => void check(id), delayMs);
}

async function check(id: number) {
  timer = null;
  const watch = current;
  if (!watch || watch.id !== id) return;
  const h = host;
  if (!h) {
    // Sem a Choose Game montada não há quem lance: a espera não sobrevive a ela.
    cancelWaitForPlayer();
    return;
  }

  let decision: WaitDecision | null = null;
  let error: string | null = null;
  try {
    const presences = await invoke<(WaitPresence & { userId?: number })[]>("get_presence", {
      userIds: [watch.targetUserId],
    });
    const entry = presences?.find((p) => p.userId === watch.targetUserId) ?? presences?.[0];
    decision = evaluateWaitPresence(entry, h.byFollowing());
  } catch (e) {
    error = String(e);
  }
  // Cancelada (ou trocada) enquanto a resposta vinha.
  if (current?.id !== id) return;

  if (decision?.joinable) {
    emit({ ...watch, phase: "launching", checks: watch.checks + 1, lastError: null });
    try {
      await h.launch(watch, decision);
    } catch {
      // O launch já avisa a falha por conta própria; a espera só termina.
    } finally {
      if (current?.id === id) emit(null);
    }
    return;
  }

  const consecutiveErrors = error ? watch.consecutiveErrors + 1 : 0;
  const now = Date.now();
  const plan = planNextCheck({ startedAt: watch.startedAt, now, consecutiveErrors, lastError: error });
  if (plan.action === "give-up") {
    emit(null);
    h.notify(
      tr("Stopped waiting for {{name}}: not in a game after {{hours}} h.", {
        name: watch.name,
        hours: Math.round(WAIT_MAX_DURATION_MS / 3_600_000),
      })
    );
    return;
  }
  emit({
    ...watch,
    checks: watch.checks + 1,
    consecutiveErrors,
    lastError: error,
    nextCheckAt: now + plan.delayMs,
  });
  schedule(id, plan.delayMs);
}

/**
 * Liga a espera à tela que a sustenta (a Choose Game). O host é lido por ref,
 * então o launch usa sempre o estado mais novo da tela; ao desmontar, a espera
 * é cancelada — fechar a Choose Game para de vigiar.
 */
export function useWaitForPlayerHost(next: WaitHost) {
  const ref = useRef(next);
  useEffect(() => {
    ref.current = next;
  });
  useEffect(() => {
    setWaitHost({
      byFollowing: () => ref.current.byFollowing(),
      launch: (watch, join) => ref.current.launch(watch, join),
      notify: (message) => ref.current.notify(message),
    });
    return () => {
      setWaitHost(null);
      cancelWaitForPlayer();
    };
  }, []);
}
