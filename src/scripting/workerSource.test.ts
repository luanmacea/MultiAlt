import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createScriptWorker } from "./workerSource";

class FakeWorker {
  static instances: FakeWorker[] = [];
  constructor(public url: string, public options?: { name?: string }) {
    FakeWorker.instances.push(this);
  }
}

const createdBlobs: Blob[] = [];
const revokeObjectURL = vi.fn();

beforeEach(() => {
  FakeWorker.instances = [];
  createdBlobs.length = 0;
  revokeObjectURL.mockClear();
  vi.stubGlobal("Worker", FakeWorker);
  vi.stubGlobal("URL", {
    createObjectURL: (blob: Blob) => {
      createdBlobs.push(blob);
      return "blob:ram-script";
    },
    revokeObjectURL,
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

async function workerSourceText() {
  createScriptWorker();
  return createdBlobs[0].text();
}

describe("createScriptWorker", () => {
  it("builds a named worker from a revoked javascript blob URL", () => {
    const worker = createScriptWorker();

    expect(worker).toBeInstanceOf(FakeWorker);
    expect(FakeWorker.instances).toHaveLength(1);
    expect(FakeWorker.instances[0].url).toBe("blob:ram-script");
    expect(FakeWorker.instances[0].options).toEqual({ name: "ram-script-worker" });
    expect(createdBlobs[0].type).toBe("text/javascript");
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:ram-script");
  });

  it("creates an independent worker per call", () => {
    createScriptWorker();
    createScriptWorker();
    expect(FakeWorker.instances).toHaveLength(2);
    expect(createdBlobs).toHaveLength(2);
    expect(revokeObjectURL).toHaveBeenCalledTimes(2);
  });
});

describe("worker sandbox source", () => {
  it("keeps escape sequences intact (String.raw template)", async () => {
    const source = await workerSourceText();
    expect(source).toContain("\\n");
    expect(source).toContain("[\\ud800-\\udfff]");
    expect(source).not.toContain("\u0000");
  });

  it("locks down the dangerous worker globals", async () => {
    const source = await workerSourceText();
    for (const name of [
      "fetch",
      "WebSocket",
      "XMLHttpRequest",
      "importScripts",
      "indexedDB",
      "localStorage",
      "Worker",
      "BroadcastChannel",
      "RTCPeerConnection",
    ]) {
      expect(source, name).toContain(`"${name}"`);
    }
    expect(source).toContain("function lockDownDangerousGlobals()");
    expect(source).toContain("const SANDBOX_BLOCKED_NAMES = new Set([");
    expect(source).toContain("function buildBlockedBindingsSource()");
  });

  it("blocks dynamic code evaluation paths", async () => {
    const source = await workerSourceText();
    expect(source).toContain("Dynamic import() is blocked in scripts.");
    expect(source).toContain("importScripts() is blocked in scripts.");
    expect(source).toContain("eval() is blocked in scripts.");
    expect(source).toContain("Indirect eval() is blocked in scripts.");
    expect(source).toContain("Function constructor APIs are blocked in scripts.");
  });

  it("hardens the intrinsics and verifies the constructor barrier", async () => {
    const source = await workerSourceText();
    expect(source).toContain("function hardenIntrinsics()");
    expect(source).toContain("function verifyConstructorBarrier()");
    expect(source).toContain("function deepFreeze(");
  });

  it("declares the sandbox limits", async () => {
    const source = await workerSourceText();
    expect(source).toContain("const HOST_REQUEST_TIMEOUT_MS = 20000;");
    expect(source).toContain("const MAX_LOG_MESSAGE_LENGTH = 4000;");
    expect(source).toContain("const MAX_PENDING_HOST_REQUESTS = 64;");
    expect(source).toContain("const MAX_EVENT_HANDLERS_PER_EVENT = 32;");
    expect(source).toContain("const MAX_TOTAL_EVENT_HANDLERS = 96;");
    expect(source).toContain("const MAX_SCRIPT_SOURCE_BYTES = 262144;");
    expect(source).toContain(
      'const ALLOWED_SCRIPT_EVENTS = new Set(["window:update", "ws", "ui"]);'
    );
  });

  it("exposes the documented ram API surface to scripts", async () => {
    const source = await workerSourceText();
    for (const call of [
      'callHost("invoke"',
      'callHost("window.snapshot"',
      'callHost("http.request"',
      'callHost("ws.connect"',
      'callHost("modal.alert"',
      'callHost("settings.get"',
      'callHost("ui.set"',
    ]) {
      expect(source, call).toContain(call);
    }
    expect(source).toContain("const script = __ram;");
  });

  it("speaks the host message protocol used by the scripts dialog", async () => {
    const source = await workerSourceText();
    expect(source).toContain('type: "host-request"');
    expect(source).toContain('type: "host-log"');
    expect(source).toContain('type: "script-finished"');
    expect(source).toContain('type: "script-error"');
    expect(source).toContain('message.type === "stop"');
  });
});
