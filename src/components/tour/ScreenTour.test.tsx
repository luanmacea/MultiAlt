import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

vi.mock("../../store", async () => (await import("../../test-utils/renderWithStore")).storeModuleMock());

import { ScreenTour, ScreenTourHost } from "./ScreenTour";
import { TourButton } from "./TourButton";
import type { TourDefinition } from "./tours";
import { TOURS_SEEN_KEY, closeTour, getActiveTour, isTourSeen, resetTourStateForTests, startTour } from "./tourState";
import { setStore } from "../../test-utils/renderWithStore";

const DEMO: TourDefinition = {
  id: "session",
  label: "Session",
  root: "[data-tour='demo-root']",
  steps: [
    { id: "one", title: "First thing", description: "Look at the first thing.", targets: ["[data-tour='a']"] },
    { id: "two", title: "Missing thing", description: "This one is not here.", targets: ["[data-tour='nope']"] },
    {
      id: "three",
      title: "Other tab",
      description: "Shown after the tab opens.",
      reveal: "[data-tour='tab-b']",
      targets: ["[data-tour='panel-b']"],
    },
  ],
};

/** Uma "tela" com duas abas: a segunda só existe depois do clique. */
function DemoScreen() {
  const [tab, setTab] = useState<"a" | "b">("a");
  return (
    <div data-tour="demo-root">
      <button data-tour="a">Thing A</button>
      <button data-tour="tab-b" onClick={() => setTab("b")}>
        Tab B
      </button>
      {tab === "b" ? <div data-tour="panel-b">Panel B</div> : null}
    </div>
  );
}

function renderDemo(onClose = vi.fn()) {
  render(
    <>
      <DemoScreen />
      <ScreenTour tour={DEMO} onClose={onClose} />
    </>
  );
  return onClose;
}

function panel() {
  return screen.getByRole("dialog");
}

beforeEach(() => {
  localStorage.clear();
  resetTourStateForTests();
  setStore({});
});

afterEach(() => {
  cleanup();
  closeTour();
});

