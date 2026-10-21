import { UnsupportedValue, WidgetFrame } from "./WidgetFrame";
import { MAX_TABLE_ROWS, isRecord, perAccountValues, toText, type WidgetProps } from "./widgetUtils";

const title = (w: WidgetProps["widget"]) => w.title ?? w.id;

function Grid({ columns, rows, firstColumn }: { columns: string[]; rows: string[][]; firstColumn?: string }) {
  return (
    <div className="max-h-[260px] overflow-auto rounded-md border theme-border">
      <table className="w-full border-collapse text-left text-[12px]">
        <thead className="sticky top-0 bg-zinc-800/80 text-zinc-400">
          <tr>
            {firstColumn ? <th className="px-2.5 py-1 font-medium">{firstColumn}</th> : null}
            {columns.map((column, i) => (
              <th key={i} className="px-2.5 py-1 font-medium">
                {column}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, r) => (
            <tr key={r} className="border-t border-zinc-800/70 text-zinc-300">
              {row.map((cell, c) => (
                <td key={c} className="max-w-[260px] truncate px-2.5 py-1 tabular-nums" title={cell}>
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** `table`: a list of rows, each a list of cells that match `columns`. */
export function TableWidget({ widget, values }: WidgetProps) {
  const raw = values[widget.id];
  const columns = widget.columns ?? [];
  let rows: string[][] | null = null;
  if (Array.isArray(raw)) {
    rows = raw
      .slice(0, MAX_TABLE_ROWS)
      .map((row) => (Array.isArray(row) ? row.map(toText) : [toText(row)]));
  }
  const width = Math.max(columns.length, ...(rows ?? []).map((r) => r.length), 0);
  const header = columns.length > 0 ? columns : Array.from({ length: width }, (_, i) => `#${i + 1}`);
  return (
    <WidgetFrame title={title(widget)} wide testId={`widget-${widget.id}`}>
      {raw === undefined || raw === null ? (
        <div className="text-[12px] text-zinc-600">No value yet</div>
      ) : rows === null ? (
        <UnsupportedValue what="" />
      ) : rows.length === 0 ? (
        <div className="text-[12px] text-zinc-600">No rows</div>
      ) : (
        <Grid columns={header} rows={rows} />
      )}
    </WidgetFrame>
  );
}

/** `accounts`: one row per account; each value is an object keyed by column title. */
export function AccountsWidget({ widget, values, accountName }: WidgetProps) {
  const entries = perAccountValues(values, widget.id);
  const columns =
    widget.columns && widget.columns.length > 0
      ? widget.columns
      : [...new Set(entries.flatMap(([, value]) => (isRecord(value) ? Object.keys(value) : [])))];
  const rows = entries.map(([userId, value]) => [
    accountName(userId),
    ...columns.map((column) => (isRecord(value) ? toText(value[column]) : columns[0] === column ? toText(value) : "")),
  ]);
  return (
    <WidgetFrame title={title(widget)} wide testId={`widget-${widget.id}`}>
      {rows.length === 0 ? (
        <div className="text-[12px] text-zinc-600">No accounts reported yet</div>
      ) : (
        <Grid columns={columns} rows={rows} firstColumn="Account" />
      )}
    </WidgetFrame>
  );
}
