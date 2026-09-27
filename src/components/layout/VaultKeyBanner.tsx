import { KeyRound } from "lucide-react";
import { useStore } from "../../store";
import { useTr } from "../../i18n/text";

/**
 * Faixa fixa para problema com o `AccountData.key`.
 *
 * Por que faixa e não toast/linha de status: sem senha, o arquivo de chave é a
 * **única** coisa que abre as contas. Se ele fica ruim, o app continua
 * funcionando o dia inteiro (a chave está em memória) e só o boot seguinte
 * descobre — aí não há senha para digitar, porque nunca houve senha. A primeira
 * tentativa usou `setActionStatusMessage`, que é explicitamente substituível:
 * qualquer "Launching…" apagava o aviso. Aqui ele sai da tela quando, e só
 * quando, o backend diz que o problema sumiu.
 *
 * O texto vem do **código** que o backend manda, não de uma frase pronta em
 * inglês: é o que faz o aviso passar por `t()` e pelo catálogo de i18n.
 */
export function VaultKeyBanner() {
  const t = useTr();
  const store = useStore();
  const warning = store.vaultKeyWarning;

  if (!warning) return null;

  // Transitório é brando de propósito: quase sempre é antivírus segurando o
  // arquivo por um instante, e a gravação seguinte resolve. Alarme falso treina
  // o usuário a ignorar alarme, e aí a rede de verdade não vale nada.
  const mild =
    warning.code === "writeFailedTransient" ||
    warning.code === "weakWrapper" ||
    warning.code === "syncUnconfirmed";

  const message = (() => {
    switch (warning.code) {
      case "writeFailedTransient":
        return t("Could not update the account key file just now ({{path}}); the app will try again on the next change.", { path: warning.path });
      case "weakWrapper":
        return t("The account key file ({{path}}) is saved without Windows protection. Your accounts still open, but the key is weaker than it should be — restart the app to try again.", { path: warning.path });
      case "syncUnconfirmed":
        return t("Saved {{path}}, but this drive did not confirm the write. A power loss right now could lose the last change.", { path: warning.path });
      // O arquivo ficou legível em disco: é o aviso mais consequente dos cinco,
      // porque o usuário acha que está criptografado e não está.
      case "migrationFailed":
        return t("{{path}} could not be encrypted and is still plain text: the login cookie of every account is readable on this PC. Nothing was lost — open Settings > Misc > Change Encryption Method to try again, or set a password.", { path: warning.path });
      default:
        return t("The account key file could not be written ({{path}}). Your accounts open while the app is running, but they may not open after you close it — make a backup now in Settings > Misc > Data.", { path: warning.path });
    }
  })();

  return (
    <div
      role="alert"
      className={[
        // `pr-28` reserva o canto: `ModalWindowControls` é `fixed top-2.5 right-3`, e
        // nas telas de senha/criptografia não há TitleBar segurando o topo — a
        // pílula de minimizar/fechar ficava **sobre** a primeira linha do aviso,
        // cortando justamente o fim do texto.
        "flex items-start gap-2 pl-4 pr-28 py-1.5 border-b shrink-0",
        mild
          ? "bg-amber-600/15 border-amber-500/20"
          : "bg-red-600/15 border-red-500/25",
      ].join(" ")}
    >
      <div className={["mt-0.5 shrink-0", mild ? "text-amber-300" : "text-red-300"].join(" ")}>
        <KeyRound size={14} strokeWidth={1.8} />
      </div>
      <div className="min-w-0 flex-1">
        <span className={["text-xs", mild ? "text-amber-200" : "text-red-200"].join(" ")}>
          {message}
        </span>
        {warning.detail ? (
          <span className="block text-[11px] theme-muted break-words">{warning.detail}</span>
        ) : null}
      </div>
    </div>
  );
}
