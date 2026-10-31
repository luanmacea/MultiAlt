import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useListSelection } from "./useListSelection";
import { applyToSet } from "../utils/listSelection";

afterEach(cleanup);

const IDS = [1, 2, 3, 4, 5];

function Harness({ locked = [], disabled = false }: { locked?: number[]; disabled?: boolean }) {
  const [on, setOn] = useState<Set<number>>(new Set());
  const sel = useListSelection<number>({
    order: IDS,
    isSelected: (id) => on.has(id),
    apply: (keys, state) => setOn((prev) => applyToSet(prev, keys, state)),
    isDisabled: (id) => locked.includes(id),
    disabled,
  });
  return (
    <ul>
      {IDS.map((id) => (
        <li key={id} data-testid={`row-${id}`} {...sel.rowProps(id)}>
          <button
            type="button"
            role="checkbox"
            aria-checked={on.has(id)}
            aria-label={`pick ${id}`}
            onClick={(e) => sel.toggle(id, e)}
          />
        </li>
      ))}
    </ul>
  );
}

const checked = () => IDS.filter((id) => screen.getByLabelText(`pick ${id}`).getAttribute("aria-checked") === "true");
const box = (id: number) => screen.getByLabelText(`pick ${id}`);
const row = (id: number) => screen.getByTestId(`row-${id}`);

describe("useListSelection", () => {
  it("a plain click toggles one row, and so does Ctrl/Cmd", () => {
    render(<Harness />);
    fireEvent.click(box(2));
    fireEvent.click(box(4), { ctrlKey: true });
    fireEvent.click(box(5), { metaKey: true });
    expect(checked()).toEqual([2, 4, 5]);
    fireEvent.click(box(4));
    expect(checked()).toEqual([2, 5]);
  });

  it("Shift+click applies the clicked row's new state to the whole range", () => {
    render(<Harness />);
    fireEvent.click(box(2));
    fireEvent.click(box(4), { shiftKey: true });
    expect(checked()).toEqual([2, 3, 4]);
    // Unticking with Shift clears the range too.
    fireEvent.click(box(2));
    fireEvent.click(box(4), { shiftKey: true });
    expect(checked()).toEqual([]);
  });

  it("Shift+click skips locked rows", () => {
    render(<Harness locked={[3]} />);
    fireEvent.click(box(1));
    fireEvent.click(box(5), { shiftKey: true });
    expect(checked()).toEqual([1, 2, 4, 5]);
  });

  it("dragging across rows applies the state to every row it passes", () => {
    render(<Harness />);
    fireEvent.mouseDown(box(2), { button: 0 });
    fireEvent.mouseMove(row(3));
    fireEvent.mouseMove(row(4));
    fireEvent.mouseUp(row(4));
    expect(checked()).toEqual([2, 3, 4]);
  });

  it("dragging from a ticked row unticks what it passes", () => {
    render(<Harness />);
    fireEvent.click(box(1));
    fireEvent.click(box(2), { shiftKey: true });
    fireEvent.click(box(3), { shiftKey: true });
    expect(checked()).toEqual([1, 2, 3]);
    fireEvent.mouseDown(box(3), { button: 0 });
    fireEvent.mouseMove(row(2));
    fireEvent.mouseMove(row(1));
    fireEvent.mouseUp(row(1));
    expect(checked()).toEqual([]);
  });

  it("a drag that ends on the row it started on does not toggle twice", () => {
    render(<Harness />);
    fireEvent.mouseDown(box(2), { button: 0 });
    fireEvent.mouseMove(row(3));
    fireEvent.mouseMove(row(2));
    fireEvent.mouseUp(box(2));
    // The click the browser sends after the mouse up is not a second toggle.
    fireEvent.click(box(2));
    expect(checked()).toEqual([2, 3]);
  });

  it("a click without moving is just a click", () => {
    render(<Harness />);
    fireEvent.mouseDown(box(3), { button: 0 });
    fireEvent.mouseUp(box(3));
    fireEvent.click(box(3));
    expect(checked()).toEqual([3]);
  });

  it("a drag skips locked rows", () => {
    render(<Harness locked={[3]} />);
    fireEvent.mouseDown(box(2), { button: 0 });
    fireEvent.mouseMove(row(4));
    fireEvent.mouseUp(row(4));
    expect(checked()).toEqual([2, 4]);
  });

  it("does nothing while the whole list is locked", () => {
    render(<Harness disabled />);
    fireEvent.click(box(2));
    fireEvent.mouseDown(box(1), { button: 0 });
    fireEvent.mouseMove(row(3));
    fireEvent.mouseUp(row(3));
    expect(checked()).toEqual([]);
  });

  it("only the primary button starts a drag", () => {
    render(<Harness />);
    fireEvent.mouseDown(box(1), { button: 2 });
    fireEvent.mouseMove(row(3));
    fireEvent.mouseUp(row(3));
    expect(checked()).toEqual([]);
  });
});
