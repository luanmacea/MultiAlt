import { describe, expect, it } from "vitest";
import { credentialErrorText, isRetryLater, parseCredentialError } from "./credentialErrors";

/** Tradução de mentira: devolve a chave com as variáveis aplicadas. */
const t = (key: string, options?: Record<string, unknown>) =>
  key.replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(options?.[name] ?? ""));

describe("credential errors", () => {
  it("reads the kind, the status and the Roblox text", () => {
    expect(parseCredentialError("credential-error:wrong-current-password:403:Incorrect password")).toEqual({
      kind: "wrong-current-password",
      status: 403,
      detail: "Incorrect password",
    });
    expect(parseCredentialError("something else")).toBeNull();
  });

  it("explains each kind in plain words, with the Roblox text when there is one", () => {
    expect(credentialErrorText("credential-error:rate-limited:429:", t)).toMatch(/too many changes in a row/);
    expect(credentialErrorText("credential-error:verification-captcha:403:", t)).toMatch(/CAPTCHA/);
    expect(credentialErrorText("credential-error:wrong-current-password:403:Incorrect password", t)).toBe(
      'Roblox says the current password is wrong. Check the password saved for this account. (Roblox: "Incorrect password")'
    );
    expect(credentialErrorText("credential-error:unknown:500:", t)).toBe("Roblox refused the change (error 500).");
  });

  it("an error that is not about credentials stays as it came", () => {
    expect(credentialErrorText("Account 9 not found", t)).toBe("Account 9 not found");
  });

  it("only limits and verification checks are worth retrying later", () => {
    expect(isRetryLater("credential-error:rate-limited:429:")).toBe(true);
    expect(isRetryLater("credential-error:verification-2sv:403:")).toBe(true);
    expect(isRetryLater("credential-error:wrong-current-password:403:")).toBe(false);
  });
});
