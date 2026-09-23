import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useModalClose } from "./useModalClose";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useModalClose", () => {
  it("starts hidden while the modal is closed", () => {
    const { result } = renderHook(() => useModalClose(false, () => {}));
    expect(result.current.visible).toBe(false);
    expect(result.current.closing).toBe(false);
  });

  it("becomes visible as soon as it opens", () => {
    const { result } = renderHook(({ open }) => useModalClose(open, () => {}), {
      initialProps: { open: true },
    });
    expect(result.current.visible).toBe(true);
    expect(result.current.closing).toBe(false);
  });

  it("keeps rendering during the close animation and hides afterwards", () => {
    const { result, rerender } = renderHook(({ open }) => useModalClose(open, () => {}), {
      initialProps: { open: true },
    });

    rerender({ open: false });
    expect(result.current.visible).toBe(true);
    expect(result.current.closing).toBe(true);

    act(() => {
      vi.advanceTimersByTime(99);
    });
    expect(result.current.visible).toBe(true);

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current.visible).toBe(false);
    expect(result.current.closing).toBe(false);
  });

  it("honours a custom animation duration", () => {
    const { result, rerender } = renderHook(({ open }) => useModalClose(open, () => {}, 400), {
      initialProps: { open: true },
    });

    rerender({ open: false });
    act(() => {
      vi.advanceTimersByTime(100);
    });
    expect(result.current.visible).toBe(true);

    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(result.current.visible).toBe(false);
  });

  it("cancels a pending close when it reopens", () => {
    const { result, rerender } = renderHook(({ open }) => useModalClose(open, () => {}), {
      initialProps: { open: true },
    });

    rerender({ open: false });
    expect(result.current.closing).toBe(true);

    rerender({ open: true });
    expect(result.current.closing).toBe(false);
    expect(result.current.visible).toBe(true);

    act(() => {
      vi.advanceTimersByTime(200);
    });
    expect(result.current.visible).toBe(true);
  });

  it("calls onClose only while the modal is open", () => {
    const onClose = vi.fn();
    const { result, rerender } = renderHook(({ open }) => useModalClose(open, onClose), {
      initialProps: { open: true },
    });

    act(() => result.current.handleClose());
    expect(onClose).toHaveBeenCalledTimes(1);

    rerender({ open: false });
    act(() => result.current.handleClose());
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("always uses the latest onClose callback", () => {
    const first = vi.fn();
    const second = vi.fn();
    const { result, rerender } = renderHook(({ onClose }) => useModalClose(true, onClose), {
      initialProps: { onClose: first },
    });

    rerender({ onClose: second });
    act(() => result.current.handleClose());

    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });

  it("clears the pending timer on unmount", () => {
    const { rerender, unmount } = renderHook(({ open }) => useModalClose(open, () => {}), {
      initialProps: { open: true },
    });

    rerender({ open: false });
    unmount();

    expect(() =>
      act(() => {
        vi.advanceTimersByTime(500);
      })
    ).not.toThrow();
    expect(vi.getTimerCount()).toBe(0);
  });
});
