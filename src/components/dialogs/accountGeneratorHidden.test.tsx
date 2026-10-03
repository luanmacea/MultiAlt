import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());
vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn(async () => {}),
  disable: vi.fn(async () => {}),
}));

import { ENABLE_ACCOUNT_GENERATOR } from "../../featureFlags";
import { GeneratorDialog } from "./GeneratorDialog";
import { AddAccountDialog } from "./AddAccountDialog";
import { Toolbar } from "../layout/Toolbar";
import { SettingsPage } from "../pages/SettingsPage";
import { SETTINGS_TABS, TAB_ORDER } from "../settings/tabs";
import { makeAccount, makeGeneratorStatus, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

/**
 * O gerador pago (BloxGen) parou de funcionar, segundo o dono, e ninguém deve
 * tentar usá-lo: com `ENABLE_ACCOUNT_GENERATOR` desligado (o padrão) nenhuma
 * tela leva até ele. O código fica; os testes do gerador ligam a flag.
 */
beforeEach(() => {
  resetTauriMocks();
  setInvokeHandler((cmd) => {
    switch (cmd) {
      case "get_all_settings":
        return {};
      case "remembered_unlock_state":
        return { remembered: false, expiresAt: null };
      case "versions_list_installed":
      case "isolation_list_adapters":
      case "list_backups":
        return [];
      default:
        return undefined;
    }
  });
});

afterEach(cleanup);

describe("Account Generator (BloxGen) hidden behind its feature flag", () => {
  it("is off by default", () => {
    expect(ENABLE_ACCOUNT_GENERATOR).toBe(false);
  });

  it("is not offered in the toolbar Add menu", async () => {
    setStore({ accounts: [makeAccount({ UserID: 1 })] });
    render(<Toolbar />);
    await userEvent.click(screen.getByRole("button", { name: /^Add/ }));

    expect(screen.getByRole("button", { name: /^Create Accounts/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Account Generator/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/BloxGen/)).not.toBeInTheDocument();
  });

  it("is not offered in the Add Account dialog", () => {
    setStore({});
    render(<AddAccountDialog open onClose={vi.fn()} />);

    expect(screen.getByRole("button", { name: /^Create Accounts/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Account Generator/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/BloxGen/)).not.toBeInTheDocument();
  });

  it("has no Settings section", async () => {
    expect(TAB_ORDER).not.toContain("generator");
    expect(SETTINGS_TABS.find((tab) => tab.id === "generator")?.hidden).toBe(true);

    setStore({});
    render(<SettingsPage active onLeave={() => {}} />);
    await screen.findByRole("button", { name: "General" });
    expect(screen.queryByRole("button", { name: "Account Generator" })).not.toBeInTheDocument();
    expect(screen.queryByText(/BloxGen/)).not.toBeInTheDocument();
  });

  it("opens the New Accounts dialog on the free signup, with no tab to switch to the paid one", () => {
    setStore({});
    // A store ainda nasce com `generatorDialogTab = "provider"`.
    render(<GeneratorDialog open onClose={vi.fn()} initialTab="provider" />);

    expect(screen.queryByRole("button", { name: /^Account Generator/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Create Accounts/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/BloxGen/)).not.toBeInTheDocument();
    expect(screen.queryByText(/API key/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Start Generator" })).not.toBeInTheDocument();
  });

  /**
   * Um script do usuário ainda pode ligar o gerador pela API (`start_generator`).
   * Se ele estiver rodando — gastando o saldo da API key —, o chip da barra de
   * status abre o diálogo nele, e o painel tem que aparecer para dar para parar.
   */
  it("still shows the paid generator while one started elsewhere is running, so it can be stopped", () => {
    setStore({
      generatorStatus: makeGeneratorStatus({ active: true, provider: "bloxgen", phase: "running" }),
    });
    render(<GeneratorDialog open onClose={vi.fn()} initialTab="provider" />);

    expect(screen.getByRole("button", { name: "Stop Generator" })).toBeInTheDocument();
  });
});
