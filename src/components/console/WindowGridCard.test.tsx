import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { WindowGridCard, clearMonitorsCache } from "./WindowGridCard";
import { ConsolePage } from "../pages/ConsolePage";
import { makePlatformCapabilities, setStore, storeRef } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import type { StoreValue } from "../../store";

/** A store que o `useStore()` dublê entrega agora. */
const getStore = () => storeRef.current;

const MONITORS = [
  { index: 0, width: 2560, height: 1440, primary: true },
  { index: 1, width: 1920, height: 1080, primary: false },
];

/**
 * `updateSetting` como o da store de verdade: grava no `settings` da store e
 * manda o `update_setting` ao backend. É o que faz o valor salvo ser o que a
 * tela mostra ao voltar.
 */
function liveStore(settings: Record<string, Record<string, string>> = {}): StoreValue {
  const store = setStore({ settings, launchLogs: [] });
  store.updateSetting = vi.fn(async (section: string, key: string, value: string) => {
    const current = getStore();
    setStore({
      ...current,
      settings: { ...(current.settings ?? {}), [section]: { ...(current.settings?.[section] ?? {}), [key]: value } },
    });
    await invokeMock("update_setting", { section, key, value });
  });
  return store;
}

function monitor(name: RegExp) {
  return screen.getByRole("button", { name });
}

beforeEach(() => {
  resetTauriMocks();
  clearMonitorsCache();
  setInvokeHandler((cmd) => (cmd === "list_display_monitors" ? MONITORS : cmd === "arrange_windows_grid" ? { arranged: 3, total: 3 } : null));
});

afterEach(cleanup);

describe("WindowGridCard", () => {
  it("sits at the top of the Console page", async () => {
    liveStore();
    render(<ConsolePage active onLeave={vi.fn()} />);
    const card = await screen.findByTestId("window-grid-card");
    const log = screen.getByTestId("launch-console");
    expect(card.compareDocumentPosition(log) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("is not shown outside Windows", () => {
    liveStore();
    getStore().platformCapabilities = makePlatformCapabilities({ os: "macos" });
    render(<ConsolePage active onLeave={vi.fn()} />);
    expect(screen.queryByTestId("window-grid-card")).not.toBeInTheDocument();
  });

  it("ticks every monitor when nothing was saved", async () => {
    liveStore();
    render(<WindowGridCard />);
    expect(await screen.findByRole("button", { name: /Monitor 0/ })).toHaveAttribute("aria-pressed", "true");
    expect(monitor(/Monitor 1/)).toHaveAttribute("aria-pressed", "true");
  });

  /**
   * Bug conhecido: a seleção nascia de `store.settings`, que não era atualizado
   * depois do `update_setting`. Ao voltar à tela, o monitor desmarcado aparecia
   * marcado de novo. O valor salvo tem que ser o que se vê.
   */
  it("shows the saved monitor selection when the card is opened again", async () => {
    liveStore();
    const { rerender } = render(<WindowGridCard />);
    await userEvent.click(await screen.findByRole("button", { name: /Monitor 1/ }));
    expect(getStore().updateSetting).toHaveBeenCalledWith("General", "GridMonitors", "0");
    expect(invokeMock).toHaveBeenCalledWith("update_setting", { section: "General", key: "GridMonitors", value: "0" });
    // A store de verdade redesenha quem a lê; o dublê precisa do rerender.
    rerender(<WindowGridCard />);
    expect(monitor(/Monitor 1/)).toHaveAttribute("aria-pressed", "false");

    cleanup();
    render(<WindowGridCard />);
    expect(await screen.findByRole("button", { name: /Monitor 1/ })).toHaveAttribute("aria-pressed", "false");
    expect(monitor(/Monitor 0/)).toHaveAttribute("aria-pressed", "true");
  });

  it("follows a selection saved somewhere else while it is open", async () => {
    liveStore({ General: { GridMonitors: "0,1" } });
    const { rerender } = render(<WindowGridCard />);
    expect(await screen.findByRole("button", { name: /Monitor 1/ })).toHaveAttribute("aria-pressed", "true");
    setStore({ ...getStore(), settings: { General: { GridMonitors: "1" } } });
    rerender(<WindowGridCard />);
    expect(monitor(/Monitor 0/)).toHaveAttribute("aria-pressed", "false");
  });

  it("saves the gap on Enter and keeps it when opened again", async () => {
    liveStore({ General: { GridGap: "20" } });
    render(<WindowGridCard />);
    const gap = screen.getByRole("spinbutton", { name: "Gap" });
    await userEvent.clear(gap);
    await userEvent.type(gap, "8{Enter}");
    expect(getStore().updateSetting).toHaveBeenCalledWith("General", "GridGap", "8");
    cleanup();
    render(<WindowGridCard />);
    expect(screen.getByRole("spinbutton", { name: "Gap" })).toHaveValue(8);
  });

  it("puts an invalid gap back to the saved one", async () => {
    liveStore({ General: { GridGap: "20" } });
    render(<WindowGridCard />);
    const gap = screen.getByRole("spinbutton", { name: "Gap" });
    await userEvent.clear(gap);
    await userEvent.tab();
    expect(gap).toHaveValue(20);
    expect(getStore().updateSetting).not.toHaveBeenCalled();
  });

  it("arranges the open windows on the ticked monitors", async () => {
    liveStore({ General: { GridMonitors: "1", GridGap: "12" } });
    render(<WindowGridCard />);
    await screen.findByRole("button", { name: /Monitor 1/ });
    await userEvent.click(screen.getByRole("button", { name: /Arrange in grid$/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("arrange_windows_grid", { monitorIndices: [1], gap: 12 })
    );
    expect(getStore().addToast).toHaveBeenCalledWith("3 window(s) arranged in grid");
  });

  /** Vem ligada: quem nunca salvou a opção vê o interruptor ligado. */
  it("turns the grid on launch on by default and off from the card", async () => {
    liveStore();
    const { rerender } = render(<WindowGridCard />);
    const toggle = screen.getByRole("switch", { name: /Arrange in grid on launch/ });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await userEvent.click(toggle);
    expect(getStore().updateSetting).toHaveBeenCalledWith("General", "AutoArrangeGrid", "false");
    rerender(<WindowGridCard />);
    expect(screen.getByRole("switch", { name: /Arrange in grid on launch/ })).toHaveAttribute("aria-checked", "false");
  });

  it("comes back with the monitors already there", async () => {
    liveStore();
    render(<WindowGridCard />);
    await screen.findByRole("button", { name: /Monitor 1/ });
    cleanup();
    setInvokeHandler((cmd) => (cmd === "list_display_monitors" ? new Promise(() => {}) : null));
    render(<WindowGridCard />);
    expect(monitor(/Monitor 1/)).toBeInTheDocument();
    expect(screen.queryByText("No monitors detected")).not.toBeInTheDocument();
  });
});
