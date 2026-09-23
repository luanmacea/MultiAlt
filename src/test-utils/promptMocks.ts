/**
 * Doubles for the async `prompt` / `confirm` dialogs in `hooks/usePrompt`.
 *
 * The real provider renders a modal and resolves a promise when the user
 * answers; tests just program the answer:
 *
 * ```ts
 * vi.mock("../../hooks/usePrompt", async () =>
 *   (await import("../../test-utils/promptMocks")).promptModuleMock()
 * );
 * promptAnswers.prompt = "REMOVE";
 * ```
 */
import { vi } from "vitest";
import type { ReactNode } from "react";

export const promptAnswers = {
  /** What `prompt()` resolves to. `null` means the user cancelled. */
  prompt: null as string | null,
  /** What `confirm()` resolves to. */
  confirm: false,
  /** What `confirmWithOptOut()` resolves to. */
  confirmWithOptOut: { confirmed: false, dontShowAgain: false },
};

export const promptMock = vi.fn(async (_message: string, _defaultValue?: string) => promptAnswers.prompt);
export const confirmMock = vi.fn(async (_message: string, _destructive?: boolean) => promptAnswers.confirm);
export const confirmWithOptOutMock = vi.fn(
  async (_message: string, _options?: unknown) => promptAnswers.confirmWithOptOut
);

export function resetPromptMocks(): void {
  promptAnswers.prompt = null;
  promptAnswers.confirm = false;
  promptAnswers.confirmWithOptOut = { confirmed: false, dontShowAgain: false };
  promptMock.mockClear();
  confirmMock.mockClear();
  confirmWithOptOutMock.mockClear();
}

export function promptModuleMock() {
  return {
    usePrompt: () => promptMock,
    useConfirm: () => confirmMock,
    useConfirmWithOptOut: () => confirmWithOptOutMock,
    PromptProvider: ({ children }: { children: ReactNode }) => children,
  };
}
