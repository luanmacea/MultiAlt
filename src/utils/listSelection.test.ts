import { describe, expect, it } from "vitest";
import {
  applyToList,
  applyToSet,
  planClick,
  planDrag,
  rangeBetween,
  startsOnOtherControl,
} from "./listSelection";

const order = [10, 20, 30, 40, 50];

describe("rangeBetween", () => {
  it("includes both ends, in screen order, whichever way you go", () => {
    expect(rangeBetween(order, 20, 40)).toEqual([20, 30, 40]);
    expect(rangeBetween(order, 40, 20)).toEqual([20, 30, 40]);
    expect(rangeBetween(order, 30, 30)).toEqual([30]);
  });

  it("is empty when an end is not on screen", () => {
    expect(rangeBetween(order, 20, 99)).toEqual([]);
    expect(rangeBetween(order, 99, 20)).toEqual([]);
  });
});

describe("planClick", () => {
  const none = () => false;

  it("toggles only the clicked row without Shift", () => {
    expect(planClick(order, 10, 30, none)).toEqual({ keys: [30], on: true });
    expect(planClick(order, 10, 30, (k) => k === 30)).toEqual({ keys: [30], on: false });
  });

  it("applies the clicked row's new state to the range from the last click with Shift", () => {
    expect(planClick(order, 10, 40, none, { shift: true })).toEqual({ keys: [10, 20, 30, 40], on: true });
    expect(planClick(order, 40, 20, (k) => k === 20, { shift: true })).toEqual({ keys: [20, 30, 40], on: false });
  });

  it("falls back to one row when there is no usable anchor", () => {
    expect(planClick(order, null, 30, none, { shift: true })).toEqual({ keys: [30], on: true });
    expect(planClick(order, 99, 30, none, { shift: true })).toEqual({ keys: [30], on: true });
    expect(planClick(order, 30, 30, none, { shift: true })).toEqual({ keys: [30], on: true });
  });

  it("skips locked rows inside a range and ignores a click on a locked row", () => {
    const locked = (k: number) => k === 30;
    expect(planClick(order, 10, 50, none, { shift: true, isDisabled: locked })).toEqual({
      keys: [10, 20, 40, 50],
      on: true,
    });
    expect(planClick(order, 10, 30, none, { isDisabled: locked })).toBeNull();
  });
});

describe("planDrag", () => {
  it("returns every row between where the drag started and the pointer, once", () => {
    const applied = new Set<number>();
    const first = planDrag(order, 20, 30, applied);
    expect(first).toEqual([20, 30]);
    first.forEach((k) => applied.add(k));
    // Mouse moved fast over two rows at once: none is skipped.
    expect(planDrag(order, 20, 50, applied)).toEqual([40, 50]);
  });

  it("does not undo rows when the pointer comes back", () => {
    const applied = new Set([20, 30, 40]);
    expect(planDrag(order, 20, 30, applied)).toEqual([]);
  });

  it("works upwards and skips locked rows", () => {
    expect(planDrag(order, 40, 20, new Set(), (k) => k === 30)).toEqual([20, 40]);
  });
});

describe("applyToList / applyToSet", () => {
  it("adds without repeating and removes without touching the rest", () => {
    expect(applyToList([1, 2], [2, 3, 4], true)).toEqual([1, 2, 3, 4]);
    expect(applyToList([1, 2, 3], [2, 9], false)).toEqual([1, 3]);
  });

  it("does not change the original set", () => {
    const original = new Set([1, 2]);
    const next = applyToSet(original, [2, 3], true);
    expect([...next]).toEqual([1, 2, 3]);
    expect([...original]).toEqual([1, 2]);
  });
});

describe("startsOnOtherControl", () => {
  it("lets the drag start on the row itself, on a checkbox and on plain text", () => {
    document.body.innerHTML = `
      <li id="row"><span id="text">name</span>
        <button id="check" role="checkbox"></button>
        <input id="native" type="checkbox" />
        <button id="action">Focus</button></li>`;
    const row = document.getElementById("row")!;
    const pick = (id: string) => document.getElementById(id);
    expect(startsOnOtherControl(row, row)).toBe(false);
    expect(startsOnOtherControl(pick("text"), row)).toBe(false);
    expect(startsOnOtherControl(pick("check"), row)).toBe(false);
    expect(startsOnOtherControl(pick("native"), row)).toBe(false);
    expect(startsOnOtherControl(pick("action"), row)).toBe(true);
  });

  it("lets a drag start on the row when the row is itself a button", () => {
    document.body.innerHTML = `<button id="row"><span id="inner">x</span></button>`;
    const row = document.getElementById("row")!;
    expect(startsOnOtherControl(document.getElementById("inner"), row)).toBe(false);
  });
});
