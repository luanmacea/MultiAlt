import { Children, cloneElement, isValidElement, type ReactNode } from "react";
import type { TOptions } from "i18next";
import { useTranslation } from "react-i18next";
import i18n from "./index";

export function tr(text: string, options?: TOptions): string {
  return i18n.t(text, {
    defaultValue: text,
    ...(options || {}),
  });
}

export function useTr() {
  const { t } = useTranslation();
  return (text: string, options?: TOptions): string =>
    t(text, {
      defaultValue: text,
      ...(options || {}),
    });
}

export function trNode(node: ReactNode, t: (text: string, options?: TOptions) => string): ReactNode {
  if (typeof node === "string") return t(node);
  if (node === null || node === undefined || typeof node === "boolean" || typeof node === "number") return node;

  // Translate string children inside fragments/elements, e.g. <>Enable Web Server<Badge/></>.
  if (isValidElement(node)) {
    const props = (node as unknown as { props?: { children?: ReactNode } }).props;
    if (!props || props.children === undefined) return node;
    const mapped = Children.map(props.children, (child) => trNode(child, t));
    const nextChildren =
      mapped && mapped.length === 1
        ? mapped[0]
        : mapped;
    return cloneElement(node, undefined, nextChildren);
  }

  // ReactNode can be an array; Children.map handles non-arrays too, but arrays reach here.
  if (Array.isArray(node)) return Children.map(node, (child) => trNode(child, t));

  return node;
}
