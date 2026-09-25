import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { VersionsDialog } from "./VersionsDialog";
import { setStore } from "../../test-utils/renderWithStore";
import {
  emitTauriEvent,
  invokeMock,
  resetTauriMocks,
  setInvokeHandler,
} from "../../test-utils/tauriMocks";

const INSTALLED = {
  channel: "LIVE",
  versionHash: "version-abcdef0123456789",
  binaryType: "WindowsPlayer",
  displayVersion: "1.2.3",
  installPath: "C:/versions/live",
  installSizeBytes: 512 * 1024 * 1024,
  installedAt: null,
  lastLaunchedAt: null,
  userLabel: null,
};

const REMOTE = {
  current: [
    {
      binaryType: "WindowsPlayer",
      versionHash: "version-1111111111111111",
      displayVersion: "2.0.0",
      deployDate: "2026-01-01",
      channel: "LIVE",
    },
  ],
  past: [
    {
      binaryType: "WindowsPlayer",
      versionHash: "version-2222222222222222",
      displayVersion: "1.9.0",
      deployDate: "2025-12-01",
      channel: "LIVE",
    },
  ],
  pastError: null as string | null,
};

/** Routes the dialog's backend calls; `overrides` wins per command. */
function backend(overrides: Record<string, unknown | ((args?: unknown) => unknown)> = {}) {
  setInvokeHandler((cmd, args) => {
    if (cmd in overrides) {
      const entry = overrides[cmd];
      return typeof entry === "function" ? (entry as (a?: unknown) => unknown)(args) : entry;
    }
    switch (cmd) {
      case "versions_list_installed":
        return [];
      case "get_all_settings":
        return { Versions: { DefaultVersion: "" } };
      case "versions_list_remote":
        return REMOTE;
      default:
        return undefined;
    }
  });
}

function renderDialog() {
  const store = setStore({});
  const onClose = vi.fn();
  render(<VersionsDialog open onClose={onClose} />);
  return { store, onClose };
}

beforeEach(() => {
  resetTauriMocks();
  backend();
});

afterEach(cleanup);

