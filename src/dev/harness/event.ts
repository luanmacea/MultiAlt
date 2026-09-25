/**
 * Dublê de `@tauri-apps/api/event` para o harness de UI.
 */
import { harnessListen } from "./bus";

export type UnlistenFn = () => void;

export interface Event<T> {
  event: string;
  id: number;
  payload: T;
}

let nextId = 1;

export function listen<T>(
  event: string,
  handler: (event: Event<T>) => void
): Promise<UnlistenFn> {
  const off = harnessListen(event, (payload) => {
    handler({ event, id: nextId++, payload: payload as T });
  });
  return Promise.resolve(off);
}

export function once<T>(
  event: string,
  handler: (event: Event<T>) => void
): Promise<UnlistenFn> {
  let off: UnlistenFn = () => {};
  off = harnessListen(event, (payload) => {
    off();
    handler({ event, id: nextId++, payload: payload as T });
  });
  return Promise.resolve(off);
}

export function emit(_event: string, _payload?: unknown): Promise<void> {
  return Promise.resolve();
}
