import { describe, expect, it } from "vitest";
import { clientHealthLabel } from "./clientHealth";
import type { ClientDrop } from "../types";

const t = (text: string, options?: Record<string, unknown>) =>
  text.replace(/\{\{(\w+)\}\}/g, (_, key: string) => String(options?.[key] ?? ""));

function drop(partial: Partial<ClientDrop>): { pid: number; logFound: boolean; drop: ClientDrop } {
  return {
    pid: 1,
    logFound: true,
    drop: { kind: "disconnected", reason: null, code: null, message: null, sinceMs: 0, ...partial },
  };
}

describe("clientHealthLabel", () => {
  it("is empty when nothing dropped", () => {
    expect(clientHealthLabel(null, t)).toBeNull();
    expect(clientHealthLabel({ pid: 1, logFound: true, drop: null }, t)).toBeNull();
  });

  it("names each reason in a short sentence", () => {
    expect(clientHealthLabel(drop({ reason: "connectionLost" }), t)?.label).toBe("Disconnected: lost connection");
    expect(clientHealthLabel(drop({ reason: "joinedElsewhere" }), t)?.label).toBe(
      "Disconnected: the account joined somewhere else"
    );
    expect(clientHealthLabel(drop({ reason: "idle" }), t)?.label).toBe("Disconnected: idle for too long");
    expect(clientHealthLabel(drop({ reason: "other", code: 280 }), t)).toEqual({
      label: "Disconnected from the game",
      detail: "Roblox error code 280",
      tone: "drop",
    });
    expect(clientHealthLabel(drop({ kind: "kicked" }), t)?.label).toBe("Kicked from the game");
    expect(clientHealthLabel(drop({ kind: "kicked", message: "bye" }), t)?.label).toBe("Kicked: bye");
    expect(clientHealthLabel(drop({ kind: "serverShutdown" }), t)?.label).toBe("The server shut down");
    expect(clientHealthLabel(drop({ kind: "crashed" }), t)?.label).toBe("Closed without leaving the game");
  });

  it("says when the window stopped responding, but a drop wins", () => {
    const hung = { pid: 1, logFound: true, drop: null, notResponding: true };
    expect(clientHealthLabel(hung, t)).toMatchObject({ label: "Not responding", tone: "hung" });
    expect(clientHealthLabel({ ...drop({ kind: "serverShutdown" }), notResponding: true }, t)).toMatchObject({
      label: "The server shut down",
      tone: "drop",
    });
  });
});
