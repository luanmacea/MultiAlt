import { describe, expect, it } from "vitest";
import { COOKIE_PATTERN, parseImportLine } from "./cookies";

const COOKIE = `_|WARNING:-DO-NOT-SHARE-THIS.--Sharing-this-will-allow-someone-to-log-in-as-you-and-to-steal-your-ROBUX-and-items.|TOKEN123`;

describe("COOKIE_PATTERN", () => {
  it("finds the cookie inside a longer line", () => {
    const match = `alt_one:hunter2:${COOKIE}`.match(COOKIE_PATTERN);
    expect(match?.[0]).toBe(COOKIE);
  });

  it("does not match a line without a cookie", () => {
    expect("alt_one:hunter2".match(COOKIE_PATTERN)).toBeNull();
  });
});

describe("parseImportLine — a line that is only a cookie", () => {
  it("keeps the whole cookie and leaves the credentials empty", () => {
    expect(parseImportLine(COOKIE)).toEqual({
      kind: "cookie",
      username: "",
      password: "",
      cookie: COOKIE,
    });
  });

  it("ignores space at the ends", () => {
    expect(parseImportLine(`   ${COOKIE}  `).cookie).toBe(COOKIE);
  });
});

/**
 * O detalhe que quebra tudo: o próprio cookie tem `:` dentro dele
 * (`_|WARNING:-DO-NOT-SHARE...`). Um `split(":")` ingênuo devolve
 * `["alt_one", "hunter2", "_|WARNING", "-DO-NOT-SHARE-THIS..."]` e importa
 * meio cookie — credencial quebrada, conta que nunca entra.
 */
describe("parseImportLine — username:password:cookie", () => {
  it("splits on the delimiter without cutting the cookie's own colon", () => {
    const parsed = parseImportLine(`alt_one:hunter2:${COOKIE}`);
    expect(parsed).toEqual({
      kind: "cookie",
      username: "alt_one",
      password: "hunter2",
      cookie: COOKIE,
    });
    expect(parsed.cookie).toContain("WARNING:-DO-NOT-SHARE");
  });

  it("keeps a password that has colons of its own", () => {
    const parsed = parseImportLine(`alt_one:a:b:c:${COOKIE}`);
    expect(parsed.username).toBe("alt_one");
    expect(parsed.password).toBe("a:b:c");
    expect(parsed.cookie).toBe(COOKIE);
  });

  it("strips only the delimiter, not punctuation the password ends with", () => {
    const parsed = parseImportLine(`alt_one:hunter2;:${COOKIE}`);
    expect(parsed.password).toBe("hunter2;");
    expect(parsed.cookie).toBe(COOKIE);
  });

  it("ignores space around the delimiter", () => {
    const parsed = parseImportLine(`  alt_one : hunter2 : ${COOKIE}  `);
    expect(parsed.username).toBe("alt_one");
    expect(parsed.password).toBe("hunter2");
    expect(parsed.cookie).toBe(COOKIE);
  });

  it("takes the cookie alone when the line has a username but no password", () => {
    const parsed = parseImportLine(`alt_one:${COOKIE}`);
    expect(parsed.kind).toBe("cookie");
    expect(parsed.username).toBe("");
    expect(parsed.password).toBe("");
    expect(parsed.cookie).toBe(COOKIE);
  });

  it("treats a cookie whose warning text does not match as a cookie anyway", () => {
    // O valor começa pelo marcador do cookie, então não é `user:pass` — pedir
    // que o backend valide é melhor que pular a linha em silêncio.
    const odd = "_|WARNING:-DO-NOT-SHARE-THIS.--Sharing-this-is-bad.|TRUNCATED";
    const parsed = parseImportLine(`alt_one:hunter2:${odd}`);
    expect(parsed.kind).toBe("cookie");
    expect(parsed.cookie).toBe(odd);
    expect(parsed.username).toBe("alt_one");
  });
});

/**
 * Credencial incompleta não entra pela metade: sem cookie não há sessão, e
 * `user:pass` sozinho é trabalho do login pelo navegador, não do import de
 * cookie.
 */
describe("parseImportLine — incomplete lines", () => {
  it("marks username:password without a cookie as userpass", () => {
    expect(parseImportLine("alt_one:hunter2")).toEqual({
      kind: "userpass",
      username: "alt_one",
      password: "hunter2",
      cookie: "",
    });
  });

  it("marks an empty line as empty", () => {
    expect(parseImportLine("")).toEqual({ kind: "empty", username: "", password: "", cookie: "" });
    expect(parseImportLine("   ").kind).toBe("empty");
  });

  it("does not turn a half-typed username:password into a credential", () => {
    expect(parseImportLine("alt_one:").kind).toBe("unknown");
    expect(parseImportLine("alt_one:").username).toBe("");
    expect(parseImportLine(":hunter2").kind).toBe("unknown");
    expect(parseImportLine(":hunter2").username).toBe("");
  });

  /**
   * `unknown` guarda a linha inteira em `cookie`: o import por cookie continua
   * mandando-a para o backend validar (era o comportamento antes do parser),
   * e o import por senha a recusa com a mensagem certa.
   */
  it("falls back to unknown, with the whole line as the cookie", () => {
    expect(parseImportLine("some-opaque-token")).toEqual({
      kind: "unknown",
      username: "",
      password: "",
      cookie: "some-opaque-token",
    });
  });
});
