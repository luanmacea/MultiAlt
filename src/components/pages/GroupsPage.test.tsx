import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { GroupsPage } from "./GroupsPage";
import type { StoreValue } from "../../store";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import { emitTauriEvent, invokeMock, resetTauriMocks, setInvokeMap, type InvokeArgs } from "../../test-utils/tauriMocks";
import { walkTour } from "../../test-utils/tourHelpers";
import type { GroupJoinSnapshot, GroupSummary } from "./groups/shared";

const ACCOUNTS = [
  makeAccount({ UserID: 11, Username: "alpha" }),
  makeAccount({ UserID: 22, Username: "bravo" }),
  makeAccount({ UserID: 33, Username: "charlie" }),
];

function group(id: number, name: string, overrides: Partial<GroupSummary> = {}): GroupSummary {
  return {
    id,
    name,
    description: "",
    memberCount: 1200,
    publicEntryAllowed: true,
    hasVerifiedBadge: false,
    isLocked: false,
    ...overrides,
  };
}

const OPEN = group(501, "Builders Club", { hasVerifiedBadge: true });
const APPROVAL = group(502, "Secret Society", { publicEntryAllowed: false, memberCount: 40 });

const IDLE: GroupJoinSnapshot = {
  running: false,
  groupId: null,
  groupName: "",
  total: 0,
  done: 0,
  currentUserId: null,
  accounts: [],
};

function wire(overrides: Record<string, unknown | ((args: InvokeArgs) => unknown)> = {}) {
  setInvokeMap({
    get_groups_join_state: IDLE,
    groups_search: { groups: [OPEN, APPROVAL], nextCursor: null },
    groups_icons: [{ targetId: 501, imageUrl: "https://img/501.png" }],
    ...overrides,
  });
}

function renderPage(overrides: Partial<StoreValue> = {}) {
  const store = setStore({ accounts: ACCOUNTS, ...overrides });
  render(<GroupsPage active onLeave={() => {}} />);
  return store;
}

/** Digita e aperta Enter (busca na hora, sem esperar os 500 ms). */
async function search(text: string) {
  await userEvent.type(screen.getByPlaceholderText("Group name, link or ID"), `${text}{Enter}`);
}

function searchCalls() {
  return invokeMock.mock.calls.filter((call) => call[0] === "groups_search");
}

