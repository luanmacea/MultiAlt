import { describe, expect, it } from "vitest";
import {
  SCRIPT_SECURITY_LIMITS,
  assertScriptPermission,
  buildScriptSettingsSection,
  getScriptSecuritySignature,
  isPrivateOrLoopbackHost,
  normalizeScriptHttpUrl,
  normalizeWebSocketUrl,
  sanitizeScriptSourceForSave,
  truncateForLog,
  utf8ByteLength,
} from "./security";
import type { ScriptPermissions } from "./types";

function permissions(overrides: Partial<ScriptPermissions> = {}): ScriptPermissions {
  return {
    allowInvoke: false,
    allowHttp: false,
    allowWebSocket: false,
    allowWindow: false,
    allowModal: false,
    allowSettings: false,
    allowUi: false,
    ...overrides,
  };
}

describe("normalizeScriptHttpUrl", () => {
  it("normalizes a public https URL and keeps path/query", () => {
    expect(normalizeScriptHttpUrl("https://example.com/api?x=1", false)).toBe(
      "https://example.com/api?x=1"
    );
  });

  it("appends the root path for a bare origin", () => {
    expect(normalizeScriptHttpUrl("https://example.com", false)).toBe("https://example.com/");
  });

  it("trims surrounding whitespace before parsing", () => {
    expect(normalizeScriptHttpUrl("  http://example.com/  ", false)).toBe("http://example.com/");
  });

  it("rejects an empty URL", () => {
    expect(() => normalizeScriptHttpUrl("   ", false)).toThrow("Missing request URL");
  });

  it("rejects an unparsable URL", () => {
    expect(() => normalizeScriptHttpUrl("not a url", false)).toThrow("Invalid request URL");
  });

  it.each(["ftp://example.com/x", "file:///etc/passwd", "javascript:alert(1)", "ws://example.com"])(
    "rejects non-http(s) protocol %s",
    (url) => {
      expect(() => normalizeScriptHttpUrl(url, false)).toThrow(
        "Request URL must start with http:// or https://"
      );
    }
  );

  it.each([
    "http://localhost:8080/x",
    "http://127.0.0.1/x",
    "http://10.1.2.3/x",
    "http://192.168.0.5/x",
    "http://172.16.0.1/x",
    "http://172.31.255.254/x",
    "http://[::1]/x",
  ])("blocks private/loopback target %s by default", (url) => {
    expect(() => normalizeScriptHttpUrl(url, false)).toThrow(
      /Private-network and localhost HTTP targets are blocked/
    );
  });

  it.each([
    "http://localhost:8080/x",
    "http://127.0.0.1/x",
    "http://10.1.2.3/x",
    "http://192.168.0.5/x",
    "http://[::1]/x",
  ])("allows private/loopback target %s when allowPrivateNetwork is true", (url) => {
    expect(() => normalizeScriptHttpUrl(url, true)).not.toThrow();
  });

  it("still rejects a bad protocol even when allowPrivateNetwork is true", () => {
    expect(() => normalizeScriptHttpUrl("ftp://127.0.0.1/x", true)).toThrow(
      "Request URL must start with http:// or https://"
    );
  });
});

describe("normalizeWebSocketUrl", () => {
  it("normalizes a public wss URL", () => {
    expect(normalizeWebSocketUrl("wss://example.com/socket", false)).toBe(
      "wss://example.com/socket"
    );
  });

  it("rejects an empty URL", () => {
    expect(() => normalizeWebSocketUrl("", false)).toThrow("WebSocket URL is required");
  });

  it("rejects an unparsable URL", () => {
    expect(() => normalizeWebSocketUrl("::::", false)).toThrow("Invalid WebSocket URL");
  });

  it.each(["http://example.com", "https://example.com", "ftp://example.com"])(
    "rejects non-ws(s) protocol %s",
    (url) => {
      expect(() => normalizeWebSocketUrl(url, false)).toThrow(
        "WebSocket URL must start with ws:// or wss://"
      );
    }
  );

  it.each(["ws://localhost:1234/", "ws://127.0.0.1:1234/", "ws://192.168.1.10/", "ws://[::1]/"])(
    "blocks private/loopback target %s by default",
    (url) => {
      expect(() => normalizeWebSocketUrl(url, false)).toThrow(
        /Private-network and localhost WebSocket targets are blocked/
      );
    }
  );

  it("allows a loopback socket when allowPrivateNetwork is true", () => {
    expect(normalizeWebSocketUrl("ws://127.0.0.1:1234/nexus", true)).toBe(
      "ws://127.0.0.1:1234/nexus"
    );
  });
});

