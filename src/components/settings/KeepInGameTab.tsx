import type { UseSettingsReturn } from "../../hooks/useSettings";
import { Toggle } from "../ui/Toggle";
import { Divider } from "../ui/Divider";
import { WarningBadge } from "../ui/WarningBadge";
import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { ElsewhereLink, isWindowsUserAgent } from "./settingsParts";

/**
 * Settings › Keep in game: o que mantém as contas no jogo sem o usuário olhar.
 *
 * Duas opções têm casa em outra página e aqui só há o caminho até lá:
 * - o padrão da reconexão (`General.AutoReconnect`) muda na página Session, ao
 *   lado das contas em jogo, onde cada conta também tem o seu;
 * - quem entra no Auto Rejoin, onde e de quanto em quanto tempo é a aba Auto
 *   Rejoin da página AFK Mode. O interruptor `BottingEnabled` fica aqui: ele
 *   só põe o Auto Rejoin na barra de ações da lista e separa os perfis de
 *   otimização, nada que a página AFK Mode mostre.
 */
export function KeepInGameTab({ s }: { s: UseSettingsReturn }) {
  const t = useTr();
  const store = useStore();
  const isWindows = isWindowsUserAgent();
  const reconnectDefault = s.getBool("General", "AutoReconnect");

  return (
    <div className="space-y-0">
      <Toggle
        checked={s.getBool("General", "BottingEnabled")}
        onChange={(v) => s.setBool("General", "BottingEnabled", v)}
        label={<>Auto Rejoin<WarningBadge>advanced</WarningBadge></>}
        description="Enable account cycling tools to keep selected alts rejoining automatically"
      />
      <ElsewhereLink
        title={t("Auto Rejoin options")}
        description={t("Who rejoins, in which place and how often is set on the AFK Mode page.")}
        action={t("Open AFK Mode")}
        onNavigate={() => store.setActivePage("afk")}
      />

      {/* commands/reconnect.rs: padrão de todas as contas; o campo
          `AutoReconnect` da conta (lista "Em jogo" da página Session) vence. */}
      {isWindows && (
        <>
          <Divider />
          <ElsewhereLink
            title={t("Reconnect accounts that drop")}
            description={
              reconnectDefault
                ? t("On for every account that doesn't set its own. It's changed next to the accounts in game.")
                : t("Off for every account that doesn't set its own. It's changed next to the accounts in game.")
            }
            action={t("Change it on the Session page")}
            onNavigate={() => store.setActivePage("session")}
          />
        </>
      )}

      {/* commands/keep_awake.rs: SetThreadExecutionState só com o sistema
          exigido — a tela continua apagando. */}
      {isWindows && (
        <>
          <Divider />
          <Toggle
            checked={s.get("General", "KeepPcAwake", "true") !== "false"}
            onChange={(v) => s.setBool("General", "KeepPcAwake", v)}
            label="Keep the PC awake while accounts are kept in game"
            description="Windows won't go to sleep while AFK Mode, Auto Rejoin or auto-reconnect is running. The screen can still turn off."
          />
        </>
      )}
      {/* commands/shake_stop.rs: com o ciclo do AFK ou uma gravação levando o
          cursor, o Parar fica fora de alcance. Lê só a posição do cursor, e só
          enquanto algo está rodando. Desligado por padrão (escolha do dono). */}
      {isWindows && (
        <Toggle
          checked={s.getBool("Afk", "ShakeToStop")}
          onChange={(v) => s.setBool("Afk", "ShakeToStop", v)}
          label="Stop AFK and recordings when I shake the mouse"
          description="Shake the mouse hard left and right to stop when the app is moving it. Only the mouse position is read."
        />
      )}
    </div>
  );
}
