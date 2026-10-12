import { describe, expect, it } from "vitest";
import { closedReasonLabel, rejoinLaunchTarget } from "./closedClients";
import type { ClientDrop, ClosedClient, RejoinTarget } from "../types";

const t = (text: string, options?: Record<string, unknown>) =>
  text.replace(/\{\{(\w+)\}\}/g, (_, key: string) => String(options?.[key] ?? ""));

function target(overrides: Partial<RejoinTarget> = {}): RejoinTarget {
  return { placeId: 606849621, jobId: "job-a", launchData: "", joinVip: false, linkCode: "", ...overrides };
}

function closed(drop: Partial<ClientDrop>): ClosedClient {
  return {
    userId: 1,
    closedAtMs: 1,
    drop: { kind: "crashed", reason: null, code: null, message: null, sinceMs: 1, ...drop },
    placeId: 606849621,
    rejoin: target(),
  };
}

describe("rejoinLaunchTarget", () => {
  it("a public server goes back to the same place and job", () => {
    expect(rejoinLaunchTarget(target({ launchData: "data" }))).toEqual({
      placeId: "606849621",
      jobId: "job-a",
      launchData: "data",
    });
  });

  it("a private server keeps the VIP join and its code", () => {
    expect(rejoinLaunchTarget(target({ jobId: "", joinVip: true, linkCode: "CODE" }))).toEqual({
      placeId: "606849621",
      jobId: "",
      launchData: "",
      joinVip: true,
      linkCode: "CODE",
    });
  });

  it("a vip: job or a pasted link stays in the job, for the launch to parse as it did the first time", () => {
    // Sem `joinVip`/`linkCode` explícitos: os `false`/"" do backend apagariam
    // o código que o `joinServer` tira do Job ID.
    const out = rejoinLaunchTarget(target({ jobId: "vip:CODE" }));
    expect(out).toEqual({ placeId: "606849621", jobId: "vip:CODE", launchData: "" });
    expect("joinVip" in out).toBe(false);
    expect("linkCode" in out).toBe(false);
  });
});

describe("closedReasonLabel", () => {
  it("uses the same words as the Session for each drop", () => {
    expect(closedReasonLabel(closed({ kind: "crashed" }), t)).toBe("Closed without leaving the game");
    expect(closedReasonLabel(closed({ kind: "kicked", message: "AFK" }), t)).toBe("Kicked: AFK");
    expect(closedReasonLabel(closed({ kind: "serverShutdown", code: 274 }), t)).toBe("The server shut down");
    expect(closedReasonLabel(closed({ kind: "disconnected", reason: "connectionLost" }), t)).toBe(
      "Disconnected: lost connection"
    );
  });
});
