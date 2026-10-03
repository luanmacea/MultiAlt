import { GameBadge } from "../ui/GameBadge";
import { DANGER_ACTION, ModeStatusBar, NEUTRAL_ACTION, PRIMARY_ACTION } from "./ModeStatusBar";
import { CloseRobloxAlert, CycleExplainer, ServerCard, TargetsCard, TimingCard } from "./rejoin/RejoinConfig";
import { LiveCycleList, LiveList } from "./rejoin/RejoinLive";
import { formatCountdown } from "./rejoin/rejoinShared";
import { useRejoinController, type RejoinTabOptions } from "./rejoin/useRejoinController";

/**
 * Aba Auto Rejoin do Modo AFK (antes o diálogo de Auto Rejoin): o ciclo que
 * fecha e reabre o cliente das alts de tempo em tempo.
 *
 * A barra de estado fica fora da área que rola — ligar e parar estão sempre à
 * vista. Abaixo, a configuração e a lista ao vivo; numa área larga (modal grande
 * ou página inteira) as duas ficam lado a lado, cada uma rolando por dentro.
 * Quem decide é a largura do **contêiner** (`@container/rejoin`), não a da
 * janela: a mesma tela vai num modal e numa página com barra lateral.
 */
export function RejoinTab(props: RejoinTabOptions) {
  const ctl = useRejoinController(props);
  const { t, status, running } = ctl;

  const facts = running
    ? [
        t("{{count}} accounts in the cycle", { count: status?.userIds?.length ?? 0 }),
        ctl.nextRejoinAtMs !== null
          ? t("Next rejoin in {{time}}", {
              time: formatCountdown(ctl.nextRejoinAtMs, ctl.nowMs) === "due"
                ? t("due")
                : formatCountdown(ctl.nextRejoinAtMs, ctl.nowMs),
            })
          : null,
        ctl.runningGame?.name ? (
          <GameBadge
            key="game"
            name={ctl.runningGame.name}
            iconUrl={ctl.runningGame.iconUrl ?? null}
            placeId={ctl.runningGame.placeId}
          />
        ) : null,
      ]
    : [
        ctl.adopt
          ? t("{{count}} accounts already in game", { count: ctl.targetIds.length })
          : t("{{count}} accounts selected", { count: ctl.targetIds.length }),
      ];

  const message = ctl.startError && !ctl.showCloseRobloxAction
    ? { text: ctl.startError, tone: "error" as const }
    : !running && ctl.startBlocker
      ? { text: ctl.startBlocker, tone: "warn" as const }
      : null;

  const actions = running ? (
    <>
      {ctl.adopt && ctl.missingFromSession.length > 0 ? (
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
        onClick={() => void ctl.handleStopAndCloseBots()}
        disabled={ctl.actionButtonsLocked}
        className={DANGER_ACTION}
      >
        {t("Stop + Close Alt Accounts")}
      </button>
    </>
  ) : (
    <button onClick={() => void ctl.handleStart()} disabled={!ctl.canStart} className={PRIMARY_ACTION}>
      {t("Start Auto Rejoin")}
    </button>
  );

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

      {/* O conteúdo rola nas duas visões. Numa área estreita as colunas
          empilham com a altura do próprio conteúdo e quem rola é este
          container; numa área larga cada coluna rola por dentro. Sem isso, a
          900x560 a lista ao vivo ficava com 0 px e nada rolava. */}
      <div
        ref={ctl.contentRef}
        className="flex-1 min-h-0 overflow-y-auto flex flex-col gap-3 pr-0.5"
      >
        <CloseRobloxAlert ctl={ctl} />
        <CycleExplainer ctl={ctl} />
        {ctl.useSplitLayout ? (
          <div className="grid grid-cols-1 gap-3 @3xl/rejoin:grid-cols-[minmax(300px,360px)_minmax(0,1fr)] @7xl/rejoin:grid-cols-[400px_minmax(0,1fr)] @3xl/rejoin:flex-1 @3xl/rejoin:min-h-[420px]">
            <div className="space-y-3 @3xl/rejoin:min-h-0 @3xl/rejoin:overflow-y-auto @3xl/rejoin:pr-1">
              <TargetsCard ctl={ctl} />
              <ServerCard ctl={ctl} columns={1} />
              <TimingCard ctl={ctl} columns={1} />
            </div>
            <div className="@3xl/rejoin:min-h-0">
              <LiveList ctl={ctl} />
            </div>
          </div>
        ) : (
          <>
            <TargetsCard ctl={ctl} />
            <ServerCard ctl={ctl} columns={3} />
            <TimingCard ctl={ctl} columns={3} />
            <LiveCycleList ctl={ctl} />
          </>
        )}
      </div>
    </div>
  );
}
