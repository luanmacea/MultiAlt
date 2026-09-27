import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { playAfkBeep, resetAfkBeepContextForTests } from "./afkBeep";

/** Dublê mínimo de Web Audio, só para ver o que o bipe faz. */
function fakeAudio() {
  const nodes: Array<{ disconnect: () => void }> = [];
  const makeOscillator = () => {
    const oscillator = {
      type: "",
      frequency: { value: 0 },
      connect: vi.fn(),
      disconnect: vi.fn(),
      start: vi.fn(),
      stop: vi.fn(),
      onended: null as null | (() => void),
    };
    nodes.push(oscillator);
    return oscillator;
  };
  const oscillators: ReturnType<typeof makeOscillator>[] = [];
  const gain = {
    gain: { value: -1, exponentialRampToValueAtTime: vi.fn() },
    connect: vi.fn(),
    disconnect: vi.fn(),
  };
  const ctx = {
    currentTime: 0,
    state: "running",
    resume: vi.fn(async () => {}),
    createOscillator: vi.fn(() => {
      const o = makeOscillator();
      oscillators.push(o);
      return o;
    }),
    createGain: vi.fn(() => gain),
    destination: {},
    close: vi.fn(async () => {}),
  };
  // Precisa ser funcao comum: arrow nao pode ser chamada com `new`, e o codigo
  // faz `new AudioContext()`.
  const Ctor = vi.fn(function fakeAudioContext() {
    return ctx;
  });
  return { Ctor, ctx, gain, oscillators };
}

type GlobalBag = Record<string, unknown>;
const originalAudio = (globalThis as GlobalBag).AudioContext;
const originalWebkit = (globalThis as GlobalBag).webkitAudioContext;

function restoreGlobals() {
  if (originalAudio === undefined) delete (globalThis as GlobalBag).AudioContext;
  else (globalThis as GlobalBag).AudioContext = originalAudio;
  if (originalWebkit === undefined) delete (globalThis as GlobalBag).webkitAudioContext;
  else (globalThis as GlobalBag).webkitAudioContext = originalWebkit;
}

beforeEach(() => {
  // O contexto é compartilhado pelo módulo: sem zerar, um teste herda o do outro.
  resetAfkBeepContextForTests();
  delete (globalThis as GlobalBag).AudioContext;
  delete (globalThis as GlobalBag).webkitAudioContext;
});

afterEach(() => {
  resetAfkBeepContextForTests();
  restoreGlobals();
});

describe("bipe do AFK mode", () => {
  it("toca um oscilador curto e de volume baixo", () => {
    const { Ctor, gain, oscillators } = fakeAudio();
    (globalThis as GlobalBag).AudioContext = Ctor;

    expect(playAfkBeep()).toBe(true);
    expect(oscillators[0].start).toHaveBeenCalledTimes(1);
    expect(oscillators[0].stop).toHaveBeenCalledWith(0.12);
    expect(gain.gain.value).toBeLessThanOrEqual(0.1);
    expect(gain.gain.value).toBeGreaterThan(0);
  });

  /**
   * Um `AudioContext` por ciclo vaza (o `onended` de contexto suspenso nunca
   * chega) e o navegador limita quantos contextos um documento pode abrir —
   * passado o limite, o bipe morre em silêncio.
   */
  it("reaproveita um único contexto entre ciclos, e não fecha o contexto", () => {
    const { Ctor, ctx, oscillators } = fakeAudio();
    (globalThis as GlobalBag).AudioContext = Ctor;

    expect(playAfkBeep()).toBe(true);
    expect(playAfkBeep()).toBe(true);
    expect(playAfkBeep()).toBe(true);

    expect(Ctor).toHaveBeenCalledTimes(1);
    expect(ctx.close).not.toHaveBeenCalled();
    expect(oscillators).toHaveLength(3);
  });

  it("solta os nós de cada bipe quando ele acaba", () => {
    const { Ctor, gain, oscillators } = fakeAudio();
    (globalThis as GlobalBag).AudioContext = Ctor;

    playAfkBeep();
    oscillators[0].onended?.();
    expect(oscillators[0].disconnect).toHaveBeenCalledTimes(1);
    expect(gain.disconnect).toHaveBeenCalledTimes(1);
  });

  it("acorda um contexto suspenso antes de tocar", () => {
    const { Ctor, ctx } = fakeAudio();
    ctx.state = "suspended";
    (globalThis as GlobalBag).AudioContext = Ctor;

    playAfkBeep();
    expect(ctx.resume).toHaveBeenCalled();
  });

  it("usa o construtor com prefixo quando é o único que existe", () => {
    const { Ctor } = fakeAudio();
    (globalThis as GlobalBag).webkitAudioContext = Ctor;

    expect(playAfkBeep()).toBe(true);
    expect(Ctor).toHaveBeenCalledTimes(1);
  });

  it("não faz nada (nem quebra) sem Web Audio na janela", () => {
    expect(playAfkBeep()).toBe(false);
  });

  it("engole erro do navegador em vez de subir para a tela", () => {
    (globalThis as GlobalBag).AudioContext = vi.fn(function boom() {
      throw new Error("audio blocked");
    });
    expect(playAfkBeep()).toBe(false);
  });
});
