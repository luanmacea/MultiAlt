import type { ReactNode } from "react";

/** Card around one widget: title on top, content below. */
export function WidgetFrame({
  title,
  wide = false,
  testId,
  children,
}: {
  title: string;
  /** Tables, charts and logs take more room than a number. */
  wide?: boolean;
  testId?: string;
  children: ReactNode;
}) {
  return (
    <div
      data-testid={testId}
      className={`min-w-0 rounded-lg border theme-border bg-[var(--panel-soft)] px-3 py-2.5 ${wide ? "col-span-full" : ""}`}
    >
      <div className="mb-1.5 truncate text-[11px] font-medium uppercase tracking-wide text-zinc-500">{title}</div>
      {children}
    </div>
  );
}

export function UnsupportedValue({ what }: { what: string }) {
  return <div className="text-[12px] italic text-zinc-500">Unsupported value{what ? ` (${what})` : ""}</div>;
}
