import { cleanup, renderHook } from "@testing-library/react";
import { createElement, isValidElement, type ReactElement, type ReactNode } from "react";
import { afterEach, describe, expect, it } from "vitest";
import i18n from "./index";
import { tr, trNode, useTr } from "./text";

afterEach(async () => {
  cleanup();
  await i18n.changeLanguage("en");
});

function childrenOf(node: ReactNode): ReactNode {
  return isValidElement(node)
    ? (node as ReactElement<{ children?: ReactNode }>).props.children
    : undefined;
}

describe("tr", () => {
  it("returns the English phrase itself when there is no translation", () => {
    expect(tr("Totally unknown phrase")).toBe("Totally unknown phrase");
  });

  it("returns the translation for a known key", async () => {
    await i18n.changeLanguage("de");
    expect(tr("Settings")).toBe("Einstellungen");
  });

  it("interpolates options", () => {
    expect(tr("Added {{name}}", { name: "Neo" })).toBe("Added Neo");
  });

  it("lets an explicit defaultValue win over the key", () => {
    expect(tr("Unknown phrase here", { defaultValue: "Fallback" })).toBe("Fallback");
  });

  it("keeps an unknown placeholder untouched", () => {
    expect(tr("Hi {{who}}")).toBe("Hi {{who}}");
  });
});

describe("useTr", () => {
  it("returns a translator bound to the current language", async () => {
    const { result } = renderHook(() => useTr());
    expect(result.current("Unknown phrase")).toBe("Unknown phrase");
    expect(result.current("Added {{name}}", { name: "Ada" })).toBe("Added Ada");
  });
});

describe("trNode", () => {
  const t = (text: string) => `[${text}]`;

  it("translates a plain string", () => {
    expect(trNode("Hello", t)).toBe("[Hello]");
  });

  it("passes through nullish, boolean and numeric nodes", () => {
    expect(trNode(null, t)).toBeNull();
    expect(trNode(undefined, t)).toBeUndefined();
    expect(trNode(true, t)).toBe(true);
    expect(trNode(7, t)).toBe(7);
  });

  it("translates the string children of an element", () => {
    const node = trNode(createElement("span", null, "Enable"), t);
    expect(childrenOf(node)).toBe("[Enable]");
  });

  it("leaves an element without children untouched", () => {
    const element = createElement("br");
    expect(trNode(element, t)).toBe(element);
  });

  it("translates each string inside a fragment and keeps other elements", () => {
    const badge = createElement("b", { key: "b" }, "!");
    const node = trNode(createElement("div", null, ["Enable ", badge]), t);
    const children = childrenOf(node) as ReactNode[];

    expect(Array.isArray(children)).toBe(true);
    expect(children[0]).toBe("[Enable ]");
    expect(isValidElement(children[1])).toBe(true);
  });

  it("translates nested element children", () => {
    const node = trNode(createElement("div", null, createElement("span", null, "Deep")), t);
    const inner = childrenOf(node) as ReactNode;
    expect(childrenOf(inner)).toBe("[Deep]");
  });

  it("maps over an array of nodes", () => {
    const result = trNode(["a", "b"], t) as ReactNode[];
    expect(result).toEqual(["[a]", "[b]"]);
  });
});
