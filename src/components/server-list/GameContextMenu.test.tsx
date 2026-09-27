import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { GameContextMenu } from "./GameContextMenu";

const GAME = { placeId: 606849621, name: "Jailbreak", playerCount: 0, likeRatio: null, iconUrl: null };

function props(overrides: Partial<Parameters<typeof GameContextMenu>[0]> = {}) {
  return {
    x: 10,
    y: 10,
    game: GAME,
    onClose: vi.fn(),
    onJoin: vi.fn(),
    onFavorite: vi.fn(),
    onCopyPlaceId: vi.fn(),
    onBrowseServers: vi.fn(),
    onBotting: vi.fn(),
    onScripts: vi.fn(),
    ...overrides,
  };
}

afterEach(cleanup);

describe("GameContextMenu — os botões não remontam", () => {
  /**
   * O componente de cada linha (`Item`) era declarado **dentro** do corpo do
   * menu: a cada render era um tipo novo, e o React desmontava e remontava os
   * botões. Como as telas donas do menu leem a store inteira, e a store se
   * atualiza sozinha a cada poucos segundos, com o menu aberto e parado os
   * botões eram recriados — no harness, 24 remoções de botão em 8 s. Um clique
   * que caísse na troca se perdia, e o hover piscava.
   */
  it("o mesmo nó de botão sobrevive a um re-render de quem abriu o menu", () => {
    const inicial = props();
    const { rerender } = render(<GameContextMenu {...inicial} />);
    const antes = screen.getAllByRole("button");

    // O dono re-renderiza (store mudou) e passa callbacks novos, como faz de verdade.
    rerender(<GameContextMenu {...inicial} onClose={vi.fn()} onJoin={vi.fn()} />);
    const depois = screen.getAllByRole("button");

    expect(depois).toHaveLength(antes.length);
    depois.forEach((botao, i) => expect(botao).toBe(antes[i]));
  });

  it("depois do re-render o clique usa o callback novo e fecha o menu", async () => {
    const inicial = props();
    const { rerender } = render(<GameContextMenu {...inicial} />);
    const onJoin = vi.fn();
    const onClose = vi.fn();
    rerender(<GameContextMenu {...inicial} onJoin={onJoin} onClose={onClose} />);

    await userEvent.click(screen.getByRole("button", { name: "Join Game" }));

    expect(onJoin).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(inicial.onJoin).not.toHaveBeenCalled();
  });

  it("o title de cada item leva o nome do jogo", () => {
    render(<GameContextMenu {...props()} />);
    expect(screen.getByRole("button", { name: "Auto Rejoin" })).toHaveAttribute(
      "title",
      "Auto Rejoin — Jailbreak"
    );
  });
});
