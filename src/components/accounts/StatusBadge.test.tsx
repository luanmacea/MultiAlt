import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { StatusBadge, STATUS_KINDS, type StatusKind } from "./StatusBadge";

/**
 * Estado da conta não pode depender só da cor (pedido do dono, 08/10/2026,
 * depois do ACCESSIBILITY.md): quem não distingue vermelho de laranja ou verde
 * de azul precisa de outra pista. Cada estado tem um ícone próprio.
 */
describe("StatusBadge", () => {
  afterEach(() => cleanup());

  it("draws a different icon for every state", () => {
    const icons = STATUS_KINDS.map((kind) => {
      render(<StatusBadge kind={kind} label={kind} />);
      const badge = screen.getByRole("img", { name: kind });
      const svg = badge.querySelector("svg");
      expect(svg, kind).not.toBeNull();
      const iconClass = [...svg!.classList].find((c) => c.startsWith("lucide-")) ?? "";
      cleanup();
      return iconClass;
    });
    expect(new Set(icons).size).toBe(STATUS_KINDS.length);
  });

  it("keeps the label for screen readers and the state for styling", () => {
    render(<StatusBadge kind="invalid" label="Invalid session" />);
    const badge = screen.getByRole("img", { name: "Invalid session" });
    expect(badge).toHaveAttribute("data-status", "invalid");
  });

  it("uses the color it is given (the aging dot changes color with age)", () => {
    render(<StatusBadge kind={"aged" satisfies StatusKind} label="aged" color="rgb(1, 2, 3)" />);
    expect(screen.getByRole("img", { name: "aged" })).toHaveStyle({ backgroundColor: "rgb(1, 2, 3)" });
  });
});