/** Promessa que o teste resolve na hora que quiser. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

beforeEach(() => {
  resetTauriMocks();
});

afterEach(cleanup);

describe("GroupsPage — search", () => {
  it("shows each group as a card with members, entry rule and verified badge", async () => {
    wire();
    renderPage();
    await search("builders");

    expect(invokeMock).toHaveBeenCalledWith("groups_search", { query: "builders", cursor: null });
    const list = await screen.findByRole("list", { name: "Groups found" });
    const cards = within(list).getAllByRole("button");
    expect(cards).toHaveLength(2);
    expect(within(cards[0]).getByText("Builders Club")).toBeInTheDocument();
    expect(within(cards[0]).getByText("Open to join")).toBeInTheDocument();
    expect(within(cards[0]).getByRole("img", { name: "Verified" })).toBeInTheDocument();
    expect(within(cards[1]).getByText("Approval required")).toBeInTheDocument();
    expect(within(cards[1]).getByText(/40 members/)).toBeInTheDocument();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("groups_icons", { groupIds: [501, 502] }));
  });

  it("shows the backend refusal instead of results", async () => {
    wire({
      groups_search: () => {
        throw "Search term not appropriate for Roblox.";
      },
    });
    renderPage();
    await search("xy");
    expect(await screen.findByRole("alert")).toHaveTextContent("Search term not appropriate for Roblox.");
  });

  it("has no Search button: typing searches by itself after a pause, once", async () => {
    wire();
    renderPage();
    expect(screen.queryByRole("button", { name: "Search" })).not.toBeInTheDocument();
    await userEvent.type(screen.getByPlaceholderText("Group name, link or ID"), "builders");
    // Ainda dentro dos 500 ms: nada saiu.
    expect(searchCalls()).toHaveLength(0);
    expect(await screen.findByRole("list", { name: "Groups found" }, { timeout: 2000 })).toBeInTheDocument();
    expect(searchCalls()).toEqual([["groups_search", { query: "builders", cursor: null }]]);
  });

  it("never lets an older answer overwrite a newer search", async () => {
    const first = deferred<unknown>();
    const second = deferred<unknown>();
    wire({
      groups_search: (args: InvokeArgs) => ((args as { query: string }).query === "pe" ? first.promise : second.promise),
    });
    renderPage();
    const input = screen.getByPlaceholderText("Group name, link or ID");
    await userEvent.type(input, "pe{Enter}");
    await userEvent.type(input, "t{Enter}");
    expect(searchCalls().map((call) => (call[1] as { query: string }).query)).toEqual(["pe", "pet"]);

    await act(async () => second.resolve({ groups: [group(601, "Pet Fans")], nextCursor: null }));
    expect(await screen.findByText("Pet Fans")).toBeInTheDocument();
    await act(async () => first.resolve({ groups: [group(602, "Peanut Club")], nextCursor: null }));
    expect(screen.queryByText("Peanut Club")).not.toBeInTheDocument();
    expect(screen.getByText("Pet Fans")).toBeInTheDocument();
  });

  it("asks for 2 characters before searching a word, but a bare ID searches", async () => {
    wire({ groups_search: { groups: [APPROVAL], nextCursor: null } });
    renderPage();
    const input = screen.getByPlaceholderText("Group name, link or ID");
    await userEvent.type(input, "x{Enter}");
    expect(screen.getByText("Type at least 2 characters to search.")).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, 700));
    expect(searchCalls()).toHaveLength(0);

    await userEvent.clear(input);
    await userEvent.type(input, "7{Enter}");
    expect(searchCalls()).toEqual([["groups_search", { query: "7", cursor: null }]]);
  });

  it("loads the next page with the cursor and keeps the first results", async () => {
    wire({
      groups_search: (args: InvokeArgs) =>
        (args as { cursor: string | null }).cursor === "c2"
          ? { groups: [group(503, "Third")], nextCursor: null }
          : { groups: [OPEN, APPROVAL], nextCursor: "c2" },
    });
    renderPage();
    await search("club");
    await userEvent.click(await screen.findByRole("button", { name: "Load more" }));
    expect(invokeMock).toHaveBeenCalledWith("groups_search", { query: "club", cursor: "c2" });
    expect(await screen.findByText("Third")).toBeInTheDocument();
    expect(screen.getByText("Builders Club")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Load more" })).not.toBeInTheDocument();
  });

  it("picks the group right away when a link is pasted", async () => {
    wire({ groups_search: { groups: [APPROVAL], nextCursor: null } });
    renderPage();
    await userEvent.click(screen.getByPlaceholderText("Group name, link or ID"));
    await userEvent.paste("https://www.roblox.com/communities/502/x");
    expect(await screen.findByText("Join Secret Society", undefined, { timeout: 2000 })).toBeInTheDocument();
    expect(searchCalls()).toEqual([
      ["groups_search", { query: "https://www.roblox.com/communities/502/x", cursor: null }],
    ]);
  });
});

describe("GroupsPage — popular groups (empty field)", () => {
  const BIG = group(701, "Big Studio", { memberCount: 90_000_000, hasVerifiedBadge: true });
  const MID = group(702, "Mid Studio", { memberCount: 5_000_000 });
  const HUGE = group(703, "Huge Studio", { memberCount: 120_000_000, publicEntryAllowed: false });

  it("shows the popular groups, biggest first, with their icons", async () => {
    wire({ groups_popular: [BIG, MID, HUGE] });
    renderPage();
    const list = await screen.findByRole("list", { name: "Popular groups" });
    expect(screen.getByRole("heading", { name: "Popular groups" })).toBeInTheDocument();
    const names = within(list)
      .getAllByRole("button")
      .map((card) => card.textContent ?? "");
    expect(names[0]).toContain("Huge Studio");
    expect(names[1]).toContain("Big Studio");
    expect(names[2]).toContain("Mid Studio");
    expect(within(list).getByText("Approval required")).toBeInTheDocument();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("groups_icons", { groupIds: [703, 701, 702] }));
  });

  it("a popular card is picked like a search result", async () => {
    wire({ groups_popular: [BIG, MID] });
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: /Mid Studio/ }));
    expect(screen.getByText("Join Mid Studio")).toBeInTheDocument();
  });

  it("falls back to the hint when the popular groups cannot load", async () => {
    wire({
      groups_popular: () => Promise.reject("Could not load the popular groups"),
    });
    renderPage();
    expect(await screen.findByText("Search by name, or paste a group link or ID.")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Popular groups" })).not.toBeInTheDocument();
  });

  it("asks only once per session and comes back when the field is cleared", async () => {
    wire({ groups_popular: [BIG, MID], groups_search: { groups: [OPEN], nextCursor: null } });
    renderPage();
    await screen.findByRole("list", { name: "Popular groups" });
    await search("builders");
    expect(await screen.findByRole("list", { name: "Groups found" })).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Popular groups" })).not.toBeInTheDocument();
    await userEvent.clear(screen.getByPlaceholderText("Group name, link or ID"));
    expect(await screen.findByRole("list", { name: "Popular groups" })).toBeInTheDocument();

    cleanup();
    render(<GroupsPage active onLeave={() => {}} />);
    expect(await screen.findByRole("list", { name: "Popular groups" })).toBeInTheDocument();
    expect(invokeMock.mock.calls.filter((call) => call[0] === "groups_popular")).toHaveLength(1);
  });
});

describe("GroupsPage — accounts and join", () => {
  it("starts with the accounts selected in the list ticked", async () => {
    wire();
    renderPage({ selectedAccounts: [ACCOUNTS[1]] });
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "bravo" })).toHaveAttribute("aria-checked", "true"));
    expect(screen.getByRole("checkbox", { name: "alpha" })).toHaveAttribute("aria-checked", "false");
  });

  it("Select all and Clear tick and untick every account", async () => {
    wire();
    renderPage();
    await userEvent.click(screen.getByRole("button", { name: "Select all" }));
    for (const name of ["alpha", "bravo", "charlie"]) {
      expect(screen.getByRole("checkbox", { name })).toHaveAttribute("aria-checked", "true");
    }
    await userEvent.click(screen.getByRole("button", { name: "Clear" }));
    for (const name of ["alpha", "bravo", "charlie"]) {
      expect(screen.getByRole("checkbox", { name })).toHaveAttribute("aria-checked", "false");
    }
  });

  it("keeps Join group off until a group and an account are picked", async () => {
    wire();
    renderPage();
    expect(screen.getByText("Pick a group above first")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Join group" })).toBeDisabled();
    await search("builders");
    await userEvent.click(await screen.findByRole("button", { name: /Builders Club/ }));
    expect(screen.getByRole("button", { name: "Join group" })).toBeDisabled();
    await userEvent.click(screen.getByRole("checkbox", { name: "alpha" }));
    expect(screen.getByRole("button", { name: "Join group" })).toBeEnabled();
  });

  it("joins the ticked accounts, shows each status and offers the browser for a challenge", async () => {
    const final: GroupJoinSnapshot = {
      running: false,
      groupId: 501,
      groupName: "Builders Club",
      total: 3,
      done: 3,
      currentUserId: null,
      accounts: [
        { userId: 11, status: "joined", reason: null },
        { userId: 22, status: "challenge", reason: null, challengeType: "captcha", detail: "HTTP 403 · challenge captcha" },
        {
          userId: 33,
          status: "failed",
          reason: "You are already in the maximum number of groups.",
          detail: "HTTP 403 · code 6",
        },
      ],
    };
    wire({ groups_join_batch: final, open_account_browser: null, groups_check_membership: "joined" });
    const store = renderPage({ selectedAccounts: ACCOUNTS });
    await search("builders");
    await userEvent.click(await screen.findByRole("button", { name: /Builders Club/ }));
    await userEvent.click(screen.getByRole("button", { name: "Join group" }));

    expect(invokeMock).toHaveBeenCalledWith("groups_join_batch", { userIds: [11, 22, 33], groupId: 501 });
    const row = (id: number) => screen.getByTestId(`group-join-row-${id}`);
    await waitFor(() => expect(within(row(11)).getByText("Joined")).toBeInTheDocument());
    expect(within(row(22)).getByText("Needs captcha")).toBeInTheDocument();
    expect(within(row(22)).getByText("Roblox asked for a captcha")).toBeInTheDocument();
    expect(within(row(22)).queryByText(/^type:/)).not.toBeInTheDocument();
    expect(within(row(33)).getByText("Failed")).toBeInTheDocument();
    expect(within(row(33)).getByText("You are already in the maximum number of groups.")).toBeInTheDocument();
    expect(within(row(33)).getByText("HTTP 403 · code 6")).toBeInTheDocument();
    // Quem entrou não ganha botão nenhum; quem não entrou ganha os três.
    expect(within(row(11)).queryByRole("button", { name: /Open in browser|Try again|Check again/ })).not.toBeInTheDocument();
    expect(within(row(33)).getByRole("button", { name: "Try again for charlie" })).toBeInTheDocument();
    expect(within(row(33)).getByRole("button", { name: "Open in browser for charlie" })).toBeInTheDocument();

    await userEvent.click(within(row(22)).getByRole("button", { name: "Open in browser for bravo" }));
    expect(invokeMock).toHaveBeenCalledWith("open_account_browser", { userId: 22, groupId: 501 });
    expect(store.addToast).toHaveBeenCalledWith(expect.stringContaining("bravo"));

    await userEvent.click(within(row(22)).getByRole("button", { name: "Check again for bravo" }));
    expect(invokeMock).toHaveBeenCalledWith("groups_check_membership", { userId: 22, groupId: 501 });
    await waitFor(() => expect(within(row(22)).getByText("Joined")).toBeInTheDocument());
    expect(within(row(22)).queryByRole("button", { name: /Open in browser/ })).not.toBeInTheDocument();
  });

  /**
   * O dono viu "Needs captcha" em conta que só tinha o aviso de termos de uso:
   * desafio que não é captcha diz "confirmar alguma coisa" e mostra o tipo.
   */
  it("says 'confirm something' and shows the type when the challenge is not a captcha", async () => {
    const final: GroupJoinSnapshot = {
      ...IDLE,
      groupId: 501,
      groupName: "Builders Club",
      total: 1,
      done: 1,
      accounts: [
        {
          userId: 11,
          status: "challenge",
          reason: "Challenge is required to authorize the request",
          challengeType: "proofofwork",
          detail: "HTTP 403 · challenge proofofwork · code 0",
        },
      ],
    };
    wire({ groups_join_batch: final });
    renderPage({ selectedAccounts: [ACCOUNTS[0]] });
    await search("builders");
    await userEvent.click(await screen.findByRole("button", { name: /Builders Club/ }));
    await userEvent.click(screen.getByRole("button", { name: "Join group" }));

    const row = screen.getByTestId("group-join-row-11");
    await waitFor(() => expect(within(row).getByText("Needs confirmation")).toBeInTheDocument());
    expect(within(row).queryByText(/captcha/i)).not.toBeInTheDocument();
    expect(within(row).getByText("Roblox asked this account to confirm something")).toBeInTheDocument();
    expect(
      within(row).getByText("Open it in the browser and accept what Roblox shows (terms, verification). Then press Try again.")
    ).toBeInTheDocument();
    expect(within(row).getByText("type: proofofwork")).toBeInTheDocument();
    expect(
      within(row).getByText("HTTP 403 · challenge proofofwork · code 0 · Challenge is required to authorize the request")
    ).toBeInTheDocument();
  });

  it("Try again re-runs the join for that account only and shows the new status", async () => {
    const final: GroupJoinSnapshot = {
      ...IDLE,
      groupId: 501,
      groupName: "Builders Club",
      total: 2,
      done: 2,
      accounts: [
        { userId: 11, status: "joined", reason: null },
        { userId: 22, status: "challenge", reason: null, challengeType: "generic" },
      ],
    };
    const retried = deferred<GroupJoinSnapshot>();
    wire({ groups_join_batch: final, groups_join_retry: () => retried.promise });
    renderPage({ selectedAccounts: ACCOUNTS.slice(0, 2) });
    await search("builders");
    await userEvent.click(await screen.findByRole("button", { name: /Builders Club/ }));
    await userEvent.click(screen.getByRole("button", { name: "Join group" }));

    const row = (id: number) => screen.getByTestId(`group-join-row-${id}`);
    await userEvent.click(await within(row(22)).findByRole("button", { name: "Try again for bravo" }));
    expect(invokeMock).toHaveBeenCalledWith("groups_join_retry", { userId: 22, groupId: 501 });
    // Um por vez: enquanto tenta, nem outro "Tentar de novo" nem o lote.
    expect(within(row(22)).getByRole("button", { name: "Try again for bravo" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Join group" })).toBeDisabled();
    // O botão não marca/desmarca a conta.
    expect(screen.getByRole("checkbox", { name: "bravo" })).toHaveAttribute("aria-checked", "true");

    await act(async () =>
      retried.resolve({
        ...final,
        accounts: [final.accounts[0], { userId: 22, status: "joined", reason: null, challengeType: null, detail: null }],
      })
    );
    await waitFor(() => expect(within(row(22)).getByText("Joined")).toBeInTheDocument());
    expect(within(row(22)).queryByRole("button", { name: /Try again/ })).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.filter((call) => call[0] === "groups_join_batch")).toHaveLength(1);
  });

  it("shows the refusal when Try again cannot start", async () => {
    const final: GroupJoinSnapshot = {
      ...IDLE,
      groupId: 501,
      groupName: "Builders Club",
      total: 1,
      done: 1,
      accounts: [{ userId: 11, status: "failed", reason: "Too many requests" }],
    };
    wire({
      groups_join_batch: final,
      groups_join_retry: () => Promise.reject("A group join is already running"),
    });
    const store = renderPage({ selectedAccounts: [ACCOUNTS[0]] });
    await search("builders");
    await userEvent.click(await screen.findByRole("button", { name: /Builders Club/ }));
    await userEvent.click(screen.getByRole("button", { name: "Join group" }));
    await userEvent.click(await screen.findByRole("button", { name: "Try again for alpha" }));
    await waitFor(() => expect(store.addToast).toHaveBeenCalledWith("A group join is already running", "error"));
  });

  /** Pedido do dono: clicar em qualquer lugar da caixa da conta marca/desmarca. */
  it("toggles an account by clicking anywhere in its box, but not through its buttons", async () => {
    const final: GroupJoinSnapshot = {
      ...IDLE,
      groupId: 501,
      groupName: "Builders Club",
      total: 1,
      done: 1,
      accounts: [{ userId: 22, status: "challenge", reason: null, challengeType: "captcha" }],
    };
    wire({ groups_join_batch: final, groups_check_membership: "notMember", open_account_browser: null });
    renderPage({ selectedAccounts: [ACCOUNTS[1]] });
    await search("builders");
    await userEvent.click(await screen.findByRole("button", { name: /Builders Club/ }));
    await userEvent.click(screen.getByRole("button", { name: "Join group" }));

    const row = screen.getByTestId("group-join-row-22");
    const box = () => screen.getByRole("checkbox", { name: "bravo" });
    await within(row).findByText("Roblox asked for a captcha");
    expect(box()).toHaveAttribute("aria-checked", "true");

    // Área vazia da caixa (o texto do aviso, fora de qualquer botão).
    await userEvent.click(within(row).getByText("Roblox asked for a captcha"));
    expect(box()).toHaveAttribute("aria-checked", "false");
    await userEvent.click(row);
    expect(box()).toHaveAttribute("aria-checked", "true");

    await userEvent.click(within(row).getByRole("button", { name: "Check again for bravo" }));
    expect(invokeMock).toHaveBeenCalledWith("groups_check_membership", { userId: 22, groupId: 501 });
    await userEvent.click(within(row).getByRole("button", { name: "Open in browser for bravo" }));
    expect(box()).toHaveAttribute("aria-checked", "true");

    // A caixa de marcar continua de verdade: teclado marca e desmarca.
    box().focus();
    await userEvent.keyboard(" ");
    expect(box()).toHaveAttribute("aria-checked", "false");
    await userEvent.keyboard("{Enter}");
    expect(box()).toHaveAttribute("aria-checked", "true");
  });

  it("follows the batch by its event, with progress and Cancel while it runs", async () => {
    wire({ groups_join_batch: () => new Promise(() => {}), groups_cancel_join: null });
    renderPage({ selectedAccounts: ACCOUNTS.slice(0, 2) });
    await search("builders");
    await userEvent.click(await screen.findByRole("button", { name: /Builders Club/ }));
    await userEvent.click(screen.getByRole("button", { name: "Join group" }));

    act(() =>
      emitTauriEvent("groups-join-state", {
        running: true,
        groupId: 501,
        groupName: "Builders Club",
        total: 2,
        done: 1,
        currentUserId: 22,
        accounts: [
          { userId: 11, status: "pending", reason: null },
          { userId: 22, status: "joining", reason: null },
        ],
      })
    );
    expect(await screen.findByText("Pending approval")).toBeInTheDocument();
    expect(screen.getByText("Joining...")).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "Group join progress" })).toHaveAttribute("aria-valuenow", "1");
    // Durante o lote, a lista de marcar fica travada — clicar na caixa também não marca.
    expect(screen.getByRole("checkbox", { name: "alpha" })).toBeDisabled();
    await userEvent.click(screen.getByTestId("group-join-row-11"));
    expect(screen.getByRole("checkbox", { name: "alpha" })).toHaveAttribute("aria-checked", "true");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(invokeMock.mock.calls.map((call) => call[0])).toContain("groups_cancel_join");
  });

  it("hides account names in the rows when names are hidden", async () => {
    wire();
    renderPage({ hideUsernames: true });
    expect(screen.queryByRole("checkbox", { name: "alpha" })).not.toBeInTheDocument();
    expect(screen.queryByText("alpha")).not.toBeInTheDocument();
  });
});

describe("GroupsPage — tutorial", () => {
  it("walks the tutorial without changing anything", async () => {
    wire();
    renderPage();
    await walkTour("groups", { invoke: invokeMock });
    expect(invokeMock).not.toHaveBeenCalledWith("groups_join_batch", expect.anything());
  });
});
