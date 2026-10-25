import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { EditorView } from "@codemirror/view";
import { CodeEditor } from "./CodeEditor";

afterEach(cleanup);

function viewOf(container: HTMLElement): EditorView {
  const dom = container.querySelector<HTMLElement>(".cm-editor");
  const found = dom ? EditorView.findFromDOM(dom) : null;
  if (!found) throw new Error("no editor view");
  return found;
}

describe("CodeEditor", () => {
  it("shows the text with line numbers and labels the editing area", () => {
    const { container } = render(
      <CodeEditor docKey="a.py" value={"x = 1\ny = 2\n"} ariaLabel="Source of a.py" onChange={() => {}} onSave={() => {}} />
    );
    expect(screen.getByRole("textbox", { name: "Source of a.py" })).toBeInTheDocument();
    expect(container.querySelector(".cm-lineNumbers")).not.toBeNull();
    expect(viewOf(container).state.doc.toString()).toBe("x = 1\ny = 2\n");
  });

  it("reports what the user types, but not a change made by the parent", () => {
    const onChange = vi.fn();
    const { container, rerender } = render(
      <CodeEditor docKey="a.py" value="a" ariaLabel="Source" onChange={onChange} onSave={() => {}} />
    );
    viewOf(container).dispatch({ changes: { from: 1, insert: "b" }, userEvent: "input.type" });
    expect(onChange).toHaveBeenCalledWith("ab");

    onChange.mockClear();
    rerender(<CodeEditor docKey="a.py" value="from disk" ariaLabel="Source" onChange={onChange} onSave={() => {}} />);
    expect(viewOf(container).state.doc.toString()).toBe("from disk");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("keeps each file's own document and undo history when switching", () => {
    const { container, rerender } = render(
      <CodeEditor docKey="a.py" value="aaa" ariaLabel="Source" onChange={() => {}} onSave={() => {}} />
    );
    rerender(<CodeEditor docKey="b.py" value="bbb" ariaLabel="Source" onChange={() => {}} onSave={() => {}} />);
    expect(viewOf(container).state.doc.toString()).toBe("bbb");
    rerender(<CodeEditor docKey="a.py" value="aaa" ariaLabel="Source" onChange={() => {}} onSave={() => {}} />);
    expect(viewOf(container).state.doc.toString()).toBe("aaa");
  });

  it("locks editing when read-only", () => {
    const { container } = render(
      <CodeEditor docKey="a.py" value="x" readOnly ariaLabel="Source" onChange={() => {}} onSave={() => {}} />
    );
    expect(viewOf(container).state.readOnly).toBe(true);
    expect(screen.getByRole("textbox", { name: "Source" })).toHaveAttribute("aria-readonly", "true");
  });
});
