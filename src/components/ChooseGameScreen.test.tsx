import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../store", async () => (await import("../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../test-utils/tauriMocks")).tauriEventMock());

import { ChooseGameScreen } from "./ChooseGameScreen";
import { makeAccount, setStore } from "../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../test-utils/tauriMocks";
import type { JoinTarget } from "../types";
import type { StoreValue } from "../store";

function joinTarget(overrides: Partial<JoinTarget> = {}): JoinTarget {
  return {
    kind: "place",
    placeId: 606849621,
    jobId: "",
    accessCode: "",
    linkCode: "",
    launchData: "",
    inviterId: null,
    note: null,
    ...overrides,
  };
}

const ACCOUNT_A = makeAccount({ UserID: 1001, Username: "alpha" });
const ACCOUNT_B = makeAccount({ UserID: 1002, Username: "bravo" });

/** Renders the screen with `selected` accounts and opens the Follow tab. */
async function renderFollowTab(selected = [ACCOUNT_A, ACCOUNT_B]): Promise<StoreValue> {
  const store = setStore({
    accounts: [ACCOUNT_A, ACCOUNT_B],
    selectedIds: new Set(selected.map((a) => a.UserID)),
    selectedAccounts: selected,
  });
  render(<ChooseGameScreen />);
  await userEvent.click(screen.getByRole("button", { name: "Follow" }));
  return store;
}

function linkInput(): HTMLInputElement {
  return screen.getByPlaceholderText(/ExperienceInvite/) as HTMLInputElement;
}

function joinButton(): HTMLButtonElement {
  return screen.getByRole("button", { name: "Join" }) as HTMLButtonElement;
}

beforeEach(() => {
  resetTauriMocks();
  localStorage.clear();
});

afterEach(cleanup);

