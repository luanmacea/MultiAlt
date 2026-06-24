import type { ScriptPermissions } from "./types";

export const SCRIPT_SECURITY_LIMITS = {
  maxScriptIdChars: 96,
  maxScriptNameChars: 120,
  maxScriptDescriptionChars: 2048,
  maxScriptSourceBytes: 262_144,
  maxLogMessageChars: 4000,
  maxPendingHostRequests: 64,
  maxHttpTimeoutMs: 30_000,
  maxHttpHeaders: 64,
  maxHttpHeaderNameChars: 128,
  maxHttpHeaderValueChars: 2048,
  maxHttpBodyBytes: 262_144,
  maxHttpResponseBytes: 524_288,
  maxWebSocketConnections: 8,
  maxWebSocketMessageBytes: 262_144,
  maxWebSocketProtocols: 8,
  maxWebSocketProtocolChars: 128,
  maxWebSocketConnectionIdChars: 96,
  maxUiElements: 80,
  maxUiOptionsPerElement: 120,
  maxUiTextChars: 2048,
  maxUiPatchBytes: 16_384,
  maxSettingsKeyChars: 80,
  maxSettingsValueChars: 4096,
} as const;

const SAFE_SCRIPT_ID_PATTERN = /^[A-Za-z0-9._-]{1,96}$/;
const STRICT_IPV4_PART_PATTERN = /^(0|[1-9]\d{0,2})$/;
const HEX_GROUP_PATTERN = /^[0-9a-f]{1,4}$/;
const AMBIGUOUS_NUMERIC_PART_PATTERN = /^(?:0x[0-9a-f]+|0[0-7]+|\d+)$/i;

function parseIPv4AddressStrict(hostname: string): [number, number, number, number] | null {
  const parts = hostname.split(".");
  if (parts.length !== 4) return null;

  const out: number[] = [];
  for (const part of parts) {
    if (!STRICT_IPV4_PART_PATTERN.test(part)) {
      return null;
    }
    const value = Number(part);
    if (!Number.isInteger(value) || value < 0 || value > 255) {
      return null;
    }
    out.push(value);
  }

  return [out[0], out[1], out[2], out[3]];
}

function isPrivateOrLoopbackIPv4(ipv4: [number, number, number, number]): boolean {
  const [a, b] = ipv4;
  return (
    a === 10 ||
    a === 127 ||
    a === 0 ||
    (a === 169 && b === 254) ||
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 168)
  );
}

function parseIPv6Address(hostname: string): Uint8Array | null {
  let host = hostname.trim().toLowerCase();
  if (!host) return null;
  if (host.startsWith("[") && host.endsWith("]")) {
    host = host.slice(1, -1);
  }

  const zoneIndex = host.indexOf("%");
  if (zoneIndex >= 0) {
    host = host.slice(0, zoneIndex);
  }
  if (!host) return null;

  const doubleColonParts = host.split("::");
  if (doubleColonParts.length > 2) {
    return null;
  }

  const parseSide = (raw: string): string[] => {
    if (!raw) return [];
    return raw.split(":").filter((part) => part.length > 0);
  };

  let left = parseSide(doubleColonParts[0] || "");
  let right = parseSide(doubleColonParts[1] || "");

  const expandIpv4Tail = (parts: string[]): string[] | null => {
    if (parts.length === 0) return parts;
    const tail = parts[parts.length - 1];
    if (!tail.includes(".")) {
      return parts;
    }
    const ipv4 = parseIPv4AddressStrict(tail);
    if (!ipv4) {
      return null;
    }
    const high = ((ipv4[0] << 8) | ipv4[1]).toString(16);
    const low = ((ipv4[2] << 8) | ipv4[3]).toString(16);
    return [...parts.slice(0, -1), high, low];
  };

  const leftExpanded = expandIpv4Tail(left);
  if (!leftExpanded) return null;
  left = leftExpanded;

  const rightExpanded = expandIpv4Tail(right);
  if (!rightExpanded) return null;
  right = rightExpanded;

  const hasDoubleColon = doubleColonParts.length === 2;
  if (hasDoubleColon) {
    if (left.length + right.length > 8) {
      return null;
    }
    const fill = 8 - (left.length + right.length);
    const middle = new Array(fill).fill("0");
    left = [...left, ...middle, ...right];
  } else if (left.length !== 8) {
    return null;
  }

  if (left.length !== 8) {
    return null;
  }

  const out = new Uint8Array(16);
  for (let i = 0; i < left.length; i += 1) {
    const part = left[i];
    if (!HEX_GROUP_PATTERN.test(part)) {
      return null;
    }
    const value = Number.parseInt(part, 16);
    if (!Number.isInteger(value) || value < 0 || value > 0xffff) {
      return null;
    }
    out[i * 2] = (value >> 8) & 0xff;
    out[i * 2 + 1] = value & 0xff;
  }

  return out;
}

