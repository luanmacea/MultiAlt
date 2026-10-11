import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());

import { AwayDigestCard, AwayDigestLine } from "./AwayDigest";
import { makeAccount, setStore } from "../../test-utils/renderWithStore";
import type { AwayEvent } from "../../utils/awayDigest";
import { awayEventTime } from "../../utils/awayDigest";

const ACCOUNTS = [
  makeAccount({ UserID: 1, Username: "alpha" }),
  makeAccount({ UserID: 2, Username: "bravo" }),
];

const LOST = { kind: "disconnected" as const, reason: "connectionLost" as const, code: 277, message: null, sinceMs: 0 };
const T0 = new Date(2026, 9, 11, 14, 32).getTime();

/** Do mais novo para o mais antigo, como a store entrega. */
function events(count: number): AwayEvent[] {
  return Array.from({ length: count }, (_, i) => ({
    id: count - i,
    kind: i % 2 === 0 ? "dropped" : "backInGame",
    userId: i % 2 === 0 ? 1 : 2,
    atMs: T0 - i * 60_000,
    drop: i % 2 === 0 ? LOST : null,
  }));
}

afterEach(cleanup);

describe("AwayDigestCard (página Session)", () => {
  it("says nothing happened when there is nothing", () => {
    setStore({ accounts: ACCOUNTS });
    render(<AwayDigestCard />);
    const card = screen.getByRole("region", { name: "While you were away" });
    expect(card).toHaveTextContent("Nothing happened yet.");
    expect(within(card).queryByRole("button", { name: "Clear" })).not.toBeInTheDocument();
  });

  it("shows the last five, newest first, with the time and the account", () => {
    setStore({ accounts: ACCOUNTS, awayEvents: events(7), awayUnread: 3 });
    render(<AwayDigestCard />);
    const rows = screen.getAllByTestId("away-event");
    expect(rows).toHaveLength(5);
    expect(rows[0]).toHaveTextContent(awayEventTime(T0));
    expect(rows[0]).toHaveTextContent("alpha");
    expect(rows[0]).toHaveTextContent("Disconnected: lost connection");
    expect(rows[0]).toHaveAttribute("data-tone", "problem");
    expect(rows[1]).toHaveTextContent("bravo");
    expect(rows[1]).toHaveAttribute("data-tone", "recovered");
    expect(screen.getByTestId("away-unread")).toHaveTextContent("3 new");
  });

  it("Show all lists everything and Show less goes back", async () => {
    setStore({ accounts: ACCOUNTS, awayEvents: events(7) });
    render(<AwayDigestCard />);
    await userEvent.click(screen.getByRole("button", { name: "Show all (7)" }));
    expect(screen.getAllByTestId("away-event")).toHaveLength(7);
    await userEvent.click(screen.getByRole("button", { name: "Show less" }));
    expect(screen.getAllByTestId("away-event")).toHaveLength(5);
  });

  it("clicking the card marks it seen; Clear empties the list", async () => {
    const store = setStore({ accounts: ACCOUNTS, awayEvents: events(2), awayUnread: 2 });
    render(<AwayDigestCard />);
    await userEvent.click(screen.getAllByTestId("away-event")[0]);
    expect(store.markAwaySeen).toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(store.clearAwayEvents).toHaveBeenCalled();
  });

  it("hides the names with Names hidden on", () => {
    setStore({ accounts: ACCOUNTS, awayEvents: events(2), hideUsernames: true, hiddenNameLetters: 0 });
    render(<AwayDigestCard />);
    const card = screen.getByRole("region", { name: "While you were away" });
    expect(card).not.toHaveTextContent("alpha");
    expect(card).not.toHaveTextContent("bravo");
  });

  it("an event with no account shows only what happened", () => {
    setStore({
      accounts: ACCOUNTS,
      awayEvents: [{ id: 1, kind: "emergencyStop", userId: null, atMs: T0, afk: true, recordings: false }],
    });
    render(<AwayDigestCard />);
    expect(screen.getByTestId("away-event")).toHaveTextContent("You shook the mouse, so MultiAlt stopped AFK mode.");
  });
});

describe("AwayDigestLine (Modo AFK)", () => {
  it("stays out of the way when nothing new happened", () => {
    setStore({ accounts: ACCOUNTS, awayEvents: events(3), awayUnread: 0 });
    const { container } = render(<AwayDigestLine />);
    expect(container).toBeEmptyDOMElement();
  });

  it("counts what happened since the last look and opens Session", async () => {
    const store = setStore({ accounts: ACCOUNTS, awayEvents: events(4), awayUnread: 3 });
    const onNavigate = vi.fn();
    render(<AwayDigestLine onNavigate={onNavigate} />);
    const line = screen.getByTestId("away-digest-line");
    expect(line).toHaveTextContent("3 events while you were away");
    // O mais recente, para não precisar abrir a Sessão por um só.
    expect(line).toHaveTextContent("alpha");
    await userEvent.click(within(line).getByRole("button", { name: "View" }));
    expect(store.setActivePage).toHaveBeenCalledWith("session");
    expect(onNavigate).toHaveBeenCalled();
  });

  it("uses the singular for one event", () => {
    setStore({ accounts: ACCOUNTS, awayEvents: events(1), awayUnread: 1 });
    render(<AwayDigestLine />);
    expect(screen.getByTestId("away-digest-line")).toHaveTextContent("1 event while you were away");
  });
});
