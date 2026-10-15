import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { PrivateServersSection, accountsWithAccess, privateServerKey } from "./PrivateServersSection";
import { ServersTab } from "./ServersTab";
import { makeAccount, renderWithStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import { clearGameIdentityCache } from "../../hooks/useGameIdentity";
import type { PrivateServer } from "../../types";

const ACCOUNT_A = makeAccount({ UserID: 1001, Username: "alpha" });
const ACCOUNT_B = makeAccount({ UserID: 1002, Username: "bravo" });

function vip(overrides: Partial<PrivateServer> = {}): PrivateServer {
  return {
    id: null,
    name: "Base",
    vipServerId: 91,
    accessCode: "aaaa-bbbb-cccc-dddd-eeee",
    maxPlayers: 12,
    playing: 2,
    owner: { id: 77, name: "owner77", displayName: "Owner Seven" },
    ...overrides,
  };
}

/** `get_private_servers` por conta; o valor `Error` vira falha daquela conta. */
function routePrivate(byAccount: Record<number, { data: PrivateServer[]; nextPageCursor?: string | null } | Error>) {
  setInvokeHandler((cmd, args) => {
    if (cmd === "get_private_servers") {
      const entry = byAccount[(args as { userId: number }).userId];
      if (entry instanceof Error) throw entry;
      return { nextPageCursor: null, ...(entry ?? { data: [] }) };
    }
    if (cmd === "start_server_scan") return 7;
    return undefined;
  });
}

function privateCalls() {
  return invokeMock.mock.calls.filter((call) => call[0] === "get_private_servers");
}

function renderSection(onJoin = vi.fn(), userIds = [1001, 1002], placeId = 606849621) {
  renderWithStore(<PrivateServersSection userIds={userIds} placeId={placeId} onJoin={onJoin} />, {
    accounts: [ACCOUNT_A, ACCOUNT_B],
  });
  return onJoin;
}

beforeEach(() => {
  resetTauriMocks();
  clearGameIdentityCache();
  localStorage.clear();
});

afterEach(cleanup);

describe("privateServerKey / accountsWithAccess", () => {
  it("identifies the same VIP seen by two accounts and keeps the selection order", () => {
    const entries = new Map([
      [1002, { servers: [vip()] }],
      [1001, { servers: [vip(), vip({ vipServerId: 92 })] }],
      [1003, { servers: [] }],
    ]);
    expect(privateServerKey(vip())).toBe("vip:91");
    expect(accountsWithAccess("vip:91", entries, [1001, 1002, 1003])).toEqual([1001, 1002]);
    expect(accountsWithAccess("vip:92", entries, [1001, 1002, 1003])).toEqual([1001]);
    expect(privateServerKey(vip({ vipServerId: null }))).toBe("code:aaaa-bbbb-cccc-dddd-eeee");
  });
});

describe("PrivateServersSection", () => {
  it("is closed by default and asks nothing until opened", async () => {
    routePrivate({ 1001: { data: [vip()] } });
    renderSection();
    await new Promise((r) => setTimeout(r, 500));
    expect(privateCalls()).toHaveLength(0);
    expect(screen.getByRole("button", { name: /Private servers/ })).toHaveAttribute("aria-expanded", "false");
  });

  it("lists each account's private servers with name, owner and players, one account at a time", async () => {
    routePrivate({ 1001: { data: [vip()] }, 1002: { data: [] } });
    renderSection();
    await userEvent.click(screen.getByRole("button", { name: /Private servers/ }));

    const first = await screen.findByTestId("private-servers-1001");
    expect(await within(first).findByText("Base")).toBeInTheDocument();
    expect(within(first).getByText("by Owner Seven")).toBeInTheDocument();
    expect(within(first).getByText("2/12")).toBeInTheDocument();
    expect(
      await within(screen.getByTestId("private-servers-1002")).findByText(
        "No private servers for this account in this game"
      )
    ).toBeInTheDocument();
    expect(privateCalls().map((call) => (call[1] as { userId: number }).userId)).toEqual([1001, 1002]);
    expect(privateCalls()[0][1]).toEqual({ userId: 1001, placeId: 606849621, cursor: null });
  });

  it("shows one account's error without hiding the others", async () => {
    routePrivate({ 1001: new Error("Failed to get private servers (status 401)"), 1002: { data: [vip()] } });
    renderSection();
    await userEvent.click(screen.getByRole("button", { name: /Private servers/ }));

    expect(
      await within(screen.getByTestId("private-servers-1001")).findByText("Error: Failed to get private servers (status 401)")
    ).toBeInTheDocument();
    expect(await within(screen.getByTestId("private-servers-1002")).findByText("Base")).toBeInTheDocument();
  });

  it("pages with Load more", async () => {
    let calls = 0;
    setInvokeHandler((cmd, args) => {
      if (cmd !== "get_private_servers") return undefined;
      calls++;
      const cursor = (args as { cursor: string | null }).cursor;
      return cursor === "page-2"
        ? { data: [vip({ vipServerId: 93, name: "Second page" })], nextPageCursor: null }
        : { data: [vip()], nextPageCursor: "page-2" };
    });
    renderSection(vi.fn(), [1001]);
    await userEvent.click(screen.getByRole("button", { name: /Private servers/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Load more" }));

    expect(await screen.findByText("Second page")).toBeInTheDocument();
    expect(screen.getByText("Base")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
    expect(calls).toBe(2);
  });

  it("joins with only the selected accounts that can see the server", async () => {
    routePrivate({ 1001: { data: [vip()] }, 1002: { data: [] } });
    const onJoin = renderSection();
    await userEvent.click(screen.getByRole("button", { name: /Private servers/ }));
    await within(screen.getByTestId("private-servers-1002")).findByText(
      "No private servers for this account in this game"
    );

    await userEvent.click(screen.getByRole("button", { name: "Join with 1" }));
    expect(onJoin).toHaveBeenCalledWith(expect.objectContaining({ accessCode: "aaaa-bbbb-cccc-dddd-eeee" }), [1001]);
  });

  it("names the owner and the account headers through accountName (the user's own accounts get the mask)", async () => {
    routePrivate({ 1001: { data: [vip()] } });
    renderWithStore(
      <PrivateServersSection
        userIds={[1001]}
        placeId={606849621}
        onJoin={vi.fn()}
        accountName={(id) => (id === 77 ? "My main" : id === 1001 ? "alpha" : null)}
      />,
      { accounts: [ACCOUNT_A] }
    );
    await userEvent.click(screen.getByRole("button", { name: /Private servers/ }));
    expect(await screen.findByText("by My main")).toBeInTheDocument();
    expect(within(screen.getByTestId("private-servers-1001")).getByText("alpha")).toBeInTheDocument();
  });

  it("a server without a join code cannot be joined", async () => {
    routePrivate({ 1001: { data: [vip({ accessCode: null })] } });
    renderSection(vi.fn(), [1001]);
    await userEvent.click(screen.getByRole("button", { name: /Private servers/ }));
    expect(await screen.findByRole("button", { name: "Join" })).toBeDisabled();
  });

  it("remembers that it was opened", async () => {
    routePrivate({ 1001: { data: [] } });
    renderSection(vi.fn(), [1001]);
    await userEvent.click(screen.getByRole("button", { name: /Private servers/ }));
    cleanup();
    renderSection(vi.fn(), [1001]);
    expect(screen.getByRole("button", { name: /Private servers/ })).toHaveAttribute("aria-expanded", "true");
  });
});

describe("Choose Game Servers tab — private servers", () => {
  it("joins through the existing VIP path (joinVip + access code) with the accounts that have access", async () => {
    routePrivate({ 1001: { data: [vip()] }, 1002: { data: [vip()] } });
    const launchAll = vi.fn(async () => ({ ok: true }));
    renderWithStore(
      <ServersTab userIds={[1001, 1002]} placeId="606849621" setPlaceId={vi.fn()} launchAll={launchAll} />,
      { accounts: [ACCOUNT_A, ACCOUNT_B], selectedIds: new Set([1001, 1002]), selectedAccounts: [ACCOUNT_A, ACCOUNT_B] }
    );
    await userEvent.click(screen.getByRole("button", { name: /Private servers/ }));
    await within(screen.getByTestId("private-servers-1002")).findByText("Base");
    await userEvent.click(within(screen.getByTestId("private-servers-1001")).getByRole("button", { name: "Join" }));

    await waitFor(() =>
      expect(launchAll).toHaveBeenCalledWith([1001, 1002], 606849621, "", undefined, {
        joinVip: true,
        linkCode: "aaaa-bbbb-cccc-dddd-eeee",
      })
    );
  });
});
