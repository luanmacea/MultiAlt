import { afterEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { useState } from "react";
import { useMouseNavigation } from "./useMouseNavigation";
import { escapeStackDepth, useEscapeStack } from "./useEscapeStack";

/** Botões laterais do mouse: 3 = Voltar (XButton1), 4 = Avançar (XButton2). */
const BACK = 3;
const FORWARD = 4;

/** Um clique do botão lateral como o WebView2 entrega: mousedown, mouseup, auxclick. */
function pressMouse(button: number, target: EventTarget = window) {
  const events = ["mousedown", "mouseup", "auxclick"].map(
    (type) => new MouseEvent(type, { button, bubbles: true, cancelable: true })
  );
  act(() => {
    for (const event of events) target.dispatchEvent(event);
  });
  return events;
}

function pressAltArrow(key: "ArrowLeft" | "ArrowRight", target: EventTarget = window) {
  const event = new KeyboardEvent("keydown", { key, altKey: true, bubbles: true, cancelable: true });
  act(() => {
    target.dispatchEvent(event);
  });
  return event;
}

function setup(initial = "accounts", enabled = true) {
  const hook = renderHook(
    ({ on }) => {
      const [page, setPage] = useState(initial);
      useMouseNavigation({ enabled: on, page, setPage });
      return { page, setPage };
    },
    { initialProps: { on: enabled } }
  );
  const go = (page: string) => act(() => hook.result.current.setPage(page));
  return { hook, go, page: () => hook.result.current.page };
}

afterEach(() => {
  expect(escapeStackDepth()).toBe(0);
});

describe("useMouseNavigation", () => {
  it("Voltar sem nada para trás não faz nada", () => {
    const { hook, page } = setup();
    pressMouse(BACK);
    expect(page()).toBe("accounts");
    hook.unmount();
  });

  it("Voltar e Avançar andam pelo histórico de páginas", () => {
    const { hook, go, page } = setup();
    go("settings");
    go("session");

    pressMouse(BACK);
    expect(page()).toBe("settings");
    pressMouse(BACK);
    expect(page()).toBe("accounts");

    pressMouse(FORWARD);
    expect(page()).toBe("settings");
    pressMouse(FORWARD);
    expect(page()).toBe("session");
    hook.unmount();
  });

  it("navegar depois de voltar apaga o Avançar", () => {
    const { hook, go, page } = setup();
    go("settings");
    go("session");
    pressMouse(BACK);
    go("theme");

    pressMouse(FORWARD);
    expect(page()).toBe("theme");
    pressMouse(BACK);
    expect(page()).toBe("settings");
    hook.unmount();
  });

  it("Voltar fecha o que está aberto por cima (topo da pilha de Escape) antes de trocar de página", () => {
    const { hook, go, page } = setup();
    go("settings");
    const fecharDialogo = vi.fn();
    const dialogo = renderHook(({ open }) => useEscapeStack(open, fecharDialogo), {
      initialProps: { open: true },
    });

    pressMouse(BACK);
    expect(fecharDialogo).toHaveBeenCalledTimes(1);
    expect(page()).toBe("settings");

    dialogo.rerender({ open: false });
    pressMouse(BACK);
    expect(fecharDialogo).toHaveBeenCalledTimes(1);
    expect(page()).toBe("accounts");

    dialogo.unmount();
    hook.unmount();
  });

  it("a casca da página e a seleção da lista não contam como 'aberto por cima'", () => {
    // PageShell (Escape = sair para Accounts) e AccountList (Escape = limpar
    // seleção) ficam na base da pilha. O Voltar tem que ir para a página
    // anterior, não para Accounts nem só limpar a seleção.
    const { hook, go, page } = setup();
    go("settings");
    go("session");
    const sairDaPagina = vi.fn();
    const casca = renderHook(() => useEscapeStack(true, sairDaPagina, { closesOnBack: false }));

    pressMouse(BACK);
    expect(sairDaPagina).not.toHaveBeenCalled();
    expect(page()).toBe("settings");

    casca.unmount();
    hook.unmount();
  });

  it("um clique do botão lateral age uma vez só (mouseup e auxclick do mesmo clique)", () => {
    const { hook, go, page } = setup();
    go("settings");
    go("session");
    go("theme");

    pressMouse(BACK);
    expect(page()).toBe("session");
    hook.unmount();
  });

  it("bloqueia o comportamento próprio do WebView2 nos botões laterais", () => {
    const { hook } = setup();
    for (const event of pressMouse(BACK)) expect(event.defaultPrevented).toBe(true);
    for (const event of pressMouse(FORWARD)) expect(event.defaultPrevented).toBe(true);
    hook.unmount();
  });

  it("não mexe nos outros botões do mouse", () => {
    const { hook, go, page } = setup();
    go("settings");
    for (const button of [0, 1, 2]) {
      for (const event of pressMouse(button)) expect(event.defaultPrevented).toBe(false);
    }
    expect(page()).toBe("settings");
    hook.unmount();
  });

  it("Alt+Esquerda / Alt+Direita fazem o mesmo que os botões", () => {
    const { hook, go, page } = setup();
    go("settings");

    const back = pressAltArrow("ArrowLeft");
    expect(back.defaultPrevented).toBe(true);
    expect(page()).toBe("accounts");

    pressAltArrow("ArrowRight");
    expect(page()).toBe("settings");
    hook.unmount();
  });

  it("Alt+setas dentro de um campo de texto ficam com o campo", () => {
    const { hook, go, page } = setup();
    go("settings");
    const input = document.createElement("input");
    const textarea = document.createElement("textarea");
    document.body.append(input, textarea);

    for (const field of [input, textarea]) {
      const event = pressAltArrow("ArrowLeft", field);
      expect(event.defaultPrevented).toBe(false);
    }
    expect(page()).toBe("settings");

    input.remove();
    textarea.remove();
    hook.unmount();
  });

  it("setas sem Alt, ou com Ctrl/Shift junto, não navegam", () => {
    const { hook, go, page } = setup();
    go("settings");
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }));
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "ArrowLeft", altKey: true, ctrlKey: true, bubbles: true })
      );
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "ArrowLeft", altKey: true, shiftKey: true, bubbles: true })
      );
    });
    expect(page()).toBe("settings");
    hook.unmount();
  });

  it("desligado (app trancado, tela de senha) não navega", () => {
    const { hook, go, page } = setup("accounts", true);
    go("settings");
    hook.rerender({ on: false });
    pressMouse(BACK);
    pressAltArrow("ArrowLeft");
    expect(page()).toBe("settings");
    hook.unmount();
  });

  it("para de escutar ao desmontar", () => {
    const { hook, go, page } = setup();
    go("settings");
    const before = page();
    hook.unmount();
    const events = pressMouse(BACK);
    expect(before).toBe("settings");
    expect(events[0].defaultPrevented).toBe(false);
  });
});
