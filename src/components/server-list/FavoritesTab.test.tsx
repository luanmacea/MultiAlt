import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { FavoritesTab } from "./FavoritesTab";
import { RecentTab } from "./RecentTab";
import { loadFavorites, saveFavorites, saveRecentGames } from "./types";
import type { FavoriteGame, RecentGame } from "./types";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { confirmMock, promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";

function favorite(overrides: Partial<FavoriteGame> = {}): FavoriteGame {
  return {
    placeId: 606849621,
    name: "Jailbreak",
    iconUrl: null,
    addedAt: Date.now(),
    vipServers: [],
    ...overrides,
  };
}

function recent(overrides: Partial<RecentGame> = {}): RecentGame {
  return {
    placeId: 920587237,
    name: "Adopt Me",
    iconUrl: "https://example.invalid/icon.png",
    lastPlayed: Date.now(),
    ...overrides,
  };
}

function renderFavorites() {
  const onSelectGame = vi.fn();
  const addToast = vi.fn();
  const onBrowseServers = vi.fn();
  const onBotting = vi.fn();
  const onScripts = vi.fn();
  render(
    <FavoritesTab
      onSelectGame={onSelectGame}
      addToast={addToast}
      onBrowseServers={onBrowseServers}
      onBotting={onBotting}
      onScripts={onScripts}
    />
  );
  return { onSelectGame, addToast, onBrowseServers, onBotting, onScripts };
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  localStorage.clear();
  // RecentGamesList resolves missing names/icons through the backend.
  setInvokeHandler(() => null);
});

afterEach(cleanup);

