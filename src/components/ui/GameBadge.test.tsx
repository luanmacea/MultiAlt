import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { GameBadge } from "./GameBadge";

afterEach(cleanup);

describe("GameBadge", () => {
  it("não desenha nada enquanto o jogo é desconhecido", () => {
    const { container } = render(<GameBadge name={null} iconUrl={null} placeId={606849621} />);
    // Um "Place 606849621" só repetiria o número que já está no campo ao lado.
    expect(container).toBeEmptyDOMElement();
  });

  it("mostra nome e ícone do jogo quando dá para saber", () => {
    render(<GameBadge name="Jailbreak" iconUrl="https://tr.rbxcdn.com/jb.png" placeId={606849621} />);

    expect(screen.getByText("Jailbreak")).toBeInTheDocument();
    const icon = screen.getByRole("presentation", { hidden: true });
    expect(icon).toHaveAttribute("src", "https://tr.rbxcdn.com/jb.png");
    // Nome longo é truncado no CSS, então o texto inteiro tem que estar no title.
    expect(screen.getByTestId("game-badge")).toHaveAttribute("title", "Jailbreak");
  });

  it("aparece só com o ícone quando o nome não veio, e só com o nome quando o ícone não veio", () => {
    const { rerender } = render(<GameBadge name={null} iconUrl="https://tr.rbxcdn.com/jb.png" placeId={1} />);
    expect(screen.getByTestId("game-badge")).toBeInTheDocument();
    expect(screen.queryByText("Place 1")).not.toBeInTheDocument();

    rerender(<GameBadge name="Jailbreak" iconUrl={null} />);
    expect(screen.getByText("Jailbreak")).toBeInTheDocument();
    expect(screen.queryByRole("presentation", { hidden: true })).not.toBeInTheDocument();
  });
});

it("aceita ícone menor para caber numa linha de rótulo", () => {
  render(<GameBadge name="Jailbreak" iconUrl="https://tr.rbxcdn.com/jb.png" iconSize={12} />);
  // Numa linha de 10px um ícone de 18px empurra o campo e desalinha a coluna vizinha.
  const icon = screen.getByRole("presentation", { hidden: true });
  expect(icon).toHaveAttribute("width", "12");
  expect(icon).toHaveStyle({ width: "12px", height: "12px" });
});
