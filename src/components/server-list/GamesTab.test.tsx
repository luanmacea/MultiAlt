import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { GamesTab } from "./GamesTab";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

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

async function renderGames(overrides: Record<string, unknown> = {}) {
  const props = {
    onSelectGame: vi.fn(),
    onJoinGame: vi.fn(),
    addToast: vi.fn(),
    onAddFavorite: vi.fn(),
    onBrowseServers: vi.fn(),
    onBotting: vi.fn(),
    onScripts: vi.fn(),
  };
  setStore({
    accounts: [ACCOUNT],
    selectedIds: new Set([1001]),
    selectedAccounts: [ACCOUNT],
  });
  render(<GamesTab {...props} {...overrides} />);
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

/**
 * A linha era um `<div onClick>`: só o mouse chegava nela. Não dá para trocar
 * a `div` por `<button>` porque `GameRowActions` já renderiza botões dentro
 * — botão dentro de botão é HTML inválido — então a linha ganhou
 * `role="button"` + teclado em vez disso.
 */
describe("GamesTab — teclado", () => {
  it("é alcançável por Tab e ativa com Enter", async () => {
    const props = await renderGames();
    const row = screen.getByRole("button", { name: /Jailbreak/ });
    expect(row.tabIndex).toBe(0);

    row.focus();
    await userEvent.keyboard("{Enter}");
    expect(props.onSelectGame).toHaveBeenCalledWith(606849621, "Jailbreak", null);
  });

  it("ativa com Espaço", async () => {
    const props = await renderGames();
    screen.getByRole("button", { name: /Jailbreak/ }).focus();
    await userEvent.keyboard(" ");
    expect(props.onSelectGame).toHaveBeenCalledWith(606849621, "Jailbreak", null);
  });

  it("não dispara a linha ao ativar um botão de ação por teclado", async () => {
    const props = await renderGames();
    const favoriteButton = screen.getByRole("button", { name: "Favorite" });
    favoriteButton.focus();
    await userEvent.keyboard("{Enter}");
    expect(props.onAddFavorite).toHaveBeenCalledTimes(1);
    expect(props.onSelectGame).not.toHaveBeenCalled();
  });
});

/**
 * As funcionalidades que agem sobre um jogo viviam atrás de "abra a tela e cole
 * o Place ID à mão". O menu do jogo é o atalho: a ação já sabe de que jogo se
 * trata, e nada precisa ser copiado.
 */
describe("GamesTab — ações do jogo pelo menu de contexto", () => {
  /**
   * As buscas são presas ao menu: as ações do card usam alguns dos mesmos
   * rótulos, então uma busca solta passaria clicando no botão do card.
   */
  async function abrirMenu(overrides: Record<string, unknown> = {}) {
    const props = await renderGames(overrides);
    fireEvent.contextMenu(screen.getByText("Jailbreak"));
    const menu = within(screen.getByTestId("game-context-menu"));
    return { ...props, menu };
  }

  it("abre o Botting Mode já com este jogo", async () => {
    const props = await abrirMenu();

    await userEvent.click(props.menu.getByRole("button", { name: "Auto Rejoin" }));

    expect(props.onBotting).toHaveBeenCalledWith(606849621);
    // Abrir uma tela sobre o jogo não é entrar no jogo.
    expect(props.onJoinGame).not.toHaveBeenCalled();
    expect(props.onSelectGame).not.toHaveBeenCalled();
  });

  it("abre os Scripts já com este jogo", async () => {
    const props = await abrirMenu();

    await userEvent.click(props.menu.getByRole("button", { name: "Scripts" }));

    expect(props.onScripts).toHaveBeenCalledWith(606849621);
    expect(props.onJoinGame).not.toHaveBeenCalled();
  });

  it("leva aos servidores deste jogo", async () => {
    const props = await abrirMenu();

    await userEvent.click(props.menu.getByRole("button", { name: "Browse servers" }));

    expect(props.onBrowseServers).toHaveBeenCalledWith(606849621, "Jailbreak");
    expect(props.onJoinGame).not.toHaveBeenCalled();
  });

  /** Tela que não passa a ação não ganha item morto: sem callback, o item some. */
  it("esconde a ação que a tela não oferece", async () => {
    const { menu } = await abrirMenu({ onBotting: undefined, onScripts: undefined });

    expect(menu.queryByRole("button", { name: "Auto Rejoin" })).not.toBeInTheDocument();
    expect(menu.queryByRole("button", { name: "Scripts" })).not.toBeInTheDocument();
    expect(menu.getByRole("button", { name: "Join Game" })).toBeInTheDocument();
  });
});

/**
 * Sair da aba Games e voltar recomeçava com o spinner e a lista vazia. Agora a
 * volta mostra a última lista (e a última busca) na hora e consulta de novo
 * por trás.
 */
describe("GamesTab — cache ao trocar de aba", () => {
  function searchCalls() {
    return invokeMock.mock.calls.filter((call) => call[0] === "search_games");
  }

  function pendingSearch() {
    setInvokeHandler((cmd) => (cmd === "search_games" ? new Promise(() => {}) : null));
  }

  it("volta mostrando a última lista e atualiza por trás", async () => {
    await renderGames();
    cleanup();

    pendingSearch();
    render(<GamesTab onSelectGame={vi.fn()} onJoinGame={vi.fn()} addToast={vi.fn()} onAddFavorite={vi.fn()} />);

    // Já no primeiro desenho, sem esperar a busca.
    expect(screen.getByText("Jailbreak")).toBeInTheDocument();
    expect(screen.getByTestId("games-updating")).toBeInTheDocument();
    await waitFor(() => expect(searchCalls()).toHaveLength(2));
  });

  it("volta com a última busca digitada e o resultado dela", async () => {
    setInvokeHandler((cmd, args) => {
      if (cmd !== "search_games") return null;
      const keyword = (args as { keyword?: string } | undefined)?.keyword ?? "";
      const game = keyword
        ? { rootPlaceId: 920587237, universeId: 2, name: "Adopt Me!", contentType: "Game" }
        : { rootPlaceId: 606849621, universeId: 1, name: "Jailbreak" };
      return keyword ? { searchResults: [{ contents: [game] }] } : { sorts: [{ games: [game] }] };
    });
    setStore({ accounts: [ACCOUNT], selectedIds: new Set([1001]), selectedAccounts: [ACCOUNT] });
    render(<GamesTab onSelectGame={vi.fn()} onJoinGame={vi.fn()} addToast={vi.fn()} onAddFavorite={vi.fn()} />);
    await screen.findByText("Jailbreak");
    fireEvent.change(screen.getByPlaceholderText("Search games..."), { target: { value: "adopt" } });
    await screen.findByText("Adopt Me!", undefined, { timeout: 2000 });
    cleanup();

    pendingSearch();
    render(<GamesTab onSelectGame={vi.fn()} onJoinGame={vi.fn()} addToast={vi.fn()} onAddFavorite={vi.fn()} />);

    expect(screen.getByPlaceholderText("Search games...")).toHaveValue("adopt");
    expect(screen.getByText("Adopt Me!")).toBeInTheDocument();
    await waitFor(() => {
      const calls = searchCalls();
      expect(calls[calls.length - 1]?.[1]).toMatchObject({ keyword: "adopt" });
    });
  });

  /** A volta não pode custar mais rede que antes: ícone que já veio não é pedido de novo. */
  it("não pede de novo o ícone de um jogo que já tem", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "search_games") {
        return { sorts: [{ games: [{ rootPlaceId: 606849621, universeId: 1, name: "Jailbreak" }] }] };
      }
      if (cmd === "batched_get_game_icon") return "https://icon.test/606849621.png";
      return null;
    });
    await renderGames();
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter((c) => c[0] === "batched_get_game_icon")).toHaveLength(1)
    );
    cleanup();

    render(<GamesTab onSelectGame={vi.fn()} onJoinGame={vi.fn()} addToast={vi.fn()} onAddFavorite={vi.fn()} />);
    await waitFor(() => expect(searchCalls()).toHaveLength(2));
    await waitFor(() => expect(screen.queryByTestId("games-updating")).not.toBeInTheDocument());
    expect(invokeMock.mock.calls.filter((c) => c[0] === "batched_get_game_icon")).toHaveLength(1);
    // E o ícone não pisca: continua lá depois da busca nova.
    expect(document.querySelector('img[src="https://icon.test/606849621.png"]')).not.toBeNull();
  });
});
