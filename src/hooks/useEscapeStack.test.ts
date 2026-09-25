import { afterEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { escapeStackDepth, useEscapeStack } from "./useEscapeStack";

/** Manda um Escape "de fora", como o app recebe de verdade (listener de `window`). */
function pressEscape(target?: EventTarget, init: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true, ...init });
  act(() => {
    (target ?? window).dispatchEvent(event);
  });
  return event;
}

afterEach(() => {
  expect(escapeStackDepth()).toBe(0);
});

/**
 * Antes desta pilha, 26 handlers de Escape espalhados pelo app escutavam
 * `window`/`document` e **nenhum** interrompia a propagação: um Escape com um
 * diálogo aberto sobre a Choose Game fechava os dois, e na lista de contas ele
 * limpava a seleção junto com o menu de contexto.
 */
describe("useEscapeStack", () => {
  it("entrega o Escape só para o topo", () => {
    const debaixo = vi.fn();
    const emCima = vi.fn();
    const a = renderHook(() => useEscapeStack(true, debaixo));
    const b = renderHook(() => useEscapeStack(true, emCima));

    pressEscape();

    expect(emCima).toHaveBeenCalledTimes(1);
    expect(debaixo).not.toHaveBeenCalled();
    b.unmount();
    a.unmount();
  });

  it("devolve o Escape para quem está abaixo quando o topo sai", () => {
    const debaixo = vi.fn();
    const emCima = vi.fn();
    const a = renderHook(() => useEscapeStack(true, debaixo));
    const b = renderHook(() => useEscapeStack(true, emCima));

    b.unmount();
    pressEscape();

    expect(debaixo).toHaveBeenCalledTimes(1);
    expect(emCima).toHaveBeenCalledTimes(0);
    a.unmount();
  });

  it("quem está inativo não entra na pilha", () => {
    const inativo = vi.fn();
    const ativo = vi.fn();
    const a = renderHook(() => useEscapeStack(false, inativo));
    const b = renderHook(() => useEscapeStack(true, ativo));

    pressEscape();

    expect(inativo).not.toHaveBeenCalled();
    expect(ativo).toHaveBeenCalledTimes(1);
    a.unmount();
    b.unmount();
  });

  it("ignora o Escape que alguém mais perto do alvo já tratou", () => {
    // `usePrompt` chama `preventDefault()` no Escape: o prompt cancela e o
    // diálogo atrás **não** pode fechar no mesmo evento.
    const dialogo = vi.fn();
    const a = renderHook(() => useEscapeStack(true, dialogo));

    pressEscape(window, { cancelable: true });
    expect(dialogo).toHaveBeenCalledTimes(1);

    const tratado = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
    tratado.preventDefault();
    act(() => {
      window.dispatchEvent(tratado);
    });
    expect(dialogo).toHaveBeenCalledTimes(1);
    a.unmount();
  });

  it("marca o evento como tratado, para ninguém mais reagir ao mesmo Escape", () => {
    const fechar = vi.fn();
    const a = renderHook(() => useEscapeStack(true, fechar));

    const event = pressEscape();

    expect(fechar).toHaveBeenCalledTimes(1);
    expect(event.defaultPrevented).toBe(true);
    a.unmount();
  });

  it("com ignoreFromFields, o Escape digitado num campo não sobe", () => {
    // O caso real: Esc no campo de link da aba Follow fechava a Choose Game
    // inteira; e Esc num campo numérico de Settings revertia o campo **e**
    // fechava o diálogo.
    const fecharTela = vi.fn();
    const a = renderHook(() => useEscapeStack(true, fecharTela, { ignoreFromFields: true }));
    const input = document.createElement("input");
    document.body.appendChild(input);

    pressEscape(input);
    expect(fecharTela).not.toHaveBeenCalled();

    pressEscape();
    expect(fecharTela).toHaveBeenCalledTimes(1);

    input.remove();
    a.unmount();
  });

  it("não deixa entrada pendurada quando o componente desmonta", () => {
    const { unmount } = renderHook(() => useEscapeStack(true, vi.fn()));
    expect(escapeStackDepth()).toBe(1);
    unmount();
    expect(escapeStackDepth()).toBe(0);
  });
});
