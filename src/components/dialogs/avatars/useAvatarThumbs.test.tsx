import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", async () => (await import("../../../test-utils/tauriMocks")).tauriCoreMock());

import { mergeThumbs, useAvatarThumbs } from "./useAvatarThumbs";
import type { FreeCatalogItem } from "../../../avatarBuilder";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../../test-utils/tauriMocks";

const HAT: FreeCatalogItem = { id: 7, kind: "Asset", typeId: 8, name: "Cap", collectibleItemId: "c-7" };
const BODY: FreeCatalogItem = { id: 7, kind: "Bundle", typeId: 1, name: "Rthro", collectibleItemId: "b-7" };

function thumbCalls() {
  return invokeMock.mock.calls.filter(([cmd]) => cmd === "batch_thumbnails");
}

beforeEach(() => {
  resetTauriMocks();
});

afterEach(cleanup);

describe("mergeThumbs", () => {
  it("devolve um mapa novo com as entradas somadas, sem mexer no anterior", () => {
    const prev = new Map<string, string | null>([["Asset:1", "a.png"]]);
    const next = mergeThumbs(prev, new Map([["Asset:2", null]]));
    expect(next).not.toBe(prev);
    expect([...next]).toEqual([
      ["Asset:1", "a.png"],
      ["Asset:2", null],
    ]);
    expect(prev.size).toBe(1);
  });
});

describe("useAvatarThumbs", () => {
  it("pede asset e bundle separados e guarda cada um pela própria chave", async () => {
    setInvokeHandler((_cmd, args) => {
      const [req] = (args as { requests: { type: string; targetId: number }[] }).requests;
      return [{ targetId: req.targetId, imageUrl: `${req.type}.png`, state: "Completed" }];
    });
    const { result } = renderHook(() => useAvatarThumbs({ retryMs: 1 }));
    act(() => result.current.ensureThumbs([HAT, BODY]));
    await waitFor(() => expect(result.current.thumbs.get("Bundle:7")).toBe("BundleThumbnail.png"));
    expect(result.current.thumbs.get("Asset:7")).toBe("Asset.png");
    expect(thumbCalls()).toHaveLength(2);
  });

  it("tenta de novo a miniatura que voltou Pending até ela chegar", async () => {
    let calls = 0;
    setInvokeHandler(() => {
      calls += 1;
      return calls === 1
        ? [{ targetId: 7, imageUrl: null, state: "Pending" }]
        : [{ targetId: 7, imageUrl: "cap.png", state: "Completed" }];
    });
    const { result } = renderHook(() => useAvatarThumbs({ retryMs: 1 }));
    act(() => result.current.ensureThumbs([HAT]));
    // Pending fica carregando (undefined), não vira "sem imagem".
    await waitFor(() => expect(result.current.thumbs.get("Asset:7")).toBe("cap.png"));
    expect(calls).toBe(2);
  });

  it("desiste depois de algumas tentativas e mostra sem imagem", async () => {
    setInvokeHandler(() => [{ targetId: 7, imageUrl: null, state: "Pending" }]);
    const { result } = renderHook(() => useAvatarThumbs({ retryMs: 1, maxRetries: 2 }));
    act(() => result.current.ensureThumbs([HAT]));
    await waitFor(() => expect(result.current.thumbs.get("Asset:7")).toBeNull());
    expect(thumbCalls()).toHaveLength(3);
  });

  it("não pede de novo o que já foi pedido", async () => {
    setInvokeHandler(() => [{ targetId: 7, imageUrl: "cap.png", state: "Completed" }]);
    const { result } = renderHook(() => useAvatarThumbs({ retryMs: 1 }));
    act(() => result.current.ensureThumbs([HAT]));
    act(() => result.current.ensureThumbs([HAT]));
    await waitFor(() => expect(result.current.thumbs.get("Asset:7")).toBe("cap.png"));
    act(() => result.current.ensureThumbs([HAT]));
    expect(thumbCalls()).toHaveLength(1);
  });
});
