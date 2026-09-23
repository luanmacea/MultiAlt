import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { FavoritesTab } from "./FavoritesTab";
import { RecentTab } from "./RecentTab";
import { loadFavorites, saveFavorites, saveRecentGames } from "./types";
import type { FavoriteGame, RecentGame } from "./types";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { promptAnswers, promptMock, resetPromptMocks } from "../../test-utils/promptMocks";

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
  render(<FavoritesTab onSelectGame={onSelectGame} addToast={addToast} />);
  return { onSelectGame, addToast };
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
    expect(screen.getByText("Right-click a game in the Games tab to add one")).toBeInTheDocument();
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

  it("adds a VIP server through two prompts and persists it", async () => {
    saveFavorites([favorite()]);
    const { addToast } = renderFavorites();
    await userEvent.click(screen.getByText("Jailbreak"));

    promptMock.mockImplementationOnce(async () => "https://vip.link/new");
    promptMock.mockImplementationOnce(async () => "Squad server");
    await userEvent.click(screen.getByRole("button", { name: "Add VIP Server" }));

    await waitFor(() => expect(addToast).toHaveBeenCalledWith("VIP server added"));
    expect(screen.getByText("Squad server")).toBeInTheDocument();
    expect(loadFavorites()[0].vipServers?.[0].link).toBe("https://vip.link/new");
  });

  it("does not add a VIP server when the link prompt is cancelled", async () => {
    saveFavorites([favorite()]);
    const { addToast } = renderFavorites();
    await userEvent.click(screen.getByText("Jailbreak"));

    promptAnswers.prompt = null;
    await userEvent.click(screen.getByRole("button", { name: "Add VIP Server" }));
    await Promise.resolve();
    expect(addToast).not.toHaveBeenCalled();
  });

  it("removes a VIP server and persists the removal", async () => {
    saveFavorites([
      favorite({ vipServers: [{ id: "v1", name: "My VIP", link: "https://vip.link/abc" }] }),
    ]);
    const { addToast } = renderFavorites();
    await userEvent.click(screen.getByText("Jailbreak"));
    await userEvent.click(screen.getByTitle("Remove"));

    expect(addToast).toHaveBeenCalledWith("VIP server removed");
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

    fireEvent.contextMenu(screen.getByText("Renamed"), { clientX: 5, clientY: 5 });
    await userEvent.click(await screen.findByRole("button", { name: "Remove" }));
    expect(addToast).toHaveBeenCalledWith("Removed from favorites");
    expect(screen.getByText("No favorites yet")).toBeInTheDocument();
  });
});

describe("RecentTab", () => {
  function renderRecent(userId: number | null = 1001) {
    const onSelectGame = vi.fn();
    render(<RecentTab onSelectGame={onSelectGame} maxRecent={8} userId={userId} />);
    return { onSelectGame };
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
    await userEvent.click(screen.getByRole("button", { name: "Clear all" }));
    expect(screen.getByText("No recent games")).toBeInTheDocument();
    expect(localStorage.getItem("ram_recent_games")).toBe("[]");
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
});
