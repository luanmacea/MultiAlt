import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { ServerListDialog } from "./ServerListDialog";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { promptAnswers, resetPromptMocks } from "../../test-utils/promptMocks";
import { loadFavorites, saveFavorites, saveRecentGames } from "./types";

const ACCOUNT = makeAccount({ UserID: 1001, Username: "alpha" });

function renderDialog() {
  const store = setStore({
    accounts: [ACCOUNT],
    selectedIds: new Set([1001]),
    selectedAccounts: [ACCOUNT],
    selectedAccount: ACCOUNT,
  });
  render(<ServerListDialog open onClose={vi.fn()} />);
  return store;
}

/** O quadro do diálogo: o painel que contém o título. */
function quadro(): HTMLElement {
  const painel = screen.getByRole("heading", { name: "Server List" }).closest(".rounded-2xl");
  if (!painel) throw new Error("painel do Server List não encontrado");
  return painel as HTMLElement;
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  localStorage.clear();
  setInvokeHandler(() => null);
});

afterEach(cleanup);

describe("ServerListDialog — cabe na janela", () => {
  /**
   * O diálogo tinha tamanho fixo (`w-[680px] h-[560px]`) e nada que o limitasse
   * à janela. Na janela mínima do app (750x450, `tauri.conf.json`) sobravam
   * 110 px divididos acima e abaixo: título e X ficavam **fora da tela** (o X em
   * y = -38) e, na base, os campos Teleport e Find player também — sem rolagem
   * que os alcançasse (`body` é `overflow: hidden`). Medido no harness.
   *
   * O jsdom não calcula layout: isto trava o teto que o conserto depende, o
   * mesmo dos outros diálogos grandes (Auto Rejoin, AFK, Generator). A prova de
   * que cabe está nas medidas do relatório da Frente C.
   */
  it("o quadro nunca passa do tamanho da janela", () => {
    renderDialog();
    const classes = quadro().className.split(/\s+/);
    expect(classes).toContain("max-h-[calc(100vh-24px)]");
    expect(classes).toContain("max-w-[calc(100vw-24px)]");
  });

  it("o miolo encolhe com o quadro em vez de empurrar a base para fora", () => {
    renderDialog();
    // Entre o quadro e a aba: o miolo é `flex-1 min-h-0`, então a lista de
    // servidores (que rola por dentro) é quem cede altura, não os campos.
    const miolo = quadro().lastElementChild as HTMLElement;
    const classes = miolo.className.split(/\s+/);
    expect(classes).toContain("flex-1");
    expect(classes).toContain("min-h-0");
  });
});

describe("ServerListDialog — menu do jogo completo, como na Choose Game", () => {
  /**
   * O Server List e a Choose Game ficaram espelhados pela metade: o Server List
   * tinha a coluna de servidores recentes, mas o menu do jogo só com Join Game,
   * Favorite e Copy Place ID (medido no harness); a Choose Game, o contrário.
   * O menu completo — Browse servers, Auto Rejoin, Scripts — passa a existir
   * nas duas telas.
   */
  const ACOES_DO_JOGO = ["Browse servers", "Auto Rejoin", "Scripts"];

  beforeEach(() => {
    setInvokeHandler((cmd) =>
      cmd === "search_games"
        ? {
            sorts: [
              { games: [{ rootPlaceId: 606849621, universeId: 1, name: "Jailbreak", playerCount: 1200 }] },
            ],
          }
        : null
    );
  });

  async function menuDoJogo(aba: "Games" | "Recent", nome: string) {
    await userEvent.click(screen.getByRole("button", { name: aba }));
    fireEvent.contextMenu(await screen.findByText(nome), { clientX: 5, clientY: 5 });
    return within(await screen.findByTestId("game-context-menu"));
  }

  it("na aba Games o menu tem as ações do jogo", async () => {
    renderDialog();
    const menu = await menuDoJogo("Games", "Jailbreak");
    for (const nome of ["Join Game", ...ACOES_DO_JOGO, "Favorite", "Copy Place ID"]) {
      expect(menu.getByRole("button", { name: nome })).toBeInTheDocument();
    }
  });

  it("Auto Rejoin abre já com este jogo", async () => {
    const store = renderDialog();
    const menu = await menuDoJogo("Games", "Jailbreak");
    await userEvent.click(menu.getByRole("button", { name: "Auto Rejoin" }));
    expect(store.setPlaceId).toHaveBeenCalledWith("606849621");
    expect(store.openBottingDialog).toHaveBeenCalledWith("606849621");
  });

  it("Scripts abre com este jogo como place atual", async () => {
    const store = renderDialog();
    const menu = await menuDoJogo("Games", "Jailbreak");
    await userEvent.click(menu.getByRole("button", { name: "Scripts" }));
    expect(store.setPlaceId).toHaveBeenCalledWith("606849621");
    expect(store.setScriptsOpen).toHaveBeenCalledWith(true);
  });

  it("Browse servers leva à aba Servers com o place do jogo", async () => {
    renderDialog();
    const menu = await menuDoJogo("Games", "Jailbreak");
    await userEvent.click(menu.getByRole("button", { name: "Browse servers" }));
    expect(await screen.findByPlaceholderText("Enter Place ID")).toHaveValue("606849621");
  });

  it("na aba Recent o menu é o completo, e o Favorite não é item morto", async () => {
    saveRecentGames([{ placeId: 920587237, name: "Adopt Me", iconUrl: null, lastPlayed: Date.now() }]);
    promptAnswers.prompt = "Adopt Me";
    renderDialog();
    const menu = await menuDoJogo("Recent", "Adopt Me");
    for (const nome of ACOES_DO_JOGO) {
      expect(menu.getByRole("button", { name: nome })).toBeInTheDocument();
    }

    await userEvent.click(menu.getByRole("button", { name: "Favorite" }));
    await waitFor(() => expect(loadFavorites().map((f) => f.placeId)).toContain(920587237));
  });

  it("na aba Favorites o menu tem as ações do jogo", async () => {
    saveFavorites([
      { placeId: 606849621, name: "Jailbreak", iconUrl: null, addedAt: Date.now(), vipServers: [] },
    ]);
    renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Favorites" }));
    fireEvent.contextMenu(await screen.findByText("Jailbreak"), { clientX: 5, clientY: 5 });
    const menu = within(await screen.findByTestId("favorite-context-menu"));
    for (const nome of ACOES_DO_JOGO) {
      expect(menu.getByRole("button", { name: nome })).toBeInTheDocument();
    }
  });

  it("a coluna de servidores recentes continua aqui", async () => {
    renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Recent" }));
    expect(await screen.findByText("No recent servers")).toBeInTheDocument();
  });
});