function isPrivateOrLoopbackIPv6(bytes: Uint8Array): boolean {
  let allZero = true;
  for (const value of bytes) {
    if (value !== 0) {
      allZero = false;
      break;
    }
  }
  if (allZero) {
    return true;
  }

  let loopback = true;
  for (let i = 0; i < 15; i += 1) {
    if (bytes[i] !== 0) {
      loopback = false;
      break;
    }
  }
  if (loopback && bytes[15] === 1) {
    return true;
  }

  if ((bytes[0] & 0xfe) === 0xfc) {
    return true;
  }

  if (bytes[0] === 0xfe && (bytes[1] & 0xc0) === 0x80) {
    return true;
  }

  const isMappedIpv4 =
    bytes[0] === 0 &&
    bytes[1] === 0 &&
    bytes[2] === 0 &&
    bytes[3] === 0 &&
    bytes[4] === 0 &&
    bytes[5] === 0 &&
    bytes[6] === 0 &&
    bytes[7] === 0 &&
    bytes[8] === 0 &&
    bytes[9] === 0 &&
    bytes[10] === 0xff &&
    bytes[11] === 0xff;

  if (isMappedIpv4) {
    return isPrivateOrLoopbackIPv4([bytes[12], bytes[13], bytes[14], bytes[15]]);
  }

  return false;
}

function isAmbiguousNumericHost(hostname: string): boolean {
  const host = hostname.trim().toLowerCase();
  if (!host) return true;
  if (host.includes(":")) {
    return false;
  }

  if (parseIPv4AddressStrict(host)) {
    return false;
  }

  if (/^[0-9.]+$/.test(host)) {
    return true;
  }

  const numericParts = host.split(".");
  if (
    numericParts.length >= 1 &&
    numericParts.length <= 4 &&
    numericParts.every((part) => AMBIGUOUS_NUMERIC_PART_PATTERN.test(part))
  ) {
    return true;
  }

  return false;
}

export function utf8ByteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}

export function truncateForLog(
  value: unknown,
  maxChars: number = SCRIPT_SECURITY_LIMITS.maxLogMessageChars
): string {
  const text = typeof value === "string" ? value : String(value ?? "");
  if (text.length <= maxChars) {
    return text;
  }
  return text.slice(0, maxChars) + "... [truncated]";
}

export function sanitizeScriptSourceForSave(input: string): string {
  let text = typeof input === "string" ? input : String(input || "");
  if (text.charCodeAt(0) === 0xfeff) {
    text = text.slice(1);
  }

  if (typeof text.normalize === "function") {
    try {
      text = text.normalize("NFKC");
    } catch {
    }
  }

  text = text
    .replace(/\r\n?/g, "\n")
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/g, "")
    .replace(/[\u200b-\u200f\u2060\ufeff]/g, "")
    .replace(/[\u202a-\u202e]/g, "")
    .replace(/[\ud800-\udfff]/g, "")
    .replace(/[\u2018\u2019\u201a\u201b]/g, "'")
    .replace(/[\u201c\u201d\u201e\u201f]/g, "\"")
    .replace(/[\u00b4\u02cb\u2032\u2035\uff40]/g, "\u0060")
    .replace(/[\u2013\u2014\u2212]/g, "-");

  const fence = text.match(/^\s*```[a-zA-Z0-9_-]*\s*\n([\s\S]*?)\n\s*```\s*$/);
  if (fence && fence[1]) {
    text = fence[1];
  }

  return text;
}

