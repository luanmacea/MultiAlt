import { cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../store", async () => (await import("../test-utils/renderWithStore")).storeModuleMock());

import { makeAccount, setStore } from "../test-utils/renderWithStore";
import { useAccountLabel, useHideAccountAvatar } from "./useAccountLabel";

afterEach(cleanup);

const ANN = makeAccount({ UserID: 1, Username: "annabelle", Alias: "" });

describe("useAccountLabel", () => {
  it("com nomes à mostra devolve alias || username", () => {
    setStore({ hideUsernames: false, hiddenNameLetters: 2 });
    const { result } = renderHook(() => useAccountLabel());
    expect(result.current(ANN)).toBe("annabelle");
    expect(result.current({ ...ANN, Alias: "Main" })).toBe("Main");
  });

  it("com nomes ocultos mascara pelas letras de prévia da store", () => {
    setStore({ hideUsernames: true, hiddenNameLetters: 2 });
    const { result } = renderHook(() => useAccountLabel());
    expect(result.current(ANN)).toBe("an********");
  });

  it("sem conta usa o fallback, mascarado do mesmo jeito", () => {
    setStore({ hideUsernames: true, hiddenNameLetters: 0 });
    const { result } = renderHook(() => useAccountLabel());
    expect(result.current(undefined, "User ID: 1")).toBe("************");
  });
});

describe("useHideAccountAvatar", () => {
  it("segue hideUsernames && !showAvatarsWhenHidden", () => {
    setStore({ hideUsernames: true, showAvatarsWhenHidden: false });
    expect(renderHook(() => useHideAccountAvatar()).result.current).toBe(true);
    setStore({ hideUsernames: true, showAvatarsWhenHidden: true });
    expect(renderHook(() => useHideAccountAvatar()).result.current).toBe(false);
    setStore({ hideUsernames: false, showAvatarsWhenHidden: false });
    expect(renderHook(() => useHideAccountAvatar()).result.current).toBe(false);
  });
});