describe("VersionsDialog — installed tab", () => {
  it("renders nothing while closed", () => {
    setStore({});
    const { container } = render(<VersionsDialog open={false} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("tells the user where to get a version when none is installed", async () => {
    renderDialog();
    expect(
      await screen.findByText("No versions installed yet. Switch to Browse or Manual install to add one.")
    ).toBeInTheDocument();
  });

  it("lists an installed version with its channel, hash and size", async () => {
    backend({ versions_list_installed: [INSTALLED] });
    renderDialog();

    expect(await screen.findByText("1.2.3")).toBeInTheDocument();
    expect(screen.getByText("LIVE")).toBeInTheDocument();
    expect(screen.getByText("version-abcdef0123456789")).toBeInTheDocument();
    expect(screen.getByText("512 MB")).toBeInTheDocument();
  });

  it("prefers the user's nickname over the reported version", async () => {
    backend({ versions_list_installed: [{ ...INSTALLED, userLabel: "Pre-Hyperion" }] });
    renderDialog();
    expect(await screen.findByText("Pre-Hyperion")).toBeInTheDocument();
    expect(screen.queryByText("1.2.3")).not.toBeInTheDocument();
  });

  it("marks and clears the default version", async () => {
    backend({ versions_list_installed: [INSTALLED] });
    renderDialog();
    await screen.findByText("1.2.3");

    await userEvent.click(screen.getByTitle("Set as default"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("versions_set_default", {
        versionId: "LIVE:version-abcdef0123456789",
      })
    );
    // The badge flips without a reload.
    expect(await screen.findByText("Default")).toBeInTheDocument();

    await userEvent.click(screen.getByTitle("Unset as default"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("versions_set_default", { versionId: null })
    );
  });

  it("shows the Default badge for the configured version", async () => {
    backend({
      versions_list_installed: [INSTALLED],
      get_all_settings: { Versions: { DefaultVersion: "LIVE:version-abcdef0123456789" } },
    });
    renderDialog();
    expect(await screen.findByText("Default")).toBeInTheDocument();
    expect(screen.getByTitle("Unset as default")).toBeInTheDocument();
  });

  it("renames a version through the inline editor", async () => {
    backend({ versions_list_installed: [INSTALLED] });
    renderDialog();
    await screen.findByText("1.2.3");

    await userEvent.click(screen.getByTitle("Rename"));
    await userEvent.type(screen.getByPlaceholderText("Nickname"), "Old build");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("versions_set_label", {
        channel: "LIVE",
        versionHash: "version-abcdef0123456789",
        label: "Old build",
      })
    );
  });

  it("opens the install folder", async () => {
    backend({ versions_list_installed: [INSTALLED] });
    renderDialog();
    await screen.findByText("1.2.3");

    await userEvent.click(screen.getByTitle("Open folder"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("versions_open_folder", {
        channel: "LIVE",
        versionHash: "version-abcdef0123456789",
      })
    );
  });

  it("uninstalls only after the browser confirmation", async () => {
    backend({ versions_list_installed: [INSTALLED] });
    // happy-dom does not implement window.confirm; the WebView does.
    const confirmSpy = vi.fn(() => false);
    const originalConfirm = window.confirm;
    window.confirm = confirmSpy as unknown as typeof window.confirm;
    renderDialog();
    await screen.findByText("1.2.3");

    await userEvent.click(screen.getByTitle("Uninstall"));
    expect(invokeMock).not.toHaveBeenCalledWith("versions_uninstall", expect.anything());

    confirmSpy.mockReturnValue(true);
    await userEvent.click(screen.getByTitle("Uninstall"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("versions_uninstall", {
        channel: "LIVE",
        versionHash: "version-abcdef0123456789",
      })
    );
    window.confirm = originalConfirm;
  });
});

describe("VersionsDialog — browse tab", () => {
  it("fetches the remote catalog on first visit", async () => {
    renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Browse" }));

    expect(await screen.findByText("2.0.0")).toBeInTheDocument();
    expect(screen.getByText("1.9.0")).toBeInTheDocument();
    expect(screen.getByText("Current")).toBeInTheDocument();
    expect(screen.getByText("Previous")).toBeInTheDocument();
  });

  it("marks catalog entries that are already installed", async () => {
    backend({
      versions_list_installed: [{ ...INSTALLED, versionHash: "version-1111111111111111" }],
    });
    renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Browse" }));

    // The first "Installed" button is the tab; the second is the catalog row.
    await screen.findByText("2.0.0");
    const buttons = screen.getAllByRole("button", { name: "Installed" });
    expect(buttons[buttons.length - 1]).toBeDisabled();
  });

  it("starts an install from the catalog", async () => {
    renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Browse" }));
    await screen.findByText("2.0.0");

    await userEvent.click(screen.getAllByRole("button", { name: "Install" })[0]);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "versions_install",
        expect.objectContaining({ channel: "LIVE", versionHash: "version-1111111111111111" })
      )
    );
  });

  it("warns when the previous-versions feed fails", async () => {
    backend({ versions_list_remote: { ...REMOTE, pastError: "weao down" } });
    renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Browse" }));
    expect(
      await screen.findByText("Could not load previous versions: weao down")
    ).toBeInTheDocument();
  });
});

describe("VersionsDialog — manual install", () => {
  async function openManual() {
    renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Manual install" }));
  }

  it("rejects a hash that is not in version-<hex> form", async () => {
    const { store } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Manual install" }));
    await userEvent.type(screen.getByPlaceholderText("version-abcdef0123456789"), "abcdef");
    await userEvent.click(screen.getByRole("button", { name: "Install" }));

    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(
        "Version hash must look like version-<16 hex chars>"
      )
    );
    expect(invokeMock).not.toHaveBeenCalledWith("versions_install", expect.anything());
  });

  it("does nothing with an empty hash", async () => {
    await openManual();
    await userEvent.click(screen.getByRole("button", { name: "Install" }));
    expect(invokeMock).not.toHaveBeenCalledWith("versions_install", expect.anything());
  });

  it("installs a manual hash with its channel and nickname", async () => {
    await openManual();
    await userEvent.clear(screen.getByPlaceholderText("LIVE"));
    await userEvent.type(screen.getByPlaceholderText("LIVE"), "ZCanary");
    await userEvent.type(
      screen.getByPlaceholderText("version-abcdef0123456789"),
      "version-9999999999999999"
    );
    await userEvent.type(screen.getByPlaceholderText("Pre-Hyperion"), "Old build");
    await userEvent.click(screen.getByRole("button", { name: "Install" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "versions_install",
        expect.objectContaining({
          channel: "ZCanary",
          versionHash: "version-9999999999999999",
          label: "Old build",
        })
      )
    );
  });

  it("reports install progress and a failed install", async () => {
    backend({
      versions_install: () => {
        throw new Error("disk full");
      },
    });
    const { store } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Manual install" }));
    await userEvent.type(
      screen.getByPlaceholderText("version-abcdef0123456789"),
      "version-9999999999999999"
    );
    await userEvent.click(screen.getByRole("button", { name: "Install" }));

    await waitFor(() =>
      expect(store.addToast).toHaveBeenCalledWith(expect.stringContaining("disk full"))
    );
  });

  it("shows the live progress line for a running install", async () => {
    let installId = "";
    backend({
      versions_install: (args?: unknown) => {
        installId = (args as { installId: string }).installId;
        return undefined;
      },
    });
    renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Manual install" }));
    await userEvent.type(
      screen.getByPlaceholderText("version-abcdef0123456789"),
      "version-9999999999999999"
    );
    await userEvent.click(screen.getByRole("button", { name: "Install" }));

    await waitFor(() => expect(installId).not.toBe(""));
    emitTauriEvent("version-install-progress", {
      installId,
      channel: "LIVE",
      versionHash: "version-9999999999999999",
      stage: "downloading",
      package: "RobloxApp.zip",
      current: 5,
      total: 10,
      message: null,
    });

    expect(await screen.findByText("downloading")).toBeInTheDocument();
    expect(screen.getByText("RobloxApp.zip")).toBeInTheDocument();
  });
});

/**
 * O backlog dizia que o hash era o problema do instalador manual, mas
 * `startInstall` já valida o formato do hash (linha 144-149) e o catálogo
 * remoto tem botão Install por linha — o que sobrava era só o `Channel`
 * sendo texto livre sem nenhuma lista, mesmo o catálogo já trazendo o canal
 * de cada versão. O campo continua editável: é uma escotilha deliberada para
 * canais que não aparecem no catálogo.
 *
 * Isto é só UI — nada aqui grava canal em lugar nenhum (ver CLAUDE.md,
 * `set_player_channel` é a única escrita permitida, e é no Rust, que este
 * arquivo não toca).
 */
describe("VersionsDialog — manual channel suggests known channels", () => {
  it("offers the channels of installed versions as datalist options", async () => {
    backend({
      versions_list_installed: [
        INSTALLED,
        {
          ...INSTALLED,
          channel: "ZCanary",
          versionHash: "version-3333333333333333",
          displayVersion: "3.0.0",
        },
      ],
    });
    renderDialog();
    await screen.findByText("1.2.3");
    await userEvent.click(screen.getByRole("button", { name: "Manual install" }));

    const input = screen.getByPlaceholderText("LIVE") as HTMLInputElement;
    const listId = input.getAttribute("list");
    expect(listId).toBeTruthy();
    // eslint-disable-next-line testing-library/no-node-access
    const datalist = document.getElementById(listId!) as HTMLDataListElement;
    expect(datalist).toBeInstanceOf(HTMLDataListElement);
    const options = Array.from(datalist.options).map((o) => o.value);
    expect(options).toEqual(["LIVE", "ZCanary"]);
  });

  it("adds the remote catalog's channels once it has been fetched", async () => {
    backend({
      versions_list_remote: {
        ...REMOTE,
        current: [
          ...REMOTE.current,
          {
            ...REMOTE.current[0],
            channel: "ZIntegration",
            versionHash: "version-4444444444444444",
            displayVersion: "2.1.0",
          },
        ],
      },
    });
    renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Browse" }));
    await screen.findByText("2.1.0");
    await userEvent.click(screen.getByRole("button", { name: "Manual install" }));

    const input = screen.getByPlaceholderText("LIVE") as HTMLInputElement;
    const listId = input.getAttribute("list");
    // eslint-disable-next-line testing-library/no-node-access
    const datalist = document.getElementById(listId!) as HTMLDataListElement;
    const options = Array.from(datalist.options).map((o) => o.value);
    expect(options).toEqual(expect.arrayContaining(["LIVE", "ZIntegration"]));
  });

  it("keeps the field a plain editable input — typing an unlisted channel still works", async () => {
    await openManualHelper();

    const input = screen.getByPlaceholderText("LIVE") as HTMLInputElement;
    expect(input.tagName).toBe("INPUT");
    await userEvent.clear(input);
    await userEvent.type(input, "SomeBrandNewChannel");
    expect(input.value).toBe("SomeBrandNewChannel");
  });

  async function openManualHelper() {
    renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Manual install" }));
  }
});
