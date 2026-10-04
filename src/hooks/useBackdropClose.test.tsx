import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useBackdropClose } from "./useBackdropClose";

function Modal({ onClose }: { onClose?: () => void }) {
  const backdrop = useBackdropClose(onClose);
  return (
    <div data-testid="backdrop" {...backdrop}>
      <div data-testid="panel">
        <input data-testid="field" />
      </div>
    </div>
  );
}

afterEach(cleanup);

describe("useBackdropClose", () => {
  it("closes when the press and the release both happen on the backdrop", () => {
    const onClose = vi.fn();
    render(<Modal onClose={onClose} />);
    const backdrop = screen.getByTestId("backdrop");
    fireEvent.mouseDown(backdrop);
    fireEvent.mouseUp(backdrop);
    fireEvent.click(backdrop);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("stays open when the press starts in an input and the release lands on the backdrop", () => {
    // É o caso do pedido: selecionar o texto de um campo arrastando para fora
    // do painel. O navegador dispara o click no ancestral comum — o fundo.
    const onClose = vi.fn();
    render(<Modal onClose={onClose} />);
    fireEvent.mouseDown(screen.getByTestId("field"));
    const backdrop = screen.getByTestId("backdrop");
    fireEvent.mouseUp(backdrop);
    fireEvent.click(backdrop);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("stays open when the press starts on the backdrop and the release lands in the panel", () => {
    const onClose = vi.fn();
    render(<Modal onClose={onClose} />);
    fireEvent.mouseDown(screen.getByTestId("backdrop"));
    fireEvent.click(screen.getByTestId("panel"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("does not carry an earlier backdrop press over to a later click", () => {
    const onClose = vi.fn();
    render(<Modal onClose={onClose} />);
    const backdrop = screen.getByTestId("backdrop");
    fireEvent.mouseDown(backdrop);
    fireEvent.click(screen.getByTestId("panel"));
    fireEvent.mouseDown(screen.getByTestId("field"));
    fireEvent.click(backdrop);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("ignores clicks inside the panel", () => {
    const onClose = vi.fn();
    render(<Modal onClose={onClose} />);
    fireEvent.mouseDown(screen.getByTestId("field"));
    fireEvent.click(screen.getByTestId("field"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("does nothing without a close handler (dialog that cannot be dismissed right now)", () => {
    render(<Modal />);
    const backdrop = screen.getByTestId("backdrop");
    expect(() => {
      fireEvent.mouseDown(backdrop);
      fireEvent.click(backdrop);
    }).not.toThrow();
  });
});

// --- Guarda: nenhum fundo de modal volta a fechar com onClick cru -----------

const SRC = path.join(process.cwd(), "src");

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const full = path.join(dir, name);
    if (statSync(full).isDirectory()) {
      out.push(...sourceFiles(full));
    } else if (/\.tsx$/.test(name) && !/\.test\.tsx$/.test(name)) {
      out.push(full);
    }
  }
  return out;
}

/** Texto da tag JSX de abertura que contém `index` (de `<` até o `>` que a fecha). */
function openingTagAround(source: string, index: number): string {
  const start = source.lastIndexOf("<", index);
  let depth = 0;
  let quote: string | null = null;
  for (let i = start + 1; i < source.length; i++) {
    const ch = source[i];
    if (quote) {
      if (ch === quote) quote = null;
      continue;
    }
    if (ch === '"' || ch === "'" || ch === "`") quote = ch;
    else if (ch === "{") depth++;
    else if (ch === "}") depth--;
    else if (ch === ">" && depth === 0) return source.slice(start, i + 1);
  }
  return source.slice(start);
}

describe("backdrop guard", () => {
  // Um fundo escurecido (`fixed inset-0 ... bg-black/NN`) é o que fecha o modal
  // ao clicar fora. Com `onClick` cru, apertar dentro de um campo e soltar fora
  // fecha o modal — o click cai no ancestral comum, que é o fundo.
  const overlays = sourceFiles(SRC).flatMap((file) => {
    const source = readFileSync(file, "utf8");
    const found: { file: string; tag: string }[] = [];
    const re = /fixed inset-0[^"`]*bg-black\/\d+/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(source))) {
      found.push({ file: path.relative(SRC, file), tag: openingTagAround(source, m.index) });
    }
    return found;
  });

  it("finds the app's modal backdrops", () => {
    expect(overlays.length).toBeGreaterThanOrEqual(10);
  });

  it.each(overlays.map((o) => [o.file, o.tag] as const))(
    "%s closes through useBackdropClose, not a raw onClick",
    (_file, tag) => {
      expect(tag).not.toMatch(/\bonClick=/);
      expect(tag).not.toMatch(/\bonMouseUp=/);
      expect(tag).toMatch(/\{\.\.\.\w+\}/);
    }
  );
});
