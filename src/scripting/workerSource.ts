const WORKER_SOURCE = String.raw`
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const GeneratorFunction = Object.getPrototypeOf(function* () {}).constructor;
const AsyncGeneratorFunction = Object.getPrototypeOf(async function* () {}).constructor;
const pendingRequests = new Map();
const eventHandlers = new Map();
let hostRequestCounter = 0;
let intrinsicsHardened = false;
let globalsLockedDown = false;
const HOST_REQUEST_TIMEOUT_MS = 20000;
const MAX_LOG_MESSAGE_LENGTH = 4000;
const MAX_PENDING_HOST_REQUESTS = 64;
const MAX_EVENT_HANDLERS_PER_EVENT = 32;
const MAX_TOTAL_EVENT_HANDLERS = 96;
const MAX_SCRIPT_SOURCE_BYTES = 262144;
const ALLOWED_SCRIPT_EVENTS = new Set(["window:update", "ws", "ui"]);

const safePostMessage = postMessage.bind(self);
const nativeSetTimeout = setTimeout.bind(self);
const nativeClearTimeout = clearTimeout.bind(self);
const nativeSetInterval = setInterval.bind(self);
const nativeClearInterval = clearInterval.bind(self);
const nativeQueueMicrotask =
  typeof queueMicrotask === "function" ? queueMicrotask.bind(self) : null;

const SANDBOX_BLOCKED_NAMES = new Set([
  "globalThis",
  "self",
  "window",
  "document",
  "navigator",
  "location",
  "origin",
  "postMessage",
  "close",
  "onmessage",
  "onerror",
  "onunhandledrejection",
  "fetch",
  "WebSocket",
  "XMLHttpRequest",
  "Worker",
  "SharedWorker",
  "BroadcastChannel",
  "EventSource",
  "importScripts",
  "indexedDB",
  "caches",
  "localStorage",
  "sessionStorage",
  "Function",
  "AsyncFunction",
  "GeneratorFunction",
  "AsyncGeneratorFunction",
]);

function toMessage(value) {
  if (value instanceof Error) {
    return value.stack || value.message || String(value);
  }
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function createHostError(message) {
  const err = new Error(message || "Unknown host error");
  return err;
}

function utf8ByteLength(value) {
  return new TextEncoder().encode(value).length;
}

function lockDownDangerousGlobals() {
  if (globalsLockedDown) {
    return;
  }

  const blockedGlobalNames = [
    "eval",
    "Function",
    "AsyncFunction",
    "GeneratorFunction",
    "AsyncGeneratorFunction",
    "fetch",
    "WebSocket",
    "XMLHttpRequest",
    "Worker",
    "SharedWorker",
    "BroadcastChannel",
    "EventSource",
    "importScripts",
  ];

  for (const name of blockedGlobalNames) {
    try {
      Object.defineProperty(self, name, {
        value: undefined,
        writable: false,
        configurable: false,
        enumerable: false,
      });
    } catch {
      try {
        self[name] = undefined;
      } catch {
      }
    }
  }

  globalsLockedDown = true;
}

function callHost(action, payload) {
  return new Promise((resolve, reject) => {
    const normalizedAction = typeof action === "string" ? action.trim() : "";
    if (!normalizedAction) {
      reject(createHostError("Host request action is required"));
      return;
    }

    if (pendingRequests.size >= MAX_PENDING_HOST_REQUESTS) {
      reject(
        createHostError(
          "Too many pending host requests (max " + MAX_PENDING_HOST_REQUESTS + ")"
        )
      );
      return;
    }

    const requestId = "req-" + ++hostRequestCounter;
    const timeout = nativeSetTimeout(() => {
      if (!pendingRequests.has(requestId)) {
        return;
      }
      pendingRequests.delete(requestId);
      reject(createHostError("Host request timed out: " + normalizedAction));
    }, HOST_REQUEST_TIMEOUT_MS);

    pendingRequests.set(requestId, {
      resolve: (value) => {
        nativeClearTimeout(timeout);
        resolve(value);
      },
      reject: (error) => {
        nativeClearTimeout(timeout);
        reject(error);
      },
    });

    safePostMessage({
      type: "host-request",
      requestId,
      action: normalizedAction,
      payload,
    });
  });
}

function addHandler(event, handler) {
  if (typeof handler !== "function") {
    return () => {};
  }
  if (typeof event !== "string" || !ALLOWED_SCRIPT_EVENTS.has(event)) {
    throw new Error("Unsupported event subscription: " + String(event));
  }

  if (!eventHandlers.has(event)) {
    eventHandlers.set(event, new Set());
  }
  const set = eventHandlers.get(event);
  if (set.size >= MAX_EVENT_HANDLERS_PER_EVENT) {
    throw new Error("Too many handlers for event: " + event);
  }

  let totalHandlers = 0;
  for (const handlers of eventHandlers.values()) {
    totalHandlers += handlers.size;
  }
  if (totalHandlers >= MAX_TOTAL_EVENT_HANDLERS) {
    throw new Error("Too many total event handlers registered");
  }

  set.add(handler);
  return () => {
    set.delete(handler);
    if (set.size === 0) {
      eventHandlers.delete(event);
    }
  };
}

function emitLocalLog(level, values) {
  const joined = values.map(toMessage).join(" ");
  const message =
    joined.length > MAX_LOG_MESSAGE_LENGTH
      ? joined.slice(0, MAX_LOG_MESSAGE_LENGTH) + "... [truncated]"
      : joined;
  safePostMessage({ type: "host-log", level, message });
}

function assertConstructorDescriptor(proto, label) {
  if (!Object.prototype.hasOwnProperty.call(proto, "constructor")) {
    return;
  }

  const desc = Object.getOwnPropertyDescriptor(proto, "constructor");
  if (!desc) {
    throw new Error("Sandbox hardening failed: missing constructor descriptor for " + label);
  }

  if (
    typeof desc.get === "function" ||
    typeof desc.set === "function" ||
    desc.value !== undefined ||
    desc.writable !== false ||
    desc.configurable !== false
  ) {
    throw new Error("Sandbox hardening failed: constructor still reachable for " + label);
  }
}

function freezeStrict(value, label) {
  if (!Object.isFrozen(value)) {
    Object.freeze(value);
  }

  if (!Object.isFrozen(value)) {
    throw new Error("Sandbox hardening failed: cannot freeze " + label);
  }
}

function hardenPrototypeStrict(proto, label) {
  if (!proto || (typeof proto !== "object" && typeof proto !== "function")) {
    return;
  }

  if (Object.prototype.hasOwnProperty.call(proto, "constructor")) {
    Object.defineProperty(proto, "constructor", {
      value: undefined,
      configurable: false,
      enumerable: false,
      writable: false,
    });
    assertConstructorDescriptor(proto, label);
  }

  freezeStrict(proto, label);
}

function hardenConstructorStrict(ctor, label) {
  if (typeof ctor !== "function") {
    return;
  }
  hardenPrototypeStrict(ctor.prototype, label + ".prototype");
  freezeStrict(ctor, label);
}

function verifyConstructorBarrier() {
  const probes = [
    ["function", (function () {}).constructor],
    ["async function", (async function () {}).constructor],
    ["generator function", (function* () {}).constructor],
    ["async generator function", (async function* () {}).constructor],
  ];

  for (const [label, value] of probes) {
    if (value !== undefined) {
      throw new Error("Sandbox hardening failed: constructor reachable from " + label);
    }
  }

  assertConstructorDescriptor(Object.prototype, "Object.prototype");
  assertConstructorDescriptor(Function.prototype, "Function.prototype");
  assertConstructorDescriptor(AsyncFunction.prototype, "AsyncFunction.prototype");
  assertConstructorDescriptor(GeneratorFunction.prototype, "GeneratorFunction.prototype");
  assertConstructorDescriptor(
    AsyncGeneratorFunction.prototype,
    "AsyncGeneratorFunction.prototype"
  );
}

function hardenIntrinsics() {
  if (intrinsicsHardened) {
    return;
  }

  hardenConstructorStrict(Object, "Object");
  hardenConstructorStrict(Function, "Function");
  hardenConstructorStrict(AsyncFunction, "AsyncFunction");
  hardenConstructorStrict(GeneratorFunction, "GeneratorFunction");
  hardenConstructorStrict(AsyncGeneratorFunction, "AsyncGeneratorFunction");

  verifyConstructorBarrier();
  intrinsicsHardened = true;
}

function deepFreeze(value, seen = new WeakSet()) {
  if (!value || (typeof value !== "object" && typeof value !== "function")) {
    return value;
  }
  if (seen.has(value)) {
    return value;
  }

  seen.add(value);

  const keys = [
    ...Object.getOwnPropertyNames(value),
    ...Object.getOwnPropertySymbols(value),
  ];

  for (const key of keys) {
    try {
      deepFreeze(value[key], seen);
    } catch {
    }
  }

  try {
    Object.freeze(value);
  } catch {
  }
  return value;
}

function createSafeTimerApi() {
  const safeTimers = {
    setTimeout: (handler, ms = 0, ...args) => {
      if (typeof handler !== "function") {
        throw new Error("setTimeout requires a function callback");
      }
      const safeMs = Number.isFinite(ms) ? Math.max(0, Number(ms)) : 0;
      return nativeSetTimeout(handler, safeMs, ...args);
    },
    clearTimeout: (id) => nativeClearTimeout(id),
    setInterval: (handler, ms = 0, ...args) => {
      if (typeof handler !== "function") {
        throw new Error("setInterval requires a function callback");
      }
      const safeMs = Number.isFinite(ms) ? Math.max(0, Number(ms)) : 0;
      return nativeSetInterval(handler, safeMs, ...args);
    },
    clearInterval: (id) => nativeClearInterval(id),
  };
  return deepFreeze(safeTimers);
}

function createSafeConsole() {
  return deepFreeze({
    log: (...args) => emitLocalLog("info", args),
    info: (...args) => emitLocalLog("info", args),
    debug: (...args) => emitLocalLog("debug", args),
    warn: (...args) => emitLocalLog("warn", args),
    error: (...args) => emitLocalLog("error", args),
  });
}

function createSafeMetadata(metadata) {
  const data =
    metadata && typeof metadata === "object" && !Array.isArray(metadata)
      ? { ...metadata }
      : {};
  return deepFreeze(data);
}

function createSafeQueueMicrotask() {
  if (nativeQueueMicrotask) {
    return nativeQueueMicrotask;
  }

  return (handler) => {
    if (typeof handler !== "function") {
      throw new Error("queueMicrotask requires a function callback");
    }
    Promise.resolve().then(handler);
  };
}

function buildBlockedBindingsSource() {
  const lines = [];
  for (const name of SANDBOX_BLOCKED_NAMES) {
    if (!/^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name)) {
      continue;
    }
    lines.push("const " + name + " = undefined;");
  }
  return lines.join("\n");
}

const BLOCKED_BINDINGS_SOURCE = buildBlockedBindingsSource();

function createRamApi() {
  const ram = {
    log: (...args) => emitLocalLog("info", args),
    info: (...args) => emitLocalLog("info", args),
    debug: (...args) => emitLocalLog("debug", args),
    warn: (...args) => emitLocalLog("warn", args),
    error: (...args) => emitLocalLog("error", args),
    now: () => Date.now(),
    randomId: () => {
      const rand = Math.random().toString(36).slice(2, 10);
      return "id-" + Date.now() + "-" + rand;
    },
    sleep: (ms) => {
      const safe = Number.isFinite(ms) ? Math.max(0, Number(ms)) : 0;
      return new Promise((resolve) => nativeSetTimeout(resolve, safe));
    },
    on: (event, handler) => addHandler(event, handler),
    invoke: (command, args = {}) => callHost("invoke", { command, args }),
    window: {
      snapshot: () => callHost("window.snapshot", {}),
      accounts: () => callHost("window.accounts", {}),
      selected: () => callHost("window.selected", {}),
    },
    http: {
      request: (input) => callHost("http.request", input),
      get: (url, options = {}) =>
        callHost("http.request", { ...options, method: "GET", url }),
      post: (url, body = null, options = {}) =>
        callHost("http.request", { ...options, method: "POST", url, body }),
    },
    ws: {
      connect: (input) => callHost("ws.connect", input),
      send: (connectionId, data) =>
        callHost("ws.send", { connectionId, data }),
      close: (connectionId, options = {}) =>
        callHost("ws.close", { connectionId, ...options }),
      list: () => callHost("ws.list", {}),
      on: (handler) => addHandler("ws", handler),
    },
    modal: {
      alert: (input) => callHost("modal.alert", input),
      confirm: (input) => callHost("modal.confirm", input),
      prompt: (input) => callHost("modal.prompt", input),
      json: (input) => callHost("modal.json", input),
    },
    settings: {
      get: (key, defaultValue = null) =>
        callHost("settings.get", { key, defaultValue }),
      set: (key, value) => callHost("settings.set", { key, value }),
      all: () => callHost("settings.all", {}),
    },
    ui: {
      set: (elements) => callHost("ui.set", { elements }),
      patch: (id, patch) => callHost("ui.patch", { id, patch }),
      clear: () => callHost("ui.clear", {}),
      on: (handler) => addHandler("ui", handler),
    },
  };
  return ram;
}

function normalizeUserCode(code) {
  let text = typeof code === "string" ? code : String(code || "");
  if (text.charCodeAt(0) === 0xfeff) {
    text = text.slice(1);
  }

  if (typeof text.normalize === "function") {
    try {
      text = text.normalize("NFKC");
    } catch {
    }
  }

  text = text.replace(/[\uff01-\uff5e]/g, (ch) =>
    String.fromCharCode(ch.charCodeAt(0) - 0xfee0)
  );

  return text
    .replace(/\r\n?/g, "\n")
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/g, "")
    .replace(/[\u200b-\u200f\u2060\ufeff]/g, "")
    .replace(/[\u202a-\u202e]/g, "")
    .replace(/[\ud800-\udfff]/g, "")
    .replace(/\u2028|\u2029/g, "\n")
    .replace(/\u00a0/g, " ")
    .replace(/[\u2018\u2019\u201a\u201b]/g, "'")
    .replace(/[\u201c\u201d\u201e\u201f]/g, '"')
    .replace(/[\u00b4\u02cb\u2032\u2035\uff40]/g, "\u0060")
    .replace(/[\u2013\u2014\u2212]/g, "-");
}

function stripToAsciiForCompile(source) {
  let out = "";
  for (let i = 0; i < source.length; i += 1) {
    const code = source.charCodeAt(i);
    if (code === 9 || code === 10 || code === 13 || (code >= 32 && code <= 126)) {
      out += source[i];
    } else {
      out += " ";
    }
  }
  return out;
}

function describeSuspiciousChar(source) {
  for (let i = 0; i < source.length; i += 1) {
    const code = source.charCodeAt(i);
    const isPrintableAscii = code >= 32 && code <= 126;
    const isWhitespace = code === 9 || code === 10 || code === 13;
    if (isPrintableAscii || isWhitespace) continue;

    const before = source.slice(Math.max(0, i - 12), i);
    const after = source.slice(i + 1, Math.min(source.length, i + 13));
    const line = source.slice(0, i).split("\n").length;
    const col = i - source.lastIndexOf("\n", i - 1);
    return (
      " [suspicious U+" +
      code.toString(16).padStart(4, "0") +
      " at line " +
      line +
      ", col " +
      col +
      "] " +
      before +
      "<?>" +
      after
    );
  }
  return "";
}

function assertNoDynamicImport(source) {
  const importCallPattern =
    /\bimport(?:\s|\/\*[\s\S]*?\*\/|\/\/[^\n]*(?:\n|$))*\(/;
  const importScriptsPattern =
    /\bimportScripts(?:\s|\/\*[\s\S]*?\*\/|\/\/[^\n]*(?:\n|$))*\(/;
  const evalPattern =
    /\beval(?:\s|\/\*[\s\S]*?\*\/|\/\/[^\n]*(?:\n|$))*\(/;
  const optionalEvalPattern = /\beval\s*\?\.\s*\(/;
  const indirectEvalPattern = /\(\s*0\s*,\s*eval\s*\)\s*\(/;

  const functionCtorPattern =
    /\b(?:Function|AsyncFunction|GeneratorFunction|AsyncGeneratorFunction)(?:\s|\/\*[\s\S]*?\*\/|\/\/[^\n]*(?:\n|$))*\(/;

  if (importCallPattern.test(source)) {
    throw new Error(
      "Dynamic import() is blocked in scripts. Use ram.http.* or ram.ws.* for external integrations."
    );
  }
  if (importScriptsPattern.test(source)) {
    throw new Error("importScripts() is blocked in scripts.");
  }
  if (evalPattern.test(source)) {
    throw new Error("eval() is blocked in scripts.");
  }
  if (optionalEvalPattern.test(source) || indirectEvalPattern.test(source)) {
    throw new Error("Indirect eval() is blocked in scripts.");
  }
  if (functionCtorPattern.test(source)) {
    throw new Error("Function constructor APIs are blocked in scripts.");
  }
}

function createSandboxRunner(source) {
  const body = [
    '"use strict";',
    "const ram = __ram;",
    "const script = __ram;",
    "const metadata = __metadata;",
    "const console = __console;",
    "const setTimeout = __setTimeout;",
    "const clearTimeout = __clearTimeout;",
    "const setInterval = __setInterval;",
    "const clearInterval = __clearInterval;",
    "const queueMicrotask = __queueMicrotask;",
    BLOCKED_BINDINGS_SOURCE,
    source,
  ].join("\n");
  return new AsyncFunction(
    "__ram",
    "__metadata",
    "__console",
    "__setTimeout",
    "__clearTimeout",
    "__setInterval",
    "__clearInterval",
    "__queueMicrotask",
    body
  );
}

function extractSyntaxContext(error, source, preludeLines) {
  const text = toMessage(error);
  const match = text.match(/<anonymous>:(\d+):(\d+)/);
  if (!match) return "";

  const rawLine = Number(match[1]);
  const rawColumn = Number(match[2]);
  if (!Number.isFinite(rawLine) || !Number.isFinite(rawColumn)) return "";

  const scriptLine = rawLine - preludeLines;
  if (scriptLine < 1) return " [line " + scriptLine + ", col " + rawColumn + "]";

  const lines = source.split("\n");
  const content = lines[scriptLine - 1] || "";
  const snippet = content.length > 180 ? content.slice(0, 180) + "..." : content;
  return " [line " + scriptLine + ", col " + rawColumn + "] " + snippet;
}

async function runUserScript(code, metadata) {
  try {
    hardenIntrinsics();
  } catch (error) {
    throw new Error("Script sandbox initialization failed: " + toMessage(error));
  }

  try {
    lockDownDangerousGlobals();
  } catch (error) {
    throw new Error("Script sandbox initialization failed: " + toMessage(error));
  }

  const ram = deepFreeze(createRamApi());
  const safeMetadata = createSafeMetadata(metadata);
  const safeConsole = createSafeConsole();
  const safeTimers = createSafeTimerApi();
  const safeQueueMicrotask = createSafeQueueMicrotask();
  const normalizedCode = normalizeUserCode(code);

  if (utf8ByteLength(normalizedCode) > MAX_SCRIPT_SOURCE_BYTES) {
    throw new Error(
      "Script compile failed: source exceeds " +
        MAX_SCRIPT_SOURCE_BYTES +
        " bytes and was rejected"
    );
  }

  try {
    assertNoDynamicImport(normalizedCode);
  } catch (error) {
    throw new Error("Script compile failed: " + toMessage(error));
  }

  let wrapped = null;
  let compileSource = normalizedCode;

  try {
    wrapped = createSandboxRunner(compileSource);
  } catch (error) {
    const fallbackCode = stripToAsciiForCompile(normalizedCode);
    if (fallbackCode !== normalizedCode) {
      try {
        assertNoDynamicImport(fallbackCode);
        wrapped = createSandboxRunner(fallbackCode);
        compileSource = fallbackCode;
        emitLocalLog("warn", [
          "Script contained non-ASCII characters; runtime sanitized them for compilation. Save script to remove hidden characters.",
        ]);
      } catch (fallbackError) {
        const context = extractSyntaxContext(fallbackError, fallbackCode, 3);
        const suspicious = describeSuspiciousChar(normalizedCode);
        throw new Error(
          "Script compile failed: " +
            toMessage(fallbackError) +
            " (check for invalid quotes/backticks or hidden characters)" +
            context +
            suspicious
        );
      }
    } else {
      const context = extractSyntaxContext(error, normalizedCode, 3);
      const suspicious = describeSuspiciousChar(normalizedCode);
      throw new Error(
        "Script compile failed: " +
          toMessage(error) +
          " (check for invalid quotes/backticks or hidden characters)" +
          context +
          suspicious
      );
    }
  }

  try {
    await wrapped(
      ram,
      safeMetadata,
      safeConsole,
      safeTimers.setTimeout,
      safeTimers.clearTimeout,
      safeTimers.setInterval,
      safeTimers.clearInterval,
      safeQueueMicrotask
    );
  } catch (error) {
    throw new Error("Script runtime failed: " + toMessage(error));
  }
}

self.addEventListener("unhandledrejection", (event) => {
  safePostMessage({
    type: "script-error",
    error: toMessage(event.reason),
  });
});

self.onmessage = async (event) => {
  const message = event.data || {};

  if (message.type === "host-response") {
    const pending = pendingRequests.get(message.requestId);
    if (!pending) return;
    pendingRequests.delete(message.requestId);
    if (message.ok) {
      pending.resolve(message.result);
    } else {
      pending.reject(createHostError(message.error));
    }
    return;
  }

  if (message.type === "host-event") {
    const handlers = eventHandlers.get(message.event);
    if (!handlers || handlers.size === 0) return;
    for (const handler of handlers) {
      try {
        Promise.resolve(handler(message.payload)).catch((err) => {
          emitLocalLog("error", [err]);
        });
      } catch (err) {
        emitLocalLog("error", [err]);
      }
    }
    return;
  }

  if (message.type === "start") {
    try {
      await runUserScript(message.code || "", message.metadata || {});
      safePostMessage({ type: "script-finished" });
    } catch (err) {
      safePostMessage({ type: "script-error", error: toMessage(err) });
    }
    return;
  }

  if (message.type === "stop") {
    for (const pending of pendingRequests.values()) {
      try {
        pending.reject(createHostError("Script stopped"));
      } catch {
      }
    }
    pendingRequests.clear();
    eventHandlers.clear();
    close();
  }
};
`;

export function createScriptWorker(): Worker {
  const blob = new Blob([WORKER_SOURCE], { type: "text/javascript" });
  const workerUrl = URL.createObjectURL(blob);
  const worker = new Worker(workerUrl, { name: "ram-script-worker" });
  URL.revokeObjectURL(workerUrl);
  return worker;
}
