import { describe, expect, it } from "vitest";
import { acceptFailureText, canAcceptByApi, pendingTitles, termsLabel, termsStateOf } from "./terms";

const t = (text: string, options?: Record<string, unknown>) =>
  text.replace(/\{\{(\w+)\}\}/g, (_, name) => String(options?.[name] ?? ""));

describe("termsStateOf", () => {
  it("reads pending, none and error from the backend entry", () => {
    expect(termsStateOf({ status: { pending: [{ id: "a", title: "Terms of Use" }] }, error: null })).toEqual({
      kind: "pending",
      pending: [{ id: "a", title: "Terms of Use" }],
    });
    expect(termsStateOf({ status: { pending: [] }, error: null })).toEqual({ kind: "none" });
    expect(termsStateOf({ status: null, error: "boom" })).toEqual({ kind: "error", message: "boom" });
  });
});

describe("termsLabel", () => {
  it("never calls a failed check a clean account", () => {
    expect(termsLabel(undefined, t)).toBe("Terms not checked yet");
    expect(termsLabel({ kind: "none" }, t)).toBe("No pending terms");
    expect(termsLabel({ kind: "error", message: "429" }, t)).toBe("Couldn't check terms: 429");
  });

  it("joins the pending titles without repeating one", () => {
    const state = {
      kind: "pending" as const,
      pending: [
        { id: "1", title: "Terms of Use" },
        { id: "2", title: "Privacy Policy" },
        { id: "3", title: "Terms of Use" },
      ],
    };
    expect(pendingTitles(state)).toBe("Terms of Use, Privacy Policy");
    expect(termsLabel(state, t)).toBe("Terms pending: Terms of Use, Privacy Policy");
  });
});

describe("canAcceptByApi", () => {
  it("needs every pending agreement to have an id", () => {
    expect(canAcceptByApi({ kind: "pending", pending: [{ id: "a", title: "x" }] })).toBe(true);
    expect(canAcceptByApi({ kind: "pending", pending: [{ id: "a", title: "x" }, { id: " ", title: "y" }] })).toBe(false);
    expect(canAcceptByApi({ kind: "none" })).toBe(false);
    expect(canAcceptByApi(undefined)).toBe(false);
  });
});

describe("acceptFailureText", () => {
  const attempt = (status: number, body: string) => ({ shape: "objects", status, body });

  it("quotes exactly what Roblox answered", () => {
    expect(acceptFailureText({ accepted: false, remaining: [], attempts: [attempt(403, '{"message":"no"}')] }, t)).toBe(
      'Roblox answered 403: {"message":"no"}. Use Open in browser to accept it yourself.'
    );
  });

  it("does not trust a 2xx that left agreements pending", () => {
    const text = acceptFailureText(
      { accepted: false, remaining: [{ id: "a", title: "x" }], attempts: [attempt(200, "{}")] },
      t
    );
    expect(text).toContain("Roblox answered 200: {}");
    expect(text).toContain("1 agreement(s) are still pending");
  });

  it("says so when the confirmation read failed", () => {
    const text = acceptFailureText({ accepted: false, remaining: null, attempts: [attempt(204, "")] }, t);
    expect(text).toContain("Roblox answered 204: (empty)");
    expect(text).toContain("couldn't be checked afterwards");
  });

  it("handles an attempt list that never got an answer", () => {
    expect(acceptFailureText({ accepted: false, remaining: null, attempts: [] }, t)).toBe(
      "Roblox did not answer. Use Open in browser."
    );
  });
});
