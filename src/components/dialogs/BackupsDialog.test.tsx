import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { BackupsDialog } from "./BackupsDialog";
import { setStore } from "../../test-utils/renderWithStore";
import {
  confirmMock,
  promptAnswers,
  promptMock,
  resetPromptMocks,
} from "../../test-utils/promptMocks";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";
import type { BackupEntry, BackupsInfo, RestoreReport } from "../../types";

const HOUR = 3600 * 1000;

function makeBackup(overrides: Partial<BackupEntry> = {}): BackupEntry {
  return {
    id: "b1",
    fileName: "backup-2026-09-24-100000.zip",
    createdAt: new Date(Date.now() - 3 * HOUR).toISOString(),
    label: "Before the move",
    sizeBytes: 1536 * 1024,
    files: ["AccountData.json", "RAMSettings.ini"],
    valid: true,
    ...overrides,
  };
}

const OLDER = makeBackup({
  id: "b0",
  fileName: "backup-2026-09-20-080000.zip",
  label: "Old one",
  createdAt: new Date(Date.now() - 5 * 24 * HOUR).toISOString(),
  sizeBytes: 400 * 1024,
  files: ["AccountData.json"],
});

const NEWER = makeBackup();

const INFO: BackupsInfo = {
  dir: "C:/Users/tester/AppData/Local/Roblox Account Manager",
  portable: false,
  totalBytes: 2 * 1024 * 1024,
  count: 2,
};

const REPORT: RestoreReport = {
  backupId: "b1",
  safetyBackupId: "safety-77",
  restored: ["AccountData.json", "RAMSettings.ini"],
  skipped: [],
  accountsReloaded: true,
  requiresRestart: true,
  restartReasons: ["RAMSettings.ini is only read at startup"],
};

type Entry = unknown | ((args?: unknown) => unknown);

/** Routes the dialog's IPC; `overrides` wins per command. */
function backend(overrides: Record<string, Entry> = {}) {
  setInvokeHandler((cmd, args) => {
    if (cmd in overrides) {
      const entry = overrides[cmd];
      return typeof entry === "function" ? (entry as (a?: unknown) => unknown)(args) : entry;
    }
    switch (cmd) {
      case "list_backups":
        return [NEWER, OLDER];
      case "backups_info":
        return INFO;
      case "create_backup":
        return NEWER;
      case "restore_backup":
        return REPORT;
      case "delete_backup":
        return true;
      default:
        return undefined;
    }
  });
}

function renderDialog() {
  const store = setStore({});
  const onClose = vi.fn();
  const view = render(<BackupsDialog open onClose={onClose} />);
  return { store, onClose, ...view };
}

function rowFor(id: string) {
  return screen.getByTestId(`backup-${id}`);
}

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  backend();
});

afterEach(cleanup);

