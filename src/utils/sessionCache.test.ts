import { describe, expect, it } from "vitest";
import { SessionCache, clearSessionCaches } from "./sessionCache";

describe("SessionCache", () => {
  it("guarda e devolve por chave", () => {
    const cache = new SessionCache<number>();
    cache.set("a", 1);
    expect(cache.get("a")).toBe(1);
    expect(cache.has("a")).toBe(true);
    expect(cache.get("b")).toBeUndefined();
  });

  /** A chave inclui place/seleção: sem teto, cresceria a cada jogo aberto. */
  it("respeita o teto, tirando a chave usada há mais tempo", () => {
    const cache = new SessionCache<number>(2);
    cache.set("a", 1);
    cache.set("b", 2);
    cache.set("a", 10); // "a" volta a ser a mais recente
    cache.set("c", 3);
    expect(cache.has("b")).toBe(false);
    expect(cache.get("a")).toBe(10);
    expect(cache.get("c")).toBe(3);
    expect(cache.size).toBe(2);
  });

  it("clearSessionCaches zera todos os caches de uma vez", () => {
    const one = new SessionCache<string>();
    const two = new SessionCache<string>();
    one.set("x", "1");
    two.set("y", "2");
    clearSessionCaches();
    expect(one.size).toBe(0);
    expect(two.size).toBe(0);
  });
});
