import { act, cleanup, renderHook, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createElement, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Account } from "../types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
  isTauri: () => false,
  convertFileSrc: (p: string) => p,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

import { StoreProvider, useStore } from "../store";
import { PromptProvider } from "./usePrompt";
import { useJoinOnlineWarning } from "./useJoinOnlineWarning";

function account(userId: number, overrides: Partial<Account> = {}): Account {
  return {
    Valid: true,
    SecurityToken: "t",
    Username: `user${userId}`,
    LastUse: new Date().toISOString(),
    Alias: "",
    Description: "",
    Password: "",
    Group: "",
    UserID: userId,
    Fields: {},
    LastAttemptedRefresh: new Date().toISOString(),
    BrowserTrackerID: "",
    ...overrides,
  };
}

let accountsData: Account[] = [];
let settingsData: Record<string, Record<string, string>> = {};
let presenceRows: Array<Record<string, number>> = [];
let presenceFails = false;

function invokeCalls(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

async function renderWarning() {
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(StoreProvider, null, createElement(PromptProvider, null, children));
  const view = renderHook(
    () => ({ confirmJoin: useJoinOnlineWarning(), store: useStore() }),
    { wrapper }
  );
  await waitFor(() => expect(view.result.current.store.initialized).toBe(true));
  return view;
}

beforeEach(() => {
  invokeMock.mockReset();
  accountsData = [account(1, { Alias: "Main" }), account(2), account(3), account(4), account(5)];
  settingsData = {};
  presenceRows = [];
  presenceFails = false;
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "needs_password":
        return false;
      case "get_accounts":
        return accountsData;
      case "get_all_settings":
        return settingsData;
      case "get_presence":
        if (presenceFails) throw new Error("presence down");
        return presenceRows;
      case "is_accounts_encrypted":
        return false;
      case "batched_get_avatar_headshots":
        return [];
      case "get_running_instances":
        return [];
      case "get_theme":
        return {};
      default:
        return null;
    }
  });
});

afterEach(() => {
  cleanup();
});

describe("useJoinOnlineWarning", () => {
  it("allows an empty selection without asking anything", async () => {
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    await act(async () => {
      allowed = await result.current.confirmJoin([]);
    });

    expect(allowed).toBe(true);
    expect(invokeCalls("get_presence")).toHaveLength(0);
  });

  it("allows the join when the warning is disabled in settings", async () => {
    settingsData = { General: { WarnOnOnlineJoin: "false" } };
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    await act(async () => {
      allowed = await result.current.confirmJoin([1]);
    });

    expect(allowed).toBe(true);
    expect(invokeCalls("get_presence")).toHaveLength(0);
  });

  it("allows the join when presence cannot be fetched", async () => {
    presenceFails = true;
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    await act(async () => {
      allowed = await result.current.confirmJoin([1]);
    });

    expect(allowed).toBe(true);
  });

  it("allows the join when every account is offline", async () => {
    presenceRows = [{ userId: 1, userPresenceType: 0 }];
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    await act(async () => {
      allowed = await result.current.confirmJoin([1, 1]);
    });

    expect(allowed).toBe(true);
    expect(invokeCalls("get_presence")[0][1]).toEqual({ userIds: [1] });
    expect(document.querySelector(".fixed.inset-0")).toBeNull();
  });

  it("warns with the account alias and its presence label, and joins anyway", async () => {
    const user = userEvent.setup();
    presenceRows = [{ userId: 1, userPresenceType: 2 }];
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    act(() => {
      void result.current.confirmJoin([1]).then((v) => {
        allowed = v;
      });
    });

    await waitFor(() => expect(screen.getByText(/Main is currently In Game/)).toBeTruthy());
    await user.click(screen.getByText("Join Anyway"));

    await waitFor(() => expect(allowed).toBe(true));
    expect(invokeCalls("update_setting")).toHaveLength(0);
  });

  it("cancels the join when the user backs out", async () => {
    const user = userEvent.setup();
    presenceRows = [{ user_id: 2, user_presence_type: 1 }];
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    act(() => {
      void result.current.confirmJoin([2]).then((v) => {
        allowed = v;
      });
    });

    await waitFor(() => expect(screen.getByText(/user2 is currently Online/)).toBeTruthy());
    await user.click(screen.getByText("Cancel"));

    await waitFor(() => expect(allowed).toBe(false));
  });

  it("summarizes several online accounts and truncates after four", async () => {
    const user = userEvent.setup();
    presenceRows = [1, 2, 3, 4, 5].map((id) => ({ userId: id, userPresenceType: 3 }));
    const { result } = await renderWarning();

    act(() => {
      void result.current.confirmJoin([1, 2, 3, 4, 5]);
    });

    await waitFor(() => expect(screen.getByText(/5 selected accounts are already online/)).toBeTruthy());
    expect(screen.getByText(/Main \(In Studio\)/)).toBeTruthy();
    expect(screen.getByText(/and 1 more/)).toBeTruthy();

    await user.click(screen.getByText("Cancel"));
  });

  it("names unknown user ids generically", async () => {
    const user = userEvent.setup();
    presenceRows = [{ userId: 999, userPresenceType: 1 }];
    const { result } = await renderWarning();

    act(() => {
      void result.current.confirmJoin([999]);
    });

    await waitFor(() => expect(screen.getByText(/User 999 is currently Online/)).toBeTruthy());
    await user.click(screen.getByText("Cancel"));
  });

  it("persists the opt-out when the user ticks the checkbox", async () => {
    const user = userEvent.setup();
    presenceRows = [{ userId: 1, userPresenceType: 1 }];
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    act(() => {
      void result.current.confirmJoin([1]).then((v) => {
        allowed = v;
      });
    });

    await waitFor(() => expect(screen.getByText("Don't show this warning again")).toBeTruthy());
    await user.click(screen.getByRole("checkbox"));
    await user.click(screen.getByText("Join Anyway"));

    await waitFor(() => expect(allowed).toBe(true));
    expect(invokeCalls("update_setting").map((c) => c[1])).toContainEqual({
      section: "General",
      key: "WarnOnOnlineJoin",
      value: "false",
    });
    await waitFor(() =>
      expect(result.current.store.toasts.join(" ")).toContain("Online-join warning disabled")
    );
  });

  it("does not persist the opt-out when the join is cancelled", async () => {
    const user = userEvent.setup();
    presenceRows = [{ userId: 1, userPresenceType: 1 }];
    const { result } = await renderWarning();

    let allowed: boolean | undefined;
    act(() => {
      void result.current.confirmJoin([1]).then((v) => {
        allowed = v;
      });
    });

    await waitFor(() => expect(screen.getByRole("checkbox")).toBeTruthy());
    await user.click(screen.getByRole("checkbox"));
    await user.click(screen.getByText("Cancel"));

    await waitFor(() => expect(allowed).toBe(false));
    expect(invokeCalls("update_setting")).toHaveLength(0);
  });
});
