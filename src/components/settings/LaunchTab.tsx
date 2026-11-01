import type { UseSettingsReturn } from "../../hooks/useSettings";
import { Toggle } from "../ui/Toggle";
import { NumberField } from "../ui/NumberField";
import { Divider } from "../ui/Divider";
import { SectionLabel } from "../ui/SectionLabel";
import { WarningBadge } from "../ui/WarningBadge";
import { Select } from "../ui/Select";
import { useTr } from "../../i18n/text";
import { isWindowsUserAgent } from "./settingsParts";

/** Espelha `MIN_JOIN_GAP_SECS` em `commands/launch.rs`: abaixo disso nada muda. */
const MIN_JOIN_DELAY_SECONDS = 8;

/** Valores de `General.WrongAccountQueue` (`WrongAccountQueue::from_setting`). */
const WRONG_ACCOUNT_QUEUE_OPTIONS = [
  { value: "ask", label: "Ask me" },
  { value: "continue", label: "Keep launching" },
  { value: "stop", label: "Stop the queue" },
];

/**
 * Settings › Launch: como as contas abrem. Saiu de General (Multi Roblox,
 * espaçamento, checagens) e de Misc (Shuffle Job ID e os dois "Auto Close")
 * em 11/10/2026 — ver docs/features/settings.md.
 */
export function LaunchTab({ s }: { s: UseSettingsReturn }) {
  const t = useTr();
  const serialLaunch = s.getBool("General", "AsyncJoin");
  const isWindows = isWindowsUserAgent();

  return (
    <div className="space-y-0">
      <SectionLabel>Several accounts at once</SectionLabel>
      <Toggle
        checked={s.getBool("General", "EnableMultiRbx")}
        onChange={(v) => s.setBool("General", "EnableMultiRbx", v)}
        label={<>Multi Roblox<WarningBadge>use at own risk</WarningBadge></>}
        description="Allow multiple Roblox instances to run simultaneously"
      />
      {/* Ideia 3 (platform/windows/core.rs, `apply_singleton_reservation`):
          experimental e desligado; o método atual continua o padrão. */}
      {isWindows && (
        <Toggle
          checked={s.getBool("General", "ReserveSingletonEvent")}
          onChange={(v) => s.setBool("General", "ReserveSingletonEvent", v)}
          disabled={!s.getBool("General", "EnableMultiRbx")}
          label={<>Experimental: keep clients open across teleports<WarningBadge>use at own risk</WarningBadge></>}
          description="Reserves the name Roblox uses to allow only one window, so a teleport can't close another account. Needs Multi Roblox. Not tested with every game yet: turn it off if a window stops opening."
        />
      )}
      {/* Opt-in e desligado por padrão (CLAUDE.md): é o único caminho que fecha
          clientes de outras contas. */}
      <Toggle
        checked={s.getBool("General", "AutoCloseRobloxForMultiRbx")}
        onChange={(v) => s.setBool("General", "AutoCloseRobloxForMultiRbx", v)}
        label="Auto Close Roblox for Multi Roblox"
        description="If Multi Roblox cannot be enabled, close open Roblox windows automatically and continue"
      />

      <Divider />
      <SectionLabel>Time between accounts</SectionLabel>
      {/* A chave continua `AsyncJoin`, mas ela SERIALIZA a fila (launch.rs
          espera o sinal `next_account`, com teto de 120 s). O rotulo antigo,
          "Async Launching", prometia o contrario do que o codigo faz. Nada na
          tela manda o sinal: na pratica sao 2 minutos entre contas — e e isso
          que a descricao diz, para nao parecer o "Start the next account..." */}
      <Toggle
        checked={serialLaunch}
        onChange={(v) => s.setBool("General", "AsyncJoin", v)}
        label="Launch one account at a time"
        description="Leaves 2 minutes between accounts, so each one has time to load. The slowest option. Off: accounts start spaced by the delay below."
      />
      <NumberField
        value={s.getNumber("General", "AccountJoinDelay", 8)}
        onChange={(v) => s.setNumber("General", "AccountJoinDelay", v)}
        label="Account Join Delay"
        description={
          serialLaunch
            ? "Not used while “Launch one account at a time” is on. It applies again when you turn that off."
            : "Roblox rejects logins that arrive too close together, so 8 seconds is the floor."
        }
        disabled={serialLaunch}
        min={MIN_JOIN_DELAY_SECONDS}
        max={60}
        step={0.5}
        suffix="sec"
      />
      {/* launch.rs (`wait_for_game_join`): segue quando o log do Roblox diz que
          a conta entrou; sem log achado, vale o delay acima. Só no Windows,
          onde o log é lido (client_health.rs). */}
      {isWindows && (
        <Toggle
          checked={s.get("General", "WaitForGameJoin", "true") !== "false"}
          onChange={(v) => s.setBool("General", "WaitForGameJoin", v)}
          label="Start the next account once the previous one is in the game"
          description={
            serialLaunch
              ? "Not used while “Launch one account at a time” is on. It applies again when you turn that off."
              : "Doesn't wait out the whole delay above: never sooner than 8 seconds, at most 20 (or the delay, if longer)."
          }
          disabled={serialLaunch}
        />
      )}
      {/* Conta errada na fila (client_health.rs, `WrongAccountQueue` em
          launch.rs): pedido do dono em 31/10/2026 — a fila parava sozinha. */}
      {isWindows && (
        <div className="flex items-center gap-3 py-2 px-1">
          <div className="min-w-0">
            <div className="text-[13px] text-zinc-300">{t("When an account opens as a different Roblox account")}</div>
            <div className="mt-0.5 text-[12px] text-zinc-500">
              {t("What happens to the accounts still waiting in the queue. The client that opened stays open.")}
            </div>
          </div>
          <div className="ml-auto min-w-[180px]">
            <Select
              ariaLabel="When an account opens as a different Roblox account"
              value={s.get("General", "WrongAccountQueue", "ask")}
              options={WRONG_ACCOUNT_QUEUE_OPTIONS}
              onChange={(value) => s.set("General", "WrongAccountQueue", value)}
            />
          </div>
        </div>
      )}

      <Divider />
      <SectionLabel>Before each launch</SectionLabel>
      <Toggle
        checked={s.get("General", "CheckModerationBeforeLaunch", "true") === "true"}
        onChange={(v) => s.setBool("General", "CheckModerationBeforeLaunch", v)}
        label="Check Bans Before Launch"
        description="Ask Roblox if an account is banned right before opening it, and skip it if so"
      />
      <Toggle
        checked={s.get("General", "WarnOnOnlineJoin", "true") === "true"}
        onChange={(v) => s.setBool("General", "WarnOnOnlineJoin", v)}
        label="Warn Before Joining Online Accounts"
        description="Show a confirmation if selected accounts are already online/in-game"
      />
      <Toggle
        checked={s.getBool("General", "AutoCloseLastProcess")}
        onChange={(v) => s.setBool("General", "AutoCloseLastProcess", v)}
        label="Auto Close Last Process"
        description="Close the previous Roblox instance when launching a new one for the same account"
      />
      <Toggle
        checked={s.getBool("General", "ShuffleJobId")}
        onChange={(v) => s.setBool("General", "ShuffleJobId", v)}
        label="Shuffle Job ID"
        description="Picks a random server when you have not chosen one. A typed Job ID or following a player wins over it."
      />
    </div>
  );
}
