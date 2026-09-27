import { Check, ChevronDown, ChevronUp, User, GripVertical } from "lucide-react";
import { useStore } from "../../store";
import type { Account } from "../../types";
import { timeAgo, getFreshnessColor, AGED_AFTER_DAYS } from "../../types";
import { Tooltip } from "../ui/Tooltip";
import { useTr } from "../../i18n/text";

function maskName(name: string, previewLetters: number): string {
  if (previewLetters > 0 && previewLetters < name.length) {
    return name.slice(0, previewLetters) + "********";
  }
  return "************";
}

export function AccountRow({ account }: { account: Account }) {
  const t = useTr();
  const store = useStore();
  const selected = store.selectedIds.has(account.UserID);
  const multiMode = store.selectedIds.size > 1;
  const avatarUrl = store.avatarUrls.get(account.UserID);

  /**
   * Vizinhos da conta **dentro do grupo dela**. As setas fazem o mesmo
   * `reorderAccounts` do arrasto, uma posição por clique — o dono pediu um
   * caminho que não dependa de arrastar, e é também o único que funciona pelo
   * teclado. Subir a primeira do grupo ficaria fora do grupo, então nas pontas
   * a seta fica desligada em vez de fazer algo inesperado.
   */
  const irmaos = store.accounts.filter(
    (a) => (a.Group || "Default") === (account.Group || "Default")
  );
  const posicao = irmaos.findIndex((a) => a.UserID === account.UserID);
  const contaAcima = posicao > 0 ? irmaos[posicao - 1] : null;
  const contaAbaixo =
    posicao >= 0 && posicao < irmaos.length - 1 ? irmaos[posicao + 1] : null;

  function moverPara(alvo: Account | null, e: React.MouseEvent) {
    // Sem isto o clique sobe para a linha e seleciona a conta.
    e.stopPropagation();
    if (!alvo) return;
    void store.reorderAccounts(account.UserID, alvo.UserID);
  }
  const freshness =
    store.settings?.General?.DisableAgingAlert === "true"
      ? null
      : getFreshnessColor(account.LastUse);
  const rawName = account.Alias || account.Username;
  const displayName = store.hideUsernames ? maskName(rawName, store.hiddenNameLetters) : rawName;
  const showUsername = !!account.Alias && !store.hideUsernames;
  const description = account.Description?.trim() || "";
  const hideAvatar = store.hideUsernames && !store.showAvatarsWhenHidden;
  const showPresence = store.settings?.General?.ShowPresence === "true";
  // Alias vai ate 240 caracteres (Task 10) e isso estoura a linha; sem esta
  // opcao o nome continua sendo cortado, que e o comportamento de sempre.
  //
  // Ligada, nao basta `break-words`: o nome e item flex, e com `min-width: auto`
  // ele nunca encolhe abaixo do trecho sem espaco (as quebras de `break-word`
  // nao contam no tamanho minimo). Um alias como `xXx_Dragon..._2024_xXx`
  // ficava numa linha so, passando por baixo do carimbo e das setas. `min-w-0`
  // deixa encolher e `wrap-anywhere` (`overflow-wrap: anywhere`) quebra o
  // trecho tambem no tamanho minimo.
  const wrapLongNames = store.settings?.General?.WrapLongNames === "true";
  const nameOverflowClass = wrapLongNames ? "min-w-0 wrap-anywhere" : "truncate";
  const presenceType = store.presenceByUserId.get(account.UserID) ?? 0;
  const launchedLocally = store.launchedByProgram.has(account.UserID);
  const isJoining = store.joiningAccounts.has(account.UserID);

  const presenceMeta =
    presenceType === 3
      ? { label: t("In Studio"), dotClass: "bg-violet-500", dotStyle: undefined as React.CSSProperties | undefined }
      : presenceType >= 2
      ? { label: t("In Game"), dotClass: "bg-emerald-500", dotStyle: undefined as React.CSSProperties | undefined }
      : presenceType === 1
        ? { label: t("Online"), dotClass: "bg-sky-500", dotStyle: undefined as React.CSSProperties | undefined }
        : { label: t("Offline"), dotClass: "", dotStyle: { backgroundColor: "var(--panel-muted)" } };

  // Conta adicionada por nome de usuário (Quick Add) entra sem cookie, e o
  // backend ainda a grava com Valid=true — nada na linha a distinguia de uma
  // conta boa. Quem não tem token não tem sessão: mesma bolinha vermelha de
  // sessão inválida, com o texto dizendo o que falta fazer.
  const hasSession = !!account.SecurityToken?.trim();

  const statusDots: Array<{ color: string; title: string }> = [];
  if (!hasSession) {
    statusDots.push({
      color: "#ef4444",
      title: t("No session — paste its cookie or use Browser Login to sign in"),
    });
  } else if (!account.Valid) {
    statusDots.push({ color: "#ef4444", title: t("Invalid session") });
  }
  if (freshness) {
    // A bolinha só dizia "aged"; quem lê precisa saber *de que* envelheceu.
    // A conta é contada a partir de `LastUse` — a mesma data que a coluna da
    // direita mostra como "3d"/"2mo" —, e isso não tem relação com a bolinha
    // vermelha, que é sessão inválida.
    const agedDays = Math.floor((Date.now() - new Date(account.LastUse).getTime()) / 86400000);
    statusDots.push({
      color: freshness,
      title: t(
        "Aged: {{days}} days since the Last Use date shown on the right ({{threshold}}+ days). Its session may still be fine — a dead session is the red dot.",
        { days: agedDays, threshold: AGED_AFTER_DAYS }
      ),
    });
  }
  if (launchedLocally) {
    statusDots.push({ color: "#f59e0b", title: t("Launched by Roblox Account Manager") });
  }
  if (showPresence && presenceType >= 1) {
    const presenceColor =
      presenceType === 3 ? "#4629d8" : presenceType >= 2 ? "#02b757" : "#00a2ff";
    statusDots.push({ color: presenceColor, title: presenceMeta.label });
  }

  function handleClick(e: React.MouseEvent) {
    e.stopPropagation();
    store.handleSelect(account.UserID, e);
  }

  function handleContext(e: React.MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (!store.selectedIds.has(account.UserID)) {
      store.selectSingle(account.UserID);
    }
    store.openContextMenu(e.clientX, e.clientY);
  }

  function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    e.stopPropagation();

    const dragged = store.dragState;
    store.setDragState(null);
    if (!dragged) return;

    if (dragged.sourceGroup === (account.Group || "Default")) {
      store.reorderAccounts(dragged.userId, account.UserID);
      return;
    }

    store.moveToGroup([dragged.userId], account.Group || "Default");
  }

  return (
    <div
      data-account-row="true"
      data-user-id={account.UserID}
      className={`group/row theme-row-hover flex items-center gap-3 px-3 py-1.5 cursor-default select-none border-l-2 transition-colors duration-100 ${
        selected ? "theme-row-selected" : "border-l-transparent"
      }`}
      style={selected ? { borderLeftColor: "var(--accent-color)" } : undefined}
      onClick={handleClick}
      onContextMenu={handleContext}
      onDragOver={(e) => {
        e.preventDefault();
        e.dataTransfer.dropEffect = "move";
      }}
      onDrop={handleDrop}
    >
      {/* Drag handle — the reorder drag starts here (not the whole row) so it
          doesn't conflict with the list's marquee multi-select on the row body. */}
      <div
        draggable
        onDragStart={(e) => {
          e.stopPropagation();
          e.dataTransfer.effectAllowed = "move";
          e.dataTransfer.setData("text/plain", String(account.UserID));
          store.setDragState({ userId: account.UserID, sourceGroup: account.Group || "Default" });
        }}
        // Cancelled drags (Esc / drop outside) must not leave a stale dragState
        // that a later cookie-text drop would act on.
        onDragEnd={() => store.setDragState(null)}
        onClick={(e) => e.stopPropagation()}
        className="shrink-0 opacity-30 group-hover/row:opacity-100 transition-opacity cursor-grab active:cursor-grabbing -ml-1 px-0.5 py-1"
        title={t("Drag to reorder")}
      >
        <GripVertical size={13} strokeWidth={1.5} className="theme-muted" />
      </div>

      <div className={`shrink-0 overflow-hidden transition-all duration-150 ease-out ${
        multiMode ? "w-4 opacity-100" : "w-0 opacity-0"
      }`}>
        <div
          className={`w-4 h-4 rounded border flex items-center justify-center transition-all duration-100 ${
            selected ? "" : "theme-border group-hover/row:brightness-110"
          }`}
          style={
            selected
              ? { backgroundColor: "var(--accent-color)", borderColor: "var(--accent-color)" }
              : undefined
          }
        >
          {selected && (
            <Check size={10} stroke="var(--forms-bg)" strokeWidth={3} />
          )}
        </div>
      </div>

      <div className="relative flex-shrink-0">
        {statusDots.length > 0 && (
          <div className="absolute -left-1.5 -top-1 z-10 flex items-center gap-0.5">
            {statusDots.map((dot, index) => (
              <Tooltip key={index} content={dot.title} side="bottom">
                <span
                  role="img"
                  aria-label={dot.title}
                  className="w-2 h-2 rounded-full"
                  style={{ boxShadow: "0 0 0 1px var(--app-bg)", backgroundColor: dot.color }}
                />
              </Tooltip>
            ))}
          </div>
        )}
        {hideAvatar ? (
          <div className="theme-avatar w-8 h-8 rounded-full bg-[var(--panel-soft)] flex items-center justify-center theme-muted">
            <User size={14} strokeWidth={1.5} />
          </div>
        ) : avatarUrl ? (
          <img
            src={avatarUrl}
            alt=""
            className="theme-avatar w-8 h-8 rounded-full bg-[var(--panel-soft)] transition-transform duration-150 group-hover/row:scale-105"
            loading="lazy"
          />
        ) : (
          <div className="theme-avatar w-8 h-8 rounded-full bg-[var(--panel-soft)] flex items-center justify-center theme-muted text-xs font-medium">
            {(account.Username || "?").charAt(0).toUpperCase()}
          </div>
        )}
      </div>

      <div data-row-marquee-surface="true" className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5 min-w-0">
          {showPresence && presenceType >= 1 && (
            <Tooltip content={presenceMeta.label} side="bottom">
              <span
                className={`w-1.5 h-1.5 rounded-full shrink-0 ${presenceMeta.dotClass} ${presenceType >= 1 ? "animate-pulse" : ""}`}
                style={presenceMeta.dotStyle}
              />
            </Tooltip>
          )}
          <div
            className={`text-[13px] ${nameOverflowClass} leading-tight transition-colors duration-100 ${
              selected ? "theme-accent" : "text-[var(--panel-fg)]"
            }`}
          >
            {displayName}
          </div>
        </div>
        {showUsername && (
          <div className={`text-[12px] theme-muted ${nameOverflowClass} leading-tight`}>
            @{account.Username}
          </div>
        )}
      </div>

      {description && (
        <div className="min-w-0 max-w-[38%]">
          <Tooltip content={description}>
            <div className="text-[12px] theme-muted truncate leading-tight text-right">
              {description}
            </div>
          </Tooltip>
        </div>
      )}

      <div className="text-[12px] w-14 text-right flex-shrink-0 tabular-nums">
        {isJoining ? (
          <span className="inline-flex items-center gap-1 theme-accent">
            <span className="w-2 h-2 border border-[var(--accent-color)] border-t-transparent rounded-full animate-spin" />
            <span>{t("Join")}</span>
          </span>
        ) : (
          <span className="theme-muted">{t(timeAgo(account.LastUse))}</span>
        )}
      </div>

      {/* Setas de ordem: o caminho que não depende de arrastar. */}
      <div className="flex items-center gap-0.5 shrink-0 -mr-1">
        <button
          type="button"
          onClick={(e) => moverPara(contaAcima, e)}
          disabled={!contaAcima}
          aria-label={t("Move up")}
          title={t("Move up")}
          className="p-0.5 rounded theme-muted opacity-40 group-hover/row:opacity-100 hover:text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] disabled:opacity-15 disabled:cursor-not-allowed transition-colors"
        >
          <ChevronUp size={13} strokeWidth={2} />
        </button>
        <button
          type="button"
          onClick={(e) => moverPara(contaAbaixo, e)}
          disabled={!contaAbaixo}
          aria-label={t("Move down")}
          title={t("Move down")}
          className="p-0.5 rounded theme-muted opacity-40 group-hover/row:opacity-100 hover:text-[var(--panel-fg)] hover:bg-[var(--panel-soft)] disabled:opacity-15 disabled:cursor-not-allowed transition-colors"
        >
          <ChevronDown size={13} strokeWidth={2} />
        </button>
      </div>
    </div>
  );
}
