import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", async () => (await import("../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../test-utils/tauriMocks")).tauriEventMock());

import { OutdatedVersionConfirm } from "./OutdatedVersionConfirm";
import { emitTauriEvent, invokeMock, listenMock, resetTauriMocks } from "../test-utils/tauriMocks";

/**
 * Relato do Discord (31/10/2026): contas banidas por "using a disallowed
 * version of Roblox". Antes de abrir uma versão velha o launch pergunta, e a
 * resposta volta para ele. Pedido do dono: um botão que já troca para a atual.
 */
describe("OutdatedVersionConfirm", () => {
  beforeEach(() => {
    resetTauriMocks();
  });
  afterEach(cleanup);

  async function ask() {
    render(<OutdatedVersionConfirm />);
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    await act(async () =>
      emitTauriEvent("outdated-version-confirm", {
        versionId: "LIVE:version-old",
        label: "My build",
        current: "version-new",
      })
    );
  }

  it("names the old and the current version and warns about bans", async () => {
    await ask();
    expect(screen.getByText(/My build/)).toBeInTheDocument();
    expect(screen.getByText(/version-new/)).toBeInTheDocument();
    expect(screen.getByText(/banned/i)).toBeInTheDocument();
  });

  it("switches to the current version with one click", async () => {
    await ask();
    fireEvent.click(screen.getByRole("button", { name: "Switch to the current version" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("answer_outdated_version_confirm", { choice: "switch" })
    );
    expect(screen.queryByText(/My build/)).not.toBeInTheDocument();
  });

  it("opens the old version only when asked", async () => {
    await ask();
    fireEvent.click(screen.getByRole("button", { name: "Open the old version anyway" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("answer_outdated_version_confirm", { choice: "open-old" })
    );
  });

  it("uses the current version this time on Escape", async () => {
    await ask();
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("answer_outdated_version_confirm", { choice: "current" })
    );
  });
});
