import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { ServersTab, type ServersTabProps } from "./ServersTab";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import type { ServerData } from "./types";

function makeServer(overrides: Partial<ServerData> = {}): ServerData {
  return {
    id: "job-aaa",
    maxPlayers: 20,
    playing: 10,
    playerTokens: [],
    fps: 59.6,
    ping: 42,
    name: null,
    vipServerId: null,
    accessCode: null,
    ...overrides,
  };
}

function renderTab(props: Partial<ServersTabProps> = {}) {
  const onJoinServer = vi.fn();
  const setPlaceId = vi.fn();
  const addToast = vi.fn();
  render(
    <ServersTab
      placeId="606849621"
      setPlaceId={setPlaceId}
      onJoinServer={onJoinServer}
      addToast={addToast}
      userId={1001}
      refreshOnOpenSignal={0}
      {...props}
    />
  );
  return { onJoinServer, setPlaceId, addToast };
}

const jobInput = () => screen.getByPlaceholderText("Job ID or private server link (optional)");

beforeEach(resetTauriMocks);
afterEach(cleanup);

describe("ServersTab — manual join", () => {
  it("joins the typed job id", async () => {
    const { onJoinServer } = renderTab();
    await userEvent.type(jobInput(), "  job-typed  ");
    await userEvent.click(screen.getByRole("button", { name: "Join" }));
    expect(onJoinServer).toHaveBeenCalledWith("job-typed");
  });

  it("joins on Enter from the Job ID field", async () => {
    const { onJoinServer } = renderTab();
    await userEvent.type(jobInput(), "job-typed{Enter}");
    expect(onJoinServer).toHaveBeenCalledWith("job-typed");
  });

  it("prefills the Job ID field from the parent", () => {
    renderTab({ prefillJobId: "vip:abc", prefillNonce: 1 });
    expect(jobInput()).toHaveValue("vip:abc");
  });
});

describe("ServersTab — place id field", () => {
  it("keeps typed digits", async () => {
    const { setPlaceId } = renderTab({ placeId: "" });
    await userEvent.type(screen.getByPlaceholderText("Enter Place ID"), "606");
    expect(setPlaceId).toHaveBeenCalledWith("6");
    expect(setPlaceId).not.toHaveBeenCalledWith("");
  });

  /** Mesma armadilha da outra aba: colar a URL juntava os dígitos todos. */
  it("fills the Place ID from a pasted game link", async () => {
    const { setPlaceId } = renderTab({ placeId: "" });
    fireEvent.change(screen.getByPlaceholderText("Enter Place ID"), {
      target: {
        value: "https://www.roblox.com/games/606849621/Jailbreak?privateServerLinkCode=98765",
      },
    });
    expect(setPlaceId).toHaveBeenLastCalledWith("606849621");
  });

  it("refuses an invite link and points at the Follow tab", async () => {
    const { setPlaceId, addToast } = renderTab({ placeId: "" });
    fireEvent.change(screen.getByPlaceholderText("Enter Place ID"), {
      target: { value: "https://www.roblox.com/share?code=abc123&type=Server" },
    });
    expect(setPlaceId).not.toHaveBeenCalled();
    expect(addToast).toHaveBeenCalledWith(expect.stringContaining("Follow"));
  });

  it("refuses to refresh without a valid Place ID", async () => {
    const { addToast } = renderTab({ placeId: "" });
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(addToast).toHaveBeenCalledWith("Enter a valid Place ID");
    expect(invokeMock).not.toHaveBeenCalledWith("get_servers", expect.anything());
  });
});

describe("ServersTab — server list", () => {
  it("shows the empty state before any refresh", () => {
    renderTab();
    expect(screen.getByText("Enter a Place ID and click Refresh")).toBeInTheDocument();
    expect(screen.getByText("0 servers")).toBeInTheDocument();
  });

  it("loads the place name and the servers", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "get_place_details") return [{ name: "Tower Defense" }];
      if (cmd === "get_servers")
        return { data: [makeServer(), makeServer({ id: "job-bbb", playing: 3 })], nextPageCursor: null };
      return undefined;
    });
    renderTab();
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));

    expect(await screen.findByText("Tower Defense")).toBeInTheDocument();
    expect(await screen.findByText("13 players", { exact: false })).toBeInTheDocument();
    expect(screen.getAllByText("42ms")).toHaveLength(2);
    expect(screen.getAllByText("60")).toHaveLength(2);
  });

  it("reports a server-load failure as a toast", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "get_place_details") return [];
      throw new Error("rate limited");
    });
    const { addToast } = renderTab();
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() =>
      expect(addToast).toHaveBeenCalledWith(expect.stringContaining("rate limited"))
    );
  });

  it("auto-refreshes when the dialog signals it opened", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "get_place_details") return [{ name: "Auto" }];
      if (cmd === "get_servers") return { data: [makeServer()], nextPageCursor: null };
      return undefined;
    });
    renderTab({ refreshOnOpenSignal: 1 });
    expect(await screen.findByText("Auto")).toBeInTheDocument();
  });
});

describe("ServersTab — joining a listed server", () => {
  async function renderWithServer(server: ServerData) {
    setInvokeHandler((cmd) => {
      if (cmd === "get_place_details") return [{ name: "Game" }];
      if (cmd === "get_servers") return { data: [server], nextPageCursor: null };
      return undefined;
    });
    const handles = renderTab();
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await screen.findByText("Game");
    const row = document.querySelector("tbody tr") as HTMLElement;
    return { ...handles, row };
  }

  it("double-click joins a public server by its job id", async () => {
    const { onJoinServer, row } = await renderWithServer(makeServer({ id: "job-public" }));
    fireEvent.doubleClick(row);
    expect(onJoinServer).toHaveBeenCalledWith("job-public");
  });

  it("double-click joins a private server through its VIP access code", async () => {
    const { onJoinServer, row } = await renderWithServer(
      makeServer({ id: "job-private", accessCode: "code-9" })
    );
    fireEvent.doubleClick(row);
    expect(onJoinServer).toHaveBeenCalledWith("VIP:code-9");
  });

  it("offers Join and Copy Job ID from the row context menu", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const { onJoinServer, addToast, row } = await renderWithServer(makeServer({ id: "job-ctx" }));

    fireEvent.contextMenu(row, { clientX: 10, clientY: 10 });
    await userEvent.click(await screen.findByRole("button", { name: /Copy Job ID/i }));
    expect(writeText).toHaveBeenCalledWith("job-ctx");
    expect(addToast).toHaveBeenCalledWith("Copied Job ID");

    fireEvent.contextMenu(row, { clientX: 10, clientY: 10 });
    await userEvent.click(await screen.findByRole("button", { name: "Join Server" }));
    expect(onJoinServer).toHaveBeenCalledWith("job-ctx");
  });
});
