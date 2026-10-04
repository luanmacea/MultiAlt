import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
const getVersionMock = vi.fn<() => Promise<string>>();
vi.mock("@tauri-apps/api/app", () => ({ getVersion: () => getVersionMock() }));

import { ChangelogPage } from "./ChangelogPage";
import { setStore } from "../../test-utils/renderWithStore";
import { resetReleaseHistoryCache } from "../../releaseNotes";
import i18n, { DEFAULT_LANGUAGE } from "../../i18n/index";
import type { StoreValue } from "../../store";
import { walkTour } from "../../test-utils/tourHelpers";

function release(tag: string, publishedAt: string, items: string[]) {
  return {
    tag_name: tag,
    published_at: publishedAt,
    html_url: `https://github.com/luanmacea/MultiAlt/releases/tag/${tag}`,
    draft: false,
    prerelease: false,
    body: ["## What's Changed", ...items.map((i) => `- ${i}`), "", "## Contributors", "[@a](https://github.com/a)"].join("\n"),
  };
}

const RELEASES = [
  release("v0.1.11-beta", "2026-10-05T12:00:00Z", ["Faster account list"]),
  release("v0.1.10-beta", "2026-10-03T12:00:00Z", ["New menu on the left", "New language: Spanish"]),
  release("v0.1.9-beta", "2026-10-02T12:00:00Z", ["Rename to MultiAlt"]),
];

function ok(data: unknown) {
  return { ok: true, status: 200, headers: new Headers(), json: async () => data } as unknown as Response;
}

const fetchMock = vi.fn();

function renderPage(active = true, overrides: Partial<StoreValue> = {}) {
  const store = setStore(overrides);
  const onLeave = vi.fn();
  const view = render(<ChangelogPage active={active} onLeave={onLeave} />);
  return { store, onLeave, ...view };
}

/** Uma entrada por versão: os filhos diretos da lista (as notas têm `<li>` próprios). */
function entries(listName = "Versions"): HTMLElement[] {
  return Array.from(screen.getByRole("list", { name: listName }).children) as HTMLElement[];
}

beforeEach(async () => {
  await i18n.changeLanguage(DEFAULT_LANGUAGE);
  resetReleaseHistoryCache();
  fetchMock.mockReset();
  fetchMock.mockResolvedValue(ok(RELEASES));
  vi.stubGlobal("fetch", fetchMock);
  getVersionMock.mockReset();
  getVersionMock.mockResolvedValue("0.1.10");
});

afterEach(async () => {
  cleanup();
  vi.unstubAllGlobals();
  await i18n.changeLanguage(DEFAULT_LANGUAGE);
});

