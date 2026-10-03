import { GameBadge } from "../ui/GameBadge";
import { DANGER_ACTION, ModeStatusBar, NEUTRAL_ACTION, PRIMARY_ACTION } from "./ModeStatusBar";
import { AccountsCard, CloseRobloxAlert, ServerCard, TimingCard } from "./rejoin/RejoinConfig";
import { LiveList } from "./rejoin/RejoinLive";
import { formatCountdown } from "./rejoin/rejoinShared";
import { useRejoinController, type RejoinTabOptions } from "./rejoin/useRejoinController";

/**
 * Aba Auto Rejoin do Modo AFK: o ciclo que fecha e reabre o cliente das contas
 * de tempo em tempo.
 *
 * Parado: marca as contas (as que têm cliente aberto, como nos cliques AFK),
 * escolhe para onde rejogam — onde já estão (padrão) ou um jogo dos Favoritos —
 * e o tempo. Rodando: a lista ao vivo e, se houver, as contas abertas fora do
 * ciclo para acrescentar.
 *
 * A barra de estado fica fora da área que rola — ligar e parar estão sempre à
 * vista. Quem decide o layout é a largura do **contêiner** (`@container/rejoin`),
 * não a da janela: a mesma tela vai num modal e numa página com barra lateral.
 */
export function RejoinTab(props: RejoinTabOptions) {
  const ctl = useRejoinController(props);
  const { t, status, running } = ctl;

  const gameFact = (g: typeof ctl.game) =>
    g?.name || g?.iconUrl ? (
      <GameBadge key="game" name={g?.name ?? null} iconUrl={g?.iconUrl ?? null} placeId={g?.placeId} />
    ) : null;

  const facts = running
    ? [
        t("{{count}} accounts in the cycle", { count: status?.userIds?.length ?? 0 }),
        ctl.nextRejoinAtMs !== null
          ? t("Next rejoin in {{time}}", {
              time:
                formatCountdown(ctl.nextRejoinAtMs, ctl.nowMs) === "due"
                  ? t("due")
                  : formatCountdown(ctl.nextRejoinAtMs, ctl.nowMs),
            })
          : null,
        gameFact(ctl.runningGame),
      ]
    : [t("{{count}} accounts selected", { count: ctl.picked.length }), gameFact(ctl.game)];

  const message =
    ctl.startError && !ctl.showCloseRobloxAction
      ? { text: ctl.startError, tone: "error" as const }
      : !running && ctl.startBlocker
        ? { text: ctl.startBlocker, tone: "warn" as const }
        : null;

  const actions = running ? (
    <>
      {ctl.missingFromSession.length > 0 ? (
        <button
          onClick={() => void ctl.handleAddToSession()}
          disabled={ctl.actionButtonsLocked}
          className={PRIMARY_ACTION}
        >
          {t("Add to Auto Rejoin ({{count}})", { count: ctl.missingFromSession.length })}
        </button>
      ) : null}
      <button onClick={ctl.handleStop} disabled={ctl.actionButtonsLocked} className={NEUTRAL_ACTION}>
        {t("Stop Auto Rejoin")}
      </button>
      <button
        onClick={() => void ctl.handleStopAndClose()}
        disabled={ctl.actionButtonsLocked}
        className={DANGER_ACTION}
      >
        {t("Stop + Close Clients")}
      </button>
    </>
  ) : (
    <button onClick={() => void ctl.handleStart()} disabled={!ctl.canStart} className={PRIMARY_ACTION}>
      {t("Start Auto Rejoin")}
    </button>
  );

  // Rodando, a lista para marcar mostra só quem está fora do ciclo.
  const outsideSession = ctl.candidates.filter((a) => !ctl.sessionSet.has(a.UserID));

  return (
    <div className="@container/rejoin flex h-full min-h-0 flex-col gap-3">
      <ModeStatusBar
        testId="rejoin-status"
        running={running}
        title={running ? t("Auto Rejoin is running") : t("Auto Rejoin is stopped")}
        facts={facts}
        actions={actions}
        message={message}
      />

      {/* Quem rola é este contêiner, em qualquer largura: nada aqui dentro
          limita a altura (a 900x560 uma lista com altura própria ficava com
          0 px). */}
      <div
        ref={ctl.contentRef}
        data-testid="rejoin-scroll"
        className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-3 pr-0.5"
      >
        <CloseRobloxAlert ctl={ctl} />
        {running ? (
          <>
            <LiveList ctl={ctl} />
            {outsideSession.length > 0 ? (
              <AccountsCard ctl={ctl} accounts={outsideSession} title={t("Other open clients")} />
            ) : null}
          </>
        ) : (
          <div className="grid grid-cols-1 items-start gap-3 @3xl/rejoin:grid-cols-[minmax(300px,380px)_minmax(0,1fr)] @7xl/rejoin:grid-cols-[440px_minmax(0,1fr)]">
            <div className="space-y-3">
              <ServerCard ctl={ctl} />
              <TimingCard ctl={ctl} />
            </div>
            <AccountsCard ctl={ctl} accounts={ctl.candidates} title={t("Accounts")} />
          </div>
        )}
      </div>
    </div>
  );
}
