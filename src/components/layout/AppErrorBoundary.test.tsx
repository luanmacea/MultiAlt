import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { AppErrorBoundary } from "./AppErrorBoundary";

/**
 * Um erro de render não pode virar tela branca silenciosa: o app é WebView2, e
 * tela branca é exatamente o sintoma que o usuário não consegue diagnosticar —
 * ele não tem como saber se quebrou o React ou o próprio WebView2.
 */
function Boom({ message = "quebrou no render" }: { message?: string }): never {
  throw new Error(message);
}

/** O boundary é o único lugar do app que espera um throw: silencia o ruído. */
let consoleError: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  consoleError.mockRestore();
  cleanup();
});

describe("AppErrorBoundary", () => {
  it("deixa a árvore passar quando nada quebra", () => {
    render(
      <AppErrorBoundary>
        <div>conteúdo do app</div>
      </AppErrorBoundary>
    );
    expect(screen.getByText("conteúdo do app")).toBeInTheDocument();
  });

  it("mostra a tela de erro em vez de sumir com a interface", () => {
    render(
      <AppErrorBoundary>
        <Boom />
      </AppErrorBoundary>
    );
    expect(screen.getByText("Something broke in the interface")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reload interface" })).toBeInTheDocument();
  });

  it("mostra a mensagem do erro, que é a única pista que o usuário tem", () => {
    render(
      <AppErrorBoundary>
        <Boom message="cookie sem dono" />
      </AppErrorBoundary>
    );
    expect(screen.getByText(/cookie sem dono/)).toBeInTheDocument();
  });

  it("não perde o que foi jogado quando não é um Error", () => {
    function ThrowString(): never {
      // eslint-disable-next-line @typescript-eslint/no-throw-literal
      throw "string crua";
    }
    render(
      <AppErrorBoundary>
        <ThrowString />
      </AppErrorBoundary>
    );
    expect(screen.getByText(/string crua/)).toBeInTheDocument();
  });

  it("o botão recarrega a interface", async () => {
    const reload = vi.fn();
    render(
      <AppErrorBoundary onReload={reload}>
        <Boom />
      </AppErrorBoundary>
    );
    await userEvent.click(screen.getByRole("button", { name: "Reload interface" }));
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("registra o erro no console, que é o que sobra para diagnosticar depois", () => {
    render(
      <AppErrorBoundary>
        <Boom message="para o log" />
      </AppErrorBoundary>
    );
    const calls = consoleError.mock.calls as unknown as unknown[][];
    const logged = calls.some((call) => call.some((arg) => String(arg).includes("para o log")));
    expect(logged).toBe(true);
  });
});
