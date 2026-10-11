import { useEffect, useRef, type ReactNode } from "react";
import { AlertTriangle, ChevronDown, Gamepad2, Lock } from "lucide-react";
import { useTr } from "../../i18n/text";
import { useGameIdentity } from "../../hooks/useGameIdentity";
import { shortJobId, type ServerGroup } from "../../utils/sessionGroups";

/**
 * Cabeçalhos dos grupos da lista "Em jogo" (página Session): jogo (com a
 * imagem quando já se sabe), servidor e os grupos à parte ("Precisa de
 * atenção", "Sem jogo"). Cada um tem a caixa que marca as contas dele.
 */

/** Caixa de seleção com o terceiro estado (parte do grupo marcada) e área de clique de 24 px. */
export function SelectBox({
  label,
  checked,
  indeterminate = false,
  onChange,
}: {
  label: string;
  checked: boolean;
  indeterminate?: boolean;
  onChange: (event: React.MouseEvent<HTMLInputElement>) => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = indeterminate && !checked;
  }, [indeterminate, checked]);
  return (
    <label className="shrink-0 -m-1 flex items-center justify-center w-6 h-6 cursor-pointer">
      <input
        ref={ref}
        type="checkbox"
        aria-label={label}
        checked={checked}
        onChange={() => {}}
        onClick={onChange}
        className="accent-[var(--accent-color)] w-3.5 h-3.5 cursor-pointer"
      />
    </label>
  );
}

function countLabel(count: number, t: ReturnType<typeof useTr>): string {
  return count === 1 ? t("1 account") : t("{{count}} accounts", { count });
}

/**
 * Cabeçalho de grupo: caixa do grupo, botão que recolhe (o título inteiro,
 * padrão acordeão) e a contagem. `tone` muda só o ícone e a cor do título.
 */
function GroupHeader({
  title,
  icon,
  count,
  selection,
  onSelect,
  expanded,
  onToggle,
  tone = "default",
  titleAttr,
}: {
  title: string;
  icon: ReactNode;
  count: number;
  selection: "all" | "some" | "none";
  onSelect: (event: React.MouseEvent<HTMLInputElement>) => void;
  expanded: boolean;
  onToggle: () => void;
  tone?: "default" | "attention";
  titleAttr?: string;
}) {
  const t = useTr();
  return (
    <div className="flex items-center gap-2 px-3 pt-2 pb-1">
      <SelectBox
        label={t("Select {{name}}", { name: title })}
        checked={selection === "all"}
        indeterminate={selection === "some"}
        onChange={onSelect}
      />
      <h3 className="min-w-0 flex-1 text-[12px]">
        <button
          type="button"
          aria-expanded={expanded}
          onClick={onToggle}
          title={titleAttr}
          className="flex w-full min-w-0 min-h-[24px] items-center gap-2 rounded-md text-left hover:bg-[var(--row-hover)] px-1 -mx-1"
        >
          <ChevronDown
            size={13}
            strokeWidth={1.75}
            aria-hidden="true"
            className={`shrink-0 theme-muted transition-transform motion-reduce:transition-none ${expanded ? "" : "-rotate-90"}`}
          />
          {icon}
          <span
            className={`truncate font-semibold ${tone === "attention" ? "text-amber-400" : "text-[var(--panel-fg)]"}`}
          >
            {title}
          </span>
          <span className="shrink-0 theme-muted font-normal tabular-nums">{countLabel(count, t)}</span>
        </button>
      </h3>
    </div>
  );
}

export function GameGroupHeader({
  placeId,
  userId,
  count,
  selection,
  onSelect,
  expanded,
  onToggle,
}: {
  placeId: number;
  /** Conta usada para resolver o jogo (a API precisa de uma sessão). */
  userId: number;
  count: number;
  selection: "all" | "some" | "none";
  onSelect: (event: React.MouseEvent<HTMLInputElement>) => void;
  expanded: boolean;
  onToggle: () => void;
}) {
  const t = useTr();
  const identity = useGameIdentity(placeId, userId);
  const name = identity?.name ?? t("Place {{placeId}}", { placeId });
  const icon = identity?.iconUrl ? (
    <img src={identity.iconUrl} alt="" className="shrink-0 w-5 h-5 rounded-[5px] object-cover" />
  ) : (
    <span aria-hidden="true" className="shrink-0 w-5 h-5 rounded-[5px] bg-[var(--panel-soft)] flex items-center justify-center theme-muted">
      <Gamepad2 size={12} strokeWidth={1.6} />
    </span>
  );
  return (
    <GroupHeader
      title={name}
      titleAttr={name}
      icon={icon}
      count={count}
      selection={selection}
      onSelect={onSelect}
      expanded={expanded}
      onToggle={onToggle}
    />
  );
}

export function AttentionGroupHeader(props: {
  count: number;
  selection: "all" | "some" | "none";
  onSelect: (event: React.MouseEvent<HTMLInputElement>) => void;
  expanded: boolean;
  onToggle: () => void;
}) {
  const t = useTr();
  return (
    <GroupHeader
      {...props}
      title={t("Needs attention")}
      tone="attention"
      icon={<AlertTriangle size={13} strokeWidth={1.75} aria-hidden="true" className="shrink-0 text-amber-400" />}
    />
  );
}

export function NoGameGroupHeader(props: {
  count: number;
  selection: "all" | "some" | "none";
  onSelect: (event: React.MouseEvent<HTMLInputElement>) => void;
  expanded: boolean;
  onToggle: () => void;
}) {
  const t = useTr();
  return (
    <GroupHeader
      {...props}
      title={t("Not in a game")}
      icon={<Gamepad2 size={13} strokeWidth={1.6} aria-hidden="true" className="shrink-0 theme-muted" />}
    />
  );
}

/** Linha do servidor dentro do jogo: público/privado, o Job ID curto e quantas contas. */
export function ServerHeader({
  server,
  selection,
  onSelect,
}: {
  server: ServerGroup;
  selection: "all" | "some" | "none";
  onSelect: (event: React.MouseEvent<HTMLInputElement>) => void;
}) {
  const t = useTr();
  const kind =
    server.jobId === null
      ? t("Server unknown")
      : server.privateServer === true
        ? t("Private server")
        : server.privateServer === false
          ? t("Public server")
          : t("Server");
  return (
    <div className="flex items-center gap-2 pl-8 pr-3 py-0.5 text-[11.5px] theme-muted">
      <SelectBox
        label={t("Select {{name}}", { name: server.jobId ? `${kind} ${shortJobId(server.jobId)}` : kind })}
        checked={selection === "all"}
        indeterminate={selection === "some"}
        onChange={onSelect}
      />
      <span className="inline-flex items-center gap-1 min-w-0">
        {server.privateServer === true && <Lock size={10} strokeWidth={1.75} aria-hidden="true" className="shrink-0" />}
        <span className="truncate">{kind}</span>
      </span>
      {server.jobId && (
        <span className="font-mono tabular-nums shrink-0" title={server.jobId}>
          {shortJobId(server.jobId)}
        </span>
      )}
      <span aria-hidden="true">·</span>
      <span className="shrink-0 tabular-nums">{server.userIds.length}</span>
    </div>
  );
}
