import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect } from "react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/plugin-autostart", () => ({ enable: vi.fn(), disable: vi.fn() }));

const announceUiScale = vi.fn();
// Automático aplicado em 170% (janela grande): é o que o campo custom exibe.
vi.mock("../../hooks/useUiScale", () => ({
  announceUiScale: (...args: unknown[]) => announceUiScale(...args),
  useAppliedUiScale: () => 1.7,
}));

import { AppTab } from "./AppTab";
import { useSettings } from "../../hooks/useSettings";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

let stored: Record<string, Record<string, string>> = {};

function Harness() {
  const s = useSettings();
  useEffect(() => {
    void s.load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  if (!s.loaded) return <div>Loading settings...</div>;
  return <AppTab s={s} />;
}

beforeEach(() => {
  resetTauriMocks();
  announceUiScale.mockClear();
  // InterfaceScale ausente => "auto".
  stored = {};
  setInvokeHandler((cmd) => {
    if (cmd === "get_all_settings") return stored;
    return undefined;
  });
});
afterEach(cleanup);

describe("Settings › App › Interface size", () => {
  it("Tabbing through the custom field unchanged keeps Automatic", async () => {
    render(<Harness />);
    const input = (await screen.findByRole("textbox", { name: "Custom interface size (%)" })) as HTMLInputElement;
    // O campo mostra o valor em vigor (170%) e o modo ainda é automático.
    expect(input.value).toBe("170");

    // Tab entrando e saindo do campo, sem digitar nada.
    await userEvent.click(input);
    await userEvent.tab();

    // Dá tempo de um eventual save com debounce disparar (não deve haver).
    await new Promise((r) => setTimeout(r, 250));
    expect(invokeMock).not.toHaveBeenCalledWith(
      "update_setting",
      expect.objectContaining({ section: "General", key: "InterfaceScale" })
    );
    expect(announceUiScale).not.toHaveBeenCalled();
  });

  it("typing a different value still saves a fixed interface size", async () => {
    render(<Harness />);
    const input = (await screen.findByRole("textbox", { name: "Custom interface size (%)" })) as HTMLInputElement;

    await userEvent.click(input);
    await userEvent.clear(input);
    await userEvent.type(input, "150");
    await userEvent.tab();

    // `useSettings.set` grava com debounce: espera o invoke sair.
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_setting", {
        section: "General",
        key: "InterfaceScale",
        value: "150",
      })
    );
    expect(announceUiScale).toHaveBeenCalledWith("150");
  });
});
