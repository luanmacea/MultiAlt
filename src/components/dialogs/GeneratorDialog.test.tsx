import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { GeneratorDialog } from "./GeneratorDialog";
import type { GeneratorDialogTab } from "../../store";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks } from "../../test-utils/tauriMocks";

function renderDialog(initialTab: GeneratorDialogTab = "provider", storeOverrides: Parameters<typeof setStore>[0] = {}) {
  const store = setStore(storeOverrides);
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

/**
 * "Add To Group" era um campo de texto livre puro: digitar "bloxgen" em vez
 * de "BloxGen" criava um grupo separado na lista de contas, porque o
 * agrupamento é por `account.Group` literal. O campo continua editável (tem
 * que dar para criar grupo novo), mas agora oferece os grupos que já existem
 * via `<datalist>`.
 */
describe("GeneratorDialog — Add To Group suggests existing groups", () => {
  it("offers the account's existing groups as datalist options", () => {
    renderDialog("provider", {
      accounts: [
        makeAccount({ UserID: 1, Group: "BloxGen" }),
        makeAccount({ UserID: 2, Group: "Mains" }),
      ],
    });

    const input = screen.getByPlaceholderText("BloxGen") as HTMLInputElement;
    const listId = input.getAttribute("list");
    expect(listId).toBeTruthy();
    // eslint-disable-next-line testing-library/no-node-access
    const datalist = document.getElementById(listId!) as HTMLDataListElement;
    expect(datalist).toBeInstanceOf(HTMLDataListElement);
    const options = Array.from(datalist.options).map((o) => o.value);
    expect(options).toEqual(["BloxGen", "Mains"]);
  });

  it("keeps the field editable for a brand new group name", async () => {
    const { store } = renderDialog("provider", {
      accounts: [makeAccount({ UserID: 1, Group: "BloxGen" })],
    });

    const input = screen.getByPlaceholderText("BloxGen") as HTMLInputElement;
    expect(input).not.toBeDisabled();
    expect(input.tagName).toBe("INPUT");
    // Typing something outside the suggested list must not be blocked.
    input.focus();
    (input as HTMLInputElement).value = "Brand New Group";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    void store;
  });

  it("does not offer parseGroupName's display name — the raw numbered group stays intact", () => {
    renderDialog("provider", {
      accounts: [makeAccount({ UserID: 1, Group: "10 Alts" })],
    });

    const input = screen.getByPlaceholderText("BloxGen") as HTMLInputElement;
    const listId = input.getAttribute("list");
    // eslint-disable-next-line testing-library/no-node-access
    const datalist = document.getElementById(listId!) as HTMLDataListElement;
    const options = Array.from(datalist.options).map((o) => o.value);
    expect(options).toEqual(["10 Alts"]);
    expect(options).not.toContain("Alts");
  });
});
