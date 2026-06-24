import { useTr } from "../../i18n/text";

export function UtilInput({
  value,
  onChange,
  placeholder,
  type = "text",
  maxLength,
  onKeyDown,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
  maxLength?: number;
  onKeyDown?: (e: React.KeyboardEvent) => void;
}) {
  const t = useTr();
  return (
    <input
      type={type}
      value={value}
      onChange={(e) => {
        if (maxLength && e.target.value.length > maxLength) return;
        if (type === "pin") {
          onChange(e.target.value.replace(/\D/g, ""));
          return;
        }
        onChange(e.target.value);
      }}
      onKeyDown={onKeyDown}
      placeholder={placeholder ? t(placeholder) : undefined}
      spellCheck={false}
      autoComplete="off"
      className="w-full px-2.5 py-1.5 bg-zinc-800/50 border border-zinc-700/50 rounded-lg text-xs text-zinc-300 focus:outline-none focus:border-zinc-600 transition-colors"
    />
  );
}
