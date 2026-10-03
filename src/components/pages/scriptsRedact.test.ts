import { describe, expect, it, vi } from "vitest";

// ScriptsPage pulls in the global store, which talks to Tauri at import time.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => null),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import { isSecretSettingKey, redactSecretSettings } from "./ScriptsPage";

describe("isSecretSettingKey", () => {
  it.each(["Password", "password", "ApiKey", "apikey", "Api_Key", "Secret", "Token", "AuthToken"])(
    "treats %s as a secret",
    (key) => {
      expect(isSecretSettingKey(key)).toBe(true);
    }
  );

  it.each([
    "AllowGetCookie",
    "AllowToken",
    "EveryRequestRequiresPassword",
    "AutoCookieRefresh",
    "AutoApiKey",
    "RequireApiKey",
  ])("keeps flag-style key %s", (key) => {
    expect(isSecretSettingKey(key)).toBe(false);
  });

  it.each(["Enabled", "Port", "PasswordHint", "TokenCount", "Theme"])(
    "keeps ordinary key %s",
    (key) => {
      expect(isSecretSettingKey(key)).toBe(false);
    }
  );
});

describe("redactSecretSettings", () => {
  it("never hands WebServer.Password or BloxGen.ApiKey to a script", () => {
    const settings = {
      WebServer: {
        Enabled: "true",
        Port: "7963",
        Password: "hunter2",
        EveryRequestRequiresPassword: "true",
        AllowGetCookie: "false",
      },
      BloxGen: {
        ApiKey: "sk-live-secret",
        AutoCookieRefresh: "true",
      },
    };

    const out = redactSecretSettings(settings);

    expect(out.WebServer).not.toHaveProperty("Password");
    expect(out.BloxGen).not.toHaveProperty("ApiKey");
    expect(JSON.stringify(out)).not.toContain("hunter2");
    expect(JSON.stringify(out)).not.toContain("sk-live-secret");
  });

  it("keeps allow/every/auto/require flags", () => {
    const out = redactSecretSettings({
      WebServer: {
        EveryRequestRequiresPassword: "true",
        AllowGetCookie: "true",
        AllowToken: "true",
        AutoCookieRefresh: "true",
        RequireApiKey: "false",
      },
    });

    expect(out.WebServer).toEqual({
      EveryRequestRequiresPassword: "true",
      AllowGetCookie: "true",
      AllowToken: "true",
      AutoCookieRefresh: "true",
      RequireApiKey: "false",
    });
  });

  it("keeps non-secret keys in a redacted section", () => {
    const out = redactSecretSettings({
      General: { Theme: "dark", SessionToken: "abc", PasswordHint: "mother" },
    });
    expect(out.General).toEqual({ Theme: "dark", PasswordHint: "mother" });
  });

  it("leaves non-object sections untouched", () => {
    const out = redactSecretSettings({
      Version: "4.0.0",
      Count: 3,
      Flag: true,
      Missing: null,
      Undef: undefined,
    });
    expect(out).toEqual({
      Version: "4.0.0",
      Count: 3,
      Flag: true,
      Missing: null,
      Undef: undefined,
    });
  });

  it("does not mutate the input object", () => {
    const settings = { WebServer: { Password: "hunter2", Port: "7963" } };
    const out = redactSecretSettings(settings);
    expect(settings.WebServer.Password).toBe("hunter2");
    expect(out).not.toBe(settings);
    expect(out.WebServer).not.toBe(settings.WebServer);
  });

  it.each([null, undefined, "string", 42, true])("passes through non-object input %s", (input) => {
    expect(redactSecretSettings(input)).toBe(input);
  });

  it("returns an empty object for empty settings", () => {
    expect(redactSecretSettings({})).toEqual({});
  });
});
