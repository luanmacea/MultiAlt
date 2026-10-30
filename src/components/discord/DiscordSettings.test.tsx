import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect } from "react";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { DiscordSettings } from "./DiscordSettings";
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
      MaskNames: "false",
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
  return <DiscordSettings s={s} />;
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
    if (cmd === "send_test_webhook" || cmd === "send_discord_status_now" || cmd === "send_discord_summary_now") {
      if (testReply instanceof Error) throw testReply.message;
      return testReply;
    }
    return undefined;
  });
});
afterEach(cleanup);

describe("Discord alerts page (webhook)", () => {
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
    // Nomes à mostra por padrão (15/10/2026): o canal é do próprio usuário.
    expect(on("Hide account names in messages")).toBe("false");
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

  it("the new events have the backend's defaults", async () => {
    render(<Harness />);
    const on = (name: string) => screen.getByRole("switch", { name }).getAttribute("aria-checked");
    await screen.findByRole("switch", { name: "An account was banned" });
    expect(on("An account was banned")).toBe("true");
    expect(on("A client was closed for memory")).toBe("true");
    expect(on("A launch batch finished")).toBe("false");
    expect(on("Windows is almost out of memory")).toBe("false");
    expect(on("A message from a Python script")).toBe("true");
  });

  /** Pedido do dono (27/10/2026): cada evento pode chamar alguém diferente. */
  it("each event has its own mention box, saved under its own key", async () => {
    render(<Harness />);
    const box = await screen.findByLabelText("Mention for this event: An account was banned");
    await userEvent.type(box, "@everyone");
    await userEvent.tab();
    await waitFor(() =>
      expect(updates()).toContainEqual({ section: "Webhook", key: "MentionModerated", value: "@everyone" })
    );
  });

  it("quiet hours save the times in 24-hour format", async () => {
    render(<Harness />);
    await userEvent.click(await screen.findByRole("switch", { name: "Quiet hours" }));
    await waitFor(() => expect(updates()).toContainEqual({ section: "Webhook", key: "QuietHours", value: "true" }));
    const start = screen.getByLabelText("Quiet hours start");
    expect(start).toHaveValue("23:00");
    fireEvent.change(start, { target: { value: "22:30" } });
    await waitFor(() => expect(updates()).toContainEqual({ section: "Webhook", key: "QuietStart", value: "22:30" }));
  });

  it("the live status and the summary are off by default and can be sent right away", async () => {
    render(<Harness />);
    const status = await screen.findByRole("switch", { name: "Keep a live status message" });
    expect(status).toHaveAttribute("aria-checked", "false");
    expect(screen.getByRole("switch", { name: "Send a daily summary" })).toHaveAttribute("aria-checked", "false");
    await userEvent.click(screen.getByRole("button", { name: "Update now" }));
    await waitFor(() => expect(invokeMock.mock.calls.some((c) => c[0] === "send_discord_status_now")).toBe(true));
    expect(await screen.findByText("Status sent. Check the Discord channel.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Send a preview" }));
    await waitFor(() => expect(invokeMock.mock.calls.some((c) => c[0] === "send_discord_summary_now")).toBe(true));
  });

  it("a failed status shows the backend's message", async () => {
    testReply = new Error("Paste the webhook URL first.");
    render(<Harness />);
    await userEvent.click(await screen.findByRole("button", { name: "Update now" }));
    expect(await screen.findByText("Paste the webhook URL first.")).toBeInTheDocument();
  });
});