describe("ChangelogPage", () => {
  it("does not ask GitHub for anything until the page is opened", () => {
    renderPage(false);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(screen.queryByRole("heading", { name: "What's new" })).not.toBeInTheDocument();
  });

  it("lists each version newest first, with its date and what changed", async () => {
    renderPage();
    expect(screen.getByRole("heading", { level: 1, name: "What's new" })).toBeInTheDocument();
    await waitFor(() => expect(entries()).toHaveLength(3));

    const [newest, current, oldest] = entries();
    expect(within(newest).getByRole("heading", { name: /v0\.1\.11/ })).toBeInTheDocument();
    expect(within(current).getByText("New menu on the left")).toBeInTheDocument();
    expect(within(current).getByText("New language: Spanish")).toBeInTheDocument();
    expect(within(current).getByText("October 3, 2026")).toBeInTheDocument();
    expect(within(oldest).getByText("Rename to MultiAlt")).toBeInTheDocument();
    // Sem contribuidores nem cabeçalho técnico: só a lista.
    expect(screen.queryByText(/Contributors/)).not.toBeInTheDocument();
  });

  it("shows a loading state while the list is on its way", async () => {
    let resolve!: (r: Response) => void;
    fetchMock.mockReturnValue(new Promise<Response>((r) => (resolve = r)));
    renderPage();
    expect(screen.getByRole("status")).toHaveTextContent("Loading the update history…");
    resolve(ok(RELEASES));
    await waitFor(() => expect(entries()).toHaveLength(3));
    expect(screen.queryByText("Loading the update history…")).not.toBeInTheDocument();
  });

  it("marks the version this computer has", async () => {
    renderPage();
    await waitFor(() => expect(entries()).toHaveLength(3));
    const [newest, current, oldest] = entries();
    expect(within(current).getByText("Your version")).toBeInTheDocument();
    expect(current).toHaveAttribute("aria-current", "true");
    expect(within(newest).queryByText("Your version")).not.toBeInTheDocument();
    expect(within(oldest).queryByText("Your version")).not.toBeInTheDocument();
  });

  it("marks nothing when the app version cannot be read", async () => {
    getVersionMock.mockRejectedValue(new Error("no tauri"));
    renderPage();
    await waitFor(() => expect(entries()).toHaveLength(3));
    expect(screen.queryByText("Your version")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Update available|Check for Updates/ })).not.toBeInTheDocument();
  });

  it("opens the update window from a newer version when the update is known", async () => {
    const { store } = renderPage(true, {
      updateInfo: {
        version: "0.1.11",
        currentVersion: "0.1.10",
        date: "",
        body: "",
        releaseChannel: "beta",
        featureChannel: "standard",
      },
    });
    await waitFor(() => expect(entries()).toHaveLength(3));
    const [newest, current] = entries();
    expect(within(newest).getByText("Not installed yet")).toBeInTheDocument();
    expect(within(current).queryByRole("button")).not.toBeInTheDocument();

    await userEvent.click(within(newest).getByRole("button", { name: "Update available" }));
    expect(store.setUpdateDialogOpen).toHaveBeenCalledWith(true);
    expect(store.checkForUpdates).not.toHaveBeenCalled();
  });

  it("checks for updates from a newer version the app does not know about yet", async () => {
    const { store } = renderPage();
    await waitFor(() => expect(entries()).toHaveLength(3));
    await userEvent.click(within(entries()[0]).getByRole("button", { name: "Check for Updates" }));
    expect(store.checkForUpdates).toHaveBeenCalledWith(true);
    expect(store.setUpdateDialogOpen).not.toHaveBeenCalled();
  });

  it("offers no update when the newest version is the one installed", async () => {
    getVersionMock.mockResolvedValue("0.1.11");
    renderPage();
    await waitFor(() => expect(entries()).toHaveLength(3));
    expect(within(entries()[0]).getByText("Your version")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Update available|Check for Updates/ })).not.toBeInTheDocument();
    expect(screen.queryByText("Not installed yet")).not.toBeInTheDocument();
  });

  it("explains a failed load and tries again", async () => {
    fetchMock.mockRejectedValueOnce(new TypeError("Failed to fetch"));
    renderPage();
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Couldn't load the update history. Check your internet connection and try again.");

    await userEvent.click(within(alert).getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(entries()).toHaveLength(3));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("says when GitHub is limiting requests", async () => {
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 403,
      headers: new Headers({ "x-ratelimit-remaining": "0" }),
      json: async () => ({}),
    } as unknown as Response);
    renderPage();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "GitHub is limiting how often the update history can be loaded. Try again in a few minutes."
    );
  });

  it("keeps the list for the session: opening the page again does not ask again", async () => {
    const first = renderPage();
    await waitFor(() => expect(entries()).toHaveLength(3));
    first.unmount();
    renderPage();
    expect(entries()).toHaveLength(3);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("says so when there is nothing to show", async () => {
    fetchMock.mockResolvedValue(ok([]));
    renderPage();
    expect(await screen.findByText("No updates to show yet.")).toBeInTheDocument();
  });

  it("opens the full history on GitHub", async () => {
    const open = vi.spyOn(window, "open").mockImplementation(() => null);
    renderPage();
    await waitFor(() => expect(entries()).toHaveLength(3));
    await userEvent.click(screen.getByRole("button", { name: "See every version on GitHub" }));
    expect(open).toHaveBeenCalledWith("https://github.com/luanmacea/MultiAlt/releases", "_blank");
    open.mockRestore();
  });

  it("goes back to the account list on Escape", async () => {
    const { onLeave } = renderPage();
    await userEvent.keyboard("{Escape}");
    expect(onLeave).toHaveBeenCalled();
  });

  it("speaks Portuguese, dates included", async () => {
    await i18n.changeLanguage("pt");
    renderPage();
    expect(screen.getByRole("heading", { level: 1, name: "Novidades" })).toBeInTheDocument();
    await waitFor(() => expect(entries("Versões")).toHaveLength(3));
    expect(screen.getByText("Sua versão")).toBeInTheDocument();
    expect(screen.getByText("3 de outubro de 2026")).toBeInTheDocument();
  });
});

describe("ChangelogPage — tutorial", () => {
  it("walks the short What's new tutorial without starting an update", async () => {
    const { store } = renderPage();
    await screen.findByText("Your version");
    await walkTour("changelog");
    expect(store.setUpdateDialogOpen).not.toHaveBeenCalled();
    expect(store.checkForUpdates).not.toHaveBeenCalled();
  });

  it("falls back to the list when the history is still loading", async () => {
    fetchMock.mockReturnValue(new Promise(() => {}));
    renderPage();
    await walkTour("changelog", { allowMissing: ["list", "current", "update"] });
  });
});
