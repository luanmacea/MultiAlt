import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { AccountFieldsDialog } from "./AccountFieldsDialog";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import type { Account } from "../../types";

/**
 * O diálogo de Fields só gravava com Enter no campo de valor ou ao remover uma
 * linha: renomear só a chave, adicionar com "+", ou editar e fechar pelo X,
 * Esc ou clique no fundo jogava a edição fora sem aviso. Agora grava ao sair
 * do campo (blur) e ao fechar — e só quando algo mudou, uma vez só.
 */
function renderDialog(fields: Record<string, string> = { Note: "old" }) {
  const account = makeAccount({ UserID: 9, Fields: fields });
  const store = setStore({ accounts: [account], selectedAccounts: [account], selectedIds: new Set([9]) });
  const onClose = vi.fn();
  const view = render(<AccountFieldsDialog open onClose={onClose} />);
  return { store, onClose, view };
}

function savedFields(store: ReturnType<typeof setStore>): Record<string, string>[] {
  const calls = (store.updateAccount as unknown as { mock: { calls: [Account][] } }).mock.calls;
  return calls.map(([acc]) => acc.Fields);
}

beforeEach(() => {
  resetTauriMocks();
  setInvokeHandler(() => []);
  vi.clearAllMocks();
});

afterEach(cleanup);

describe("AccountFieldsDialog — gravar sem depender do Enter", () => {
  it("renomear só a chave grava ao sair do campo, com o flash verde", async () => {
    const { store } = renderDialog();
    const key = screen.getByPlaceholderText("Key");
    await userEvent.clear(key);
    await userEvent.type(key, "Renamed");
    fireEvent.blur(key);

    expect(savedFields(store)).toEqual([{ Renamed: "old" }]);
    expect(await screen.findByText(/^Fields - /)).toHaveClass("text-emerald-400");
  });

  it("editar o valor e fechar pelo X grava uma vez só", async () => {
    const { store, onClose } = renderDialog();
    const value = screen.getByPlaceholderText("Value");
    await userEvent.clear(value);
    await userEvent.type(value, "new");
    await userEvent.click(screen.getByRole("button", { name: "Close" }));

    expect(savedFields(store)).toEqual([{ Note: "new" }]);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("linha adicionada com + é gravada ao fechar", async () => {
    const { store } = renderDialog({});
    await userEvent.click(screen.getByText("+"));
    await userEvent.click(screen.getByRole("button", { name: "Close" }));

    expect(savedFields(store)).toEqual([{ Field: "Value" }]);
  });

  it("editar o valor e fechar com Esc grava", async () => {
    const { store, onClose } = renderDialog();
    const value = screen.getByPlaceholderText("Value");
    await userEvent.clear(value);
    await userEvent.type(value, "esc");
    await userEvent.keyboard("{Escape}");

    expect(savedFields(store)).toEqual([{ Note: "esc" }]);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("editar o valor e clicar no fundo grava", async () => {
    const { store, onClose, view } = renderDialog();
    const value = screen.getByPlaceholderText("Value");
    await userEvent.clear(value);
    await userEvent.type(value, "backdrop");
    const backdrop = view.container.firstElementChild as HTMLElement;
    fireEvent.mouseDown(backdrop);
    fireEvent.click(backdrop);

    expect(savedFields(store)).toEqual([{ Note: "backdrop" }]);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("Enter e depois fechar não grava de novo", async () => {
    const { store } = renderDialog();
    const value = screen.getByPlaceholderText("Value");
    await userEvent.clear(value);
    await userEvent.type(value, "enter{Enter}");
    fireEvent.blur(value);
    await userEvent.keyboard("{Escape}");

    expect(savedFields(store)).toEqual([{ Note: "enter" }]);
  });

  it("abrir e fechar sem mudar nada não grava", async () => {
    const { store, onClose } = renderDialog();
    fireEvent.blur(screen.getByPlaceholderText("Key"));
    fireEvent.blur(screen.getByPlaceholderText("Value"));
    await userEvent.click(screen.getByRole("button", { name: "Close" }));

    expect(store.updateAccount).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
