import { useTr } from "../../i18n/text";
import { NumericInput } from "./NumericInput";

export function NumberField({
  value,
  onChange,
  label,
  description,
  min,
  max,
  step,
  suffix,
  disabled = false,
}: {
  value: number;
  onChange: (v: number) => void;
  label: string;
  /** Uma linha abaixo do rotulo, como no `Toggle`. Use para dizer o efeito real. */
  description?: string;
  min?: number;
  max?: number;
  step?: number;
  suffix?: string;
  disabled?: boolean;
}) {
  const t = useTr();
  const useInteger = step === undefined ? true : Number.isInteger(step);

  return (
    <div className="flex items-center gap-3 py-2 px-1">
      <div className="min-w-0">
        <div className="text-[13px] text-zinc-300">{t(label)}</div>
        {description && (
          <div className="text-[12px] text-zinc-500 leading-snug mt-0.5">{t(description)}</div>
        )}
      </div>
      <div className="flex items-center gap-1.5 ml-auto shrink-0">
        <NumericInput
          value={value}
          onChange={onChange}
          min={min}
          max={max}
          step={step ?? 1}
          integer={useInteger}
          disabled={disabled}
          ariaLabel={t(label)}
          className={`w-20 px-2.5 py-1 rounded-md text-[13px] text-right transition-colors [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none ${
            disabled
              ? "cursor-not-allowed border border-zinc-800/60 bg-zinc-800/40 text-zinc-500"
              : "bg-zinc-800/60 border border-zinc-700/60 text-zinc-200 focus:outline-none focus:border-sky-500/40"
          }`}
        />
        {suffix && <span className="text-[12px] text-zinc-600">{t(suffix)}</span>}
      </div>
    </div>
  );
}