describe("ChooseGameScreen — JoinLinkSection", () => {
  it("resolves the pasted link with the first selected account and launches everyone", async () => {
    setInvokeHandler((cmd) => {
      if (cmd === "resolve_join_link") return joinTarget({ kind: "job", jobId: "job-1" });
      return undefined;
    });
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "https://www.roblox.com/games/606849621");
    await userEvent.click(joinButton());

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("resolve_join_link", {
        userId: 1001,
        link: "https://www.roblox.com/games/606849621",
      })
    );
    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith([1001, 1002], {
        placeId: "606849621",
        jobId: "job-1",
        launchData: undefined,
        joinVip: undefined,
        linkCode: undefined,
      })
    );
  });

  it("uses joinServer when exactly one account is selected", async () => {
    setInvokeHandler(() => joinTarget({ kind: "place" }));
    const store = await renderFollowTab([ACCOUNT_A]);

    await userEvent.type(linkInput(), "roblox://placeId=606849621");
    await userEvent.click(joinButton());

    await waitFor(() => expect(store.joinServer).toHaveBeenCalledTimes(1));
    expect(store.joinServer).toHaveBeenCalledWith(1001, expect.objectContaining({ placeId: "606849621" }));
    expect(store.launchMultiple).not.toHaveBeenCalled();
  });

  it("sends a private server as joinVip + linkCode and mirrors it into the Job ID field", async () => {
    setInvokeHandler(() =>
      joinTarget({ kind: "private", linkCode: "abc123", jobId: "ignored-job" })
    );
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "https://www.roblox.com/share?code=abc123&type=Server");
    await userEvent.click(joinButton());

    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith([1001, 1002], {
        placeId: "606849621",
        jobId: "",
        launchData: undefined,
        joinVip: true,
        linkCode: "abc123",
      })
    );
    expect(store.setJobId).toHaveBeenCalledWith("vip:abc123");
  });

  it("falls back to accessCode when the private link carries no linkCode", async () => {
    setInvokeHandler(() => joinTarget({ kind: "private", accessCode: "access-9" }));
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "vip-link");
    await userEvent.click(joinButton());

    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith(
        [1001, 1002],
        expect.objectContaining({ joinVip: true, linkCode: "access-9" })
      )
    );
  });

  it("forwards launchData from an invite", async () => {
    setInvokeHandler(() =>
      joinTarget({ kind: "invite", jobId: "job-7", launchData: "payload" })
    );
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "https://ro.blox.com/Ebh5");
    await userEvent.click(joinButton());

    await waitFor(() =>
      expect(store.launchMultiple).toHaveBeenCalledWith(
        [1001, 1002],
        expect.objectContaining({ jobId: "job-7", launchData: "payload" })
      )
    );
  });

  it.each([
    ["invite", { kind: "invite" as const, jobId: "job-1" }, "Invite · place 606849621 · server job-1"],
    ["private", { kind: "private" as const, linkCode: "abc" }, "Private server · place 606849621"],
    ["job", { kind: "job" as const, jobId: "job-2" }, "Server · place 606849621 · server job-2"],
    ["place", { kind: "place" as const }, "Game · place 606849621"],
  ])("renders the resolved summary for a %s link", async (_kind, overrides, expected) => {
    setInvokeHandler(() => joinTarget(overrides));
    await renderFollowTab();

    await userEvent.type(linkInput(), "some-link");
    await userEvent.click(joinButton());

    expect(await screen.findByText(expected)).toBeInTheDocument();
  });

  it("shows the expiry warning but still launches when the target carries a note", async () => {
    setInvokeHandler(() => joinTarget({ kind: "place", note: "Expired" }));
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "https://www.roblox.com/share?code=old");
    await userEvent.click(joinButton());

    expect(await screen.findByText(/no longer valid \(Expired\)/i)).toBeInTheDocument();
    await waitFor(() => expect(store.launchMultiple).toHaveBeenCalledTimes(1));
  });

  it("shows a backend error inline and keeps the typed link", async () => {
    setInvokeHandler(() => {
      throw new Error("link expired or invalid");
    });
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "https://bad.link/x");
    await userEvent.click(joinButton());

    expect(await screen.findByText(/link expired or invalid/i)).toBeInTheDocument();
    expect(linkInput()).toHaveValue("https://bad.link/x");
    expect(store.launchMultiple).not.toHaveBeenCalled();
    // Re-enabled after the failure so the user can retry.
    await waitFor(() => expect(joinButton()).toBeEnabled());
  });

  it("surfaces a launch failure inline without clearing the link", async () => {
    setInvokeHandler(() => joinTarget({ kind: "place" }));
    const store = await renderFollowTab();
    (store.launchMultiple as ReturnType<typeof vi.fn>).mockRejectedValue(new Error("multi roblox off"));

    await userEvent.type(linkInput(), "https://www.roblox.com/games/1");
    await userEvent.click(joinButton());

    expect(await screen.findByText(/multi roblox off/i)).toBeInTheDocument();
    expect(linkInput()).toHaveValue("https://www.roblox.com/games/1");
  });

  it("submits on Enter", async () => {
    setInvokeHandler(() => joinTarget({ kind: "place" }));
    const store = await renderFollowTab();

    await userEvent.type(linkInput(), "https://www.roblox.com/games/606849621{Enter}");

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("resolve_join_link", expect.anything()));
    await waitFor(() => expect(store.launchMultiple).toHaveBeenCalledTimes(1));
  });

  it("keeps Join disabled until a link is typed", async () => {
    await renderFollowTab();
    expect(joinButton()).toBeDisabled();

    await userEvent.type(linkInput(), "  ");
    expect(joinButton()).toBeDisabled();

    await userEvent.type(linkInput(), "link");
    expect(joinButton()).toBeEnabled();
  });

  it("keeps Join disabled while no account is selected", async () => {
    await renderFollowTab([]);
    await userEvent.type(linkInput(), "link");
    expect(joinButton()).toBeDisabled();
  });

  it("disables the input and the button while the link is resolving", async () => {
    let release: (value: JoinTarget) => void = () => {};
    setInvokeHandler(
      () => new Promise<JoinTarget>((resolve) => { release = resolve; })
    );
    await renderFollowTab();

    await userEvent.type(linkInput(), "link");
    await userEvent.click(joinButton());

    expect(await screen.findByRole("button", { name: "Resolving link..." })).toBeDisabled();
    expect(linkInput()).toBeDisabled();

    release(joinTarget());
    await waitFor(() => expect(joinButton()).toBeEnabled());
  });

  it("labels the section with the selected account count", async () => {
    await renderFollowTab();
    expect(screen.getAllByText("2 accounts").length).toBeGreaterThan(0);

    cleanup();
    await renderFollowTab([ACCOUNT_A]);
    expect(screen.getAllByText("1 account").length).toBeGreaterThan(0);
  });
});

describe("ChooseGameScreen — shell", () => {
  it("closes on Back and on Escape", async () => {
    const store = setStore({
      accounts: [ACCOUNT_A],
      selectedIds: new Set([1001]),
      selectedAccounts: [ACCOUNT_A],
    });
    render(<ChooseGameScreen />);

    await userEvent.click(screen.getByRole("button", { name: /Back/ }));
    expect(store.setChooseGameOpen).toHaveBeenCalledWith(false);

    await userEvent.keyboard("{Escape}");
    expect(store.setChooseGameOpen).toHaveBeenCalledTimes(2);
  });

  it("summarises how many accounts will launch", async () => {
    setStore({
      accounts: [ACCOUNT_A, ACCOUNT_B],
      selectedIds: new Set([1001, 1002]),
      selectedAccounts: [ACCOUNT_A, ACCOUNT_B],
    });
    render(<ChooseGameScreen />);
    expect(screen.getByText("2 accounts will be launched together")).toBeInTheDocument();
    expect(screen.getByText("alpha")).toBeInTheDocument();
    expect(screen.getByText("bravo")).toBeInTheDocument();
  });
});