export function normalizeScriptHttpUrl(input: string, allowPrivateNetwork: boolean): string {
  const value = input.trim();
  if (!value) {
    throw new Error("Missing request URL");
  }

  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    throw new Error("Invalid request URL");
  }

  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    throw new Error("Request URL must start with http:// or https://");
  }

  if (!allowPrivateNetwork && isPrivateOrLoopbackHost(parsed.hostname)) {
    throw new Error(
      "Private-network and localhost HTTP targets are blocked by default. Set allowPrivateNetwork: true to override."
    );
  }

  return parsed.toString();
}

export function normalizeWebSocketUrl(input: string, allowPrivateNetwork: boolean): string {
  const value = input.trim();
  if (!value) {
    throw new Error("WebSocket URL is required");
  }

  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    throw new Error("Invalid WebSocket URL");
  }
  if (parsed.protocol !== "ws:" && parsed.protocol !== "wss:") {
    throw new Error("WebSocket URL must start with ws:// or wss://");
  }

  if (!allowPrivateNetwork && isPrivateOrLoopbackHost(parsed.hostname)) {
    throw new Error(
      "Private-network and localhost WebSocket targets are blocked by default. Set allowPrivateNetwork: true to override."
    );
  }

  return parsed.toString();
}

export function isPrivateOrLoopbackHost(hostname: string): boolean {
  const host = hostname.trim().toLowerCase().replace(/\.$/, "");
  if (!host) return true;

  if (host === "localhost" || host.endsWith(".localhost")) {
    return true;
  }

  if (host.includes("%")) {
    return true;
  }

  if (isAmbiguousNumericHost(host)) {
    return true;
  }

  const ipv4 = parseIPv4AddressStrict(host);
  if (ipv4) {
    return isPrivateOrLoopbackIPv4(ipv4);
  }

  const ipv6 = parseIPv6Address(host);
  if (ipv6) {
    return isPrivateOrLoopbackIPv6(ipv6);
  }

  if (host.includes(":")) {
    return true;
  }

  return false;
}

export function getScriptSecuritySignature(script: {
  trusted: boolean;
  permissions: ScriptPermissions;
}): string {
  const p = script.permissions;
  return [
    script.trusted ? "1" : "0",
    p.allowInvoke ? "1" : "0",
    p.allowHttp ? "1" : "0",
    p.allowWebSocket ? "1" : "0",
    p.allowWindow ? "1" : "0",
    p.allowModal ? "1" : "0",
    p.allowSettings ? "1" : "0",
    p.allowUi ? "1" : "0",
  ].join("");
}

export function assertScriptPermission(
  script: { trusted: boolean; permissions: ScriptPermissions },
  permission: keyof ScriptPermissions,
  action: string,
  trustRequired: boolean
): void {
  if (!script.permissions[permission]) {
    throw new Error(`Permission denied for ${action}`);
  }
  if (trustRequired && !script.trusted) {
    throw new Error(`Action ${action} requires trusted mode`);
  }
}

function fnv1a32(value: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < value.length; i += 1) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

export function buildScriptSettingsSection(scriptId: string): string {
  const raw = String(scriptId || "").trim();
  if (SAFE_SCRIPT_ID_PATTERN.test(raw)) {
    return `Script.${raw}`;
  }

  const bytes = new TextEncoder().encode(raw);
  let hex = "";
  for (let i = 0; i < bytes.length && hex.length < 48; i += 1) {
    hex += bytes[i].toString(16).padStart(2, "0");
  }
  if (!hex) {
    hex = "empty";
  }
  const hash = fnv1a32(raw).toString(16).padStart(8, "0");
  return `Script.id-${hash}-${hex}`;
}
