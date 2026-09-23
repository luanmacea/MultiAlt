/**
 * Shared Tauri IPC doubles for component tests. Nothing here touches the
 * network or the real Tauri runtime.
 *
 * Usage (paths are relative to the test file):
 *
 * ```ts
 * vi.mock("@tauri-apps/api/core", async () =>
 *   (await import("../../test-utils/tauriMocks")).tauriCoreMock()
 * );
 * vi.mock("@tauri-apps/api/event", async () =>
 *   (await import("../../test-utils/tauriMocks")).tauriEventMock()
 * );
 * ```
 */
import { vi } from "vitest";

export type InvokeArgs = Record<string, unknown> | undefined;
export type InvokeHandler = (cmd: string, args: InvokeArgs) => unknown;

const noopHandler: InvokeHandler = () => undefined;
let handler: InvokeHandler = noopHandler;

/** Every `invoke(cmd, args)` the component tree makes lands here. */
export const invokeMock = vi.fn(async (cmd: string, args?: InvokeArgs) => handler(cmd, args));

/** Routes `invoke` calls; throw inside the handler to simulate a backend error. */
export function setInvokeHandler(next: InvokeHandler): void {
  handler = next;
}

/** Convenience: route by command name, falling back to `undefined`. */
export function setInvokeMap(map: Record<string, unknown | ((args: InvokeArgs) => unknown)>): void {
  handler = (cmd, args) => {
    const entry = map[cmd];
    return typeof entry === "function" ? (entry as (a: InvokeArgs) => unknown)(args) : entry;
  };
}

export const unlistenMock = vi.fn();

/** `listen()` resolves to an unsubscribe function, like the real API. */
export const listenMock = vi.fn(async (_event: string, _cb: (e: unknown) => void) => unlistenMock);

/** Fires the callbacks registered through `listenMock` for one event name. */
export function emitTauriEvent(event: string, payload: unknown): void {
  for (const call of listenMock.mock.calls) {
    if (call[0] === event) call[1]({ event, id: 0, payload });
  }
}

export const windowMock = {
  isMaximized: vi.fn(async () => false),
  onResized: vi.fn(async () => unlistenMock),
  minimize: vi.fn(async () => {}),
  toggleMaximize: vi.fn(async () => {}),
  close: vi.fn(async () => {}),
  hide: vi.fn(async () => {}),
  show: vi.fn(async () => {}),
  setFocus: vi.fn(async () => {}),
  startDragging: vi.fn(async () => {}),
};

export function tauriCoreMock() {
  return {
    invoke: invokeMock,
    convertFileSrc: (path: string) => path,
    isTauri: () => true,
  };
}

export function tauriEventMock() {
  return {
    listen: listenMock,
    once: vi.fn(async () => unlistenMock),
    emit: vi.fn(async () => {}),
    emitTo: vi.fn(async () => {}),
    TauriEvent: {},
  };
}

export function tauriWindowMock() {
  return {
    getCurrentWindow: () => windowMock,
    getCurrent: () => windowMock,
  };
}

/** Clears call history and the invoke routing table. Call in `beforeEach`. */
export function resetTauriMocks(): void {
  handler = noopHandler;
  invokeMock.mockClear();
  listenMock.mockClear();
  unlistenMock.mockClear();
  for (const fn of Object.values(windowMock)) fn.mockClear();
}
