import { act, cleanup, fireEvent, renderHook, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createElement, type ReactNode } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { PromptProvider, useConfirm, useConfirmWithOptOut, usePrompt } from "./usePrompt";

type Dialogs = {
  prompt: ReturnType<typeof usePrompt>;
  confirm: ReturnType<typeof useConfirm>;
  confirmWithOptOut: ReturnType<typeof useConfirmWithOptOut>;
};

function useDialogs(): Dialogs {
  return {
    prompt: usePrompt(),
    confirm: useConfirm(),
    confirmWithOptOut: useConfirmWithOptOut(),
  };
}

function renderDialogs() {
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(PromptProvider, null, children);
  return renderHook(() => useDialogs(), { wrapper });
}

afterEach(() => {
  cleanup();
});

function overlay() {
  const node = document.querySelector(".fixed.inset-0");
  if (!node) throw new Error("no dialog overlay rendered");
  return node;
}

describe("dialog hooks without a provider", () => {
  it("resolve to the inert defaults", async () => {
    const { result } = renderHook(() => useDialogs());

    await expect(result.current.prompt("anything")).resolves.toBeNull();
    await expect(result.current.confirm("anything")).resolves.toBe(false);
    await expect(result.current.confirmWithOptOut("anything")).resolves.toEqual({
      confirmed: false,
      dontShowAgain: false,
    });
  });
});

describe("prompt", () => {
  it("shows the message, prefills the default value and resolves the typed text", async () => {
    const user = userEvent.setup();
    const { result } = renderDialogs();

    let resolved: string | null | undefined;
    act(() => {
      void result.current.prompt("Enter a name", "abc").then((v) => {
        resolved = v;
      });
    });

    expect(screen.getByText("Enter a name")).toBeTruthy();
    const input = screen.getByRole("textbox") as HTMLInputElement;
    expect(input.value).toBe("abc");

    await user.clear(input);
    await user.type(input, "xyz");
    await user.click(screen.getByText("OK"));

    await waitFor(() => expect(resolved).toBe("xyz"));
    expect(document.querySelector(".fixed.inset-0")).toBeNull();
  });

  it("resolves null when cancelled", async () => {
    const user = userEvent.setup();
    const { result } = renderDialogs();

    let resolved: string | null | undefined = "untouched";
    act(() => {
      void result.current.prompt("Enter a name").then((v) => {
        resolved = v;
      });
    });

    await user.click(screen.getByText("Cancel"));
    await waitFor(() => expect(resolved).toBeNull());
  });

  it("submits on Enter and cancels on Escape", async () => {
    const { result } = renderDialogs();

    let resolved: string | null | undefined;
    act(() => {
      void result.current.prompt("Enter a name", "hello").then((v) => {
        resolved = v;
      });
    });

    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    await waitFor(() => expect(resolved).toBe("hello"));

    resolved = undefined;
    act(() => {
      void result.current.prompt("Again", "x").then((v) => {
        resolved = v;
      });
    });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Escape" });
    await waitFor(() => expect(resolved).toBeNull());
  });

  it("cancels when the backdrop is clicked", async () => {
    const { result } = renderDialogs();

    let resolved: string | null | undefined = "untouched";
    act(() => {
      void result.current.prompt("Enter a name", "v").then((v) => {
        resolved = v;
      });
    });

    fireEvent.click(overlay());
    await waitFor(() => expect(resolved).toBeNull());
  });

  it("ignores clicks inside the dialog body", async () => {
    const { result } = renderDialogs();

    let resolved: string | null | undefined;
    act(() => {
      void result.current.prompt("Enter a name", "v").then((v) => {
        resolved = v;
      });
    });

    fireEvent.click(screen.getByText("Enter a name"));
    await new Promise((r) => setTimeout(r, 150));
    expect(resolved).toBeUndefined();
    expect(document.querySelector(".fixed.inset-0")).not.toBeNull();
  });
});

