import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => (await import("../../test-utils/tauriMocks")).tauriCoreMock());

import { FeedbackDialog } from "./FeedbackDialog";
import { useEscapeStack } from "../../hooks/useEscapeStack";
import { invokeMock, resetTauriMocks } from "../../test-utils/tauriMocks";

describe("FeedbackDialog", () => {
  beforeEach(() => resetTauriMocks());
  afterEach(() => cleanup());

  it("opens the bug form by kind only, never by URL, and closes", async () => {
    const onClose = vi.fn();
    render(<FeedbackDialog open onClose={onClose} />);

    await userEvent.click(screen.getByRole("button", { name: /Report a problem/ }));

    expect(invokeMock).toHaveBeenCalledWith("open_feedback_form", { kind: "bug" });
    expect(onClose).toHaveBeenCalled();
  });

  it("opens the idea form", async () => {
    render(<FeedbackDialog open onClose={() => {}} />);

    await userEvent.click(screen.getByRole("button", { name: /Suggest an idea/ }));

    expect(invokeMock).toHaveBeenCalledWith("open_feedback_form", { kind: "idea" });
  });

  it("says the app sends nothing on its own and a GitHub account is needed", () => {
    render(<FeedbackDialog open onClose={() => {}} />);

    const dialog = screen.getByRole("dialog", { name: "Send feedback" });
    expect(dialog).toHaveTextContent("The app sends nothing on its own");
    expect(dialog).toHaveTextContent("free GitHub account");
  });

  it("closes with Escape", async () => {
    const onClose = vi.fn();
    render(<FeedbackDialog open onClose={onClose} />);

    await userEvent.keyboard("{Escape}");

    expect(onClose).toHaveBeenCalled();
  });

  it("Escape closes only the dialog, not the page behind it", async () => {
    // A página atrás (ex.: Grupos) também volta para a lista de contas com Esc;
    // com o diálogo aberto, o Esc é só dele (achado no teste real de 08/10/2026).
    const pageEscape = vi.fn();
    const onClose = vi.fn();
    function PageWithDialog({ open }: { open: boolean }) {
      useEscapeStack(true, pageEscape);
      return <FeedbackDialog open={open} onClose={onClose} />;
    }
    // A página já estava montada; o diálogo abre depois, como no app.
    const { rerender } = render(<PageWithDialog open={false} />);
    rerender(<PageWithDialog open />);

    await userEvent.keyboard("{Escape}");

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(pageEscape).not.toHaveBeenCalled();
  });

  it("renders nothing when closed", () => {
    render(<FeedbackDialog open={false} onClose={() => {}} />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