describe("ScreenTour — navigation", () => {
  it("starts on the first step with a counter and Back disabled", () => {
    renderDemo();
    expect(screen.getByRole("heading", { name: "First thing" })).toBeInTheDocument();
    expect(screen.getByText("Step 1 of 3")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Back/ })).toBeDisabled();
    expect(panel()).toHaveAttribute("data-target-found", "true");
  });

  it("moves with Next and Back", async () => {
    renderDemo();
    await userEvent.click(screen.getByRole("button", { name: /Next/ }));
    expect(screen.getByRole("heading", { name: "Missing thing" })).toBeInTheDocument();
    expect(screen.getByText("Step 2 of 3")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Back/ }));
    expect(screen.getByRole("heading", { name: "First thing" })).toBeInTheDocument();
  });

  it("moves with the arrow keys", async () => {
    renderDemo();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("heading", { name: "Missing thing" })).toBeInTheDocument();
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.getByRole("heading", { name: "First thing" })).toBeInTheDocument();
  });

  it("finishes on the last step with Done", async () => {
    const onClose = renderDemo();
    await userEvent.click(screen.getByRole("button", { name: /Next/ }));
    await userEvent.click(screen.getByRole("button", { name: /Next/ }));
    expect(screen.getByText("Step 3 of 3")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes from the X button", async () => {
    const onClose = renderDemo();
    await userEvent.click(screen.getByRole("button", { name: "Close tutorial" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes with Escape", async () => {
    const onClose = renderDemo();
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe("ScreenTour — anchors", () => {
  it("shows a step whose anchor is missing in the middle, without crashing", async () => {
    renderDemo();
    await userEvent.click(screen.getByRole("button", { name: /Next/ }));
    expect(panel()).toHaveAttribute("data-target-found", "false");
    expect(screen.getByText("This part is not on screen right now.")).toBeInTheDocument();
    expect(document.querySelector(".walkthrough-focus-ring")).toBeNull();
  });

  it("opens the tab a step needs before pointing at it", async () => {
    renderDemo();
    expect(screen.queryByText("Panel B")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Next/ }));
    await userEvent.click(screen.getByRole("button", { name: /Next/ }));
    expect(await screen.findByText("Panel B")).toBeInTheDocument();
    await waitFor(() => expect(panel()).toHaveAttribute("data-target-found", "true"));
  });

  /**
   * Achado no harness (760x600): a lista de contas do AFK ficava abaixo da
   * dobra e o passo dela apontava para fora da tela — o tutorial rolava até o
   * alvo do passo anterior, que ainda estava no estado na troca de passo.
   */
  it("scrolls the new step's part into view, not the previous one", async () => {
    const calls: Element[] = [];
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function (this: Element) {
      calls.push(this);
    };
    try {
      const twoSteps: TourDefinition = {
        ...DEMO,
        steps: [
          { id: "a", title: "A", description: "a", targets: ["[data-tour='a']"] },
          { id: "b", title: "B", description: "b", targets: ["[data-tour='tab-b']"] },
        ],
      };
      render(
        <>
          <DemoScreen />
          <ScreenTour tour={twoSteps} onClose={vi.fn()} />
        </>
      );
      await userEvent.click(screen.getByRole("button", { name: /Next/ }));
      await waitFor(() => expect(calls[calls.length - 1]).toBe(screen.getByText("Tab B")));
      expect(calls.filter((el) => el === screen.getByText("Thing A"))).toHaveLength(1);
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });

  it("closes itself when its screen goes away", async () => {
    vi.useFakeTimers();
    try {
      const onClose = vi.fn();
      const { rerender } = render(
        <>
          <DemoScreen />
          <ScreenTour tour={DEMO} onClose={onClose} />
        </>
      );
      rerender(<ScreenTour tour={DEMO} onClose={onClose} />);
      act(() => {
        vi.advanceTimersByTime(500);
      });
      expect(onClose).toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("Tutorial button and host", () => {
  it("shows a 'new' dot until the tour is opened once, and opens it", async () => {
    render(
      <>
        <DemoScreen />
        <TourButton tour="session" />
        <ScreenTourHost tours={{ session: DEMO }} />
      </>
    );
    const button = screen.getByRole("button", { name: /Tutorial/ });
    expect(screen.getByTestId("tour-new-dot")).toBeInTheDocument();
    await userEvent.click(button);
    expect(getActiveTour()).toBe("session");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.queryByTestId("tour-new-dot")).not.toBeInTheDocument();
    expect(isTourSeen("session")).toBe(true);
    expect(JSON.parse(localStorage.getItem(TOURS_SEEN_KEY) ?? "[]")).toContain("session");

    await userEvent.click(screen.getByRole("button", { name: "Close tutorial" }));
    expect(getActiveTour()).toBeNull();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("never opens on its own", () => {
    render(
      <>
        <DemoScreen />
        <TourButton tour="session" />
        <ScreenTourHost tours={{ session: DEMO }} />
      </>
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("remembers seen tours across app restarts", () => {
    startTour("theme");
    closeTour();
    resetTourStateForTests();
    expect(isTourSeen("theme")).toBe(true);
    expect(isTourSeen("avatars")).toBe(false);
  });

  it("keeps working when storage is blocked", async () => {
    const getItem = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const setItem = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    try {
      resetTourStateForTests();
      render(
        <>
          <DemoScreen />
          <TourButton tour="session" />
          <ScreenTourHost tours={{ session: DEMO }} />
        </>
      );
      fireEvent.click(screen.getByRole("button", { name: /Tutorial/ }));
      expect(screen.getByRole("dialog")).toBeInTheDocument();
      expect(isTourSeen("session")).toBe(true);
    } finally {
      getItem.mockRestore();
      setItem.mockRestore();
    }
  });

  it("closes a screen tour when the intro walkthrough opens", () => {
    startTour("session");
    setStore({ firstRunWalkthroughOpen: true });
    render(
      <>
        <DemoScreen />
        <ScreenTourHost tours={{ session: DEMO }} />
      </>
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(getActiveTour()).toBeNull();
  });
});
