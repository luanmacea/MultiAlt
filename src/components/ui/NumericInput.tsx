import { useCallback, useEffect, useMemo, useRef, useState } from "react";

function clamp(value: number, min?: number, max?: number): number {
  let next = value;
  if (typeof min === "number") next = Math.max(min, next);
  if (typeof max === "number") next = Math.min(max, next);
  return next;
}

function stepPrecision(step: number): number {
  const text = String(step);
  if (text.includes("e-") || text.includes("E-")) {
    const exp = Number.parseInt(text.split(/e-/i)[1] || "0", 10);
    return Number.isFinite(exp) ? Math.max(0, exp) : 0;
  }
  const dot = text.indexOf(".");
  return dot >= 0 ? text.length - dot - 1 : 0;
}

function formatNumber(value: number, precision: number): string {
  if (precision <= 0) return String(Math.round(value));
  const fixed = value.toFixed(precision);
  return fixed.replace(/\.0+$/, "").replace(/(\.\d*?)0+$/, "$1");
}

function parseDraft(raw: string): number | null {
  const normalized = raw.trim().replace(",", ".");
  if (
    normalized.length === 0 ||
    normalized === "-" ||
    normalized === "." ||
    normalized === "-."
  ) {
    return null;
  }
  const parsed = Number.parseFloat(normalized);
  return Number.isFinite(parsed) ? parsed : null;
}

export function NumericInput({
  value,
  onChange,
  onCommit,
  min,
  max,
  step = 1,
  integer,
  disabled,
  showStepper,
  className,
  containerClassName,
  incrementLabel = "Increment",
  decrementLabel = "Decrement",
}: {
  value: number;
  onChange: (value: number) => void;
  onCommit?: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  integer?: boolean;
  disabled?: boolean;
  showStepper?: boolean;
  className?: string;
  containerClassName?: string;
  incrementLabel?: string;
  decrementLabel?: string;
}) {
  const effectiveStep = Number.isFinite(step) && step > 0 ? step : 1;
  const precision = useMemo(() => stepPrecision(effectiveStep), [effectiveStep]);
  const preferInteger = integer ?? Number.isInteger(effectiveStep);
  const [draft, setDraft] = useState(() => formatNumber(value, preferInteger ? 0 : precision));
  const [focused, setFocused] = useState(false);
  const latestValueRef = useRef(value);

  useEffect(() => {
    latestValueRef.current = value;
  }, [value]);

  useEffect(() => {
    if (focused) return;
    setDraft(formatNumber(value, preferInteger ? 0 : precision));
  }, [focused, precision, preferInteger, value]);

  const normalize = useCallback(
    (next: number) => {
      const clamped = clamp(next, min, max);
      if (preferInteger) return Math.round(clamped);
      return Number.parseFloat(formatNumber(clamped, precision));
    },
    [max, min, precision, preferInteger]
  );

  const emit = useCallback(
    (next: number, committing: boolean) => {
      if (next !== latestValueRef.current) {
        onChange(next);
      }
      if (committing) {
        onCommit?.(next);
      }
    },
    [onChange, onCommit]
  );

  const commit = useCallback(
    (raw?: string) => {
      const parsed = parseDraft(raw ?? draft);
      const next = normalize(parsed ?? latestValueRef.current);
      setDraft(formatNumber(next, preferInteger ? 0 : precision));
      emit(next, true);
    },
    [draft, emit, normalize, precision, preferInteger]
  );

  const nudge = useCallback(
    (direction: 1 | -1) => {
      if (disabled) return;
      const base = latestValueRef.current;
      const next = normalize(base + direction * effectiveStep);
      setFocused(false);
      setDraft(formatNumber(next, preferInteger ? 0 : precision));
      emit(next, true);
    },
    [disabled, effectiveStep, emit, normalize, precision, preferInteger]
  );

  return (
    <div className={containerClassName || "relative w-full"}>
      <input
        type="text"
        inputMode={preferInteger ? "numeric" : "decimal"}
        value={draft}
        disabled={disabled}
        onFocus={() => setFocused(true)}
        onChange={(e) => {
          const nextDraft = e.target.value;
          setDraft(nextDraft);
          const parsed = parseDraft(nextDraft);
          if (parsed === null) return;
          emit(normalize(parsed), false);
        }}
        onBlur={() => {
          setFocused(false);
          commit();
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            commit();
            (e.currentTarget as HTMLInputElement).blur();
          } else if (e.key === "Escape") {
            setDraft(formatNumber(latestValueRef.current, preferInteger ? 0 : precision));
            (e.currentTarget as HTMLInputElement).blur();
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            nudge(1);
          } else if (e.key === "ArrowDown") {
            e.preventDefault();
            nudge(-1);
          }
        }}
        className={className}
      />
      {showStepper ? (
        <div className="absolute inset-y-1 right-1 flex w-7 flex-col overflow-hidden rounded-md border theme-border bg-[linear-gradient(180deg,rgba(255,255,255,0.12),rgba(255,255,255,0.03))]">
          <button
            type="button"
            aria-label={incrementLabel}
            disabled={disabled}
            className="flex h-1/2 items-center justify-center border-b theme-border text-[9px] text-[var(--panel-muted)] hover:text-[var(--panel-fg)] hover:bg-[rgba(255,255,255,0.06)] transition disabled:opacity-50 disabled:cursor-not-allowed"
            onClick={() => nudge(1)}
          >
            ^
          </button>
          <button
            type="button"
            aria-label={decrementLabel}
            disabled={disabled}
            className="flex h-1/2 items-center justify-center text-[9px] text-[var(--panel-muted)] hover:text-[var(--panel-fg)] hover:bg-[rgba(255,255,255,0.06)] transition disabled:opacity-50 disabled:cursor-not-allowed"
            onClick={() => nudge(-1)}
          >
            v
          </button>
        </div>
      ) : null}
    </div>
  );
}
