import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", async () => (await import("../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../test-utils/tauriMocks")).tauriEventMock());
const updateSetting = vi.fn(async () => {});
vi.mock("../store", () => ({ useStore: () => ({ updateSetting }) }));

import { MultiRobloxOffConfirm } from "./MultiRobloxOffConfirm";
import { emitTauriEvent, invokeMock, listenMock, resetTauriMocks } from "../test-utils/tauriMocks";

/**
 * Pedido do dono (04/11/2026): com o Multi Roblox desligado, abrir várias
 * contas pergunta se liga, em vez de a pessoa perguntar no Discord.
 */
describe("MultiRobloxOffConfirm", () => {
  beforeEach(() => {
    resetTauriMocks();
    updateSetting.mockClear();
  });
  afterEach(cleanup);

  async function ask() {
    render(<MultiRobloxOffConfirm />);
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    await act(async () => emitTauriEvent("multi-roblox-off-confirm", { accounts: 5, open: 0 }));
  }

  it("turns Multi Roblox on and releases the launch", async () => {
    await ask();
    fireEvent.click(screen.getByRole("button", { name: "Turn on Multi Roblox" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("answer_multi_roblox_off"));
    expect(updateSetting).toHaveBeenCalledWith("General", "EnableMultiRbx", "true");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("keeps it off and remembers when 'don't ask again' is ticked", async () => {
    await ask();
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Keep it off" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("answer_multi_roblox_off"));
    expect(updateSetting).toHaveBeenCalledWith("General", "AskEnableMultiRbx", "false");
    expect(updateSetting).not.toHaveBeenCalledWith("General", "EnableMultiRbx", "true");
  });

  it("Escape keeps it off without touching any setting", async () => {
    await ask();
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("answer_multi_roblox_off"));
    expect(updateSetting).not.toHaveBeenCalled();
  });
});
