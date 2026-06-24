import { useTr } from "../../i18n/text";

export function ColorRow({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
}) {
  const t = useTr();
  const safeColor = value.startsWith("#") && (value.length === 7 || value.length === 4) ? value : "#000000";
  return (
    <div className="flex items-center justify-between py-1.5">
      <span className="text-xs text-[var(--panel-fg)]">{t(label)}</span>
      <div className="flex items-center gap-2 min-w-0">
        <input
          type="color"
          value={safeColor}
          onChange={(e) => onChange(e.target.value)}
          className="w-7 h-7 rounded border theme-border cursor-pointer bg-transparent shrink-0"
        />
        <input
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="theme-input w-28 px-2 py-1 rounded text-[10px] font-mono"
          spellCheck={false}
        />
      </div>
    </div>
  );
}
