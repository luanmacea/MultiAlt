import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", async () => (await import("../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../hooks/usePrompt", async () => (await import("../test-utils/promptMocks")).promptModuleMock());

import { MultiRobloxConfirm } from "./MultiRobloxConfirm";
import { emitTauriEvent, invokeMock, listenMock, resetTauriMocks } from "../test-utils/tauriMocks";
import { confirmMock, promptAnswers, resetPromptMocks } from "../test-utils/promptMocks";

/**
 * Pedido do dono (23/10/2026): quando abrir mais uma conta fecharia uma janela,
 * perguntar em vez de mostrar erro. A resposta volta para o launch que espera.
 */
describe("MultiRobloxConfirm", () => {
  beforeEach(() => {
    resetTauriMocks();
    resetPromptMocks();
  });
  afterEach(cleanup);

  async function mount() {
    render(<MultiRobloxConfirm />);
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
  }

  it("asks about the administrator window and sends a yes back", async () => {
    promptAnswers.confirm = true;
    await mount();
    await act(async () => emitTauriEvent("multi-roblox-confirm", { kind: "admin", programs: "" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("answer_multi_roblox_confirm", { allow: true }));
    expect(confirmMock.mock.calls[0][0]).toContain("administrator");
    expect(confirmMock.mock.calls[0][2]).toMatchObject({ confirmLabel: "Open anyway" });
  });

  it("names the program and sends a no when the person cancels", async () => {
    promptAnswers.confirm = false;
    await mount();
    await act(async () => emitTauriEvent("multi-roblox-confirm", { kind: "program", programs: "RobloxCopy.exe" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("answer_multi_roblox_confirm", { allow: false }));
    expect(confirmMock.mock.calls[0][0]).toContain("RobloxCopy.exe");
  });
});
