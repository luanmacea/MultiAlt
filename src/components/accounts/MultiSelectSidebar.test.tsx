import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());
vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());
vi.mock("@tauri-apps/api/event", async () => (await import("../../test-utils/tauriMocks")).tauriEventMock());
vi.mock("../../hooks/usePrompt", async () => (await import("../../test-utils/promptMocks")).promptModuleMock());

import { MultiSelectSidebar } from "./MultiSelectSidebar";
import { defaultSettings, makeAccount, setStore } from "../../test-utils/renderWithStore";
import { invokeMock, resetTauriMocks } from "../../test-utils/tauriMocks";
import {
  confirmWithOptOutMock,
  promptAnswers,
  resetPromptMocks,
} from "../../test-utils/promptMocks";
import type { StoreValue } from "../../store";

const A = makeAccount({ UserID: 1, Username: "ann" });
const B = makeAccount({ UserID: 2, Username: "bob" });

const writeText = vi.fn(async () => {});

function renderSidebar(overrides: Partial<StoreValue> = {}, selected = [A, B]) {
  const store = setStore({
    accounts: [A, B],
    selectedIds: new Set(selected.map((a) => a.UserID)),
    selectedAccounts: selected,
    ...overrides,
  });
  render(<MultiSelectSidebar />);
  return store;
}

const copyButton = () => screen.getByRole("button", { name: /Copy All Cookies/ });

beforeEach(() => {
  resetTauriMocks();
  resetPromptMocks();
  writeText.mockClear();
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
});

afterEach(cleanup);

/**
 * O botão de cópia em massa da sidebar punha o `.ROBLOSECURITY` de toda a
 * seleção na área de transferência num clique, sem aviso nenhum.
 */
describe("MultiSelectSidebar — copiar cookies avisa antes", () => {
  it("asks before copying, naming the count and what a cookie hands over", async () => {
    renderSidebar();
    await userEvent.click(copyButton());
    await waitFor(() => expect(confirmWithOptOutMock).toHaveBeenCalledTimes(1));
    const [message] = confirmWithOptOutMock.mock.calls[0];
    expect(message).toContain("2 accounts");
    expect(message).toMatch(/2-step verification/);
    expect(writeText).not.toHaveBeenCalled();
  });

  it("copies the cookies one per line once the warning is accepted", async () => {
    promptAnswers.confirmWithOptOut = { confirmed: true, dontShowAgain: false };
    const store = renderSidebar();
    await userEvent.click(copyButton());
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("cookie-1\ncookie-2"));
    expect(store.addToast).toHaveBeenCalledWith(expect.stringContaining("2"));
  });

  it("remembers the opt-out in the settings", async () => {
    promptAnswers.confirmWithOptOut = { confirmed: true, dontShowAgain: true };
    renderSidebar();
    await userEvent.click(copyButton());
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_setting", {
        section: "General",
        key: "WarnOnCopyCredential",
        value: "false",
      })
    );
  });

  it("skips the warning once it has been turned off", async () => {
    const settings = defaultSettings();
    settings.General.WarnOnCopyCredential = "false";
    renderSidebar({ settings });
    await userEvent.click(copyButton());
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("cookie-1\ncookie-2"));
    expect(confirmWithOptOutMock).not.toHaveBeenCalled();
  });
});
