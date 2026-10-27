import { describe, expect, it } from "vitest";
import {
  MIN_ROBLOX_PASSWORD,
  looksLikeEmail,
  planBulkCredentials,
  resolveCurrentPassword,
  runnableCount,
  validateBulkRequest,
  type BulkCredentialsRequest,
} from "./bulkCredentials";

const base: BulkCredentialsRequest = {
  changePassword: false,
  newPassword: "",
  changeEmail: false,
  newEmail: "",
  sharedCurrentPassword: "",
  randomPassword: false,
};

describe("validateBulkRequest", () => {
  it("refuses when neither password nor email is chosen", () => {
    expect(validateBulkRequest(base)).toEqual({ kind: "nothing-selected" });
  });

  it("refuses a password shorter than Roblox allows", () => {
    expect(validateBulkRequest({ ...base, changePassword: true, newPassword: "short1" })).toEqual({
      kind: "weak-password",
      min: MIN_ROBLOX_PASSWORD,
    });
    expect(validateBulkRequest({ ...base, changePassword: true, newPassword: "longenough1" })).toBeNull();
  });

  it("a random password per account needs no typed password", () => {
    expect(validateBulkRequest({ ...base, changePassword: true, randomPassword: true })).toBeNull();
    // Desligado, a senha digitada continua obrigatória.
    expect(validateBulkRequest({ ...base, changePassword: true, randomPassword: false })).toEqual({
      kind: "weak-password",
      min: MIN_ROBLOX_PASSWORD,
    });
  });

  it("refuses an obviously bad email", () => {
    expect(validateBulkRequest({ ...base, changeEmail: true, newEmail: "not-an-email" })).toEqual({ kind: "bad-email" });
    expect(validateBulkRequest({ ...base, changeEmail: true, newEmail: "me@example.com" })).toBeNull();
  });
});

describe("resolveCurrentPassword", () => {
  it("prefers the account's stored password", () => {
    expect(resolveCurrentPassword({ Password: "stored" }, "shared")).toBe("stored");
  });

  it("falls back to the shared password when nothing is stored", () => {
    expect(resolveCurrentPassword({ Password: "" }, "shared")).toBe("shared");
    expect(resolveCurrentPassword({ Password: "   " }, "shared")).toBe("shared");
  });

  it("returns null when neither is available (account is skipped)", () => {
    expect(resolveCurrentPassword({ Password: "" }, "")).toBeNull();
  });
});

describe("planBulkCredentials", () => {
  const accounts = [
    { UserID: 1, Password: "pw1" },
    { UserID: 2, Password: "" },
    { UserID: 3, Password: "pw3" },
  ];

  it("skips accounts with no current password and keeps the order", () => {
    const plan = planBulkCredentials(accounts, { ...base, changePassword: true, newPassword: "newpass12" });
    expect(plan.map((s) => s.userId)).toEqual([1, 2, 3]);
    expect(plan[1].skip).toBe("no-current-password");
    expect(plan[1].currentPassword).toBeNull();
    expect(runnableCount(plan)).toBe(2);
  });

  it("runs the account with no stored password once a shared one is given", () => {
    const plan = planBulkCredentials(accounts, {
      ...base,
      changePassword: true,
      newPassword: "newpass12",
      sharedCurrentPassword: "common",
    });
    expect(plan[1].skip).toBeNull();
    expect(plan[1].currentPassword).toBe("common");
    expect(plan[0].currentPassword).toBe("pw1"); // stored still wins
    expect(runnableCount(plan)).toBe(3);
  });
});

describe("looksLikeEmail", () => {
  it("accepts a normal address and rejects junk", () => {
    expect(looksLikeEmail(" me@example.com ")).toBe(true);
    expect(looksLikeEmail("me@@x")).toBe(false);
    expect(looksLikeEmail("nope")).toBe(false);
  });
});
