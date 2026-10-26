import { describe, expect, it } from "vitest";
import {
  countOutcomes,
  failureRow,
  needsLogin,
  patchRow,
  pendingRows,
  rowsFromFollow,
  rowsFromFriendLink,
  sortRows,
} from "./actionReport";

describe("actionReport", () => {
  /**
   * Relato do dono (25/10/2026): "Atualizar cookies" com sessões inválidas não
   * dizia nada. A sessão morta tem de virar "precisa de login", não uma falha
   * qualquer — é o que oferece o "Fazer login de novo".
   */
  it("reconhece a sessão morta nas mensagens do backend", () => {
    expect(needsLogin("Roblox invalidated this session. Re-login required.")).toBe(true);
    expect(needsLogin("Request failed with status 401")).toBe(true);
    expect(needsLogin("x (invalid session — log the account in again)")).toBe(true);
    expect(needsLogin('{"errors":[{"code":9002}]}')).toBe(true);
    expect(needsLogin("Too many requests (429)")).toBe(false);
    expect(needsLogin("timeout")).toBe(false);
  });

  it("separa 'precisa de login' de 'falhou', guardando o motivo", () => {
    expect(failureRow(1, "status 401", "sign in")).toEqual({ userId: 1, outcome: "login", detail: "sign in" });
    expect(failureRow(2, "Too many requests", "sign in")).toEqual({
      userId: 2,
      outcome: "failed",
      detail: "Too many requests",
    });
    expect(failureRow(3, "", "sign in")).toEqual({ userId: 3, outcome: "failed", detail: undefined });
  });

  it("monta, troca e conta as linhas", () => {
    let rows = pendingRows([1, 2, 2, 3]);
    expect(rows.map((r) => r.userId)).toEqual([1, 2, 3]);
    rows = patchRow(rows, { userId: 2, outcome: "ok" });
    rows = patchRow(rows, { userId: 3, outcome: "login" });
    expect(countOutcomes(rows)).toMatchObject({ pending: 1, ok: 1, login: 1 });
    expect(sortRows(rows).map((r) => r.outcome)).toEqual(["login", "pending", "ok"]);
  });

  it("traduz o estado ao vivo do Make Friends e do Follow", () => {
    const friends = rowsFromFriendLink(
      {
        active: false,
        phase: "done",
        processed: 2,
        total: 3,
        mode: "mesh",
        mainUserId: null,
        accounts: [
          { userId: 1, state: "done", error: null },
          { userId: 2, state: "failed", error: "status 401" },
          { userId: 3, state: "processing", error: null },
        ],
      },
      "sign in"
    );
    expect(friends.map((r) => r.outcome)).toEqual(["ok", "login", "running"]);

    const follow = rowsFromFollow(
      {
        active: true,
        processed: 1,
        total: 4,
        targetUserId: 9,
        targetName: "x",
        accounts: [
          { userId: 1, state: "followed", error: null },
          { userId: 2, state: "already", error: null },
          { userId: 3, state: "browser", error: null },
          { userId: 4, state: "failed", error: "captcha" },
        ],
      },
      "sign in",
      "in the browser"
    );
    expect(follow.map((r) => r.outcome)).toEqual(["ok", "already", "running", "failed"]);
    expect(follow[2].detail).toBe("in the browser");
  });
});
