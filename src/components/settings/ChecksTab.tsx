import { useTr } from "../../i18n/text";
import { useStore } from "../../store";
import { ActionCard } from "./settingsParts";

/**
 * Settings › Checks: os checklists num lugar só (pedido do dono, 26/10/2026 —
 * o do launch ficava em Launch e o do navegador em Accounts, e ninguém achava).
 * Cada cartão abre o mesmo `DiagnosticsDialog` que a faixa de erro do launch
 * abre; nenhum deles fecha nada nem baixa nada.
 */
export function ChecksTab() {
  const t = useTr();
  const store = useStore();

  return (
    <div className="space-y-0">
      <div className="px-1 pb-1 text-[12px] text-zinc-500">
        {t("Something does nothing? Run the matching check: it says what is in the way and what to do.")}
      </div>
      {/* Ideia 16: a mesma checagem que a faixa de erro do launch abre. */}
      <ActionCard
        title={t("Launch does nothing?")}
        description={t(
          "Checks Roblox, folders, internet, stuck Roblox processes and Multi Roblox. It never closes anything."
        )}
        action={t("Run check")}
        onAction={() => store.setDiagnosticsOpen(true)}
      />
      {/* Um usuário relatou o navegador "não fazer nada" ao criar contas — era o
          Chrome do PC, usado quando o download falha. A checagem diz qual abre
          e se ele responde (chromium/diagnose.rs). */}
      <ActionCard
        title={t("Login browser does nothing?")}
        description={t(
          "Checks which browser logins and account creation use, whether it opens and answers, and whether the Roblox page loads. Downloads nothing."
        )}
        action={t("Run check")}
        onAction={() => store.setBrowserCheckOpen(true)}
      />
    </div>
  );
}
