import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect } from "react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { LaunchTab } from "./LaunchTab";
import { useSettings } from "../../hooks/useSettings";
import { invokeMock, resetTauriMocks, setInvokeHandler } from "../../test-utils/tauriMocks";

let stored: Record<string, Record<string, string>> = {};
let userAgent: { mockRestore: () => void } | null = null;

function Harness() {
  const s = useSettings();
  useEffect(() => {
    void s.load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  if (!s.loaded) return <div>Loading settings...</div>;
  return <LaunchTab s={s} />;
}

beforeEach(() => {
  resetTauriMocks();
  stored = {};
  setInvokeHandler((cmd) => {
    if (cmd === "get_all_settings") return stored;
    return undefined;
  });
  userAgent = vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Mozilla/5.0 (Windows NT 10.0; Win64; x64)");
});
afterEach(() => {
  cleanup();
  userAgent?.mockRestore();
});

/**
 * Pedido do Discord (05/11/2026): abrir as contas com a build do Voidstrap /
 * Froststrap (mods e FastFlags deles). `General.RobloxBuildSource`.
 */
describe("Settings › Launch › Roblox build", () => {
  it("defaults to the app's own build and saves a chosen bootstrapper", async () => {
    render(<Harness />);
    const select = await screen.findByRole("button", { name: "Roblox build" });
    expect(select).toHaveTextContent("Official (downloaded by the app)");
    expect(screen.queryByRole("textbox", { name: "Bootstrapper folder" })).not.toBeInTheDocument();

    await userEvent.click(select);
    await userEvent.click(screen.getByRole("button", { name: "Voidstrap" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_setting", {
        section: "General",
        key: "RobloxBuildSource",
        value: "Voidstrap",
      })
    );
  });

  it("asks for the folder only with a custom bootstrapper", async () => {
    stored = { General: { RobloxBuildSource: "custom" } };
    render(<Harness />);
    const field = await screen.findByRole("textbox", { name: "Bootstrapper folder" });
    await userEvent.type(field, "D:\\Strap");
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "update_setting",
        expect.objectContaining({ section: "General", key: "RobloxBuildSourcePath" })
      )
    );
  });
});
