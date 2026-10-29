import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { FreezeButton, FrozenChip, freezeRemaining, type FrozenClient } from "./FreezeControl";

afterEach(cleanup);

const frozen = (ids: number[], untilMs: number | null = null) =>
  new Map<number, FrozenClient>(ids.map((id) => [id, { userId: id, sinceMs: 0, untilMs }]));

describe("freezeRemaining", () => {
  it("counts down in seconds, then minutes, and is empty until unfrozen", () => {
    expect(freezeRemaining(null, 0)).toBe("");
    expect(freezeRemaining(12_000, 0)).toBe("12s");
    expect(freezeRemaining(245_000, 0)).toBe("4m 05s");
    expect(freezeRemaining(1_000, 5_000)).toBe("0s");
  });
});

describe("FreezeButton", () => {
  it("offers how long to freeze the target accounts", async () => {
    const onFreeze = vi.fn();
    render(<FreezeButton targets={[1, 2]} frozen={frozen([1])} onFreeze={onFreeze} onUnfreeze={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: /Freeze/ }));
    expect(screen.getByText("Until I unfreeze")).toBeInTheDocument();
    await userEvent.click(screen.getByText("30 seconds"));
    expect(onFreeze).toHaveBeenCalledWith(30);
  });

  it("turns into Unfreeze when every target is already frozen", async () => {
    const onUnfreeze = vi.fn();
    render(<FreezeButton targets={[1, 2]} frozen={frozen([1, 2])} onFreeze={vi.fn()} onUnfreeze={onUnfreeze} />);
    await userEvent.click(screen.getByRole("button", { name: "Unfreeze (2)" }));
    expect(onUnfreeze).toHaveBeenCalledOnce();
  });

  it("has nothing to freeze with no account in game", () => {
    render(<FreezeButton targets={[]} frozen={frozen([])} onFreeze={vi.fn()} onUnfreeze={vi.fn()} />);
    expect(screen.getByTestId("freeze-button")).toHaveAttribute("aria-disabled", "true");
  });
});

describe("FrozenChip", () => {
  it("shows the time left and unfreezes on click", async () => {
    const onUnfreeze = vi.fn();
    render(<FrozenChip name="alpha" entry={{ userId: 1, sinceMs: 0, untilMs: 9_000 }} now={0} onUnfreeze={onUnfreeze} />);
    await userEvent.click(screen.getByRole("button", { name: "Frozen · 9s" }));
    expect(onUnfreeze).toHaveBeenCalledOnce();
  });
});
