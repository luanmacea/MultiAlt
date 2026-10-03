import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";

const getVersionMock = vi.fn<() => Promise<unknown>>();
vi.mock("@tauri-apps/api/app", () => ({ getVersion: () => getVersionMock() }));

import { useUpdateHandoffToast } from "./useUpdateHandoffToast";
import { UPDATE_HANDOFF_KEY, writeUpdateHandoff } from "../updateHandoff";

const t = (text: string, options?: Record<string, unknown>) =>
  text.replace(/\{\{(\w+)\}\}/g, (_, k) => String(options?.[k] ?? ""));

describe("useUpdateHandoffToast", () => {
  beforeEach(() => {
    localStorage.clear();
    getVersionMock.mockReset();
  });

  afterEach(cleanup);

  it("comemora a versão nova quando o app volta atualizado", async () => {
    writeUpdateHandoff(localStorage, "0.1.6", "0.1.7", Date.now());
    getVersionMock.mockResolvedValue("0.1.7");
    const addToast = vi.fn();

    renderHook(() => useUpdateHandoffToast(true, addToast, t));

    await waitFor(() => expect(addToast).toHaveBeenCalledWith("Updated to v0.1.7 ✨", "success"));
    expect(localStorage.getItem(UPDATE_HANDOFF_KEY)).toBeNull();
  });

  it("avisa quando a instalação não terminou", async () => {
    writeUpdateHandoff(localStorage, "0.1.6", "0.1.7", Date.now());
    getVersionMock.mockResolvedValue("0.1.6");
    const addToast = vi.fn();

    renderHook(() => useUpdateHandoffToast(true, addToast, t));

    await waitFor(() =>
      expect(addToast).toHaveBeenCalledWith(
        "The update to v0.1.7 didn't finish. Check for updates to try again.",
        "warn"
      )
    );
  });

  it("espera o app estar pronto e anuncia uma vez só", async () => {
    writeUpdateHandoff(localStorage, "0.1.6", "0.1.7", Date.now());
    getVersionMock.mockResolvedValue("0.1.7");
    const addToast = vi.fn();

    const { rerender } = renderHook(({ ready }) => useUpdateHandoffToast(ready, addToast, t), {
      initialProps: { ready: false },
    });
    expect(getVersionMock).not.toHaveBeenCalled();

    rerender({ ready: true });
    await waitFor(() => expect(addToast).toHaveBeenCalledTimes(1));
    rerender({ ready: true });
    expect(getVersionMock).toHaveBeenCalledTimes(1);
  });

  it("sem versão legível não consome a anotação", async () => {
    writeUpdateHandoff(localStorage, "0.1.6", "0.1.7", Date.now());
    getVersionMock.mockResolvedValue(undefined);
    const addToast = vi.fn();

    renderHook(() => useUpdateHandoffToast(true, addToast, t));

    await waitFor(() => expect(getVersionMock).toHaveBeenCalled());
    expect(addToast).not.toHaveBeenCalled();
    expect(localStorage.getItem(UPDATE_HANDOFF_KEY)).not.toBeNull();
  });
});
