import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { RecentGamesList } from "./RecentGamesList";
import { saveRecentGames } from "./types";
import { resetPromptMocks } from "../../test-utils/promptMocks";

/** Já com nome e ícone: `resolveRecentGame` não dispara rede nenhuma. */
const GAME = { placeId: 606849621, name: "Jailbreak", iconUrl: "icon.png", lastPlayed: Date.now() };

function renderRecent(onSelect = vi.fn()) {
  saveRecentGames([GAME]);
  render(<RecentGamesList userId={null} maxRecent={10} onSelect={onSelect} onBrowseServers={vi.fn()} />);
  return onSelect;
}

beforeEach(() => {
  localStorage.clear();
  resetPromptMocks();
});

afterEach(cleanup);

describe("RecentGamesList — teclado", () => {
  /**
   * A linha era um `<div onClick>`: só o mouse chegava nela. As mesmas ações
   * de Games (ver servidores, favoritar, entrar) já existem aqui — faltava
   * dar foco e teclado à linha em si.
   */
  it("é alcançável por Tab e ativa com Enter", async () => {
    const onSelect = renderRecent();
    const row = screen.getByRole("button", { name: /Jailbreak/ });
    expect(row).toHaveAttribute("tabIndex", "0");

    row.focus();
    await userEvent.keyboard("{Enter}");
    expect(onSelect).toHaveBeenCalledWith(606849621, "Jailbreak", "icon.png");
  });

  it("ativa com Espaço", async () => {
    const onSelect = renderRecent();
    screen.getByRole("button", { name: /Jailbreak/ }).focus();
    await userEvent.keyboard(" ");
    expect(onSelect).toHaveBeenCalledWith(606849621, "Jailbreak", "icon.png");
  });

  it("não dispara a linha ao ativar um botão de ação por teclado", async () => {
    const onSelect = renderRecent();
    const joinButton = screen.getByRole("button", { name: "Join Game" });
    joinButton.focus();
    await userEvent.keyboard("{Enter}");
    // O próprio botão de ação chama onSelect (é a ação "Join Game"), mas só
    // uma vez — a linha não deve reagir de novo ao Enter que já ativou o botão.
    expect(onSelect).toHaveBeenCalledTimes(1);
  });
});