describe("confirm", () => {
  it("resolves true on confirm and false on cancel", async () => {
    const user = userEvent.setup();
    const { result } = renderDialogs();

    let resolved: boolean | undefined;
    act(() => {
      void result.current.confirm("Delete it?").then((v) => {
        resolved = v;
      });
    });

    expect(screen.getByText("Delete it?")).toBeTruthy();
    await user.click(screen.getByText("Confirm"));
    await waitFor(() => expect(resolved).toBe(true));

    resolved = undefined;
    act(() => {
      void result.current.confirm("Delete it?").then((v) => {
        resolved = v;
      });
    });
    await user.click(screen.getByText("Cancel"));
    await waitFor(() => expect(resolved).toBe(false));
  });

  it("styles a destructive confirm in red", async () => {
    const { result } = renderDialogs();
    act(() => {
      void result.current.confirm("Delete it?", true);
    });

    expect(screen.getByText("Confirm").className).toContain("bg-red-600");
  });

  it("confirms on Enter and cancels on Escape", async () => {
    const { result } = renderDialogs();

    let resolved: boolean | undefined;
    act(() => {
      void result.current.confirm("Sure?").then((v) => {
        resolved = v;
      });
    });

    fireEvent.keyDown(screen.getByText("Confirm"), { key: "Enter" });
    await waitFor(() => expect(resolved).toBe(true));

    resolved = undefined;
    act(() => {
      void result.current.confirm("Sure?").then((v) => {
        resolved = v;
      });
    });
    fireEvent.keyDown(screen.getByText("Confirm"), { key: "Escape" });
    await waitFor(() => expect(resolved).toBe(false));
  });
});

describe("confirmWithOptOut", () => {
  it("uses the default labels", async () => {
    const { result } = renderDialogs();
    act(() => {
      void result.current.confirmWithOptOut("Join anyway?");
    });

    expect(screen.getByText("Join anyway?")).toBeTruthy();
    expect(screen.getByText("Don't show this again")).toBeTruthy();
    expect(screen.getByText("Confirm")).toBeTruthy();
    expect(screen.getByText("Cancel")).toBeTruthy();
  });

  it("renders the custom labels", async () => {
    const { result } = renderDialogs();
    act(() => {
      void result.current.confirmWithOptOut("Join anyway?", {
        confirmLabel: "Join Anyway",
        cancelLabel: "Stop",
        optOutLabel: "Never warn me",
        destructive: true,
      });
    });

    expect(screen.getByText("Join Anyway").className).toContain("bg-red-600");
    expect(screen.getByText("Stop")).toBeTruthy();
    expect(screen.getByText("Never warn me")).toBeTruthy();
  });

  it("reports the opt-out only when the user confirms", async () => {
    const user = userEvent.setup();
    const { result } = renderDialogs();

    let resolved: { confirmed: boolean; dontShowAgain: boolean } | undefined;
    act(() => {
      void result.current.confirmWithOptOut("Join anyway?").then((v) => {
        resolved = v;
      });
    });

    await user.click(screen.getByRole("checkbox"));
    await user.click(screen.getByText("Confirm"));
    await waitFor(() => expect(resolved).toEqual({ confirmed: true, dontShowAgain: true }));

    resolved = undefined;
    act(() => {
      void result.current.confirmWithOptOut("Join anyway?").then((v) => {
        resolved = v;
      });
    });
    await user.click(screen.getByRole("checkbox"));
    await user.click(screen.getByText("Cancel"));
    await waitFor(() => expect(resolved).toEqual({ confirmed: false, dontShowAgain: false }));
  });

  it("resets the checkbox for the next dialog", async () => {
    const user = userEvent.setup();
    const { result } = renderDialogs();

    act(() => {
      void result.current.confirmWithOptOut("First?");
    });
    await user.click(screen.getByRole("checkbox"));
    expect((screen.getByRole("checkbox") as HTMLInputElement).checked).toBe(true);
    await user.click(screen.getByText("Confirm"));

    await waitFor(() => expect(document.querySelector(".fixed.inset-0")).toBeNull());
    act(() => {
      void result.current.confirmWithOptOut("Second?");
    });
    expect((screen.getByRole("checkbox") as HTMLInputElement).checked).toBe(false);
  });

  it("confirms on Enter and cancels on Escape", async () => {
    const { result } = renderDialogs();

    let resolved: { confirmed: boolean; dontShowAgain: boolean } | undefined;
    act(() => {
      void result.current.confirmWithOptOut("Join?").then((v) => {
        resolved = v;
      });
    });

    fireEvent.keyDown(screen.getByText("Confirm"), { key: "Escape" });
    await waitFor(() => expect(resolved).toEqual({ confirmed: false, dontShowAgain: false }));
  });
});