describe("isPrivateOrLoopbackHost", () => {
  it.each([
    ["localhost", true],
    ["LOCALHOST", true],
    ["localhost.", true],
    ["app.localhost", true],
    ["", true],
    ["   ", true],
    ["127.0.0.1", true],
    ["127.255.1.2", true],
    ["0.0.0.0", true],
    ["10.0.0.1", true],
    ["169.254.10.1", true],
    ["172.16.0.1", true],
    ["172.31.255.255", true],
    ["192.168.1.1", true],
    ["::1", true],
    ["[::1]", true],
    ["::", true],
    ["fe80::1", true],
    ["fd00::1", true],
    ["fc00::1", true],
    ["::ffff:127.0.0.1", true],
    ["fe80::1%eth0", true],
    ["2130706433", true],
    ["0x7f.0.0.1", true],
    ["8.8.8.8", false],
    ["172.15.0.1", false],
    ["172.32.0.1", false],
    ["192.169.1.1", false],
    ["example.com", false],
    ["api.roblox.com", false],
    ["::ffff:8.8.8.8", false],
    ["2606:4700:4700::1111", false],
  ])("classifies %s as private=%s", (host, expected) => {
    expect(isPrivateOrLoopbackHost(host as string)).toBe(expected);
  });
});

describe("utf8ByteLength", () => {
  it.each([
    ["", 0],
    ["abc", 3],
    ["é", 2],
    ["中", 3],
    ["😀", 4],
  ])("measures %s as %i bytes", (value, expected) => {
    expect(utf8ByteLength(value as string)).toBe(expected);
  });

  it("counts bytes, not code units", () => {
    const emoji = "😀";
    expect(emoji.length).toBe(2);
    expect(utf8ByteLength(emoji)).toBe(4);
  });
});

describe("truncateForLog", () => {
  it("returns short strings unchanged", () => {
    expect(truncateForLog("hello")).toBe("hello");
  });

  it("truncates at the given limit and marks it", () => {
    expect(truncateForLog("abcdefg", 3)).toBe("abc... [truncated]");
  });

  it("keeps a string exactly at the limit", () => {
    expect(truncateForLog("abc", 3)).toBe("abc");
  });

  it("uses the shared default limit", () => {
    const long = "x".repeat(SCRIPT_SECURITY_LIMITS.maxLogMessageChars + 10);
    const out = truncateForLog(long);
    expect(out).toHaveLength(SCRIPT_SECURITY_LIMITS.maxLogMessageChars + "... [truncated]".length);
    expect(out.endsWith("... [truncated]")).toBe(true);
  });

  it.each([
    [null, ""],
    [undefined, ""],
    [123, "123"],
    [true, "true"],
  ])("stringifies non-string input %s", (input, expected) => {
    expect(truncateForLog(input)).toBe(expected);
  });

  it("stringifies objects", () => {
    expect(truncateForLog({})).toBe("[object Object]");
  });
});

describe("sanitizeScriptSourceForSave", () => {
  it("strips a leading BOM", () => {
    expect(sanitizeScriptSourceForSave("﻿const a = 1;")).toBe("const a = 1;");
  });

  it("normalizes CRLF and lone CR to LF", () => {
    expect(sanitizeScriptSourceForSave("a\r\nb\rc")).toBe("a\nb\nc");
  });

  it("removes control and zero-width characters", () => {
    expect(sanitizeScriptSourceForSave("a\u0000b​c⁠d‮e")).toBe("abcde");
  });

  it("normalizes smart quotes, backticks and dashes", () => {
    expect(sanitizeScriptSourceForSave("‘a’ “b” ′ —")).toBe(
      "'a' \"b\" ` -"
    );
  });

  it("applies NFKC normalization", () => {
    expect(sanitizeScriptSourceForSave("ａｂ")).toBe("ab");
  });

  it("unwraps a fenced code block", () => {
    expect(sanitizeScriptSourceForSave("```js\nconst a = 1;\nram.info(a);\n```")).toBe(
      "const a = 1;\nram.info(a);"
    );
  });

  it("unwraps a fence without a language tag", () => {
    expect(sanitizeScriptSourceForSave("```\nlet x = 2;\n```")).toBe("let x = 2;");
  });

  it("leaves an inline fence-looking string alone", () => {
    expect(sanitizeScriptSourceForSave("const s = '```';")).toBe("const s = '```';");
  });

  it("keeps tabs and newlines", () => {
    expect(sanitizeScriptSourceForSave("a\n\tb")).toBe("a\n\tb");
  });

  it.each([
    [null, ""],
    [undefined, ""],
    ["", ""],
  ])("coerces non-string input %s to a string", (input, expected) => {
    expect(sanitizeScriptSourceForSave(input as unknown as string)).toBe(expected);
  });
});

