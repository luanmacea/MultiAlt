import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/window", async () => (await import("../../test-utils/tauriMocks")).tauriWindowMock());

import { EncryptionSetupScreen } from "./EncryptionSetupScreen";
import { setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, windowMock } from "../../test-utils/tauriMocks";
import type { StoreValue } from "../../store";

function renderSetup(overrides: Partial<StoreValue> = {}) {
  const store = setStore({ encryptionSetupMode: "firstRun", ...overrides });
  render(<EncryptionSetupScreen />);
  return store;
}

beforeEach(resetTauriMocks);
afterEach(cleanup);

/**
 * A janela não tem borda do Windows (`decorations: false`), e esta tela troca
 * a árvore inteira do app — sem TitleBar. Na primeira execução ela não tinha
 * pílula de minimizar/fechar, nem Cancel, e o Esc não fazia nada: só se saía
 * concluindo, ou com Alt+F4. Se o Continue falhasse (o cenário do harness
 * `vault-key-warning-setup`), o usuário ficava preso. A tela de senha já tinha a
 * pílula; esta passou a ter a mesma.
 */
describe("EncryptionSetupScreen — controles de janela", () => {
  it("mostra minimizar, maximizar e fechar na primeira execução", () => {
    renderSetup();
    expect(screen.getByText("Set Up Encryption")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Minimize" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Maximize" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("o fechar da pílula fecha a janela mesmo sem ter concluído", async () => {
    const store = renderSetup();
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(windowMock.close).toHaveBeenCalledTimes(1);
    // Fechar a janela não é escolher um método.
    expect(store.applyEncryptionMethod).not.toHaveBeenCalled();
  });

  it("mostra a pílula também quando a tela veio das Settings", () => {
    renderSetup({ encryptionSetupMode: "settings", accountsEncrypted: false });
    expect(screen.getByRole("button", { name: "Minimize" })).toBeInTheDocument();
  });

  /**
   * A janela é sem decoração e o único ponto que a arrastava era a TitleBar —
   * que esta tela (e a de senha) troca por esta pílula. Com diálogo aberto, o
   * fundo do modal cobre a TitleBar e é a mesma pílula que aparece: sem a alça,
   * não havia como mover a janela em nenhum desses casos.
   */
  it("a pílula tem uma alça que arrasta a janela", () => {
    renderSetup();
    fireEvent.mouseDown(screen.getByRole("button", { name: "Move window" }), { button: 0 });
    expect(windowMock.startDragging).toHaveBeenCalledTimes(1);
  });

  it("clique com o botão direito na alça não arrasta", () => {
    renderSetup();
    fireEvent.mouseDown(screen.getByRole("button", { name: "Move window" }), { button: 2 });
    expect(windowMock.startDragging).not.toHaveBeenCalled();
  });
});

describe("EncryptionSetupScreen — Escape", () => {
  it("cancela quando a tela veio das Settings, como o botão Cancel", async () => {
    const store = renderSetup({ encryptionSetupMode: "settings", accountsEncrypted: false });
    await userEvent.keyboard("{Escape}");
    expect(store.closeEncryptionSetup).toHaveBeenCalledTimes(1);
  });

  // Na primeira execução não há para onde voltar (a escolha é obrigatória, e o
  // store ignora o pedido nesse modo): quem quer sair usa a pílula.
  it("não finge cancelar na primeira execução", async () => {
    const store = renderSetup();
    await userEvent.keyboard("{Escape}");
    expect(store.closeEncryptionSetup).not.toHaveBeenCalled();
  });
});
