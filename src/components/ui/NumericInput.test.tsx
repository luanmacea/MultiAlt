import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { NumericInput } from "./NumericInput";

afterEach(cleanup);

// Uso real do campo (ex.: tamanho da interface): `onChange` não mexe no valor
// (é só preview), e o valor confirmado só volta pelo `onCommit`.
function Harness({ onCommit }: { onCommit: (value: number) => void }) {
  const [value, setValue] = useState(100);
  return (
    <NumericInput
      ariaLabel="Custom"
      value={value}
      onChange={() => {}}
      onCommit={(v) => {
        setValue(v);
        onCommit(v);
      }}
      min={80}
      max={200}
      integer
    />
  );
}

describe("NumericInput", () => {
  it("Escape discards the typed draft and does not commit it", async () => {
    const onCommit = vi.fn();
    render(<Harness onCommit={onCommit} />);
    const input = screen.getByRole("textbox", { name: "Custom" }) as HTMLInputElement;

    await userEvent.click(input);
    await userEvent.clear(input);
    await userEvent.type(input, "200");
    await userEvent.keyboard("{Escape}");

    // O rascunho 200 foi jogado fora, voltando ao valor original...
    expect(input.value).toBe("100");
    // ...e o blur seguinte não committou o 200 digitado.
    expect(onCommit).not.toHaveBeenCalledWith(200);
  });

  it("Enter still commits the typed value", async () => {
    const onCommit = vi.fn();
    render(<Harness onCommit={onCommit} />);
    const input = screen.getByRole("textbox", { name: "Custom" }) as HTMLInputElement;

    await userEvent.click(input);
    await userEvent.clear(input);
    await userEvent.type(input, "150");
    await userEvent.keyboard("{Enter}");

    expect(onCommit).toHaveBeenCalledWith(150);
    expect(input.value).toBe("150");
  });
});