describe("FavoritesTab", () => {
  it("explains how to add the first favorite", () => {
    renderFavorites();
    expect(screen.getByText("No favorites yet")).toBeInTheDocument();
    expect(
      screen.getByText("Use the star on a game in the Games or Recent tab to add one")
    ).toBeInTheDocument();
  });

  it("lists saved favorites and expands one on click", async () => {
    saveFavorites([favorite()]);
    const { onSelectGame } = renderFavorites();

    expect(screen.getByText("Jailbreak")).toBeInTheDocument();
    expect(screen.getByText("ID: 606849621")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Join Game" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByText("Jailbreak"));
    await userEvent.click(screen.getByRole("button", { name: "Join Game" }));
    expect(onSelectGame).toHaveBeenCalledWith(606849621);
  });

  it("joins a VIP server with its link", async () => {
    saveFavorites([
      favorite({ vipServers: [{ id: "v1", name: "My VIP", link: "https://vip.link/abc" }] }),
    ]);
    const { onSelectGame } = renderFavorites();

    expect(screen.getByText("1 VIP")).toBeInTheDocument();
    await userEvent.click(screen.getByText("Jailbreak"));
    await userEvent.click(screen.getByRole("button", { name: "Join" }));
    expect(onSelectGame).toHaveBeenCalledWith(606849621, "https://vip.link/abc");
  });

  it("migrates a legacy single privateServer into the VIP list", () => {
    localStorage.setItem(
      "ram_favorite_games",
      JSON.stringify([{ placeId: 1, name: "Old", iconUrl: null, addedAt: 0, privateServer: "legacy-link" }])
    );
    renderFavorites();
    expect(screen.getByText("1 VIP")).toBeInTheDocument();
  });

  /**
   * O fluxo era dois `prompt()` encadeados: cancelar o segundo (o nome,
   * opcional) jogava fora o link que a pessoa acabou de digitar no primeiro.
   * Agora é um formulário inline na própria linha, sem diálogos empilhados.
   */
  it("adds a VIP server through the inline form and persists it", async () => {
    saveFavorites([favorite()]);
    const { addToast } = renderFavorites();
    await userEvent.click(screen.getByText("Jailbreak"));

    await userEvent.click(screen.getByRole("button", { name: "Add VIP Server" }));
    await userEvent.type(
      screen.getByPlaceholderText("Private server link or VIP code"),
      "https://vip.link/new"
    );
    await userEvent.type(
      screen.getByPlaceholderText("Name for this server (optional)"),
      "Squad server"
    );
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(addToast).toHaveBeenCalledWith("VIP server added"));
    expect(screen.getByText("Squad server")).toBeInTheDocument();
    expect(loadFavorites()[0].vipServers?.[0].link).toBe("https://vip.link/new");
    // O formulário fecha depois de salvar; não sobra um segundo diálogo.
    expect(screen.queryByPlaceholderText("Private server link or VIP code")).not.toBeInTheDocument();
  });

  it("does not add a VIP server when the inline form is cancelled", async () => {
    saveFavorites([favorite()]);
    const { addToast } = renderFavorites();
    await userEvent.click(screen.getByText("Jailbreak"));

    await userEvent.click(screen.getByRole("button", { name: "Add VIP Server" }));
    await userEvent.type(
      screen.getByPlaceholderText("Private server link or VIP code"),
      "https://vip.link/discarded"
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(addToast).not.toHaveBeenCalled();
    expect(loadFavorites()[0].vipServers).toHaveLength(0);
    expect(screen.queryByPlaceholderText("Private server link or VIP code")).not.toBeInTheDocument();
  });

  /**
   * A validação roda ANTES do launch, não só lá na hora de entrar no
   * servidor: texto claramente sem cara de link/código não é salvo, e a
   * pessoa não descobre isso só quando o launch falhar.
   */
  it("rejects a clearly invalid VIP link and does not persist it", async () => {
    saveFavorites([favorite()]);
    const { addToast } = renderFavorites();
    await userEvent.click(screen.getByText("Jailbreak"));

    await userEvent.click(screen.getByRole("button", { name: "Add VIP Server" }));
    await userEvent.type(
      screen.getByPlaceholderText("Private server link or VIP code"),
      "this is not a link"
    );
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(addToast).not.toHaveBeenCalled();
    expect(loadFavorites()[0].vipServers).toHaveLength(0);
    // O formulário continua aberto com o erro explicado.
    expect(screen.getByPlaceholderText("Private server link or VIP code")).toBeInTheDocument();
    expect(
      screen.getByText("That doesn't look like a private server link or VIP code.")
    ).toBeInTheDocument();
  });

  it("accepts a bare vip: code without inventing a new format", async () => {
    saveFavorites([favorite()]);
    const { addToast } = renderFavorites();
    await userEvent.click(screen.getByText("Jailbreak"));

    await userEvent.click(screen.getByRole("button", { name: "Add VIP Server" }));
    await userEvent.type(
      screen.getByPlaceholderText("Private server link or VIP code"),
      "vip:11111111-2222-3333-4444-555555555555"
    );
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(addToast).toHaveBeenCalledWith("VIP server added"));
    // O link é salvo cru, do jeito que o Rust (`extract_private_server_link_code`) espera.
    expect(loadFavorites()[0].vipServers?.[0].link).toBe(
      "vip:11111111-2222-3333-4444-555555555555"
    );
  });

  it("removes a VIP server and persists the removal", async () => {
    saveFavorites([
      favorite({ vipServers: [{ id: "v1", name: "My VIP", link: "https://vip.link/abc" }] }),
    ]);
    const { addToast } = renderFavorites();
    await userEvent.click(screen.getByText("Jailbreak"));
    // Apagar o VIP é destrutivo: passa pelo confirm.
    promptAnswers.confirm = true;
    await userEvent.click(screen.getByTitle("Remove"));

    await waitFor(() => expect(addToast).toHaveBeenCalledWith("VIP server removed"));
    expect(loadFavorites()[0].vipServers).toHaveLength(0);
  });

  it("renames and removes a favorite from its context menu", async () => {
    saveFavorites([favorite()]);
    const { addToast } = renderFavorites();

    promptAnswers.prompt = "Renamed";
    fireEvent.contextMenu(screen.getByText("Jailbreak"), { clientX: 5, clientY: 5 });
    await userEvent.click(await screen.findByRole("button", { name: "Rename" }));
    await waitFor(() => expect(screen.getByText("Renamed")).toBeInTheDocument());
    expect(loadFavorites()[0].name).toBe("Renamed");

    promptAnswers.confirm = true;
    fireEvent.contextMenu(screen.getByText("Renamed"), { clientX: 5, clientY: 5 });
    await userEvent.click(await screen.findByRole("button", { name: "Remove" }));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("Removed from favorites"));
    expect(screen.getByText("No favorites yet")).toBeInTheDocument();
  });

  /**
   * Apagar favorito/VIP mexe em dado que só existe nesta máquina: a frase tem
   * que dizer o que desaparece, não perguntar "tem certeza?".
   */
  describe("confirma antes de apagar", () => {
    it("diz o que o favorito leva embora e não apaga se recusado", async () => {
      saveFavorites([
        favorite({
          vipServers: [
            { id: "v1", name: "My VIP", link: "https://vip.link/abc" },
            { id: "v2", name: "Other VIP", link: "https://vip.link/def" },
          ],
        }),
      ]);
      const { addToast } = renderFavorites();

      fireEvent.contextMenu(screen.getByText("Jailbreak"), { clientX: 5, clientY: 5 });
      await userEvent.click(await screen.findByRole("button", { name: "Remove" }));

      await waitFor(() => expect(confirmMock).toHaveBeenCalled());
      const [message, destructive] = confirmMock.mock.calls[0];
      expect(message).toContain("Jailbreak");
      expect(message).toContain("VIP servers (2)");
      expect(destructive).toBe(true);
      expect(loadFavorites()).toHaveLength(1);
      expect(addToast).not.toHaveBeenCalled();
    });

    it("nomeia o VIP que vai apagar e não apaga se recusado", async () => {
      saveFavorites([
        favorite({ vipServers: [{ id: "v1", name: "My VIP", link: "https://vip.link/abc" }] }),
      ]);
      renderFavorites();
      await userEvent.click(screen.getByText("Jailbreak"));
      await userEvent.click(screen.getByTitle("Remove"));

      await waitFor(() => expect(confirmMock).toHaveBeenCalled());
      expect(confirmMock.mock.calls[0][0]).toContain("My VIP");
      expect(confirmMock.mock.calls[0][1]).toBe(true);
      expect(loadFavorites()[0].vipServers).toHaveLength(1);
    });

    it("renomear não pergunta nada", async () => {
      saveFavorites([favorite()]);
      renderFavorites();

      promptAnswers.prompt = "Renamed";
      fireEvent.contextMenu(screen.getByText("Jailbreak"), { clientX: 5, clientY: 5 });
      await userEvent.click(await screen.findByRole("button", { name: "Rename" }));

      await waitFor(() => expect(screen.getByText("Renamed")).toBeInTheDocument());
      expect(confirmMock).not.toHaveBeenCalled();
    });
  });

  /**
   * Ver os servidores de um favorito só existia na aba Games: para escolher
   * servidor de um jogo salvo, a pessoa tinha que ir lá e pesquisar o mesmo
   * jogo de novo.
   */
  it("leva aos servidores do favorito sem lançar nada", async () => {
    saveFavorites([favorite()]);
    const { onBrowseServers, onSelectGame } = renderFavorites();

    await userEvent.click(screen.getByRole("button", { name: "Browse servers" }));

    expect(onBrowseServers).toHaveBeenCalledWith(606849621);
    expect(onSelectGame).not.toHaveBeenCalled();
  });

  it("mostra as ações da linha sem depender do hover", () => {
    saveFavorites([favorite()]);
    renderFavorites();

    const actions = screen.getByTestId("game-actions-606849621");
    expect(actions.className).not.toMatch(/opacity-0/);
    expect(actions.className).not.toMatch(/group-hover:opacity/);
  });

  /** Ver servidores não pode roubar o clique que abre/fecha o favorito. */
  it("não expande o favorito ao usar a ação da linha", async () => {
    saveFavorites([favorite()]);
    renderFavorites();

    await userEvent.click(screen.getByRole("button", { name: "Browse servers" }));

    expect(screen.queryByRole("button", { name: "Join Game" })).not.toBeInTheDocument();
  });
});

