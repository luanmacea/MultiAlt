import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect } from "react";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { NotificationsTab } from "./NotificationsTab";
import { useSettings } from "../../hooks/useSettings";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

const VALID = "https://discord.com/api/webhooks/123456789012345678/AbCdEf_ghIJ-klmnopQRSTuvwxyz0123456789";

/** O que o backend grava num RAMSettings.ini novo (store.rs). */
function defaults(): Record<string, Record<string, string>> {
  return {
    Webhook: {
      Enabled: "false",
      NotifyDropped: "true",
      NotifyReconnected: "false",
      NotifyReconnectFailed: "true",
      NotifyNotResponding: "true",
      NotifyLaunchFailed: "false",
      HideNames: "true",
    },
  };
}

let stored: Record<string, Record<string, string>> = {};
let testReply: unknown = undefined;

function Harness() {
  const s = useSettings();
  useEffect(() => {
    void s.load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  if (!s.loaded) return <div>Loading settings...</div>;
  return <NotificationsTab s={s} />;
}

function updates() {
  return invokeMock.mock.calls.filter((c) => c[0] === "update_setting").map((c) => c[1]);
}

beforeEach(() => {
  resetTauriMocks();
  stored = defaults();
  testReply = undefined;
  setInvokeHandler((cmd) => {
    if (cmd === "get_all_settings") return stored;
    if (cmd === "send_test_webhook") {
      if (testReply instanceof Error) throw testReply.message;
      return testReply;
    }
    return undefined;
  });
});
afterEach(cleanup);

describe("Settings › Notifications (Discord webhook)", () => {
  it("is off by default, says the data leaves the PC, and has the documented event defaults", async () => {
    render(<Harness />);
    const main = await screen.findByRole("switch", { name: "Send notifications to Discord" });
    expect(main).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText(/leaves your PC/)).toBeInTheDocument();
    const on = (name: string) => screen.getByRole("switch", { name }).getAttribute("aria-checked");
    expect(on("An account dropped or its client closed unexpectedly")).toBe("true");
    expect(on("Auto-reconnect gave up")).toBe("true");
    expect(on("A client stopped responding")).toBe("true");
    expect(on("An account reconnected")).toBe("false");
    expect(on("A launch failed")).toBe("false");
    expect(on("Hide account names in messages")).toBe("true");
  });

  it("an invalid URL is refused next to the field and never saved", async () => {
    render(<Harness />);
    const field = await screen.findByLabelText("Webhook URL");
    await userEvent.type(field, "https://evil.example/api/webhooks/1/2");
    await userEvent.tab();
    expect(await screen.findByRole("alert")).toHaveTextContent("That is not a Discord webhook URL.");
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(updates().some((u) => (u as { key: string }).key === "Url")).toBe(false);
  });

  it("a valid URL is saved when the field loses focus", async () => {
    render(<Harness />);
    const field = await screen.findByLabelText("Webhook URL");
    await userEvent.type(field, VALID);
    await userEvent.tab();
    await waitFor(() => expect(updates()).toContainEqual({ section: "Webhook", key: "Url", value: VALID }));
  });

  it("the test message goes through the backend with the typed URL and shows the result", async () => {
    render(<Harness />);
    const field = await screen.findByLabelText("Webhook URL");
    await userEvent.type(field, VALID);
    await userEvent.click(screen.getByRole("button", { name: "Send test message" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("send_test_webhook", { url: VALID, mention: "" })
    );
    expect(await screen.findByRole("status")).toHaveTextContent("Test message sent.");
  });

  it("a failed test shows the backend's message", async () => {
    testReply = new Error("Discord answered 404: the webhook was deleted or the URL is wrong.");
    stored.Webhook.Url = VALID;
    render(<Harness />);
    await userEvent.click(await screen.findByRole("button", { name: "Send test message" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Discord answered 404");
  });

  it("the test button without a URL asks for one and calls nothing", async () => {
    render(<Harness />);
    await userEvent.click(await screen.findByRole("button", { name: "Send test message" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Paste the webhook URL first.");
    expect(invokeMock).not.toHaveBeenCalledWith("send_test_webhook", expect.anything());
  });

  it("each event box saves its own key", async () => {
    render(<Harness />);
    await userEvent.click(await screen.findByRole("switch", { name: "A launch failed" }));
    await waitFor(() =>
      expect(updates()).toContainEqual({ section: "Webhook", key: "NotifyLaunchFailed", value: "true" })
    );
  });
});
