import { trNode, useTr } from "../../i18n/text";

export function UtilButton({
  children,
  onClick,
  disabled,
  variant = "default",
}: {
  children: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
  variant?: "default" | "danger" | "success";
}) {
  const t = useTr();
  const styles = {
    default: "bg-zinc-800 border border-zinc-700/50 text-zinc-300 hover:bg-zinc-700",
    danger: "bg-red-500/10 border border-red-500/30 text-red-400 hover:bg-red-500/20",
    success: "bg-emerald-500/10 border border-emerald-500/30 text-emerald-400 hover:bg-emerald-500/20",
  };
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`px-3 py-1.5 text-xs font-medium rounded-lg transition-colors disabled:opacity-40 disabled:cursor-not-allowed ${styles[variant]}`}
    >
      {trNode(children, t)}
    </button>
  );
}