/**
 * Usar o Botting no jogo favorito exigia copiar o Place ID e abrir a tela do
 * Botting à mão. O menu do favorito passa a oferecer as ações do jogo.
 */
describe("FavoritesTab — ações do jogo no menu", () => {
  async function abrirMenu() {
    saveFavorites([favorite()]);
    const props = renderFavorites();
    fireEvent.contextMenu(screen.getByText("Jailbreak"), { clientX: 5, clientY: 5 });
    const menu = within(await screen.findByTestId("favorite-context-menu"));
    return { ...props, menu };
  }

  it("abre o Botting Mode com o jogo favorito", async () => {
    const { menu, onBotting, onSelectGame } = await abrirMenu();

    await userEvent.click(menu.getByRole("button", { name: "Botting Mode" }));

    expect(onBotting).toHaveBeenCalledWith(606849621);
    // Abrir a tela do jogo não é entrar no jogo (nem no VIP dele).
    expect(onSelectGame).not.toHaveBeenCalled();
  });

  it("abre os Scripts e os servidores com o jogo favorito", async () => {
    const { menu, onScripts, onBrowseServers, onSelectGame } = await abrirMenu();

    await userEvent.click(menu.getByRole("button", { name: "Scripts" }));
    expect(onScripts).toHaveBeenCalledWith(606849621);

    fireEvent.contextMenu(screen.getByText("Jailbreak"), { clientX: 5, clientY: 5 });
    const outra = within(await screen.findByTestId("favorite-context-menu"));
    await userEvent.click(outra.getByRole("button", { name: "Browse servers" }));
    expect(onBrowseServers).toHaveBeenCalledWith(606849621);
    expect(onSelectGame).not.toHaveBeenCalled();
  });
});