describe("BackupsDialog — listing", () => {
  it("renders nothing while closed", () => {
    setStore({});
    const { container } = render(<BackupsDialog open={false} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the data folder, the totals and every backup", async () => {
    renderDialog();

    expect(await screen.findByText(INFO.dir)).toBeInTheDocument();
    expect(screen.getByText(/2 backups/)).toBeInTheDocument();
    expect(screen.getByText("Before the move")).toBeInTheDocument();
    expect(screen.getByText("Old one")).toBeInTheDocument();
    expect(within(rowFor("b1")).getByText("1.5 MB")).toBeInTheDocument();
    expect(within(rowFor("b1")).getByText("2 files")).toBeInTheDocument();
    expect(within(rowFor("b0")).getByText("400 KB")).toBeInTheDocument();
  });

  it("lists the newest backup first even when the backend does not sort", async () => {
    backend({ list_backups: [OLDER, NEWER] });
    const { container } = renderDialog();

    await screen.findByText("Before the move");
    const ids = [...container.querySelectorAll("[data-testid^='backup-']")].map((el) =>
      el.getAttribute("data-testid")
    );
    expect(ids).toEqual(["backup-b1", "backup-b0"]);
  });

  it("shows the relative age with the absolute date in the tooltip", async () => {
    renderDialog();
    const row = await screen.findByTestId("backup-b1");
    const age = within(row).getByText("3h ago");
    expect(age.parentElement).toHaveAttribute(
      "title",
      new Date(NEWER.createdAt).toLocaleString()
    );
  });

  it("falls back to the file name for an unlabelled backup and tags automatic ones", async () => {
    backend({
      list_backups: [makeBackup({ id: "auto", label: null, automatic: true })],
    });
    renderDialog();

    const row = await screen.findByTestId("backup-auto");
    expect(within(row).getAllByText(NEWER.fileName).length).toBeGreaterThan(0);
    expect(within(row).getByText("Automatic")).toBeInTheDocument();
  });

  it("explains the empty state", async () => {
    backend({ list_backups: [], backups_info: { ...INFO, count: 0, totalBytes: 0 } });
    renderDialog();
    expect(
      await screen.findByText(
        "No backups yet. Create one before moving the app or editing your accounts."
      )
    ).toBeInTheDocument();
  });

  it("warns about portable mode", async () => {
    backend({ backups_info: { ...INFO, portable: true } });
    renderDialog();
    expect(await screen.findByText(/Portable mode/)).toBeInTheDocument();
  });

  it("hides the portable warning when the data lives in the user profile", async () => {
    renderDialog();
    await screen.findByText(INFO.dir);
    expect(screen.queryByText(/Portable mode/)).not.toBeInTheDocument();
  });

  it("surfaces a backend failure in the dialog", async () => {
    backend({
      list_backups: () => {
        throw new Error("backups folder is read-only");
      },
    });
    renderDialog();
    expect(await screen.findByRole("alert")).toHaveTextContent("backups folder is read-only");
  });

  it("opens the backups folder", async () => {
    renderDialog();
    await screen.findByText(INFO.dir);

    await userEvent.click(screen.getByRole("button", { name: "Open folder" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("open_backups_folder"));
  });
});

/**
 * A8 do checkup. O dono decidiu manter a chave (`AccountData.key`) dentro do zip
 * — sem ela o backup nao restaura — com um aviso de usar senha para quem guarda
 * backup em nuvem. O aviso so existia na tela de configurar criptografia, que so
 * abre sozinha com zero contas: quem ja tem contas nunca o via. Ele tem que
 * estar onde o backup e criado.
 */
describe("BackupsDialog — the zip carries the key", () => {
  it("warns, where backups are created, that without a password the zip carries the key", async () => {
    setStore({ accountsEncrypted: false });
    render(<BackupsDialog open onClose={vi.fn()} />);

    const warning = await screen.findByText(/carries the key that opens your accounts/i);
    expect(warning).toHaveTextContent(/OneDrive/);
    expect(warning).toHaveTextContent(/set a password/i);
    expect(warning).toHaveTextContent(/Change Encryption Method/);
  });

  it("does not show it with a password: then the zip holds no key", async () => {
    setStore({ accountsEncrypted: true });
    render(<BackupsDialog open onClose={vi.fn()} />);

    await screen.findByText(INFO.dir);
    expect(screen.queryByText(/carries the key that opens your accounts/i)).not.toBeInTheDocument();
  });
});

describe("BackupsDialog — creating", () => {
  it("creates a labelled backup and reloads the list", async () => {
    let listCalls = 0;
    backend({
      list_backups: () => {
        listCalls += 1;
        return listCalls === 1 ? [OLDER] : [NEWER, OLDER];
      },
    });
    promptAnswers.prompt = "Before the move";
    renderDialog();
    await screen.findByText("Old one");

    await userEvent.click(screen.getByRole("button", { name: "Create backup" }));

    expect(promptMock).toHaveBeenCalled();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("create_backup", { label: "Before the move" })
    );
    expect(await screen.findByText("Before the move")).toBeInTheDocument();
  });

  it("creates an unlabelled backup when the name is left empty", async () => {
    promptAnswers.prompt = "   ";
    renderDialog();
    await screen.findByText(INFO.dir);

    await userEvent.click(screen.getByRole("button", { name: "Create backup" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("create_backup", { label: null })
    );
  });

  it("creates nothing when the name prompt is cancelled", async () => {
    promptAnswers.prompt = null;
    renderDialog();
    await screen.findByText(INFO.dir);

    await userEvent.click(screen.getByRole("button", { name: "Create backup" }));
    await waitFor(() => expect(promptMock).toHaveBeenCalled());
    expect(invokeMock).not.toHaveBeenCalledWith("create_backup", expect.anything());
  });

  it("shows why the backup could not be created", async () => {
    backend({
      create_backup: () => {
        throw new Error("disk full");
      },
    });
    promptAnswers.prompt = "";
    renderDialog();
    await screen.findByText(INFO.dir);

    await userEvent.click(screen.getByRole("button", { name: "Create backup" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("disk full");
  });
});

describe("BackupsDialog — restoring", () => {
  it("asks for confirmation and does nothing when the user backs out", async () => {
    promptAnswers.confirm = false;
    renderDialog();
    await screen.findByText("Before the move");

    await userEvent.click(within(rowFor("b1")).getByRole("button", { name: "Restore" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    // A confirmação precisa avisar sobre a substituição e o backup de segurança.
    const message = String(confirmMock.mock.calls[0][0]);
    expect(message).toContain("will be replaced");
    expect(message).toContain("safety backup");
    expect(confirmMock.mock.calls[0][1]).toBe(true);
    expect(invokeMock).not.toHaveBeenCalledWith("restore_backup", expect.anything());
  });

  it("restores after confirmation and reports the restart requirement", async () => {
    promptAnswers.confirm = true;
    renderDialog();
    await screen.findByText("Before the move");

    await userEvent.click(within(rowFor("b1")).getByRole("button", { name: "Restore" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("restore_backup", { id: "b1" }));
    expect(await screen.findByText("Backup restored")).toBeInTheDocument();
    expect(screen.getByText(`Restored from ${NEWER.fileName}`)).toBeInTheDocument();
    expect(
      screen.getByText(/Safety backup of your previous data: safety-77/)
    ).toBeInTheDocument();
    expect(screen.getByText(/Replaced: AccountData.json, RAMSettings.ini/)).toBeInTheDocument();
    expect(screen.getByText("Restart required")).toBeInTheDocument();
    expect(
      screen.getByText(/RAMSettings.ini is only read at startup/)
    ).toBeInTheDocument();
  });

  it("omits the restart banner when the backend does not ask for one", async () => {
    promptAnswers.confirm = true;
    backend({
      restore_backup: { ...REPORT, requiresRestart: false, restartReasons: [] },
    });
    renderDialog();
    await screen.findByText("Before the move");

    await userEvent.click(within(rowFor("b1")).getByRole("button", { name: "Restore" }));

    expect(await screen.findByText("Backup restored")).toBeInTheDocument();
    expect(screen.queryByText("Restart required")).not.toBeInTheDocument();
    expect(
      screen.getByText("Your accounts were reloaded, no restart needed.")
    ).toBeInTheDocument();
  });

  it("warns when files were skipped or no safety backup was written", async () => {
    promptAnswers.confirm = true;
    backend({
      restore_backup: {
        ...REPORT,
        safetyBackupId: null,
        skipped: ["RAMScripts.json"],
      },
    });
    renderDialog();
    await screen.findByText("Before the move");

    await userEvent.click(within(rowFor("b1")).getByRole("button", { name: "Restore" }));

    expect(
      await screen.findByText("No safety backup could be created for the replaced data.")
    ).toBeInTheDocument();
    expect(screen.getByText(/Skipped: RAMScripts.json/)).toBeInTheDocument();
  });

  it("shows a failed restore instead of a success report", async () => {
    promptAnswers.confirm = true;
    backend({
      restore_backup: () => {
        throw new Error("archive is corrupt");
      },
    });
    renderDialog();
    await screen.findByText("Before the move");

    await userEvent.click(within(rowFor("b1")).getByRole("button", { name: "Restore" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("archive is corrupt");
    expect(screen.queryByText("Backup restored")).not.toBeInTheDocument();
  });

  it("refuses to restore an invalid backup but still allows deleting it", async () => {
    promptAnswers.confirm = true;
    backend({ list_backups: [makeBackup({ id: "bad", valid: false, label: "Truncated" })] });
    renderDialog();
    await screen.findByText("Truncated");

    const row = rowFor("bad");
    expect(within(row).getByText("Invalid")).toBeInTheDocument();
    const restore = within(row).getByRole("button", { name: "Restore" });
    expect(restore).toBeDisabled();

    await userEvent.click(restore);
    expect(invokeMock).not.toHaveBeenCalledWith("restore_backup", expect.anything());

    await userEvent.click(within(row).getByTitle("Delete this backup"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("delete_backup", { id: "bad" }));
  });
});

describe("BackupsDialog — deleting", () => {
  it("keeps the backup when the confirmation is declined", async () => {
    promptAnswers.confirm = false;
    renderDialog();
    await screen.findByText("Before the move");

    await userEvent.click(within(rowFor("b1")).getByTitle("Delete this backup"));

    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(String(confirmMock.mock.calls[0][0])).toContain("cannot be undone");
    expect(invokeMock).not.toHaveBeenCalledWith("delete_backup", expect.anything());
  });

  it("deletes after confirmation and reloads the list", async () => {
    promptAnswers.confirm = true;
    let listCalls = 0;
    backend({
      list_backups: () => {
        listCalls += 1;
        return listCalls === 1 ? [NEWER, OLDER] : [OLDER];
      },
    });
    renderDialog();
    await screen.findByText("Before the move");

    await userEvent.click(within(rowFor("b1")).getByTitle("Delete this backup"));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("delete_backup", { id: "b1" }));
    await waitFor(() => expect(screen.queryByText("Before the move")).not.toBeInTheDocument());
    expect(screen.getByText("Old one")).toBeInTheDocument();
  });
});
