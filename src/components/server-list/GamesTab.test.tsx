import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { GamesTab } from "./GamesTab";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

const ACCOUNT = makeAccount({ UserID: 1001, Username: "alpha" });

/** A busca inicial (`search_games` com termo vazio) devolve um jogo só. */
function searchHandler() {
  setInvokeHandler((cmd) => {
    if (cmd === "search_games") {
      return {
        sorts: [
          {
            games: [
              { rootPlaceId: 606849621, universeId: 1, name: "Jailbreak", playerCount: 1200 },
            ],
          },
        ],
      };
    }
    return null;
  });
}

async function renderGames() {
  const props = {
    onSelectGame: vi.fn(),
    onJoinGame: vi.fn(),
    addToast: vi.fn(),
    onAddFavorite: vi.fn(),
    onBrowseServers: vi.fn(),
  };
  setStore({
    accounts: [ACCOUNT],
    selectedIds: new Set([1001]),
    selectedAccounts: [ACCOUNT],
  });
  render(<GamesTab {...props} />);
  await screen.findByText("Jailbreak");
  return props;
}

beforeEach(() => {
  resetTauriMocks();
  localStorage.clear();
  searchHandler();
});

afterEach(cleanup);

describe("GamesTab — ações do card", () => {
  /**
   * As ações ficavam atrás de `opacity-0 group-hover:opacity-100`: a ação mais
   * útil da tela era invisível em repouso, e quem não passa o mouse por cima
   * nunca descobre que ela existe.
   */
  it("mostra as ações do card sem depender do hover", async () => {
    await renderGames();

    const actions = screen.getByTestId("game-actions-606849621");
    expect(actions.className).not.toMatch(/opacity-0/);
    expect(actions.className).not.toMatch(/group-hover:opacity/);
  });

  it("leva aos servidores do jogo sem lançar nada", async () => {
    const props = await renderGames();

    await userEvent.click(screen.getByRole("button", { name: "Browse servers" }));

    expect(props.onBrowseServers).toHaveBeenCalledWith(606849621, "Jailbreak");
    expect(props.onSelectGame).not.toHaveBeenCalled();
    expect(props.onJoinGame).not.toHaveBeenCalled();
  });

  /** Favoritar só existia no botão direito — ninguém achava. */
  it("oferece favoritar junto das outras ações do card", async () => {
    const props = await renderGames();

    await userEvent.click(screen.getByRole("button", { name: "Favorite" }));

    expect(props.onAddFavorite).toHaveBeenCalledWith(
      expect.objectContaining({ placeId: 606849621, name: "Jailbreak" })
    );
    expect(props.onSelectGame).not.toHaveBeenCalled();
  });

  it("mantém o clique na linha lançando o jogo", async () => {
    const props = await renderGames();

    await userEvent.click(screen.getByText("Jailbreak"));

    await waitFor(() => expect(props.onSelectGame).toHaveBeenCalledWith(606849621, "Jailbreak", null));
  });

  /** O menu de contexto continua existindo — as ações do card o complementam. */
  it("ainda lança pelo botão de Join Game", async () => {
    const props = await renderGames();

    await userEvent.click(screen.getByRole("button", { name: "Join Game" }));

    expect(props.onJoinGame).toHaveBeenCalledWith(606849621);
  });
});