describe("RecentTab", () => {
  function renderRecent(userId: number | null = 1001) {
    const onSelectGame = vi.fn();
    const onBrowseServers = vi.fn();
    const onAddFavorite = vi.fn();
    const onBotting = vi.fn();
    const onScripts = vi.fn();
    render(
      <RecentTab
        onSelectGame={onSelectGame}
        maxRecent={8}
        userId={userId}
        onBrowseServers={onBrowseServers}
        onAddFavorite={onAddFavorite}
        onBotting={onBotting}
        onScripts={onScripts}
      />
    );
    return { onSelectGame, onBrowseServers, onAddFavorite, onBotting, onScripts };
  }

  it("shows the empty state when nothing has been played", () => {
    renderRecent();
    expect(screen.getByText("No recent games")).toBeInTheDocument();
  });

  it("lists recent games with their place id and cap", () => {
    saveRecentGames([recent()]);
    renderRecent();
    expect(screen.getByText("Adopt Me")).toBeInTheDocument();
    expect(screen.getByText("ID: 920587237")).toBeInTheDocument();
    expect(screen.getByText("1 of 8 max")).toBeInTheDocument();
  });

  it("selects a game with its cached name and icon", async () => {
    saveRecentGames([recent()]);
    const { onSelectGame } = renderRecent();
    await userEvent.click(screen.getByText("Adopt Me"));
    expect(onSelectGame).toHaveBeenCalledWith(920587237, "Adopt Me", "https://example.invalid/icon.png");
  });

  it("formats how long ago each game was played", () => {
    saveRecentGames([
      recent({ placeId: 1, name: "Now", lastPlayed: Date.now() }),
      recent({ placeId: 2, name: "Minutes", lastPlayed: Date.now() - 5 * 60_000 }),
      recent({ placeId: 3, name: "Hours", lastPlayed: Date.now() - 3 * 3_600_000 }),
      recent({ placeId: 4, name: "Days", lastPlayed: Date.now() - 2 * 86_400_000 }),
    ]);
    renderRecent();
    expect(screen.getByText("just now")).toBeInTheDocument();
    expect(screen.getByText("5m ago")).toBeInTheDocument();
    expect(screen.getByText("3h ago")).toBeInTheDocument();
    expect(screen.getByText("2d ago")).toBeInTheDocument();
  });

  it("clears the whole list", async () => {
    saveRecentGames([recent()]);
    renderRecent();
    // Apagar a lista é destrutivo: passa pelo confirm.
    promptAnswers.confirm = true;
    await userEvent.click(screen.getByRole("button", { name: "Clear all" }));
    await waitFor(() => expect(screen.getByText("No recent games")).toBeInTheDocument());
    expect(localStorage.getItem("ram_recent_games")).toBe("[]");
  });

  /** A lista só existe nesta máquina: apagada, não volta. */
  it("diz quantos jogos apaga antes de limpar e não limpa se recusado", async () => {
    saveRecentGames([recent({ placeId: 1, name: "One" }), recent({ placeId: 2, name: "Two" })]);
    renderRecent();

    await userEvent.click(screen.getByRole("button", { name: "Clear all" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    const [message, destructive] = confirmMock.mock.calls[0];
    expect(message).toContain("2");
    expect(message).toContain("cannot be recovered");
    expect(destructive).toBe(true);
    expect(screen.getByText("One")).toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem("ram_recent_games") || "[]")).toHaveLength(2);
  });

  /**
   * A lista de recentes só sabia lançar: nem servidores, nem favoritar, nem
   * menu de contexto. Agora oferece as mesmas ações das outras listas.
   */
  it("leva aos servidores do jogo recente sem lançar nada", async () => {
    saveRecentGames([recent()]);
    const { onBrowseServers, onSelectGame } = renderRecent();

    await userEvent.click(screen.getByRole("button", { name: "Browse servers" }));

    expect(onBrowseServers).toHaveBeenCalledWith(920587237);
    expect(onSelectGame).not.toHaveBeenCalled();
  });

  it("favorita um jogo recente pela ação da linha", async () => {
    saveRecentGames([recent()]);
    const { onAddFavorite, onSelectGame } = renderRecent();

    await userEvent.click(screen.getByRole("button", { name: "Favorite" }));

    expect(onAddFavorite).toHaveBeenCalledWith(
      expect.objectContaining({ placeId: 920587237, name: "Adopt Me" })
    );
    expect(onSelectGame).not.toHaveBeenCalled();
  });

  it("mostra as ações da linha sem depender do hover", () => {
    saveRecentGames([recent()]);
    renderRecent();

    const actions = screen.getByTestId("game-actions-920587237");
    expect(actions.className).not.toMatch(/opacity-0/);
    expect(actions.className).not.toMatch(/group-hover:opacity/);
  });

  it("backfills a missing game name from the backend", async () => {
    saveRecentGames([recent({ placeId: 555, name: "555", iconUrl: null })]);
    setInvokeHandler((cmd) => {
      if (cmd === "get_place_details") return [{ name: "Resolved Game" }];
      if (cmd === "batched_get_game_icon") return "https://example.invalid/resolved.png";
      return undefined;
    });
    renderRecent();
    expect(await screen.findByText("Resolved Game")).toBeInTheDocument();
  });

  /** Era a única lista de jogos sem clique direito. */
  describe("menu de contexto do jogo recente", () => {
    async function abrirMenu() {
      saveRecentGames([recent()]);
      const props = renderRecent();
      fireEvent.contextMenu(screen.getByText("Adopt Me"), { clientX: 5, clientY: 5 });
      const menu = within(await screen.findByTestId("game-context-menu"));
      return { ...props, menu };
    }

    it("abre o Botting Mode com o jogo recente", async () => {
      const { menu, onBotting, onSelectGame } = await abrirMenu();

      await userEvent.click(menu.getByRole("button", { name: "Botting Mode" }));

      expect(onBotting).toHaveBeenCalledWith(920587237);
      expect(onSelectGame).not.toHaveBeenCalled();
    });

    it("abre os Scripts com o jogo recente", async () => {
      const { menu, onScripts } = await abrirMenu();

      await userEvent.click(menu.getByRole("button", { name: "Scripts" }));

      expect(onScripts).toHaveBeenCalledWith(920587237);
    });

    it("favorita pelo menu", async () => {
      const { menu, onAddFavorite } = await abrirMenu();

      await userEvent.click(menu.getByRole("button", { name: "Favorite" }));

      expect(onAddFavorite).toHaveBeenCalledWith(
        expect.objectContaining({ placeId: 920587237, name: "Adopt Me" })
      );
    });

    it("entra pelo menu com o nome e o ícone que já estavam em cache", async () => {
      const { menu, onSelectGame } = await abrirMenu();

      await userEvent.click(menu.getByRole("button", { name: "Join Game" }));

      // Mesmo caminho do clique na linha: nome e ícone vão com o place.
      expect(onSelectGame).toHaveBeenCalledWith(920587237, "Adopt Me", expect.anything());
    });
  });
});
