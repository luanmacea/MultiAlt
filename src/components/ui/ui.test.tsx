import "@testing-library/jest-dom/vitest";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { Toggle } from "./Toggle";
import { ToggleRow } from "./ToggleRow";
import { TextField } from "./TextField";
import { NumberField } from "./NumberField";
import { NumericInput } from "./NumericInput";
import { Select } from "./Select";
import { TextAreaField } from "./TextAreaField";
import { UtilButton } from "./UtilButton";
import { UtilInput } from "./UtilInput";
import { SectionHeader } from "./SectionHeader";
import { SectionLabel } from "./SectionLabel";
import { WarningBadge } from "./WarningBadge";
import { RestartBadge } from "./RestartBadge";
import { Tooltip } from "./Tooltip";

afterEach(cleanup);

describe("Toggle", () => {
  it("flips the value when the row is clicked", async () => {
    const onChange = vi.fn();
    render(<Toggle checked={false} onChange={onChange} label="Show Presence" />);
    await userEvent.click(screen.getByText("Show Presence"));
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("flips back from on to off", async () => {
    const onChange = vi.fn();
    render(<Toggle checked onChange={onChange} label="Show Presence" />);
    await userEvent.click(screen.getByText("Show Presence"));
    expect(onChange).toHaveBeenCalledWith(false);
  });

  it("ignores clicks while disabled", async () => {
    const onChange = vi.fn();
    render(<Toggle checked={false} onChange={onChange} label="Show Presence" disabled />);
    await userEvent.click(screen.getByText("Show Presence"));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("renders a description and a rich label", () => {
    render(
      <Toggle
        checked={false}
        onChange={vi.fn()}
        label={<>Allow External Connections<RestartBadge /></>}
        description="Accept connections from other devices."
      />
    );
    expect(screen.getByText("Allow External Connections")).toBeInTheDocument();
    expect(screen.getByText("restart required")).toBeInTheDocument();
    expect(screen.getByText("Accept connections from other devices.")).toBeInTheDocument();
  });
});

describe("ToggleRow", () => {
  it("exposes its state through aria-pressed and toggles it", async () => {
    const onChange = vi.fn();
    render(<ToggleRow label="Auto scroll" checked={false} onChange={onChange} />);
    const button = screen.getByRole("button", { pressed: false });

    await userEvent.click(button);
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("reports the pressed state when on", () => {
    render(<ToggleRow label="Auto scroll" checked onChange={vi.fn()} />);
    expect(screen.getByRole("button")).toHaveAttribute("aria-pressed", "true");
  });
});

describe("TextField", () => {
  it("reports each typed character", async () => {
    const onChange = vi.fn();
    render(<TextField value="" onChange={onChange} label="Password" />);
    await userEvent.type(screen.getByRole("textbox"), "a");
    expect(onChange).toHaveBeenCalledWith("a");
  });

  it("filters characters that match the pattern", async () => {
    const onChange = vi.fn();
    render(<TextField value="" onChange={onChange} label="Password" pattern={/[^0-9a-zA-Z ]/g} />);
    await userEvent.type(screen.getByRole("textbox"), "!");
    expect(onChange).toHaveBeenCalledWith("");
  });

  it("does not accept input while disabled", async () => {
    const onChange = vi.fn();
    render(<TextField value="" onChange={onChange} label="Password" disabled />);
    expect(screen.getByRole("textbox")).toBeDisabled();
    await userEvent.type(screen.getByRole("textbox"), "a");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("names the input after its label", () => {
    render(<TextField value="" onChange={vi.fn()} label="Expected Title" />);
    expect(screen.getByLabelText("Expected Title")).toBeInTheDocument();
  });
});

describe("TextAreaField", () => {
  it("names the textarea after its label", () => {
    render(<TextAreaField value="" onChange={vi.fn()} label="Allowlisted fast flags JSON" />);
    expect(screen.getByLabelText("Allowlisted fast flags JSON")).toBeInTheDocument();
  });

  it("does not accept input while disabled", async () => {
    const onChange = vi.fn();
    render(<TextAreaField value="" onChange={onChange} label="Notes" disabled />);
    expect(screen.getByLabelText("Notes")).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Notes"), "a");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("shows the error under the field", () => {
    render(<TextAreaField value="{}" onChange={vi.fn()} label="Notes" error="Bad JSON" />);
    expect(screen.getByText("Bad JSON")).toBeInTheDocument();
  });
});

describe("NumericInput / NumberField", () => {
  it("emits the parsed number as it is typed", async () => {
    const onChange = vi.fn();
    render(<NumericInput value={0} onChange={onChange} min={0} max={100} />);
    await userEvent.type(screen.getByRole("textbox"), "7");
    expect(onChange).toHaveBeenCalledWith(7);
  });

  it("clamps to the maximum on commit", async () => {
    const onChange = vi.fn();
    render(<NumericInput value={10} onChange={onChange} min={1} max={65535} />);
    const input = screen.getByRole("textbox");
    await userEvent.clear(input);
    await userEvent.type(input, "99999{Enter}");
    expect(onChange).toHaveBeenLastCalledWith(65535);
  });

  it("clamps to the minimum on commit", async () => {
    const onChange = vi.fn();
    render(<NumericInput value={10} onChange={onChange} min={5} max={100} />);
    const input = screen.getByRole("textbox");
    await userEvent.clear(input);
    await userEvent.type(input, "1{Enter}");
    expect(onChange).toHaveBeenLastCalledWith(5);
  });

  it("steps with the arrow keys", async () => {
    const onChange = vi.fn();
    render(<NumericInput value={10} onChange={onChange} min={0} max={100} step={5} />);
    const input = screen.getByRole("textbox");
    input.focus();
    await userEvent.keyboard("{ArrowUp}");
    expect(onChange).toHaveBeenLastCalledWith(15);
  });

  it("keeps a fractional step's precision", async () => {
    const onChange = vi.fn();
    render(<NumericInput value={8} onChange={onChange} min={0} max={60} step={0.5} />);
    screen.getByRole("textbox").focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(onChange).toHaveBeenLastCalledWith(7.5);
  });

  it("restores the previous value on Escape", async () => {
    const onChange = vi.fn();
    render(<NumericInput value={12} onChange={onChange} min={0} max={100} />);
    const input = screen.getByRole("textbox") as HTMLInputElement;
    await userEvent.clear(input);
    await userEvent.type(input, "44{Escape}");
    expect(input).toHaveValue("12");
  });

  it("labels the field and its unit", () => {
    render(
      <NumberField value={8} onChange={vi.fn()} label="Account Join Delay" suffix="sec" step={0.5} />
    );
    expect(screen.getByText("Account Join Delay")).toBeInTheDocument();
    expect(screen.getByText("sec")).toBeInTheDocument();
    expect(screen.getByRole("textbox")).toHaveValue("8");
  });

  it("exposes stepper buttons when asked", async () => {
    const onChange = vi.fn();
    function Controlled() {
      const [value, setValue] = useState(1);
      return (
        <NumericInput
          value={value}
          onChange={(v) => {
            onChange(v);
            setValue(v);
          }}
          showStepper
          min={0}
          max={9}
        />
      );
    }
    render(<Controlled />);

    await userEvent.click(screen.getByRole("button", { name: "Increment" }));
    expect(onChange).toHaveBeenLastCalledWith(2);

    await userEvent.click(screen.getByRole("button", { name: "Decrement" }));
    expect(onChange).toHaveBeenLastCalledWith(1);
  });

  it("does not step past the maximum", async () => {
    const onChange = vi.fn();
    render(<NumericInput value={9} onChange={onChange} showStepper min={0} max={9} />);
    await userEvent.click(screen.getByRole("button", { name: "Increment" }));
    expect(onChange).not.toHaveBeenCalled();
  });
});

describe("Select", () => {
  const OPTIONS = [
    { value: "en", label: "English" },
    { value: "de", label: "German" },
  ];

  it("shows the selected option's label", () => {
    render(<Select value="de" options={OPTIONS} onChange={vi.fn()} />);
    expect(screen.getByRole("button", { name: "German" })).toBeInTheDocument();
  });

  it("opens the list and reports the picked value", async () => {
    const onChange = vi.fn();
    render(<Select value="en" options={OPTIONS} onChange={onChange} />);
    await userEvent.click(screen.getByRole("button", { name: "English" }));
    await userEvent.click(screen.getByRole("button", { name: "German" }));
    expect(onChange).toHaveBeenCalledWith("de");
  });

  it("closes on Escape without changing the value", async () => {
    const onChange = vi.fn();
    render(<Select value="en" options={OPTIONS} onChange={onChange} />);
    await userEvent.click(screen.getByRole("button", { name: "English" }));
    expect(screen.getByRole("button", { name: "German" })).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("button", { name: "German" })).not.toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });

  it("closes when clicking outside", async () => {
    render(
      <div>
        <Select value="en" options={OPTIONS} onChange={vi.fn()} />
        <button>outside</button>
      </div>
    );
    await userEvent.click(screen.getByRole("button", { name: "English" }));
    await userEvent.click(screen.getByRole("button", { name: "outside" }));
    expect(screen.queryByRole("button", { name: "German" })).not.toBeInTheDocument();
  });

  it("does not open while disabled", async () => {
    render(<Select value="en" options={OPTIONS} onChange={vi.fn()} disabled />);
    const button = screen.getByRole("button", { name: "English" });
    expect(button).toBeDisabled();
    await userEvent.click(button);
    expect(screen.queryByRole("button", { name: "German" })).not.toBeInTheDocument();
  });

  it("falls back to the raw value when no option matches", () => {
    render(<Select value="fr" options={OPTIONS} onChange={vi.fn()} />);
    expect(screen.getByRole("button", { name: "fr" })).toBeInTheDocument();
  });

  /** O rotulo do Select fica fora dele; sem `ariaLabel` o botao so tem o valor. */
  it("takes an accessible name of its own", () => {
    render(<Select value="en" options={OPTIONS} onChange={vi.fn()} ariaLabel="Priority Class" />);
    expect(screen.getByRole("button", { name: "Priority Class" })).toBeInTheDocument();
  });
});

describe("UtilButton / UtilInput", () => {
  it("calls back when clicked and stays inert when disabled", async () => {
    const onClick = vi.fn();
    render(<UtilButton onClick={onClick}>Refresh</UtilButton>);
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(onClick).toHaveBeenCalledTimes(1);

    cleanup();
    const onClick2 = vi.fn();
    render(<UtilButton onClick={onClick2} disabled>Refresh</UtilButton>);
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(onClick2).not.toHaveBeenCalled();
  });

  it("reports typed text and honours maxLength", async () => {
    const onChange = vi.fn();
    render(<UtilInput value="abc" onChange={onChange} maxLength={3} placeholder="Code" />);
    await userEvent.type(screen.getByPlaceholderText("Code"), "d");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("keeps only digits for a pin field", async () => {
    const onChange = vi.fn();
    render(<UtilInput value="" onChange={onChange} type="pin" placeholder="PIN" />);
    await userEvent.type(screen.getByPlaceholderText("PIN"), "a");
    expect(onChange).toHaveBeenCalledWith("");
  });

  it("forwards key presses", async () => {
    const onKeyDown = vi.fn();
    render(<UtilInput value="" onChange={vi.fn()} onKeyDown={onKeyDown} placeholder="Code" />);
    await userEvent.type(screen.getByPlaceholderText("Code"), "{Enter}");
    expect(onKeyDown).toHaveBeenCalled();
  });
});

describe("labels and badges", () => {
  it("renders section headings and badges", () => {
    render(
      <div>
        <SectionHeader>Permissions</SectionHeader>
        <SectionLabel>Server</SectionLabel>
        <WarningBadge>use at own risk</WarningBadge>
        <RestartBadge />
      </div>
    );
    expect(screen.getByText("Permissions")).toBeInTheDocument();
    expect(screen.getByText("Server")).toBeInTheDocument();
    expect(screen.getByText("use at own risk")).toBeInTheDocument();
    expect(screen.getByText("restart required")).toBeInTheDocument();
  });
});

describe("Tooltip", () => {
  it("renders its child and reveals the content on hover", async () => {
    render(
      <Tooltip content="Invalid session" delayMs={0}>
        <span>anchor</span>
      </Tooltip>
    );
    expect(screen.getByText("anchor")).toBeInTheDocument();
    expect(screen.queryByText("Invalid session")).not.toBeInTheDocument();

    await userEvent.hover(screen.getByText("anchor"));
    expect(await screen.findByText("Invalid session")).toBeInTheDocument();
  });

  it("hides the content again once the pointer leaves", async () => {
    render(
      <Tooltip content="Invalid session" delayMs={0}>
        <span>anchor</span>
      </Tooltip>
    );
    await userEvent.hover(screen.getByText("anchor"));
    await screen.findByText("Invalid session");

    await userEvent.unhover(screen.getByText("anchor"));
    expect(screen.queryByText("Invalid session")).not.toBeInTheDocument();
  });
});
