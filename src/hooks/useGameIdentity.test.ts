import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
  isTauri: () => false,
  convertFileSrc: (p: string) => p,
}));

import { clearGameIdentityCache, useGameIdentity } from "./useGameIdentity";

const ICON = "https://tr.rbxcdn.com/jailbreak.png";

/** Resposta boa: o backend conhece o place. */
function backendKnowsJailbreak(cmd: string) {
  if (cmd === "get_place_details") {
    return [
      {
        placeId: 606849621,
        universeId: 245662005,
        name: "Jailbreak",
        description: "",
        sourceName: "Jailbreak",
        sourceDescription: "",
        url: "https://www.roblox.com/games/606849621",
      },
    ];
  }
  if (cmd === "batched_get_game_icon") return ICON;
  return undefined;
}

function callsFor(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

/** Espera real, porque o hook espera a digitação parar antes de perguntar. */
function sleep(ms: number) {
  return act(async () => {
    await new Promise((resolve) => setTimeout(resolve, ms));
  });
}

beforeEach(() => {
  clearGameIdentityCache();
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => backendKnowsJailbreak(cmd));
});

afterEach(cleanup);

describe("useGameIdentity", () => {
  it("diz que jogo é aquele Place ID, com nome e ícone", async () => {
    const { result } = renderHook(() => useGameIdentity("606849621", 5));

    await waitFor(() => expect(result.current?.name).toBe("Jailbreak"));
    expect(result.current?.iconUrl).toBe(ICON);
    expect(result.current?.placeId).toBe(606849621);
    expect(result.current?.loading).toBe(false);

    expect(invokeMock).toHaveBeenCalledWith("get_place_details", {
      placeIds: [606849621],
      userId: 5,
    });
    expect(invokeMock).toHaveBeenCalledWith("batched_get_game_icon", {
      placeId: 606849621,
      userId: 5,
    });
  });

  it("aceita o link do jogo colado, não só o número", async () => {
    const { result } = renderHook(() =>
      useGameIdentity("https://www.roblox.com/games/606849621/Jailbreak", null)
    );

    await waitFor(() => expect(result.current?.name).toBe("Jailbreak"));
    expect(result.current?.placeId).toBe(606849621);
  });

  it("não fala com o backend quando o texto não carrega Place ID", async () => {
    const { result } = renderHook(() => useGameIdentity("", null));
    expect(result.current).toBeNull();

    const semPlace = renderHook(() => useGameIdentity("cole aqui", null));
    expect(semPlace.result.current).toBeNull();

    await sleep(600);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("pergunta uma vez por place: a segunda tela lê do cache, sem esperar", async () => {
    const primeira = renderHook(() => useGameIdentity("606849621", null));
    await waitFor(() => expect(primeira.result.current?.name).toBe("Jailbreak"));
    expect(callsFor("get_place_details")).toHaveLength(1);
    primeira.unmount();

    const segunda = renderHook(() => useGameIdentity("606849621", null));
    // Sem `waitFor`: o valor tem que estar pronto no primeiro render, senão o
    // nome pisca em toda tela que abre com o mesmo place.
    expect(segunda.result.current?.name).toBe("Jailbreak");
    expect(segunda.result.current?.loading).toBe(false);
    expect(callsFor("get_place_details")).toHaveLength(1);
  });

  it("espera a digitação parar em vez de pedir um place por tecla", async () => {
    vi.useFakeTimers();
    try {
      const { rerender } = renderHook(({ id }) => useGameIdentity(id, null), {
        initialProps: { id: "6" },
      });
      rerender({ id: "60" });
      rerender({ id: "6068" });
      rerender({ id: "606849621" });
      expect(invokeMock).not.toHaveBeenCalled();

      await act(async () => {
        vi.advanceTimersByTime(1000);
      });

      expect(callsFor("get_place_details")).toHaveLength(1);
      expect(callsFor("get_place_details")[0][1]).toEqual({
        placeIds: [606849621],
        userId: null,
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it("falha do backend deixa o jogo sem identificar, sem entrar em laço", async () => {
    invokeMock.mockImplementation(() => {
      throw new Error("sem rede");
    });

    const { result } = renderHook(() => useGameIdentity("606849621", null));
    await waitFor(() => expect(result.current?.loading).toBe(false));
    expect(result.current?.name).toBeNull();
    expect(result.current?.iconUrl).toBeNull();

    const depois = callsFor("get_place_details").length;
    await sleep(600);
    expect(callsFor("get_place_details")).toHaveLength(depois);

    // Outra tela abrindo o mesmo place não repete o pedido que acabou de falhar.
    renderHook(() => useGameIdentity("606849621", null));
    await sleep(600);
    expect(callsFor("get_place_details")).toHaveLength(depois);
  });

  it("resposta atrasada não carimba o nome do place que o usuário já abandonou", async () => {
    const atrasado: { libera: (() => void) | null } = { libera: null };
    invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
      const place = (args.placeIds as number[] | undefined)?.[0] ?? args.placeId;
      if (place === 1818) {
        return new Promise((resolve) => {
          atrasado.libera = () =>
            resolve(cmd === "get_place_details" ? [{ name: "Classic" }] : null);
        });
      }
      return backendKnowsJailbreak(cmd);
    });

    const { result, rerender } = renderHook(({ id }) => useGameIdentity(id, null), {
      initialProps: { id: "1818" },
    });
    await sleep(600);
    rerender({ id: "606849621" });
    await waitFor(() => expect(result.current?.name).toBe("Jailbreak"));

    atrasado.libera?.();
    await sleep(50);
    expect(result.current?.name).toBe("Jailbreak");
    expect(result.current?.placeId).toBe(606849621);
  });

  it("aguenta o backend devolvendo lista vazia em vez de dado", async () => {
    invokeMock.mockImplementation(() => []);

    const { result } = renderHook(() => useGameIdentity("606849621", null));
    await waitFor(() => expect(result.current?.loading).toBe(false));
    expect(result.current?.placeId).toBe(606849621);
    expect(result.current?.name).toBeNull();
    expect(result.current?.iconUrl).toBeNull();
  });
});