describe("assertScriptPermission", () => {
  it("throws when the permission flag is off", () => {
    const script = { trusted: true, permissions: permissions() };
    expect(() => assertScriptPermission(script, "allowHttp", "ram.http.request", false)).toThrow(
      "Permission denied for ram.http.request"
    );
  });

  it("throws when trust is required but the script is untrusted", () => {
    const script = { trusted: false, permissions: permissions({ allowInvoke: true }) };
    expect(() => assertScriptPermission(script, "allowInvoke", "ram.invoke", true)).toThrow(
      "Action ram.invoke requires trusted mode"
    );
  });

  it("passes when the permission is granted and trust is not required", () => {
    const script = { trusted: false, permissions: permissions({ allowUi: true }) };
    expect(() => assertScriptPermission(script, "allowUi", "ram.ui.set", false)).not.toThrow();
  });

  it("passes when the permission is granted and the script is trusted", () => {
    const script = { trusted: true, permissions: permissions({ allowInvoke: true }) };
    expect(() => assertScriptPermission(script, "allowInvoke", "ram.invoke", true)).not.toThrow();
  });

  it("checks the permission flag before the trust flag", () => {
    const script = { trusted: false, permissions: permissions() };
    expect(() => assertScriptPermission(script, "allowInvoke", "ram.invoke", true)).toThrow(
      "Permission denied for ram.invoke"
    );
  });
});

describe("getScriptSecuritySignature", () => {
  it("is all zeros for an untrusted script without permissions", () => {
    expect(getScriptSecuritySignature({ trusted: false, permissions: permissions() })).toBe(
      "00000000"
    );
  });

  it("is all ones for a trusted script with every permission", () => {
    expect(
      getScriptSecuritySignature({
        trusted: true,
        permissions: permissions({
          allowInvoke: true,
          allowHttp: true,
          allowWebSocket: true,
          allowWindow: true,
          allowModal: true,
          allowSettings: true,
          allowUi: true,
        }),
      })
    ).toBe("11111111");
  });

  it("encodes trusted first, then the permission order", () => {
    expect(
      getScriptSecuritySignature({ trusted: false, permissions: permissions({ allowHttp: true }) })
    ).toBe("00100000");
  });

  it("changes when any single flag changes", () => {
    const base = getScriptSecuritySignature({ trusted: false, permissions: permissions() });
    const withUi = getScriptSecuritySignature({
      trusted: false,
      permissions: permissions({ allowUi: true }),
    });
    expect(withUi).not.toBe(base);
  });
});

describe("buildScriptSettingsSection", () => {
  it("uses the raw id when it is filesystem/INI safe", () => {
    expect(buildScriptSettingsSection("my-script_1.0")).toBe("Script.my-script_1.0");
  });

  it("trims the id before validating", () => {
    expect(buildScriptSettingsSection("  abc  ")).toBe("Script.abc");
  });

  it("hashes an empty id deterministically", () => {
    expect(buildScriptSettingsSection("")).toBe("Script.id-811c9dc5-empty");
  });

  it("hashes ids with unsafe characters", () => {
    const section = buildScriptSettingsSection("bad id/../x");
    expect(section).toMatch(/^Script\.id-[0-9a-f]{8}-[0-9a-f]+$/);
    expect(section).not.toContain("/");
    expect(section).not.toContain(" ");
  });

  it("is deterministic and collision-distinct", () => {
    expect(buildScriptSettingsSection("a b")).toBe(buildScriptSettingsSection("a b"));
    expect(buildScriptSettingsSection("a b")).not.toBe(buildScriptSettingsSection("a c"));
  });

  it("hashes ids longer than the safe-id limit", () => {
    const long = "a".repeat(SCRIPT_SECURITY_LIMITS.maxScriptIdChars + 1);
    const section = buildScriptSettingsSection(long);
    expect(section.startsWith("Script.id-")).toBe(true);
    expect(section.length).toBeLessThanOrEqual("Script.id-".length + 8 + 1 + 48);
  });
});

describe("SCRIPT_SECURITY_LIMITS", () => {
  it("exposes positive integer limits", () => {
    for (const [key, value] of Object.entries(SCRIPT_SECURITY_LIMITS)) {
      expect(Number.isInteger(value), `${key} should be an integer`).toBe(true);
      expect(value, `${key} should be positive`).toBeGreaterThan(0);
    }
  });
});
