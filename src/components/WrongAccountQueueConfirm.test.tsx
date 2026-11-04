import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", async () => (await import("../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../store", () => ({
  useStore: () => ({
    accounts: [{ UserID: 7, Username: "bazooka_2g1em", Alias: "" }],
    hideUsernames: false,
    hiddenNameLetters: 0,
  }),
}));

import { WrongAccountQueueConfirm } from "./WrongAccountQueueConfirm";
import { emitTauriEvent, invokeMock, listenMock, resetTauriMocks } from "../test-utils/tauriMocks";

/**
 * Pedido do dono (31/10/2026): quando um cliente entra como outra conta, a fila
 * não para sozinha — pergunta se continua, com "não perguntar de novo".
 */
describe("WrongAccountQueueConfirm", () => {
  beforeEach(() => {
    resetTauriMocks();
  });
  afterEach(cleanup);

  async function ask(queueAsked = true) {
    render(<WrongAccountQueueConfirm />);
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    await act(async () =>
      emitTauriEvent("roblox-client-wrong-account", {
        userId: 7,
        loggedUserId: 99,
        queueStopped: false,
        queueAsked,
      })
    );
  }

  it("asks only when the backend is holding the queue", async () => {
    await ask(false);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("names the account and keeps launching on yes", async () => {
    await ask();
    expect(screen.getByText(/bazooka_2g1em opened as a different Roblox account \(user 99\)/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Keep launching" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("answer_wrong_account_queue", { keepLaunching: true, remember: false })
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("remembers the answer when 'don't ask again' is ticked", async () => {
    await ask();
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Keep launching" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("answer_wrong_account_queue", { keepLaunching: true, remember: true })
    );
  });

  it("Escape stops the queue, as it did before", async () => {
    await ask();
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("answer_wrong_account_queue", { keepLaunching: false, remember: false })
    );
  });

  it("closes by itself when the queue ends elsewhere", async () => {
    await ask();
    await act(async () => emitTauriEvent("launch-complete", {}));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("answer_wrong_account_queue", expect.anything());
  });
});
