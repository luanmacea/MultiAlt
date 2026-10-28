import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";

vi.mock("@tauri-apps/api/event", async () => (await import("../test-utils/tauriMocks")).tauriEventMock());

import { IsolationProgressOverlay, resetIsolationSkipNotice } from "./IsolationProgressOverlay";
import { emitTauriEvent, listenMock, resetTauriMocks } from "../test-utils/tauriMocks";

function progress(stage: string, message: string, finished = true) {
  return { stage, message, pathsCleaned: 0, bytesFreed: 0, finished };
}

async function mount() {
  render(<IsolationProgressOverlay />);
  await waitFor(() => expect(listenMock).toHaveBeenCalled());
}

/**
 * Ticket (21/10/2026): quem joga com várias contas (ou com a main aberta num
 * executor) via "Isolation issue" em vermelho a cada launch. Pular o
 * isolamento com cliente aberto é a regra, não um erro.
 */
describe("IsolationProgressOverlay — isolamento pulado com cliente aberto", () => {
  beforeEach(() => {
    resetTauriMocks();
    resetIsolationSkipNotice();
  });
  afterEach(cleanup);

  it("avisa como informação, não como problema", async () => {
    await mount();
    act(() => emitTauriEvent("isolation-progress", progress("skipped", "2 Roblox clients are open.")));
    expect(await screen.findByText("Isolation skipped")).toBeInTheDocument();
    expect(screen.queryByText("Isolation issue")).not.toBeInTheDocument();
  });

  it("avisa uma vez só enquanto os clientes seguem abertos", async () => {
    await mount();
    act(() => emitTauriEvent("isolation-progress", progress("skipped", "2 Roblox clients are open.")));
    expect(await screen.findByText("Isolation skipped")).toBeInTheDocument();
    cleanup();

    await mount();
    act(() => emitTauriEvent("isolation-progress", progress("skipped", "3 Roblox clients are open.")));
    expect(screen.queryByText("Isolation skipped")).not.toBeInTheDocument();
  });

  it("volta a avisar depois de um isolamento que rodou de verdade", async () => {
    await mount();
    act(() => emitTauriEvent("isolation-progress", progress("skipped", "1 Roblox client is open.")));
    act(() => emitTauriEvent("isolation-progress", progress("finished", "Done")));
    act(() => emitTauriEvent("isolation-progress", progress("skipped", "1 Roblox client is open.")));
    expect(await screen.findByText("Isolation skipped")).toBeInTheDocument();
  });

  it("falha de verdade continua em destaque", async () => {
    await mount();
    act(() => emitTauriEvent("isolation-progress", progress("spoof-failed", "MAC spoof failed")));
    expect(await screen.findByText("Isolation issue")).toBeInTheDocument();
  });
});
