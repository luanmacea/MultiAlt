import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect } from "react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());

import { VersionsTab } from "./VersionsTab";
import { useSettings } from "../../hooks/useSettings";
import { resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

const installed = [
  { channel: "LIVE", versionHash: "version-oldoldoldold0001", installPath: "C:\v1", displayVersion: "0.711.0", userLabel: null },
  { channel: "LIVE", versionHash: "version-newnewnewnew0002", installPath: "C:\v2", displayVersion: "0.712.1", userLabel: null },
];

function Harness() {
  const s = useSettings();
  useEffect(() => {
    void s.load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  if (!s.loaded) return <div>Loading settings...</div>;
  return <VersionsTab s={s} />;
}

function backend(check: () => unknown) {
  setInvokeHandler((cmd) => {
    if (cmd === "get_all_settings") return {};
    if (cmd === "versions_list_installed") return installed;
    if (cmd === "versions_check_outdated") return check();
    return undefined;
  });
}

beforeEach(resetTauriMocks);
afterEach(cleanup);

/**
 * Pedido do dono (02/11/2026): um botão para ver se as versões instaladas
 * ficaram para trás do Roblox, sem esperar o launch perguntar.
 */
describe("Settings › Versions › Check for updates", () => {
  it("marks each installed version as outdated or up to date", async () => {
    backend(() => [
      { versionId: "LIVE:version-oldoldoldold0001", current: "version-newnewnewnew0002", outdated: true },
      { versionId: "LIVE:version-newnewnewnew0002", current: "version-newnewnewnew0002", outdated: false },
    ]);
    render(<Harness />);
    await userEvent.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText(/Outdated\. The current build is/)).toBeInTheDocument();
    expect(screen.getByText("Up to date")).toBeInTheDocument();
  });

  it("says so when Roblox can't be reached, without calling anything outdated", async () => {
    backend(() => {
      throw new Error("offline");
    });
    render(<Harness />);
    await userEvent.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText(/Couldn't reach Roblox/)).toBeInTheDocument();
    expect(screen.queryByText(/Outdated/)).not.toBeInTheDocument();
  });

  it("shows 'Couldn't check' for a version whose build lookup had no answer", async () => {
    backend(() => [
      { versionId: "LIVE:version-oldoldoldold0001", current: null, outdated: null },
      { versionId: "LIVE:version-newnewnewnew0002", current: null, outdated: null },
    ]);
    render(<Harness />);
    await userEvent.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect((await screen.findAllByText("Couldn't check")).length).toBe(2);
  });
});
