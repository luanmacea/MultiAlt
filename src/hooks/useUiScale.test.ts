import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Monitor de mentira: tamanho físico + fator de escala, como o Tauri entrega
 * em `currentMonitor()`. O tamanho lógico do **monitor** (físico / escala) é o
 * que decide o zoom automático — não o da janela: uma janela restaurada num
 * monitor grande não pode encolher a interface (achado no teste de 08/10/2026).
 */
const mon = {
  physical: { width: 1920, height: 1080 },
  scale: 1.5,
  /** Área interna da janela em px físicos (`innerSize`), na escala do monitor. */
  window: { width: 1650, height: 1050 },
  moved: [] as Array<() => void>,
  resized: [] as Array<() => void>,
  scaleChanged: [] as Array<() => void>,
};

const setZoom = vi.fn(async (_factor: number) => {});
const unlisten = vi.fn();

vi.mock("@tauri-apps/api/window", () => ({
  currentMonitor: async () => ({ size: { ...mon.physical }, scaleFactor: mon.scale }),
  getCurrentWindow: () => ({
    onMoved: async (cb: () => void) => {
      mon.moved.push(cb);
      return unlisten;
    },
    onResized: async (cb: () => void) => {
      mon.resized.push(cb);
      return unlisten;
    },
    innerSize: async () => ({ ...mon.window }),
    scaleFactor: async () => mon.scale,
    onScaleChanged: async (cb: () => void) => {
      mon.scaleChanged.push(cb);
      return unlisten;
    },
  }),
}));

vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ setZoom }),
}));

import { announceUiScale, useAppliedUiScale, useUiScale } from "./useUiScale";

/** Deixa as promessas do hook (currentMonitor/setZoom) assentarem. */
async function settle() {
  await act(async () => {
    for (let i = 0; i < 5; i++) await Promise.resolve();
  });
}

function zooms() {
  return setZoom.mock.calls.map((c) => c[0]);
}

/** A janela mudou de tamanho (px físicos), como maximizar num monitor 4K. */
function resizeTo(width: number, height: number) {
  mon.window = { width, height };
  for (const cb of mon.resized) cb();
}

/** A janela foi arrastada para outro monitor. */
function moveTo(width: number, height: number, scale = mon.scale) {
  mon.physical = { width, height };
  mon.scale = scale;
  for (const cb of mon.moved) cb();
}

beforeEach(() => {
  vi.useFakeTimers();
  mon.physical = { width: 1920, height: 1080 }; // 1280x720 lógico a 150%
  mon.scale = 1.5;
  mon.window = { width: 1650, height: 1050 }; // 1100x700 lógico: a janela padrão
  mon.moved = [];
  mon.resized = [];
  mon.scaleChanged = [];
  setZoom.mockClear();
  unlisten.mockClear();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useUiScale", () => {
  it("automático: aplica o zoom pelo tamanho lógico do monitor", async () => {
    renderHook(() => useUiScale("auto"));
    await settle();
    expect(zooms()).toEqual([0.85]);
  });

  it("monitor grande fica em 100% mesmo com a janela pequena", async () => {
    mon.physical = { width: 2560, height: 1440 };
    mon.scale = 1;
    renderHook(() => useUiScale("auto"));
    await settle();
    expect(zooms()).toEqual([1]);
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
    expect(mon.moved).toHaveLength(0);

    rerender({ value: "110", ready: true });
    await settle();
    expect(zooms()).toEqual([1.1]);
  });

  it("valor fixo ignora o monitor", async () => {
    renderHook(() => useUiScale("90"));
    await settle();
    expect(zooms()).toEqual([0.9]);
  });

  it("recalcula ao mudar de monitor, com debounce, e só chama setZoom quando muda", async () => {
    renderHook(() => useUiScale("auto"));
    await settle();
    expect(zooms()).toEqual([0.85]);

    // Arrastar a janela dispara vários eventos: um cálculo só no fim.
    moveTo(1920, 1080);
    moveTo(2560, 1440, 1);
    await settle();
    expect(zooms()).toEqual([0.85]);

    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    await settle();
    expect(zooms()).toEqual([0.85, 1]);

    // Outro monitor grande: o fator continua 1, nada de setZoom repetido.
    moveTo(3840, 2160, 1.5);
    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    await settle();
    expect(zooms()).toEqual([0.85, 1]);
  });

  it("recalcula ao trocar de monitor com outra escala", async () => {
    renderHook(() => useUiScale("auto"));
    await settle();

    // Mesmo monitor físico passado para 100%: 1920x1080 lógico -> 1.0.
    mon.scale = 1;
    for (const cb of mon.scaleChanged) cb();
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
    // Valor fixo não escuta a janela andar, mas escuta troca de escala.
    expect(mon.moved).toHaveLength(0);
    mon.scale = 1;
    for (const cb of mon.scaleChanged) cb();
    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    await settle();
    expect(zooms()).toEqual([0.9, 0.9]);
  });

  it("valor fixo não escuta a janela mudar de monitor", async () => {
    renderHook(() => useUiScale("100"));
    await settle();
    expect(mon.moved).toHaveLength(0);
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
    // onMoved, onResized e onScaleChanged.
    expect(unlisten).toHaveBeenCalledTimes(3);
  });

  /**
   * Monitor 4K a 100% (pedido do dono, 11/10/2026): com a janela grande a
   * interface cresce junto; a janela padrão no mesmo monitor fica em 100%.
   */
  it("automático cresce a interface com a janela grande num monitor 4K", async () => {
    mon.physical = { width: 3840, height: 2160 };
    mon.scale = 1;
    mon.window = { width: 1100, height: 700 };
    renderHook(() => useUiScale("auto"));
    await settle();
    expect(zooms()).toEqual([1]);

    // Maximizar dispara vários eventos de tamanho: um cálculo só no fim.
    resizeTo(2000, 1300);
    resizeTo(3840, 2090);
    await settle();
    expect(zooms()).toEqual([1]);
    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    await settle();
    expect(zooms()).toEqual([1, 1.7]);

    // Restaurar volta para 100%.
    resizeTo(1100, 700);
    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    await settle();
    expect(zooms()).toEqual([1, 1.7, 1]);
  });

  it("janela grande em monitor com escala usa o tamanho lógico, não o físico", async () => {
    // 4K a 150%: 2560x1440 lógico; janela maximizada ~2560x1390 lógico -> 110%.
    mon.physical = { width: 3840, height: 2160 };
    mon.scale = 1.5;
    mon.window = { width: 3840, height: 2085 };
    renderHook(() => useUiScale("auto"));
    await settle();
    expect(zooms()).toEqual([1.1]);
  });

  it("valor fixo não cresce com a janela nem escuta o tamanho dela", async () => {
    mon.physical = { width: 3840, height: 2160 };
    mon.scale = 1;
    mon.window = { width: 3840, height: 2090 };
    renderHook(() => useUiScale("100"));
    await settle();
    expect(zooms()).toEqual([1]);
    expect(mon.resized).toHaveLength(0);
  });

  it("conta para a tela de Settings o zoom em vigor", async () => {
    mon.physical = { width: 3840, height: 2160 };
    mon.scale = 1;
    mon.window = { width: 3840, height: 2090 };
    renderHook(() => useUiScale("auto"));
    const shown = renderHook(() => useAppliedUiScale());
    await settle();
    expect(shown.result.current).toBe(1.7);
  });
});
