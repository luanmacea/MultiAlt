import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { useEffect, type ReactNode } from "react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { GeneratorTab } from "./GeneratorTab";
import { useSettings, type UseSettingsReturn } from "../../hooks/useSettings";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

/**
 * Este arquivo é próprio (não `settingsTabs.test.tsx`, de outro agente) para
 * cobrir só a mudança do backlog: "Add To Group" da aba Generator vira
 * `<input>` com `<datalist>` sugerindo os grupos que já existem, continuando
 * editável para criar grupo novo.
 */
function SettingsHarness({ children }: { children: (s: UseSettingsReturn) => ReactNode }) {
  const s = useSettings();
  useEffect(() => {
    void s.load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  if (!s.loaded) return <div>Loading settings...</div>;
  return <>{children(s)}</>;
}

function renderTab(initial: Record<string, Record<string, string>> = {}) {
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "get_all_settings":
        return initial;
      default:
        return undefined;
    }
  });
  render(<SettingsHarness>{(s) => <GeneratorTab s={s} />}</SettingsHarness>);
}

beforeEach(() => {
  resetTauriMocks();
  setStore({});
});

afterEach(cleanup);

describe("GeneratorTab — Add To Group suggests existing groups", () => {
  it("offers the account's existing groups as datalist options", async () => {
    setStore({
      accounts: [
        makeAccount({ UserID: 1, Group: "BloxGen" }),
        makeAccount({ UserID: 2, Group: "Mains" }),
      ],
    });
    renderTab();

    const input = (await screen.findByPlaceholderText("BloxGen")) as HTMLInputElement;
    const listId = input.getAttribute("list");
    expect(listId).toBeTruthy();
    // eslint-disable-next-line testing-library/no-node-access
    const datalist = document.getElementById(listId!) as HTMLDataListElement;
    expect(datalist).toBeInstanceOf(HTMLDataListElement);
    const options = Array.from(datalist.options).map((o) => o.value);
    expect(options).toEqual(["BloxGen", "Mains"]);
  });

  it("keeps the field a plain editable input, not a closed dropdown", async () => {
    setStore({ accounts: [makeAccount({ UserID: 1, Group: "BloxGen" })] });
    renderTab();

    const input = (await screen.findByPlaceholderText("BloxGen")) as HTMLInputElement;
    expect(input.tagName).toBe("INPUT");
    expect(input).not.toBeDisabled();
  });

  it("does not offer parseGroupName's display name — the raw numbered group stays intact", async () => {
    // Bug de P0: se a opcao fosse o displayName ("Alts"), escolhe-la trocaria
    // a conta de grupo silenciosamente, perdendo o prefixo "10 ".
    setStore({ accounts: [makeAccount({ UserID: 1, Group: "10 Alts" })] });
    renderTab();

    const input = (await screen.findByPlaceholderText("BloxGen")) as HTMLInputElement;
    const listId = input.getAttribute("list");
    // eslint-disable-next-line testing-library/no-node-access
    const datalist = document.getElementById(listId!) as HTMLDataListElement;
    const options = Array.from(datalist.options).map((o) => o.value);
    expect(options).toEqual(["10 Alts"]);
    expect(options).not.toContain("Alts");
  });
});
