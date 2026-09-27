import { useEffect, useState } from "react";
import { useStore } from "../../store";
import { SidebarSection } from "./SidebarSection";
import { Select } from "../ui/Select";
import { useTr } from "../../i18n/text";
import {
  EMPTY_ACCOUNT_LAUNCH_OVERRIDES,
  readAccountLaunchOverrides,
  writeAccountLaunchOverrides,
  type Account,
  type AccountLaunchOverrides,
} from "../../types";

/**
 * Exceções de launch desta conta: FPS, volume, qualidade, tela cheia e
 * minimizar, por cima do perfil global de Settings.
 *
 * Campo vazio quer dizer **herda o global**, e é por isso que tudo aqui é
 * `string` em vez de número: um FPS apagado não é FPS 0. Cada controle grava na
 * hora em `Account.Fields`, do mesmo jeito que o Alias — não existe botão
 * "Salvar" que se possa esquecer de apertar antes de lançar.
 *
 * Ressalva que a descrição na tela também diz: os arquivos que o Roblox lê
 * (`ClientAppSettings.json` e `GlobalBasicSettings_13.xml`) são globais. Isto
 * funciona porque a fila de launch é sequencial e o app reescreve os arquivos
 * imediatamente antes de cada cliente abrir; se o jogador mudar as
 * configurações dentro do jogo, o Roblox reescreve o XML e o valor pode vazar
 * para a próxima conta que abrir sem exceção própria.
 */
export function AccountLaunchOverrides({ account }: { account: Account }) {
  const t = useTr();
  const store = useStore();
  const [draft, setDraft] = useState<AccountLaunchOverrides>(EMPTY_ACCOUNT_LAUNCH_OVERRIDES);

  useEffect(() => {
    setDraft(readAccountLaunchOverrides(account.Fields));
  }, [account.UserID]);

  function save(next: AccountLaunchOverrides) {
    setDraft(next);
    void store.updateAccount({
      ...account,
      Fields: writeAccountLaunchOverrides(account.Fields, next),
    });
  }

  const ligado = draft.enabled;

  return (
    <SidebarSection title={t("Launch Exceptions")}>
      <p className="text-[12px] theme-muted mb-1.5">
        {t("Launch this account with its own FPS, volume, quality and screen mode. Empty fields follow the global settings.")}
      </p>

      <label className="flex items-center gap-2 text-[12px] cursor-pointer select-none">
        <input
          type="checkbox"
          checked={ligado}
          onChange={(e) => save({ ...draft, enabled: e.target.checked })}
          className="accent-[var(--accent-color)]"
        />
        <span className="text-[var(--panel-fg)]">{t("Use exceptions for this account")}</span>
      </label>

      {ligado && (
        <div className="flex flex-col gap-2 mt-1.5">
          <div className="flex items-center gap-1.5">
            <span className="theme-muted text-[11px] w-16 shrink-0">{t("FPS")}</span>
            <input
              value={draft.maxFps}
              inputMode="numeric"
              placeholder={t("Global")}
              aria-label={t("FPS")}
              onChange={(e) => setDraft({ ...draft, maxFps: e.target.value.replace(/[^0-9]/g, "") })}
              onBlur={() => save(draft)}
              onKeyDown={(e) => e.key === "Enter" && save(draft)}
              className="sidebar-input flex-1 min-w-0"
            />
          </div>

          <div className="flex items-center gap-1.5">
            <span className="theme-muted text-[11px] w-16 shrink-0">{t("Volume")}</span>
            <input
              value={draft.volume}
              inputMode="decimal"
              placeholder="0–10"
              aria-label={t("Volume")}
              onChange={(e) => setDraft({ ...draft, volume: e.target.value.replace(/[^0-9.]/g, "") })}
              onBlur={() => save(draft)}
              onKeyDown={(e) => e.key === "Enter" && save(draft)}
              className="sidebar-input flex-1 min-w-0"
            />
          </div>

          <div className="flex items-center gap-1.5">
            <span className="theme-muted text-[11px] w-16 shrink-0">{t("Quality")}</span>
            <Select
              value={draft.graphics}
              ariaLabel={t("Quality")}
              options={[
                { value: "", label: t("Global") },
                { value: "auto", label: t("Automatic") },
                ...Array.from({ length: 10 }, (_, i) => ({
                  value: String(i + 1),
                  label: String(i + 1),
                })),
              ]}
              onChange={(graphics) => save({ ...draft, graphics })}
              className="flex-1 min-w-0"
            />
          </div>

          <div className="flex items-center gap-1.5">
            <span className="theme-muted text-[11px] w-16 shrink-0">{t("Screen")}</span>
            <Select
              value={draft.fullscreen}
              ariaLabel={t("Screen")}
              options={[
                { value: "", label: t("Global") },
                { value: "true", label: t("Fullscreen") },
                { value: "false", label: t("Windowed") },
              ]}
              onChange={(fullscreen) => save({ ...draft, fullscreen })}
              className="flex-1 min-w-0"
            />
          </div>

          {draft.fullscreen === "false" && (
            <div className="flex items-center gap-1.5">
              <span className="theme-muted text-[11px] w-16 shrink-0">{t("Size")}</span>
              <input
                value={draft.windowWidth}
                inputMode="numeric"
                placeholder={t("width")}
                aria-label={t("Window width")}
                onChange={(e) =>
                  setDraft({ ...draft, windowWidth: e.target.value.replace(/[^0-9]/g, "") })
                }
                onBlur={() => save(draft)}
                onKeyDown={(e) => e.key === "Enter" && save(draft)}
                className="sidebar-input flex-1 min-w-0"
              />
              <input
                value={draft.windowHeight}
                inputMode="numeric"
                placeholder={t("height")}
                aria-label={t("Window height")}
                onChange={(e) =>
                  setDraft({ ...draft, windowHeight: e.target.value.replace(/[^0-9]/g, "") })
                }
                onBlur={() => save(draft)}
                onKeyDown={(e) => e.key === "Enter" && save(draft)}
                className="sidebar-input flex-1 min-w-0"
              />
            </div>
          )}

          <div className="flex items-center gap-1.5">
            <span className="theme-muted text-[11px] w-16 shrink-0">{t("On open")}</span>
            <Select
              value={draft.startMinimized}
              ariaLabel={t("On open")}
              options={[
                { value: "", label: t("Global") },
                { value: "true", label: t("Minimized") },
                { value: "false", label: t("Normal window") },
              ]}
              onChange={(startMinimized) => save({ ...draft, startMinimized })}
              className="flex-1 min-w-0"
            />
          </div>

          <p className="text-[11px] theme-muted">
            {t("Roblox keeps these settings in files shared by every client. The app rewrites them right before each account opens, so changing settings inside the game can leak to the next account without its own exceptions.")}
          </p>
        </div>
      )}
    </SidebarSection>
  );
}
