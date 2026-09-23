import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
}));

import { useSettings } from "./useSettings";

function updateCalls() {
  return invokeMock.mock.calls
    .filter((c) => c[0] === "update_setting")
    .map((c) => c[1] as { section: string; key: string; value: string });
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "get_all_settings") {
      return { General: { Theme: "dark", Ratio: "1.5", Flag: "true" } };
    }
    return null;
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useSettings", () => {
  it("starts empty and not loaded", () => {
    const { result } = renderHook(() => useSettings());
    expect(result.current.settings).toEqual({});
    expect(result.current.loaded).toBe(false);
    expect(result.current.saving).toBe(false);
  });

  it("loads all settings from the backend", async () => {
    const { result } = renderHook(() => useSettings());

    await act(async () => {
      await result.current.load();
    });

    expect(result.current.loaded).toBe(true);
    expect(result.current.get("General", "Theme")).toBe("dark");
  });

  it("stays unloaded when the backend fails", async () => {
    invokeMock.mockRejectedValueOnce(new Error("no file"));
    const { result } = renderHook(() => useSettings());

    await act(async () => {
      await result.current.load();
    });

    expect(result.current.loaded).toBe(false);
    expect(result.current.settings).toEqual({});
  });

  describe("readers", () => {
    it("returns the fallback for unknown sections and keys", async () => {
      const { result } = renderHook(() => useSettings());
      await act(async () => {
        await result.current.load();
      });

      expect(result.current.get("Nope", "Key")).toBe("");
      expect(result.current.get("General", "Missing", "fb")).toBe("fb");
      expect(result.current.getBool("General", "Missing")).toBe(false);
      expect(result.current.getNumber("General", "Missing", 42)).toBe(42);
    });

    it("parses booleans strictly as the string 'true'", async () => {
      const { result } = renderHook(() => useSettings());
      await act(async () => {
        await result.current.load();
      });

      expect(result.current.getBool("General", "Flag")).toBe(true);
      await act(async () => {
        await result.current.set("General", "Flag", "TRUE");
      });
      expect(result.current.getBool("General", "Flag")).toBe(false);
    });

    it("parses numbers and falls back for non-numeric values", async () => {
      const { result } = renderHook(() => useSettings());
      await act(async () => {
        await result.current.load();
      });

      expect(result.current.getNumber("General", "Ratio")).toBe(1.5);
      expect(result.current.getNumber("General", "Theme", -1)).toBe(-1);
    });
  });

  describe("writes", () => {
    it("updates local state immediately and persists after the debounce", async () => {
      vi.useFakeTimers();
      const { result } = renderHook(() => useSettings());

      await act(async () => {
        await result.current.set("General", "Theme", "light");
      });

      expect(result.current.get("General", "Theme")).toBe("light");
      expect(result.current.saving).toBe(true);
      expect(updateCalls()).toHaveLength(0);

      await act(async () => {
        vi.advanceTimersByTime(160);
      });

      expect(updateCalls()).toEqual([{ section: "General", key: "Theme", value: "light" }]);
      expect(result.current.saving).toBe(false);
    });

    it("coalesces repeated writes to the same key", async () => {
      vi.useFakeTimers();
      const { result } = renderHook(() => useSettings());

      await act(async () => {
        await result.current.set("General", "Theme", "a");
        await result.current.set("General", "Theme", "b");
        await result.current.set("General", "Theme", "c");
      });

      await act(async () => {
        vi.advanceTimersByTime(160);
      });

      expect(updateCalls()).toEqual([{ section: "General", key: "Theme", value: "c" }]);
    });

    it("flushes several keys in one batch", async () => {
      vi.useFakeTimers();
      const { result } = renderHook(() => useSettings());

      await act(async () => {
        await result.current.set("General", "A", "1");
        await result.current.setBool("General", "B", true);
        await result.current.setNumber("Other", "C", 2.5);
      });

      await act(async () => {
        vi.advanceTimersByTime(160);
      });

      expect(updateCalls()).toEqual(
        expect.arrayContaining([
          { section: "General", key: "A", value: "1" },
          { section: "General", key: "B", value: "true" },
          { section: "Other", key: "C", value: "2.5" },
        ])
      );
      expect(result.current.get("Other", "C")).toBe("2.5");
      expect(result.current.getBool("General", "B")).toBe(true);
    });

    it("announces a successful save through the ram-action-status event", async () => {
      vi.useFakeTimers();
      const events: CustomEvent[] = [];
      const listener = (e: Event) => events.push(e as CustomEvent);
      window.addEventListener("ram-action-status", listener);

      const { result } = renderHook(() => useSettings());
      await act(async () => {
        await result.current.set("General", "Theme", "light");
      });
      await act(async () => {
        vi.advanceTimersByTime(160);
      });

      window.removeEventListener("ram-action-status", listener);
      expect(events).toHaveLength(1);
      expect(events[0].detail).toMatchObject({ message: "Settings saved", tone: "success" });
    });

    it("does not announce anything when nothing was pending", async () => {
      vi.useFakeTimers();
      const events: Event[] = [];
      const listener = (e: Event) => events.push(e);
      window.addEventListener("ram-action-status", listener);

      const { result } = renderHook(() => useSettings());
      await act(async () => {
        await result.current.set("General", "Theme", "light");
      });
      await act(async () => {
        vi.advanceTimersByTime(500);
      });

      window.removeEventListener("ram-action-status", listener);
      expect(events).toHaveLength(1);
    });

    it("swallows backend errors but stops the saving indicator", async () => {
      vi.useFakeTimers();
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "update_setting") throw new Error("disk full");
        return null;
      });

      const { result } = renderHook(() => useSettings());
      await act(async () => {
        await result.current.set("General", "Theme", "light");
      });
      await act(async () => {
        vi.advanceTimersByTime(160);
      });

      expect(result.current.saving).toBe(false);
    });

    it("flushes anything still pending when the component unmounts", async () => {
      vi.useFakeTimers();
      const { result, unmount } = renderHook(() => useSettings());

      await act(async () => {
        await result.current.set("General", "Theme", "light");
      });
      expect(updateCalls()).toHaveLength(0);

      await act(async () => {
        unmount();
        await Promise.resolve();
      });

      expect(updateCalls()).toEqual([{ section: "General", key: "Theme", value: "light" }]);
    });
  });
});
