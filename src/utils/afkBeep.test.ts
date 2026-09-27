import { afterEach, describe, expect, it, vi } from "vitest";
import { playAfkBeep } from "./afkBeep";

/** Dublê mínimo de Web Audio, só para ver o que o bipe faz. */
function fakeAudio() {
  const oscillator = {
    type: "",
    frequency: { value: 0 },
    connect: vi.fn(),
    start: vi.fn(),
    stop: vi.fn(),
    onended: null as null | (() => void),
  };
  const gain = {
    gain: { value: -1, exponentialRampToValueAtTime: vi.fn() },
    connect: vi.fn(),
  };
  const ctx = {
    currentTime: 0,
    createOscillator: vi.fn(() => oscillator),
    createGain: vi.fn(() => gain),
    destination: {},
    close: vi.fn(async () => {}),
  };
  // Precisa ser funcao comum: arrow nao pode ser chamada com `new`, e o codigo
  // faz `new AudioContext()`.
  const Ctor = vi.fn(function fakeAudioContext() {
    return ctx;
  });
  return { Ctor, ctx, oscillator, gain };
}

const original = (globalThis as Record<string, unknown>).AudioContext;

afterEach(() => {
  if (original === undefined) delete (globalThis as Record<string, unknown>).AudioContext;
  else (globalThis as Record<string, unknown>).AudioContext = original;
});

describe("bipe do AFK mode", () => {
  it("toca um oscilador curto e de volume baixo", () => {
    const { Ctor, oscillator, gain } = fakeAudio();
    (globalThis as Record<string, unknown>).AudioContext = Ctor;

    expect(playAfkBeep()).toBe(true);
    expect(oscillator.start).toHaveBeenCalledTimes(1);
    expect(oscillator.stop).toHaveBeenCalledWith(0.12);
    expect(gain.gain.value).toBeLessThanOrEqual(0.1);
    expect(gain.gain.value).toBeGreaterThan(0);
  });

  it("fecha o contexto quando o bipe acaba", () => {
    const { Ctor, ctx, oscillator } = fakeAudio();
    (globalThis as Record<string, unknown>).AudioContext = Ctor;

    playAfkBeep();
    oscillator.onended?.();
    expect(ctx.close).toHaveBeenCalledTimes(1);
  });

  it("não faz nada (nem quebra) sem Web Audio na janela", () => {
    delete (globalThis as Record<string, unknown>).AudioContext;
    expect(playAfkBeep()).toBe(false);
  });

  it("engole erro do navegador em vez de subir para a tela", () => {
    (globalThis as Record<string, unknown>).AudioContext = vi.fn(() => {
      throw new Error("audio blocked");
    });
    expect(playAfkBeep()).toBe(false);
  });
});
