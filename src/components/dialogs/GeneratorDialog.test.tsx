import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { GeneratorDialog } from "./GeneratorDialog";
import type { GeneratorDialogTab } from "../../store";
import { setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks } from "../../test-utils/tauriMocks";

function renderDialog(initialTab: GeneratorDialogTab = "provider") {
  const store = setStore({});
  const onClose = vi.fn();
  render(<GeneratorDialog open onClose={onClose} initialTab={initialTab} />);
  return { store, onClose };
}

beforeEach(() => {
  resetTauriMocks();
});

afterEach(cleanup);

/**
 * As duas formas de conseguir conta nova moram neste diálogo, e até aqui cada
 * tela as chamava por um nome diferente ("Create Accounts"/"Create in the
 * browser", "Account Generator"/"Buy from a provider"). O nome de cada função é
 * um só, e é o mesmo do menu `Add` e do AddAccountDialog.
 */
describe("GeneratorDialog — one name per function", () => {
  it("names the two tabs exactly like the Add menu does", () => {
    renderDialog();

    const paid = screen.getByRole("button", { name: /^Account Generator/ });
    expect(paid).toHaveTextContent(/paid/i);
    expect(paid).toHaveTextContent(/BloxGen/);

    const free = screen.getByRole("button", { name: /^Create Accounts/ });
    expect(free).toHaveTextContent(/free/i);
    expect(free).toHaveTextContent(/CAPTCHA/);
  });

  /**
   * O título do diálogo era "Account Generator" — o nome de **uma** das duas
   * funções servindo de nome para o conjunto, que é justamente o que confundia.
   */
  it("does not reuse the paid tab's name as the title of the whole dialog", () => {
    renderDialog();
    expect(screen.getAllByText("Account Generator")).toHaveLength(1);
  });
});

/**
 * O gerador gasta dinheiro de verdade: cada conta sai do saldo da API key em
 * `core.bloxgen.net` (`commands/generators.rs`, `/api/generate` + `/api/balance`).
 * Nada na tela dizia isso, nem que a aba ao lado faz o mesmo de graça.
 */
describe("GeneratorDialog — what the provider tab costs", () => {
  it("says BloxGen is a third-party service", () => {
    renderDialog();
    expect(screen.getByText(/third-party service/i)).toBeInTheDocument();
  });

  it("says generating spends the balance on the API key", () => {
    renderDialog();
    expect(screen.getByText(/costs money/i)).toBeInTheDocument();
  });

  it("points at the free alternative in the next tab", () => {
    renderDialog();
    expect(screen.getByText(/Create Accounts tab/)).toBeInTheDocument();
  });
});
