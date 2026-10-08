import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Janela de mentira: tamanho físico + fator de escala do monitor, como o Tauri
 * entrega. O tamanho lógico (físico / escala) é o que decide o zoom automático.
 */
const win = {
  physical: { width: 1920, height: 1035 },
  scale: 1.5,
  resized: [] as Array<() => void>,
  scaleChanged: [] as Array<() => void>,
};

const setZoom = vi.fn(async (_factor: number) => {});
const unlisten = vi.fn();

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    innerSize: async () => ({ ...win.physical }),
    scaleFactor: async () => win.scale,
    onResized: async (cb: () => void) => {
      win.resized.push(cb);
      return unlisten;
    },
    onScaleChanged: async (cb: () => void) => {
      win.scaleChanged.push(cb);
      return unlisten;
    },
  }),
}));

vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ setZoom }),
}));

import { announceUiScale, useUiScale } from "./useUiScale";

/** Deixa as promessas do hook (innerSize/scaleFactor/setZoom) assentarem. */
async function settle() {
  await act(async () => {
    for (let i = 0; i < 5; i++) await Promise.resolve();
  });
}

function zooms() {
  return setZoom.mock.calls.map((c) => c[0]);
}

function resizeTo(width: number, height: number, scale = win.scale) {
  win.physical = { width, height };
  win.scale = scale;
  for (const cb of win.resized) cb();
}

beforeEach(() => {
  vi.useFakeTimers();
  win.physical = { width: 1920, height: 1035 }; // 1280x690 lógico a 150%
  win.scale = 1.5;
  win.resized = [];
  win.scaleChanged = [];
  setZoom.mockClear();
  unlisten.mockClear();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useUiScale", () => {
  it("automático: aplica o zoom pelo tamanho lógico da janela", async () => {
    renderHook(() => useUiScale("auto"));
    await settle();
    expect(zooms()).toEqual([0.85]);
  });

  it("setting ausente conta como automático", async () => {
    renderHook(() => useUiScale(undefined));
    await settle();
    expect(zooms()).toEqual([0.85]);
  });

  it("espera as settings chegarem: quem escolheu valor fixo não vê o automático piscar", async () => {
    const { rerender } = renderHook(({ value, ready }) => useUiScale(value, ready), {
      initialProps: { value: undefined as string | undefined, ready: false },
    });
    await settle();
    expect(zooms()).toEqual([]);
    expect(win.resized).toHaveLength(0);

    rerender({ value: "110", ready: true });
    await settle();
    expect(zooms()).toEqual([1.1]);
  });

  it("valor fixo ignora a janela", async () => {
    renderHook(() => useUiScale("90"));
    await settle();
    expect(zooms()).toEqual([0.9]);
  });

  it("recalcula ao redimensionar, com debounce, e só chama setZoom quando muda", async () => {
    renderHook(() => useUiScale("auto"));
    await settle();
    expect(zooms()).toEqual([0.85]);

    // Arrastar a borda dispara vários eventos: um cálculo só no fim.
    resizeTo(2400, 1200); // 1600x800 lógico
    resizeTo(3000, 1500);
    resizeTo(3840, 2100); // 2560x1400 lógico
    await settle();
    expect(zooms()).toEqual([0.85]);

    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    await settle();
    expect(zooms()).toEqual([0.85, 1]);

    // Mudou o tamanho, mas o fator continua 1: nada de setZoom repetido.
    resizeTo(3600, 2000);
    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    await settle();
    expect(zooms()).toEqual([0.85, 1]);
  });

  it("recalcula ao trocar de monitor com outra escala", async () => {
    renderHook(() => useUiScale("auto"));
    await settle();

    // Mesma janela física, monitor a 100%: 1920x1035 lógico -> 1.0.
    win.scale = 1;
    for (const cb of win.scaleChanged) cb();
    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    await settle();
    expect(zooms()).toEqual([0.85, 1]);
  });

  it("troca de monitor reaplica o zoom mesmo com o mesmo fator", async () => {
    // A troca de DPI mexe na escala do WebView2; reaplicar é barato e garante
    // que o zoom não fique para trás sem o hook perceber.
    renderHook(() => useUiScale("90"));
    await settle();
    expect(zooms()).toEqual([0.9]);
    // Valor fixo não escuta mudança de tamanho, mas escuta troca de monitor.
    expect(win.resized).toHaveLength(0);
    win.scale = 1;
    for (const cb of win.scaleChanged) cb();
    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    await settle();
    expect(zooms()).toEqual([0.9, 0.9]);
  });

  it("valor fixo não escuta o redimensionamento", async () => {
    renderHook(() => useUiScale("100"));
    await settle();
    expect(win.resized).toHaveLength(0);
    expect(zooms()).toEqual([1]);
  });

  it("trocar a opção aplica na hora, uma vez por mudança", async () => {
    const { rerender } = renderHook(({ value }) => useUiScale(value), {
      initialProps: { value: "auto" as string | undefined },
    });
    await settle();
    rerender({ value: "80" });
    await settle();
    rerender({ value: "80" });
    await settle();
    rerender({ value: "auto" });
    await settle();
    expect(zooms()).toEqual([0.85, 0.8, 0.85]);
  });

  it("a escolha feita em Settings vale antes de a store recarregar", async () => {
    renderHook(() => useUiScale("auto"));
    await settle();
    act(() => announceUiScale("110"));
    await settle();
    expect(zooms()).toEqual([0.85, 1.1]);
  });

  it("solta os listeners ao desmontar", async () => {
    const { unmount } = renderHook(() => useUiScale("auto"));
    await settle();
    unmount();
    expect(unlisten).toHaveBeenCalledTimes(2);
  });
});
