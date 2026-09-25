import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { AccountList } from "./AccountList";
import { ContextMenu } from "../menus/ContextMenu";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks } from "../../test-utils/tauriMocks";
import { resetPromptMocks } from "../../test-utils/promptMocks";

const A = makeAccount({ UserID: 1, Username: "ann" });
const B = makeAccount({ UserID: 2, Username: "bob" });

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
});

afterEach(cleanup);

/**
 * Um Escape tinha de fazer **uma** coisa. Antes da pilha de Escape, o handler da
 * lista (`deselectAll`) e o do menu de contexto (`closeContextMenu`) escutavam
 * separado e nenhum interrompia o evento: fechar o menu com Escape apagava a
 * seleção junto, e a pessoa perdia as contas que tinha marcado só porque abriu
 * um menu por engano.
 */
describe("Escape na lista de contas", () => {
  function renderListaComMenu(contextMenuAberto: boolean) {
    const store = setStore({
      accounts: [A, B],
      selectedIds: new Set([A.UserID, B.UserID]),
      selectedAccounts: [A, B],
      contextMenu: contextMenuAberto ? { x: 10, y: 10 } : null,
    });
    render(
      <>
        <AccountList />
        <ContextMenu />
      </>
    );
    return store;
  }

  /** O div da lista, que e quem escuta as teclas de navegacao. */
  const listEl = () => document.querySelector("[data-tour='accounts-list']") as HTMLElement;

  it("com o menu aberto, fecha só o menu e mantém a seleção", () => {
    const store = renderListaComMenu(true);

    // O Escape nasce na lista (e onde esta o foco) e sobe: e assim que os dois
    // handlers disputavam o mesmo evento.
    fireEvent.keyDown(listEl(), { key: "Escape" });

    expect(store.closeContextMenu).toHaveBeenCalledTimes(1);
    expect(store.deselectAll).not.toHaveBeenCalled();
  });

  it("sem menu aberto, o Escape limpa a seleção", () => {
    const store = renderListaComMenu(false);

    fireEvent.keyDown(listEl(), { key: "Escape" });

    expect(store.deselectAll).toHaveBeenCalledTimes(1);
    expect(store.closeContextMenu).not.toHaveBeenCalled();
  });
});
